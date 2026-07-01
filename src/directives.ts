// State management (global + per-session overlay), user directives parsed
// from message text (Operator/EMBODY/DISMISS), history strip + synthetic-
// reminder patching used by the chat.messages.transform hook.

import path from "node:path";
import {
  BUILD_SWITCH_MARKER, DEFAULT_AGENT_NAME, DEFAULT_SPIRIT, GLOBAL_STATE_PATH,
  HISTORY_DIRECTIVE_LINE, HISTORY_DISMISS_LINE,
  MODE_PRESERVATION_BLOCK,
  PLAN_APPROVED_MARKER, PLAN_MODE_MARKER, RUNTIME_DIR, TRACK_MODE_MARKER,
} from "./paths.ts";
import { normalizeForMatch, readJson, writeJson } from "./util.ts";
import { normalizeAgentName, normalizeSpirit } from "./spirit.ts";

export function sessionStatePath(sessionID) {
  return path.join(RUNTIME_DIR, `${sessionID}.json`);
}

export function defaultState() {
  return {
    version: 4,
    operator: null,
    agentName: DEFAULT_AGENT_NAME,
    activeName: DEFAULT_AGENT_NAME,
    embodiedSpirit: DEFAULT_SPIRIT,
    ignoredSpiritDirective: null,
    lastSpiritChangeAt: null,
    lastUpdatedAt: null,
  };
}

async function loadGlobalState() {
  const state = await readJson(GLOBAL_STATE_PATH, defaultState());
  return { ...defaultState(), ...(state || {}) };
}

export async function loadState(sessionID) {
  const globalState = await loadGlobalState();
  if (!sessionID) return globalState;

  const sessionState = await readJson(sessionStatePath(sessionID), null);
  return { ...globalState, ...(sessionState || {}) };
}

export async function saveState(sessionID, partial) {
  const current = await loadState(sessionID);
  const next = {
    ...current,
    ...partial,
    lastUpdatedAt: new Date().toISOString(),
  };
  next.embodiedSpirit = (await normalizeSpirit(next.embodiedSpirit)) || DEFAULT_SPIRIT;
  next.agentName = normalizeAgentName(next.agentName);
  next.activeName = next.agentName;
  delete next.conjuredSpirit;
  delete next.summonedSpirit;

  await writeJson(GLOBAL_STATE_PATH, { ...defaultState(), ...next });
  if (sessionID) {
    await writeJson(sessionStatePath(sessionID), next);
  }

  return next;
}

// ── directive parsing ──────────────────────────────────────────────────────

function lastDirectiveValue(text, label) {
  const pattern = new RegExp(
    String.raw`(?:^|\n)\s*${label}:\s*(.+?)\s*(?=\n|$)`,
    "gi",
  );
  const matches = Array.from(String(text || "").matchAll(pattern));
  return matches.length ? matches.at(-1)?.[1] ?? null : null;
}

function hasDirectiveLine(text, label) {
  const pattern = new RegExp(String.raw`(?:^|\n)\s*${label}\s*(?::\s*.+)?(?=\n|$)`, "i");
  return pattern.test(String(text || ""));
}

function parseUserDirectives(text) {
  const source = String(text || "");
  return {
    operator: (lastDirectiveValue(source, "Operator") || "").trim() || null,
    embody: (lastDirectiveValue(source, "EMBODY") || "").trim() || null,
    dismiss: hasDirectiveLine(source, "DISMISS"),
  };
}

export async function recordDirectives(sessionID, text) {
  const directives = parseUserDirectives(text);
  const current = await loadState(sessionID);
  const updates: Record<string, unknown> = {};

  if (directives.operator) updates.operator = directives.operator;
  if (directives.dismiss) updates.ignoredSpiritDirective = null;
  if (directives.embody) {
    const resolved = await normalizeSpirit(directives.embody);
    if (resolved) {
      updates.embodiedSpirit = resolved;
      updates.lastSpiritChangeAt = new Date().toISOString();
      updates.ignoredSpiritDirective = null;
    } else {
      updates.ignoredSpiritDirective = directives.embody;
    }
  }

  if (!Object.keys(updates).length) return current;
  return saveState(sessionID, updates);
}

// ── history strip + synthetic-reminder patching ────────────────────────────
// User-visible directive lines (Operator/EMBODY/DISMISS) get stripped from
// prior turns in the conversation history so the agent doesn't re-read them
// as content. Synthetic reminders that announce plan-mode or read-only
// constraints get a mode-preservation block appended so they don't flatten
// the active spirit's voice.

function stripControlDirectivesFromHistory(text) {
  return String(text || "")
    .split("\n")
    .filter(
      (line) =>
        !HISTORY_DIRECTIVE_LINE.test(line) && !HISTORY_DISMISS_LINE.test(line),
    )
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

export function patchDirectiveHistory(messages) {
  if (!Array.isArray(messages)) return messages;

  for (const message of messages) {
    if (message?.role && message.role !== "user") continue;
    if (typeof message?.content === "string") {
      message.content = stripControlDirectivesFromHistory(message.content);
    }
    if (Array.isArray(message?.content)) {
      for (const part of message.content) {
        if (part?.type === "text" && typeof part.text === "string") {
          part.text = stripControlDirectivesFromHistory(part.text);
        }
      }
    }
    if (Array.isArray(message?.parts)) {
      for (const part of message.parts) {
        if (part?.type === "text" && typeof part.text === "string") {
          part.text = stripControlDirectivesFromHistory(part.text);
        }
      }
    }
  }

  return messages;
}

function appendBeforeReminderClose(text, addition) {
  if (!text.includes("</system-reminder>")) {
    return `${text}\n\n${addition}`;
  }

  return text.replace("</system-reminder>", `${addition}\n</system-reminder>`);
}

const SYNTHETIC_REMINDER_PHRASES = [
  PLAN_MODE_MARKER,
  TRACK_MODE_MARKER,
  BUILD_SWITCH_MARKER,
  PLAN_APPROVED_MARKER,
  "plan mode",
  "read only",
  "read-only",
  "do not execute",
  "you can now edit files",
  "continue with your tasks",
];

function patchSyntheticReminderText(text) {
  const source = String(text || "");
  if (!source) return source;

  const normalized = normalizeForMatch(source);
  const isSyntheticConstraint = SYNTHETIC_REMINDER_PHRASES.some(
    (phrase) => normalized.includes(normalizeForMatch(phrase)),
  );
  if (isSyntheticConstraint && !source.includes("## Identity And Mode Preservation")) {
    return appendBeforeReminderClose(source, `\n${MODE_PRESERVATION_BLOCK}\n`);
  }

  return source;
}

export function patchSyntheticReminders(messages) {
  if (!Array.isArray(messages)) return messages;

  for (const message of messages) {
    if (!Array.isArray(message?.parts)) continue;
    for (const part of message.parts) {
      if (part?.type !== "text" || typeof part.text !== "string") continue;
      if (!part.synthetic && !part.text.includes("<system-reminder>")) continue;
      part.text = patchSyntheticReminderText(part.text);
    }
  }

  return messages;
}

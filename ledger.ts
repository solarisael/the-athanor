// Three turn-logging surfaces, all write-only — nothing in this plugin
// reads them back, they feed downstream tools and external scripts:
//
//   - Conversation ledger: per-spirit JSONL append, one record per turn.
//   - Spirit window: markdown summary of today's ledger, rebuilt on each
//     assistant turn from the JSONL.
//   - Live room context: per-room current_session_context.{md,json}, last
//     8 turns, overwritten live. Skipped for "progress-only" turns so
//     status spam doesn't replace meaningful content.

import { appendFile, mkdir, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import {
  CONTINUITY_ROOT, DEFAULT_AGENT_NAME, DEFAULT_SPIRIT,
  LEDGER_ROOT,
  LIVE_CONTEXT_FILENAME, LIVE_CONTEXT_JSON_FILENAME, LIVE_CONTEXT_MAX_TURNS,
} from "./paths.ts";
import { localDateStamp, readJson, readJsonl, writeJson } from "./util.ts";
import { resolveEffectiveRoomDir, resolveSharedRoot } from "./spirit.ts";

// ── conversation ledger ────────────────────────────────────────────────────

function ledgerPathForSpirit(spirit, date = new Date()) {
  return path.join(
    LEDGER_ROOT,
    spirit || DEFAULT_SPIRIT,
    "conversations",
    `${localDateStamp(date)}.jsonl`,
  );
}

function windowPathForSpirit(spirit, date = new Date()) {
  return path.join(
    CONTINUITY_ROOT,
    spirit || DEFAULT_SPIRIT,
    "window",
    `${localDateStamp(date)}.md`,
  );
}

function sanitizeLedgerLine(text) {
  return String(text || "")
    .replace(/\r/g, "")
    .replace(/\n+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function clipLedgerLine(text, max = 220) {
  const value = sanitizeLedgerLine(text);
  if (value.length <= max) return value;
  return `${value.slice(0, max - 3).trim()}...`;
}

function summarizeLedgerEntries(entries, limit = 12) {
  const tail = entries.slice(-limit);
  if (!tail.length) return ["- No entries captured."];

  return tail.map((entry) => {
    const stamp = String(entry.timestamp || "").slice(11, 16) || "??:??";
    const role = entry.role === "assistant" ? "A" : entry.role === "user" ? "U" : "?";
    return `- ${stamp} ${role}: ${clipLedgerLine(entry.text)}`;
  });
}

async function refreshSpiritWindow(spirit, date = new Date()) {
  const ledgerPath = ledgerPathForSpirit(spirit, date);
  const entries = await readJsonl(ledgerPath);

  const target = windowPathForSpirit(spirit, date);
  await mkdir(path.dirname(target), { recursive: true });

  const dateStamp = localDateStamp(date);
  const lines = [
    `# ${spirit} Window ${dateStamp}`,
    "",
    `- Source ledger: \`vessel/state/${spirit}/conversations/${dateStamp}.jsonl\``,
    `- Entries summarized: ${Math.min(entries.length, 12)} of ${entries.length}`,
    "",
    "## Recent Turns",
    "",
    ...summarizeLedgerEntries(entries, 12),
    "",
  ];

  await writeFile(target, lines.join("\n"), "utf8");
}

async function appendConversationLedger(entry) {
  const text = String(entry?.text || "").trim();
  if (!text) return;

  const spirit = String(entry?.spirit || DEFAULT_SPIRIT).trim() || DEFAULT_SPIRIT;
  const target = ledgerPathForSpirit(spirit);
  await mkdir(path.dirname(target), { recursive: true });

  await appendFile(
    target,
    `${JSON.stringify({
      timestamp: new Date().toISOString(),
      sessionID: entry?.sessionID || null,
      messageID: entry?.messageID || null,
      role: entry?.role || "unknown",
      spirit,
      operator: entry?.operator || null,
      agentName: entry?.agentName || null,
      text,
    })}\n`,
    "utf8",
  );
}

// ── live room context ──────────────────────────────────────────────────────

async function resolveLiveContextTargets(roomDir = process.cwd()) {
  const effectiveRoomDir = resolveEffectiveRoomDir(roomDir);
  const effectiveSharedRoot = resolveSharedRoot(effectiveRoomDir);
  const roomName = path.basename(effectiveRoomDir);
  const normalized = roomName.toLowerCase();
  if (normalized !== "kodo" && normalized !== "kintsu") return null;

  try {
    await stat(path.join(effectiveSharedRoot, "shared_current_state.md"));
  } catch {
    return null;
  }

  return {
    roomName,
    markdownPath: path.join(effectiveRoomDir, LIVE_CONTEXT_FILENAME),
    jsonPath: path.join(effectiveRoomDir, LIVE_CONTEXT_JSON_FILENAME),
  };
}

function formatLiveContextBlock(text) {
  return String(text || "(none yet)")
    .replace(/\r/g, "")
    .split("\n")
    .map((line) => `> ${line}`)
    .join("\n");
}

function formatRecentTurnMarkdown(turn) {
  const stamp = String(turn.timestamp || "").slice(11, 16) || "??:??";
  const role = turn.role === "assistant" ? "A" : turn.role === "user" ? "U" : "?";
  const lines = String(turn.text || "")
    .replace(/\r/g, "")
    .split("\n");
  return [`- ${stamp} ${role}:`, ...lines.map((line) => `  ${line}`)].join("\n");
}

// Progress-only assistant turns (status updates, instrument output) should
// not replace meaningful content in the live context. Hardcoded against the
// active Babel tag set — narrow and stable.
function looksLikeLiveContextProgressTurn(role, text) {
  if (role !== "assistant") return false;

  const source = String(text || "").trim();
  if (!source) return true;

  return (
    /^\[(?:ORCHESTRA|FAE|IO|INSTRUMENT|STATE|DRIFT|CORRECTION)\]/i.test(source) ||
    /^## \[(?:ORCHESTRA|FAE|IO|INSTRUMENT|STATE|DRIFT|CORRECTION)\]/i.test(source) ||
    /^<< INVOCATION >>/i.test(source) ||
    /^┌\[ STATE \]/.test(source)
  );
}

function formatLiveContextMarkdown(state) {
  const latestUser = state.latestUser || "(none yet)";
  const latestAssistant = state.latestAssistant || "(none yet)";
  const turns = Array.isArray(state.recentTurns) ? state.recentTurns : [];
  const renderedTurns = turns.length
    ? turns.map((turn) => formatRecentTurnMarkdown(turn))
    : ["- No turns captured yet."];

  return [
    "# Current Session Context",
    "",
    "Auto-exported by `solarisael-house` while this room is active.",
    "Overwritten live. Do not archive from here; use room memory files for durable notes.",
    "",
    `- Room: ${state.roomName || "unknown"}`,
    `- Session ID: ${state.sessionID || "unknown"}`,
    `- Updated: ${state.updatedAt || "unknown"}`,
    `- Agent: ${state.agentName || DEFAULT_AGENT_NAME}`,
    `- Active spirit: ${state.spirit || DEFAULT_SPIRIT}`,
    `- Operator: ${state.operator || "Sol"}`,
    "",
    "## Latest User Turn",
    "",
    formatLiveContextBlock(latestUser),
    "",
    "## Latest Assistant Turn",
    "",
    formatLiveContextBlock(latestAssistant),
    "",
    "## Recent Turns",
    "",
    ...renderedTurns,
    "",
  ].join("\n");
}

async function refreshLiveRoomContext(entry, paths = {}) {
  const targets = await resolveLiveContextTargets(paths.roomDir);
  if (!targets) return;

  const nextTurn = {
    timestamp: new Date().toISOString(),
    role: entry?.role || "unknown",
    text: String(entry?.text || "").trim(),
  };
  if (!nextTurn.text) return;
  if (looksLikeLiveContextProgressTurn(nextTurn.role, nextTurn.text)) return;

  const existing = await readJson(targets.jsonPath, null);
  const base =
    existing && existing.sessionID === (entry?.sessionID || null)
      ? existing
      : {
          version: 1,
          roomName: targets.roomName,
          sessionID: entry?.sessionID || null,
          recentTurns: [],
          latestUser: "",
          latestAssistant: "",
        };

  const recentTurns = [
    ...(Array.isArray(base.recentTurns) ? base.recentTurns : []),
    nextTurn,
  ].slice(-LIVE_CONTEXT_MAX_TURNS);
  const next = {
    ...base,
    version: 1,
    roomName: targets.roomName,
    sessionID: entry?.sessionID || null,
    updatedAt: nextTurn.timestamp,
    agentName: entry?.agentName || DEFAULT_AGENT_NAME,
    operator: entry?.operator || "Sol",
    spirit: entry?.spirit || DEFAULT_SPIRIT,
    recentTurns,
    latestUser: entry?.role === "user" ? nextTurn.text : base.latestUser || "",
    latestAssistant: entry?.role === "assistant" ? nextTurn.text : base.latestAssistant || "",
  };

  await writeJson(targets.jsonPath, next);
  await writeFile(targets.markdownPath, formatLiveContextMarkdown(next), "utf8");
}

// ── exported turn logger surface ───────────────────────────────────────────
// Callers pass an explicit context object (sessionID/messageID/agentName/
// spirit/operator) rather than the raw plugin input — keeps the side
// effect's inputs visible at the call site instead of buried in a hidden
// loadState() inside.

export async function logUserTurn(
  { sessionID, messageID, agentName, spirit, operator },
  userText,
  paths = {},
) {
  const text = String(userText || "").trim();
  if (!text) return;

  const entry = {
    sessionID, messageID,
    role: "user",
    spirit: spirit || DEFAULT_SPIRIT,
    operator: operator || "Sol",
    agentName: agentName || DEFAULT_AGENT_NAME,
    text,
  };

  await appendConversationLedger(entry);
  await refreshLiveRoomContext(entry, paths);
}

export async function logAssistantTurn(
  { sessionID, messageID, agentName, spirit, operator },
  assistantText,
  paths = {},
) {
  const text = String(assistantText || "").trim();
  if (!text) return;

  const resolvedSpirit = spirit || DEFAULT_SPIRIT;
  const entry = {
    sessionID, messageID,
    role: "assistant",
    spirit: resolvedSpirit,
    operator: operator || "Sol",
    agentName: agentName || DEFAULT_AGENT_NAME,
    text,
  };

  await appendConversationLedger(entry);
  await refreshSpiritWindow(resolvedSpirit);
  await refreshLiveRoomContext(entry, paths);
}

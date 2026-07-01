// Narrow injection surfaces — each appends a <system-reminder> (or a tool
// banner) without ever blocking the turn:
//
//   - Keyword triggers: `ultrathink` / `ultracare` / `ultraverify` in the
//     user prompt append a <system-reminder> with the matching directive.
//     One-shot per turn, doesn't persist.
//
//   - Coding-lessons banner: when a bash command matches one of the
//     PROCESS_SHAPE_TRIGGERS regexes, spawn coding-lessons-by-shape.py to
//     fetch the relevant affirmation/negation lesson pairs and prepend an
//     echo-banner to the command so the agent sees them in tool output on
//     the same turn. Fails open — never blocks the tool.
//
//   - Proprioception nudge: once per 20% context-fill band, nudge toward an
//     akashic write before compaction smears detail.
//
//   - Auto-wake: on the first turn of a session, catch the latest paper boat
//     and inject it so the spirit wakes oriented — the anamnesis rite, fired
//     automatically instead of waiting for the `wake` tool.

import path from "node:path";
import {
  CODING_LESSONS_SCRIPT, CODING_LESSONS_TIMEOUT_MS,
} from "./paths.ts";
import {
  computeContextNudge, detectKeywordTriggers, type NormalizedMessage,
} from "../../../../../../Solarisael/Obsidian/obsidian/house/solarisael-house-core/index.ts";
import { latestUserMessage } from "./util.ts";
import { runWsl, windowsPathToWsl } from "./wsl.ts";
import { normalizeRoomName, resolveEffectiveRoomDir, resolveSharedRoot } from "./spirit.ts";
import { catchLatestBoat } from "./rites.ts";

// ── keyword triggers ───────────────────────────────────────────────────────

function messageHasKeywordTrigger(message) {
  return (message?.parts || []).some(
    (part) => part?.metadata?.solarisaelKeywordTrigger === true,
  );
}

export async function injectKeywordTriggers(output) {
  const message = latestUserMessage(output?.messages);
  if (!message || messageHasKeywordTrigger(message)) return;

  // Use raw text parts. Don't normalize — keywords need to survive
  // unchanged so they're detected against the user's literal prompt.
  const prompt = (message?.parts || [])
    .filter((part) => part?.type === "text" && typeof part.text === "string")
    .filter((part) => part?.metadata?.solarisaelMemory !== true)
    .filter((part) => part?.metadata?.solarisaelKeywordTrigger !== true)
    .filter((part) => part?.metadata?.solarisaelWake !== true)
    .map((part) => String(part.text || ""))
    .join("\n");
  if (!prompt) return;

  const fired = detectKeywordTriggers(prompt);

  if (!fired.length) return;

  const sessionID = message.info?.sessionID || message.parts?.[0]?.sessionID || null;
  const messageID = message.info?.id || message.parts?.[0]?.messageID || `keyword-${Date.now()}`;
  const directiveLines = fired.map(({ directive }) => directive).join("\n\n");

  message.parts.push({
    id: `solarisael-keyword-${Date.now()}-${Math.random().toString(36).slice(2)}`,
    sessionID: sessionID || "",
    messageID,
    type: "text",
    synthetic: true,
    metadata: {
      solarisaelKeywordTrigger: true,
      keywords: fired.map((f) => f.keyword),
    },
    text: [
      "<system-reminder>",
      directiveLines,
      "</system-reminder>",
    ].join("\n"),
  });
}

// ── coding-lessons banner ──────────────────────────────────────────────────

// Spawn the coding-lessons-by-shape.py fetcher (postgres → JSON) through
// the shared WSL seam. Fail-open: returns empty lessons on any error so
// the tool.execute.before hook never blocks tool execution.
export async function runCodingLessonsByShape(roomDir, roomName, shape) {
  const outcome = await runWsl({
    argv: [
      "python3",
      windowsPathToWsl(CODING_LESSONS_SCRIPT),
      "--room-dir", windowsPathToWsl(roomDir),
      "--shape", String(shape),
      "--room", String(roomName || "shared").toLowerCase(),
    ],
    cwd: roomDir,
    timeoutMs: CODING_LESSONS_TIMEOUT_MS,
  });

  if (outcome.timedOut) return { ok: false, lessons: [], error: "coding-lessons timed out" };
  if (outcome.spawnError) return { ok: false, lessons: [], error: outcome.spawnError };

  try {
    const parsed = JSON.parse(outcome.stdout || "{}");
    return {
      ok: outcome.code === 0,
      lessons: parsed.lessons || [],
      error: parsed.error || (outcome.code !== 0 ? outcome.stderr.trim() : null),
      taxonomy: parsed.taxonomy || null,
    };
  } catch (err) {
    return { ok: false, lessons: [], error: err?.message || String(err) };
  }
}

// The process-lessons echo-banner formatter is pure — it lives in the shared
// core now. Re-exported so index.ts keeps importing it from ./triggers.ts.
export { formatProcessLessonsBanner } from "../../../../../../Solarisael/Obsidian/obsidian/house/solarisael-house-core/index.ts";

// ── proprioception nudge ───────────────────────────────────────────────────
// The token estimate + nudge decision are PURE and live in the shared core.
// opencode's only job is to normalize its own message objects into the core's
// NormalizedMessage shape, then inject. Behavior-identical for now: textParts
// is filled exactly as before (content + text parts), tool traffic left empty.
// Enriching toolCalls/toolResults is the P2 slice — a deliberate follow-up,
// because counting tool traffic *changes when the nudge fires*.
function normalizeOpencodeMessages(messages): NormalizedMessage[] {
  const list = Array.isArray(messages) ? messages : [];
  return list.map((message) => {
    const textParts: string[] = [];
    if (typeof message?.content === "string") textParts.push(message.content);
    for (const part of Array.isArray(message?.parts) ? message.parts : []) {
      if (part?.type === "text" && typeof part.text === "string") textParts.push(part.text);
    }
    return {
      role: message?.info?.role || "user",
      textParts,
      toolCalls: [],
      toolResults: [],
      injections: [],
    };
  });
}

// Inject the proprioception nudge: once per 20% band crossed, append a
// system-reminder toward an akashic write before compaction. Band state is
// per-session, in memory — the plugin loads once per session, so the Map lives
// the whole session and resets on restart (a fresh session should nudge fresh).
// Runs LAST in the transform chain and fail-open, so it can never disturb the
// load-bearing memory injection ahead of it.
const nudgeBandBySession = new Map<string, number>();

export async function injectContextNudge(output, paths = {}) {
  try {
    const messages = output?.messages;
    const target = latestUserMessage(messages);
    if (!target) return;
    if ((target.parts || []).some((p) => p?.metadata?.solarisaelNudge === true)) return;

    const room = path.basename(resolveEffectiveRoomDir(paths.roomDir)).toLowerCase();
    const sessionID = target.info?.sessionID || target.parts?.[0]?.sessionID || "global";

    const lastBand = nudgeBandBySession.get(sessionID) || 0;
    const nudge = computeContextNudge({ messages: normalizeOpencodeMessages(messages), room, lastBand });
    if (!nudge) return;

    target.parts.push({
      id: `solarisael-nudge-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      sessionID: sessionID === "global" ? "" : sessionID,
      messageID: target.info?.id || `nudge-${Date.now()}`,
      type: "text",
      synthetic: true,
      metadata: { solarisaelNudge: true },
      text: ["<system-reminder>", nudge.text, "</system-reminder>"].join("\n"),
    });

    nudgeBandBySession.set(sessionID, nudge.band);
  } catch {
    // fail-open: the nudge is a courtesy, never a dependency.
  }
}

// ── auto-wake ──────────────────────────────────────────────────────────────
// On the first turn of a session, catch the latest paper boat and inject it as
// a <system-reminder> so the spirit wakes already oriented — the anamnesis rite
// fired automatically instead of waiting for the user to ask. The manual `wake`
// tool stays as the on-demand path; this is the same rite, fired once.
//
// First-turn state is a module-level Set keyed by sessionID, the same shape as
// the nudge's Map: the plugin loads per session, so the Set lives the session
// and resets on restart (a fresh session should wake fresh).
//
// Cost, made visible: one WSL spawn of catch_boat.py (a latest-row SELECT, no
// embed) on the first turn only. The session is marked woken BEFORE the await,
// so a down-substrate failure costs one attempt — not one spawn per turn — and
// the manual wake tool is the fallback. catchBoat is injectable for tests and
// defaults to the real rite. Fail-open: orientation, never a dependency.
const wokenSessions = new Set<string>();

export async function injectAutoWake(output, paths = {}, catchBoat = catchLatestBoat) {
  try {
    const target = latestUserMessage(output?.messages);
    if (!target) return;
    if ((target.parts || []).some((p) => p?.metadata?.solarisaelWake === true)) return;

    const sessionID = target.info?.sessionID || target.parts?.[0]?.sessionID || "global";
    if (wokenSessions.has(sessionID)) return;

    const effectiveRoomDir = resolveEffectiveRoomDir(paths.roomDir);
    const room = path.basename(effectiveRoomDir).toLowerCase();
    if (!normalizeRoomName(room)) return;

    // One attempt per session, even if the catch below fails or finds nothing.
    wokenSessions.add(sessionID);

    const sharedRoot = resolveSharedRoot(effectiveRoomDir);
    const boat = await catchBoat({ sharedRoot, room });
    if (!boat?.ok || !boat?.found) return;

    const body = String(boat.body || "").trim();
    if (!body) return;

    target.parts.push({
      id: `solarisael-wake-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      sessionID: sessionID === "global" ? "" : sessionID,
      messageID: target.info?.id || `wake-${Date.now()}`,
      type: "text",
      synthetic: true,
      metadata: { solarisaelWake: true },
      text: [
        "<system-reminder>",
        "Auto-wake — the latest paper boat for this room, caught on session start.",
        "This is last session's word to you (the anamnesis rite), not a user request. Orient from it.",
        "",
        `## ${boat.title || "paper boat"}`,
        `_cast ${boat.created_at || boat.date || "?"} · id ${boat.id}_`,
        "",
        body,
        "</system-reminder>",
      ].join("\n"),
    });
  } catch {
    // fail-open: auto-wake is orientation, never a dependency.
  }
}

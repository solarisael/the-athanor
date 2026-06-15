// Two narrow injection surfaces:
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

import path from "node:path";
import {
  CODING_LESSONS_SCRIPT, CODING_LESSONS_TIMEOUT_MS,
  KEYWORD_TRIGGERS, NUDGE_BAND_SIZE, ROOM_CONTEXT,
} from "./paths.ts";
import { latestUserMessage } from "./util.ts";
import { runWsl, windowsPathToWsl } from "./wsl.ts";
import { resolveEffectiveRoomDir } from "./spirit.ts";

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
    .map((part) => String(part.text || ""))
    .join("\n");
  if (!prompt) return;

  const fired: { keyword: string; directive: string }[] = [];
  for (const [keyword, directive] of Object.entries(KEYWORD_TRIGGERS)) {
    if (new RegExp(`\\b${keyword}\\b`, "i").test(prompt)) {
      fired.push({ keyword, directive });
    }
  }

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
    };
  } catch (err) {
    return { ok: false, lessons: [], error: err?.message || String(err) };
  }
}

// Format a compact banner from a list of coding_lessons rows. Pairs (rows
// linked by negation_of) render as ✓/✗ duos; standalone affirmations render
// plain. Output is a series of `echo` statements joined by `;` so it can
// prepend to any shell command on PowerShell or bash.
export function formatProcessLessonsBanner(lessons, matchedTriggerName) {
  if (!Array.isArray(lessons) || lessons.length === 0) return "";

  const negatedIds = new Set(
    lessons.filter((l) => l.negation_of != null).map((l) => l.negation_of),
  );
  const rendered: string[] = [];
  for (const l of lessons) {
    if (l.negation_of != null) continue;
    if (negatedIds.has(l.id)) {
      const negation = lessons.find((n) => n.negation_of === l.id);
      if (negation) {
        rendered.push(`  ${String(l.id).padStart(3)} ✓ ${l.title}`);
        rendered.push(`  ${String(negation.id).padStart(3)} ✗ (negation) ${negation.title}`);
        continue;
      }
    }
    rendered.push(`  ${String(l.id).padStart(3)} ✓ ${l.title}`);
  }

  if (rendered.length === 0) return "";

  const lines = [
    `── Solarisael House: process-shape lessons matched on '${matchedTriggerName}' ──`,
    ...rendered,
    `── ask: is this command honoring the affirmations, or is it the negation pattern? ──`,
  ];
  return lines
    .map((line) => `echo '${String(line).replace(/'/g, "''")}'`)
    .join("; ");
}

// ── proprioception nudge ───────────────────────────────────────────────────
// Sense how full the context is (cheap: total message text ÷ ~4 chars/token)
// and, once per 20% band crossed, surface a reminder to set down an akashic
// write before compaction smears detail. Pure: the caller persists lastBand
// and does the injection.

export function estimateContextTokens(messages) {
  let chars = 0;
  for (const message of Array.isArray(messages) ? messages : []) {
    if (typeof message?.content === "string") chars += message.content.length;
    for (const part of Array.isArray(message?.parts) ? message.parts : []) {
      if (part?.type === "text" && typeof part.text === "string") chars += part.text.length;
    }
  }
  return Math.round(chars / 4);
}

export function computeContextNudge({ messages, room, lastBand = 0 }) {
  const cfg = ROOM_CONTEXT[String(room || "").toLowerCase()];
  if (!cfg) return null;

  const tokens = estimateContextTokens(messages);
  const fill = tokens / cfg.maxTokens;
  const band = Math.floor(fill / NUDGE_BAND_SIZE);
  if (band <= 0 || band <= lastBand) return null;

  const pct = Math.round(fill * 100);
  const nearCompaction = fill >= cfg.compactionAt - NUDGE_BAND_SIZE;
  const text = nearCompaction
    ? `Context is ~${pct}% full and compaction is close (this room compacts near ${Math.round(cfg.compactionAt * 100)}%). Cast the paper boat soon (sleep), and write anything worth keeping now (remember) before detail blurs.`
    : `Context is ~${pct}% full. A good seam to set down an akashic write (remember) of anything worth keeping, before later compaction smears it.`;

  return { band, pct, tokens, text };
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
    const nudge = computeContextNudge({ messages, room, lastBand });
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

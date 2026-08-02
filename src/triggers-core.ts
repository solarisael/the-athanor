// Solarisael House — shared trigger core (PURE).
//
// Silhouette: harness-agnostic decision logic. No IO, no platform message
// objects, no injection. Each harness normalizes its own transcript into
// NormalizedMessage[] and calls these; the return values are plain data the
// harness then injects its own way. This is the single source the
// duplication lesson demands — a fix here lands once, for every harness.

import {
  DEFAULT_ROOM_CONTEXT,
  KEYWORD_TRIGGERS, NUDGE_BAND_SIZE, NUDGE_EVERY_TOKENS,
  PROCESS_SHAPE_TRIGGERS, ROOM_CONTEXT,
} from "./constants.ts";
import { RESERVED_ROOM_KEYS, ROOM_KEY_PATTERN } from "./paths.ts";

// The boundary between harness and core. Every countable surface of a turn
// is a field here so the core sees the WHOLE context, not just visible text.
// Each harness fills what it has; empty arrays are fine.
export type NormalizedMessage = {
  role: "user" | "assistant" | "system" | string;
  textParts: string[];     // visible user/assistant text
  toolCalls: string[];     // serialized tool inputs (name + args)
  toolResults: string[];   // tool output text
  injections: string[];    // synthetic system-reminders already added this turn
};

export type NudgeDecision = {
  band: number;
  pct: number;
  tokens: number;
  text: string;
};

// Sense context fill. Counts EVERY surface (text + tool calls + tool results
// + injections) at ~4 chars/token. The old estimate kept only user/assistant
// text and dropped tool traffic, undercounting a tool-heavy turn badly — the
// normalized shape fixes that by construction. (P2, 2026-06-29)
export function estimateContextTokens(messages: NormalizedMessage[]): number {
  let chars = 0;
  for (const m of Array.isArray(messages) ? messages : []) {
    for (const bucket of [m.textParts, m.toolCalls, m.toolResults, m.injections]) {
      for (const s of Array.isArray(bucket) ? bucket : []) {
        if (typeof s === "string") chars += s.length;
      }
    }
  }
  return Math.round(chars / 4);
}

// Decide whether to nudge toward an akashic write. Pure: the caller owns
// lastBand state and does the injection. Bands off an ABSOLUTE token cadence
// (NUDGE_EVERY_TOKENS), not a fraction of maxTokens — a 1M-token room never
// crossed the first 20%-of-budget band, so the nudge never fired. (2026-06-29)
export function computeContextNudge(
  { messages, room, lastBand = 0 }:
  { messages: NormalizedMessage[]; room: string; lastBand?: number },
): NudgeDecision | null {
  const roomName = String(room || "").toLowerCase();
  if (!ROOM_KEY_PATTERN.test(roomName) || RESERVED_ROOM_KEYS.includes(roomName)) return null;
  const cfg = ROOM_CONTEXT[roomName] || DEFAULT_ROOM_CONTEXT;

  const tokens = estimateContextTokens(messages);
  const band = Math.floor(tokens / NUDGE_EVERY_TOKENS);
  if (band <= 0 || band <= lastBand) return null;

  const fill = tokens / cfg.maxTokens;
  const pct = Math.round(fill * 100);
  const nearCompaction = fill >= cfg.compactionAt - NUDGE_BAND_SIZE;
  const text = nearCompaction
    ? `Context is ~${pct}% full and compaction is close (this room compacts near ${Math.round(cfg.compactionAt * 100)}%). Cast the paper boat soon (sleep), and write anything worth keeping now (remember) before detail blurs.`
    : `Context is ~${pct}% full. A good seam to set down an akashic write (remember) of anything worth keeping, before later compaction smears it.`;

  return { band, pct, tokens, text };
}

// Which keyword directives fired in a prompt. Pure: the harness supplies the
// already-joined raw prompt text and injects the returned directives.
export function detectKeywordTriggers(
  prompt: string,
): { keyword: string; directive: string }[] {
  const fired: { keyword: string; directive: string }[] = [];
  if (!prompt) return fired;
  for (const [keyword, directive] of Object.entries(KEYWORD_TRIGGERS)) {
    if (new RegExp(`\\b${keyword}\\b`, "i").test(prompt)) fired.push({ keyword, directive });
  }
  return fired;
}

// Which process-shape trigger (if any) a command matches. Pure: the harness
// supplies the command string and uses the name to fetch + banner lessons.
export function matchProcessShape(command: string): string | null {
  const text = String(command || "");
  for (const { name, pattern } of PROCESS_SHAPE_TRIGGERS) {
    if (pattern.test(text)) return name;
  }
  return null;
}

// Render a compact echo-banner from typed lessons rows. Pairs (linked by
// negationOf) render as ✓/✗ duos; standalone affirmations render plain.
// Output is `echo` statements joined by `;` so it prepends to any command on
// PowerShell or bash. Pure string work.
export function formatProcessLessonsBanner(
  lessons: any[],
  matchedTriggerName: string,
): string {
  if (!Array.isArray(lessons) || lessons.length === 0) return "";

  const negatedIds = new Set(
    lessons.filter((lesson) => lesson.negationOf != null).map((lesson) => lesson.negationOf),
  );
  const rendered: string[] = [];
  for (const lesson of lessons) {
    if (lesson.negationOf != null) continue;
    if (negatedIds.has(lesson.id)) {
      const negation = lessons.find((candidate) => candidate.negationOf === lesson.id);
      if (negation) {
        rendered.push(`  ${String(lesson.id).padStart(3)} ✓ ${lesson.title}`);
        rendered.push(`  ${String(negation.id).padStart(3)} ✗ (negation) ${negation.title}`);
        continue;
      }
    }
    rendered.push(`  ${String(lesson.id).padStart(3)} ✓ ${lesson.title}`);
  }
  if (rendered.length === 0) return "";

  const lines = [
    `── Solarisael House: process-shape lessons matched on '${matchedTriggerName}' ──`,
    ...rendered,
    `── ask: is this command honoring the affirmations, or is it the negation pattern? ──`,
  ];
  return lines.map((line) => `echo '${String(line).replace(/'/g, "''")}'`).join("; ");
}

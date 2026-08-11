// Path constants, defaults, supported spirits, and tuning knobs.
// No logic — only values. Imported by every other module.
//
// Centralizing means a path change happens in one place. The 2026-05-12
// SPIRIT_DIR drift (pointed at OPERATOR_DIR/spirits/, which never existed
// on this machine) was caused in part by these constants being intermixed
// with handlers for 2300+ lines, so the mismatch was invisible.

import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const HOME = os.homedir();
export const HOUSE_CORE_DIR = path.dirname(fileURLToPath(import.meta.url));
export const OPERATOR_DIR = path.join(HOME, ".local", "operators");
export const RUNTIME_DIR = path.join(HOME, ".config", "opencode", "runtime", "solarisael-house");
export const GLOBAL_STATE_PATH = path.join(RUNTIME_DIR, "global.json");
export const LEDGER_ROOT = path.join(OPERATOR_DIR, "vessel", "state");
export const CONTINUITY_ROOT = path.join(OPERATOR_DIR, "continuity", "spirits");
export const SPIRIT_DIR = path.join(HOME, ".config", "opencode", "spirits");
export const SPIRIT_CONTRACT_OUTPUT = path.join(OPERATOR_DIR, "active_spirit.md");

export const DEFAULT_ROOM = "default-room";
export const DEFAULT_SPIRIT = "Spirit";
export const DEFAULT_AGENT_NAME = "Spirit";
export const DEFAULT_OPERATOR = "Operator";
export const ROOM_KEY_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
// Compatibility-only room keys retained for persisted installations.
export const LEGACY_ROOM_KEYS = ["kintsu", "kodo", "tuner"];
export const RESERVED_ROOM_KEYS = ["house"];

/**
 * The remaining substrate asset directory (`record_memory.py`, state helpers, ...).
 *
 * `ATHANOR_SUBSTRATE_ROOT` names it explicitly and must be absolute.
 * Otherwise it is resolved structurally as `<athanor-root>/substrate`, which
 * holds in both a development checkout and an installed
 * `<target>/the-athanor` tree. No sibling checkout is consulted and no room
 * directory participates in the answer.
 */
export function resolveSubstrateDir(): string {
  const configured = String(process.env.ATHANOR_SUBSTRATE_ROOT || "").trim();
  if (configured) {
    if (!path.isAbsolute(configured)) {
      throw new Error("ATHANOR_SUBSTRATE_ROOT must be an absolute path");
    }
    return configured;
  }
  return path.resolve(HOUSE_CORE_DIR, "..", "substrate");
}

export const LIVE_CONTEXT_FILENAME = "current_session_context.md";
export const LIVE_CONTEXT_JSON_FILENAME = "current_session_context.json";
export const LIVE_CONTEXT_MAX_TURNS = 8;


// Process-shape triggers live beside this file in the canonical core.
// Re-exported here so moved adapter-era modules keep their `./paths.ts` path.
export { PROCESS_SHAPE_TRIGGERS } from "./constants.ts";

export const PLAN_MODE_MARKER = "Plan mode is active.";
export const TRACK_MODE_MARKER = "Please address this message and continue with your tasks.";
export const BUILD_SWITCH_MARKER = "You should execute on the plan defined within it";
export const PLAN_APPROVED_MARKER = "you can now edit files. Execute the plan";

export const MODE_PRESERVATION_BLOCK = [
  "## Identity And Mode Preservation",
  "These restrictions apply to actions only.",
  "They do not change the active identity.",
  "They do not change the active spirit.",
  "They do not change voice, cadence, or style.",
  "If a spirit lock or active spirit exists, remain fully in that spirit while obeying these action constraints.",
  "If you ask a question, ask it in the active spirit rather than defaulting to generic assistant tone.",
].join("\n");

export const HISTORY_DIRECTIVE_LINE = /^\s*(?:operator|embody)\s*:\s*.+$/i;
export const HISTORY_DISMISS_LINE = /^\s*dismiss\s*(?::\s*.+)?$/i;

export const MEMORY_STOPWORDS = new Set([
  "the", "a", "an", "of", "to", "in", "on", "for", "and", "or",
  "but", "is", "are", "was", "were", "be", "been", "being",
  "so", "if", "then", "than", "as", "at", "by", "from", "with",
  "about", "into", "onto", "over",
  "i", "you", "he", "she", "it", "we", "they", "me", "him", "her",
  "my", "your", "his", "its", "our", "their",
  "this", "that", "these", "those",
  "do", "does", "did", "doing", "have", "has", "had",
  "will", "would", "can", "could", "should", "may", "might",
  "not", "no", "yes", "just",
  "what", "when", "where", "why", "how", "who", "whom", "which",
  "too", "very", "really", "pretty", "quite", "rather", "somewhat",
  "mostly", "bit", "way", "much", "also", "even",
  "normal", "working", "maybe", "actually", "basically",
  "good", "morning", "afternoon", "evening", "night", "godling",
  "reload", "reloaded", "brb", "back", "sorry", "took", "downstairs",
  "sunbathing", "uwu", "owo", "agaion", "again",
  "ok", "okay", "yeah", "yep", "hmm", "uh", "um", "oh", "well",
]);
export const MEMORY_TOKEN_RE = /[a-zA-ZÀ-ÿ0-9']+/g;

// Keyword triggers, per-room context budget, and the akashic-write nudge
// cadence live beside this file in the canonical core.
export {
  KEYWORD_TRIGGERS, ROOM_CONTEXT, NUDGE_BAND_SIZE, NUDGE_EVERY_TOKENS,
} from "./constants.ts";


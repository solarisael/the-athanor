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
export const PLUGIN_DIR = path.dirname(fileURLToPath(import.meta.url));
export const OPERATOR_DIR = path.join(HOME, ".local", "operators");
export const RUNTIME_DIR = path.join(HOME, ".config", "opencode", "runtime", "solarisael-house");
export const GLOBAL_STATE_PATH = path.join(RUNTIME_DIR, "global.json");
export const LEDGER_ROOT = path.join(OPERATOR_DIR, "vessel", "state");
export const CONTINUITY_ROOT = path.join(OPERATOR_DIR, "continuity", "spirits");
export const SPIRIT_DIR = path.join(HOME, ".config", "opencode", "spirits");
export const SPIRIT_CONTRACT_OUTPUT = path.join(OPERATOR_DIR, "active_spirit.md");

export const DEFAULT_SPIRIT = "Kintsu";
export const DEFAULT_AGENT_NAME = "Kintsu";
export const SUPPORTED_SPIRITS = ["Kintsu", "Kodo", "Tuner"];

export const LIVE_CONTEXT_FILENAME = "current_session_context.md";
export const LIVE_CONTEXT_JSON_FILENAME = "current_session_context.json";
export const LIVE_CONTEXT_MAX_TURNS = 8;

export const MEMORY_INDEX_FILENAME = path.join("memory", "index.json");
export const MEMORY_IMPORTANT_INDEX_FILENAME = path.join("memory", "important_index.json");
export const MEMORY_STATE_FILENAME = path.join("memory", "loaded_state.json");
export const MEMORY_PREFETCH_FILENAME = path.join("memory", "prefetch.json");
export const MEMORY_DEBUG_LOG_FILENAME = path.join("memory", "retrieval_debug.log");
export const HOUSE_MEMORY_DIRNAME = "house";

export const MEMORY_MAX_SYNC_INJECTIONS = 3;
export const MEMORY_MAX_PREFETCH_QUEUE = 5;
export const MEMORY_MAX_IMPORTANT_MATCHES = 8;
export const MEMORY_MAX_INJECTION_CHARS = 8000;
export const MEMORY_MAX_EXCERPT_CHARS = 3500;
export const MEMORY_MIN_SCORE_TO_INJECT = 3;
export const MEMORY_MIN_PROMPT_LEN = 3;
export const MEMORY_CONCEPT_WEIGHT = 3.0;
export const MEMORY_CONTEXT_WEIGHT = 1.0;
export const MEMORY_FILE_ONE_LINE_WEIGHT = 0.5;
export const MEMORY_SESSION_REPEAT_PENALTY_BASE = 0.75;
// Audit ticket #2: touch-based recency decay half-life. Weight applied as
// `exp(-ln2 * age_days / half_life)` where age is `now - last_touched_at`.
// With daily-plus contact (Sol's actual cadence), a 7-day half-life puts
// yesterday at ~0.91 weight, a week ago at 0.50, two weeks at 0.25, a
// month at 0.05. Canon-touching threads exempt entirely (handled in
// computeThreadRecencyPenalty). Dial 5↔10 once we feel how it plays.
export const MEMORY_RECENCY_HALF_LIFE_DAYS = 7;
export const MEMORY_DEBUG_TOP_CANDIDATES = 12;
export const MEMORY_POSTGRES_SOURCE_SCRIPT = path.join(PLUGIN_DIR, "postgres-memory-source.py");
export const MEMORY_POSTGRES_TIMEOUT_MS = 8000;
export const MEMORY_SEMANTIC_TOP_K = 5;
export const MEMORY_SEMANTIC_MIN_SIM = 0.40;
// Audit ticket #4: when a semantic chunk on file F has cosine >= this
// threshold, the lexical thread excerpt for the SAME file is redundant
// (the semantic chunk view is more precise about what part is relevant).
// Drop the lexical excerpts pointing at that file from the merge.
// Lexical keeps the win on long-tail proper-noun / technical-identifier
// matches that semantic doesn't find at high confidence.
export const MEMORY_LEXICAL_DEMOTION_SIM_THRESHOLD = 0.60;
// Content (pg_trgm GIN on memory_chunks.body) — added 2026-05-19 zeal pass.
// Catches proper-noun / exact-string matches semantic cosine misses.
// word_similarity threshold 0.30 means "noticeable substring overlap"; the
// SQL filter already drops anything below this. The demotion threshold
// for lexical-demotion is higher: a content hit at word_similarity >= 0.70
// is strong enough to redundantize the lexical thread excerpt on same file.
export const MEMORY_CONTENT_TOP_K = 5;
export const MEMORY_CONTENT_MIN_SIM = 0.30;
export const MEMORY_CONTENT_DEMOTION_SIM_THRESHOLD = 0.70;

export const CODING_LESSONS_SCRIPT = path.join(PLUGIN_DIR, "coding-lessons-by-shape.py");
export const CODING_LESSONS_TIMEOUT_MS = 2000;

// PROCESS_SHAPE_TRIGGERS now lives in the shared house core (single source
// for every harness — opencode, OMP, future). Re-exported here so existing
// importers keep their `./paths.ts` path. The 6-deep relative climb is the
// honest cost of the core living in the vault, one tree over.
export { PROCESS_SHAPE_TRIGGERS } from "../../../../../../Solarisael/Obsidian/obsidian/house/solarisael-house-core/constants.ts";

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
// cadence now live in the shared house core (single source for every
// harness). Re-exported so existing importers keep their `./paths.ts` path.
export {
  KEYWORD_TRIGGERS, ROOM_CONTEXT, NUDGE_BAND_SIZE, NUDGE_EVERY_TOKENS,
} from "../../../../../../Solarisael/Obsidian/obsidian/house/solarisael-house-core/constants.ts";


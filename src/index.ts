export {
  KEYWORD_TRIGGERS,
  PROCESS_SHAPE_TRIGGERS,
  ROOM_CONTEXT,
  NUDGE_BAND_SIZE,
  NUDGE_EVERY_TOKENS,
} from "./constants.ts";

export {
  estimateContextTokens,
  computeContextNudge,
  detectKeywordTriggers,
  matchProcessShape,
  formatProcessLessonsBanner,
} from "./triggers-core.ts";

export {
  CODING_LESSONS_SCRIPT,
  MEMORY_POSTGRES_SOURCE_SCRIPT,
  POSTGRES_MEMORY_SOURCE_SCRIPT,
} from "./paths.ts";

export type { NormalizedMessage, NudgeDecision } from "./triggers-core.ts";

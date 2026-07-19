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
  classifyRetrievalQuery,
  parseRetrievalQuery,
  shouldAutoRecall,
} from "./query-routing.ts";

export {
  ADVISOR_REVIEW_CHANNEL,
  CONTEXT_MODES,
  RISK_LEVELS,
  WORKER_LANES,
  buildDispatchReceipt,
  getWorkerLane,
  listWorkerLanes,
} from "./routing.ts";

export type {
  ContextHint,
  ContextMode,
  DispatchReceipt,
  DispatchRequest,
  RiskLevel,
  WorkerLane,
  WorkerLaneName,
} from "./routing.ts";

export {
  CODING_LESSONS_SCRIPT,
  MEMORY_POSTGRES_SOURCE_SCRIPT,
  POSTGRES_MEMORY_SOURCE_SCRIPT,
} from "./paths.ts";

export { runAnamnesisQuery } from "./memory.ts";

export type { NormalizedMessage, NudgeDecision } from "./triggers-core.ts";

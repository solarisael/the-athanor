export const CORE_API_VERSION = 1;

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
  FAMILIAR_SPELLBOOK_FILENAMES,
  buildFamiliarDispatchReceipt,
  getFamiliar,
  listFamiliars,
  parseFamiliarSpellbook,
} from "./familiars.ts";

export type {
  FamiliarDefinition,
  FamiliarDispatchReceipt,
  FamiliarDispatchRequest,
  FamiliarSpellbook,
  FamiliarSpellbookResult,
} from "./familiars.ts";

export {
  LESSONS_SCRIPT,
  MEMORY_POSTGRES_SOURCE_SCRIPT,
  POSTGRES_MEMORY_SOURCE_SCRIPT,
} from "./paths.ts";

export {
  runAnamnesisQuery,
  runRecallQuery,
  runVaultRecallQuery,
} from "./memory.ts";
export { clearVaultSearchCache, searchVault } from "./vault-search.ts";

export {
  logAssistantTurn,
  logUserTurn,
} from "./ledger.ts";

export type { NormalizedMessage, NudgeDecision } from "./triggers-core.ts";

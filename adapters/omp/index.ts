export const ADAPTER_API_VERSION = 1;

// The Athanor — OMP adapter entrypoint.
//
// This file stays where OMP config expects it. The implementation is split into
// shaped modules under ./house-proof/ so this door only wires hooks.

import {
  kittenLineageDisabled,
  kittenLifecycleJoinKey,
  noteKittenLifecycle,
  noteKittenLineageWrite,
  noteKittenProgress,
  stampAttemptId,
  type KittenQuestProgress,
} from "./kitten-lineage.ts";
import {
  normalizeQuestMemories,
  settleQuestLifecycle,
  type QuestMemory,
} from "./house-proof/lineage.ts";
import {
  hostSessionIdentity,
  type HostBinding,
} from "./house-proof/host.ts";
import {
  adoptTopLevelSession,
  registerTopLevelSession,
  retireTopLevelSession,
  topLevelSession,
} from "./house-proof/top-level-session-fence.ts";
import { responseDigest, settlePresence } from "./house-proof/presence.ts";
import { logConversationWindow, type ConversationCapture } from "./house-proof/conversation-log.ts";
import { flushGigaTurns, ingestGigaLoggedTurnsDetached, isSubagentSessionContext, setGigaEnablement } from "./giga.ts";
import { automaticRecallAllowed, registerSubagentRecallProtocol } from "./house-proof/recall.ts";
import { writeRustMemory } from "./house-proof/tools.ts";
import {
  noteHallwayKnockTurnEnd,
  noteHallwayKnockTurnStart,
  startHallwayKnockDoorman,
  stopHallwayKnockDoorman,
} from "./house-proof/knock.ts";
import {
  noteChatMessageStart,
  noteChatMessageEnd,
  noteChatMessageUpdate,
  noteChatToolEnd,
  noteChatToolStart,
  noteChatTurnEnd,
  startChatDoorman,
  stopChatDoorman,
} from "./house-proof/chat.ts";
import {
  applyPromptDirectives,
  roomContext,
} from "./house-proof/room.ts";
import { conversationText, messageText } from "./house-proof/text.ts";
import { anchorTurnAdditions, currentTurnOrigin, turnKeysByMessage } from "./house-proof/turn-origin.ts";
import { registerSolarisaelTools } from "./house-proof/tools.ts";
import {
  blockLessonRefusal,
  capturedAgentSession,
  installLessonTtsrBridge,
  recordNativeFires,
  syncLessonTtsr,
  contextLessonManagerAvailable,
} from "./house-proof/lesson-ttsr.ts";
import { boatLineTokens, installBoatDoor } from "./house-proof/boat-door.ts";
import { classWord, modeDoor, readModeDoorLines, MODE_DOOR_FILE } from "./house-proof/class-word.ts";
import {
  CONTEXT_BLOCK_TYPES,
  contextBlockKind,
  planContextLessons,
  prepareContext,
  readLegacyContextProposal,
  type ContextBlockKind,
} from "./house-proof/context.ts";
import { installSemanticJudgmentShadow } from "./house-proof/semantic-judgment.ts";
import { resolveJudgmentCredential } from "./house-proof/judgment-host.ts";
export {
  scoreToolCallShadow,
  scoreCompletedDraftShadow,
  selectSemanticRoute,
  SEMANTIC_SCORE_SCHEMA_VERSION,
  SEMANTIC_SHADOW_THRESHOLDS,
  SEMANTIC_THRESHOLD_POLICY,
  type SemanticJudge,
  type SemanticReceipt,
  type SemanticCoverage,
  type SemanticDisposition,
} from "./house-proof/semantic-judgment.ts";
import { AUTOMATIC_CONTEXT_IO_TIMEOUT_MS } from "./house-proof/constants.ts";
import { showHouseContextFeedback } from "./house-proof/feedback.ts";
import {
  activeProjectFromEvidence,
  RecallPolicyHostClient,
  toolEvidenceRevision,
  TOOL_EVIDENCE_EPOCH,
  isMutateTool,
  markToolEvidence,
  mutateToolPaths,
} from "./house-proof/recall-policy.ts";
import {
  closeInsulaWriter,
  endInsulaSpan,
  insulaErrorClass,
  insulaToolOperation,
  noteInsulaProviderRequestId,
  recordInsulaPoint,
  startInsulaSpan,
  type InsulaOutcome,
  type InsulaSpan,
} from "./house-proof/insula.ts";
import { showInsulaCockpit } from "./house-proof/vitals.ts";


type AutomaticContextBudgetResult<T> =
  | { status: "settled"; value: T }
  | { status: "failed"; error: unknown }
  | { status: "timeout" };
type AutomaticContextWork<T> =
  | Promise<T>
  | ((signal: AbortSignal, deadline: number) => Promise<T>);

export async function settleAutomaticContextWithinBudget<T>(
  work: AutomaticContextWork<T>,
  timeoutMs = AUTOMATIC_CONTEXT_IO_TIMEOUT_MS,
): Promise<AutomaticContextBudgetResult<T>> {
  const controller = new AbortController();
  const deadline = Date.now() + timeoutMs;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let timedOut = false;
  const timeout = new Promise<AutomaticContextBudgetResult<T>>((resolve) => {
    timer = setTimeout(() => {
      timedOut = true;
      controller.abort();
      resolve({ status: "timeout" });
    }, timeoutMs);
  });
  let started: Promise<T>;
  try {
    started = typeof work === "function"
      ? work(controller.signal, deadline)
      : work;
  } catch (error) {
    if (timer) clearTimeout(timer);
    return { status: "failed", error };
  }
  const observed = Promise.resolve(started).then<AutomaticContextBudgetResult<T>>(
    (value) => ({ status: "settled", value }),
    (error) => ({ status: "failed", error }),
  );
  const result = await Promise.race([observed, timeout]);
  if (timer) clearTimeout(timer);
  if (timedOut && result.status !== "timeout") return { status: "timeout" };
  return result;
}

const modelDefaultsApplied = new Set();
const KITTEN_QUEST_WRITE_LIMIT = 1_024;
const recordedKittenQuests = new Set<string>();
const kittenQuestWrites = new Map<string, Promise<boolean>>();
const uncertainKittenQuests = new Map<string, { binding: HostBinding; receipt: Readonly<Record<string, unknown>> }>();
const kittenQuestProgress = new Map<string, KittenQuestProgress>();
const kittenRoomsByToolCallId = new Map<string, string>();
const kittenRoomsByAgentId = new Map<string, string>();
const kittenBindingsByToolCallId = new Map<string, HostBinding>();
const pendingPresenceContracts = new Map<
  string,
  { contractId: string; directiveIds: string[]; nonemptyGuardId: string | null }
>();
// Insula correlation. The main turn lifecycle opens one provider request per
// room and session; side-stream provider hooks carry no turn event and never
// enter this map. Tool spans hang from the request that asked for them. Both
// maps are bounded: a lost correlation costs a parent link and nothing else.
type InsulaSettlement = {
  outcomeClass: InsulaOutcome;
  errorClass: string | null;
  durationUs: number | null;
  tokensIn: number;
  tokensOut: number;
  // Cache buckets ride bytesIn/bytesOut on the usage point: tokensIn stays the
  // full input so older vitals rows keep comparing, and price needs the split.
  cacheRead: number;
  cacheWrite: number;
};

const insulaRequestSpans = new Map<string, InsulaSpan>();
const insulaToolSpans = new Map<string, InsulaSpan>();
// The last settled request per room+session, kept so the next turn's verdict
// can hang from the request it judges and join that request's usage point.
export type SettledInsulaRequest = Pick<InsulaSpan, "room" | "traceId" | "spanId" | "providerRequestId">;
const lastSettledInsulaRequests = new Map<string, SettledInsulaRequest>();

const INSULA_STOP_REASONS: Record<string, { outcomeClass: InsulaOutcome; errorClass: string | null }> = {
  stop: { outcomeClass: "ok", errorClass: null },
  toolUse: { outcomeClass: "ok", errorClass: null },
  length: { outcomeClass: "degraded", errorClass: "max_tokens" },
  error: { outcomeClass: "error", errorClass: "provider_error" },
  aborted: { outcomeClass: "cancelled", errorClass: "provider_aborted" },
};

function insulaToolKey(room: string, session: string, toolCallId: string): string {
  return `${room}\0${session}\0${toolCallId}`;
}

function insulaRequestKey(room: string, session: string): string {
  return `${room}:${session}`;
}

function insulaSessionBinding(ctx: any): { room: string; session: string } {
  const { room, effectiveRoomDir } = roomContext(ctx?.cwd);
  return { room, session: hostSessionIdentity(ctx, effectiveRoomDir) };
}

/**
 * Read the finalized assistant's own normalized usage. Buckets are summed as
 * the provider reported them and never estimated from text, so an unmetered
 * response stays honestly empty instead of becoming a guess.
 */
function insulaAssistantSettlement(message: any): InsulaSettlement | null {
  if (message?.role !== "assistant") return null;
  const mapped = INSULA_STOP_REASONS[String(message?.stopReason ?? "")]
    ?? { outcomeClass: "unknown" as InsulaOutcome, errorClass: "provider_stop_unknown" };
  const usage = message?.usage;
  const bucket = (value: unknown): number =>
    typeof value === "number" && Number.isFinite(value) && value > 0 ? value : 0;
  const durationMs = typeof message?.duration === "number" && Number.isFinite(message.duration)
    ? Math.max(0, message.duration)
    : null;
  return {
    outcomeClass: mapped.outcomeClass,
    errorClass: mapped.errorClass,
    durationUs: durationMs === null ? null : durationMs * 1_000,
    tokensIn: bucket(usage?.input) + bucket(usage?.cacheRead) + bucket(usage?.cacheWrite),
    tokensOut: bucket(usage?.output),
    cacheRead: bucket(usage?.cacheRead),
    cacheWrite: bucket(usage?.cacheWrite),
  };
}

/**
 * Settle the open provider request once and account for its usage. The span is
 * removed before it ends, which is what makes a repeated settlement — a second
 * turn_end, or agent_end after turn_end — emit nothing at all.
 */
function settleInsulaRequest(
  key: string,
  outcomeClass: InsulaOutcome,
  errorClass: string | null,
  usage: InsulaSettlement | null = null,
): void {
  const span = insulaRequestSpans.get(key);
  if (!span) return;
  insulaRequestSpans.delete(key);
  endInsulaSpan(span, outcomeClass, errorClass, usage?.durationUs ?? null);
  lastSettledInsulaRequests.set(key, {
    room: span.room,
    traceId: span.traceId,
    spanId: span.spanId,
    providerRequestId: span.providerRequestId,
  });
  trimOldestMap(lastSettledInsulaRequests, 256);
  const measured = usage && (usage.tokensIn > 0 || usage.tokensOut > 0) ? usage : null;
  // One usage point per settled request, always. Unmetered usage is a degraded
  // point rather than a zero-token ok one, so Vitals can tell "nothing was
  // reported" apart from "zero was reported".
  recordInsulaPoint({
    room: span.room,
    operation: "provider_usage",
    traceId: span.traceId,
    parentSpanId: span.spanId,
    providerRequestId: span.providerRequestId,
    outcomeClass: measured ? "ok" : "degraded",
    errorClass: measured ? null : "usage_unavailable",
    tokensIn: measured?.tokensIn ?? 0,
    tokensOut: measured?.tokensOut ?? 0,
    bytesIn: measured?.cacheRead ?? 0,
    bytesOut: measured?.cacheWrite ?? 0,
    scope: span.providerRequestId ? "provider_request" : "trace_span",
  });
}


function pruneInsulaRoomKeys(roomPrefix: string, current: string): void {
  for (const key of [...insulaRequestSpans.keys()]) {
    if (!key.startsWith(roomPrefix) || key === current) continue;
    settleInsulaRequest(key, "cancelled", "session_switch");
  }
}

function openInsulaRequest(room: string, session: string, replacementError: string): void {
  const key = insulaRequestKey(room, session);
  settleInsulaRequest(key, "cancelled", replacementError);
  const span = startInsulaSpan({ room, operation: "provider_request" });
  if (!span) return;
  insulaRequestSpans.set(key, span);
  trimOldestMap(insulaRequestSpans, 256);
}

function retireInsulaSession(room: string, session: string): void {
  const request = insulaRequestKey(room, session);
  settleInsulaRequest(request, "cancelled", "session_shutdown");
  const toolPrefix = `${room}\0${session}\0`;
  for (const [key, span] of insulaToolSpans) {
    if (!key.startsWith(toolPrefix)) continue;
    endInsulaSpan(span, "cancelled", "session_shutdown");
    insulaToolSpans.delete(key);
  }
}

function retireStaleInsulaSessions(room: string, session: string): void {
  const roomRequestPrefix = `${room}:`;
  const currentRequest = insulaRequestKey(room, session);
  pruneInsulaRoomKeys(roomRequestPrefix, currentRequest);
  const roomToolPrefix = `${room}\0`;
  const currentToolPrefix = `${room}\0${session}\0`;
  for (const [key, span] of insulaToolSpans) {
    if (!key.startsWith(roomToolPrefix) || key.startsWith(currentToolPrefix)) continue;
    endInsulaSpan(span, "cancelled", "session_switch");
    insulaToolSpans.delete(key);
  }
}

function retireAllInsulaSpans(): void {
  for (const key of [...insulaRequestSpans.keys()]) settleInsulaRequest(key, "cancelled", "shutdown");
  for (const span of insulaToolSpans.values()) endInsulaSpan(span, "cancelled", "shutdown");
  insulaToolSpans.clear();
}

function trimOldestMap<K, V>(map: Map<K, V>, limit: number): void {
  if (map.size <= limit) return;
  const oldest = map.keys().next();
  if (!oldest.done) map.delete(oldest.value);
}


// before_agent_start prompt per room+session, held for one turn only.
const activeTurnPrompts = new Map<string, string>();

function activeTurnPromptKey(room: string, session: string): string {
  return `${room}\0${session}`;
}

function trimOldestSet<T>(set: Set<T>, limit: number): void {
  if (set.size <= limit) return;
  const oldest = set.values().next();
  if (!oldest.done) set.delete(oldest.value);
}

function cacheKittenTaskRoom(
  room: string,
  toolCallId: unknown,
  input: unknown,
  binding?: HostBinding,
): void {
  const callId = String(toolCallId ?? "").trim();
  if (callId) {
    kittenRoomsByToolCallId.set(callId, room);
    if (binding) kittenBindingsByToolCallId.set(callId, binding);
    trimOldestMap(kittenRoomsByToolCallId, 1_024);
    trimOldestMap(kittenBindingsByToolCallId, 1_024);
  }
  const tasks = Array.isArray((input as { tasks?: unknown })?.tasks)
    ? (input as { tasks: Array<{ name?: unknown }> }).tasks
    : [];
  for (const task of tasks) {
    const name = String(task?.name ?? "").trim();
    if (name) kittenRoomsByAgentId.set(name, room);
  }
  trimOldestMap(kittenRoomsByAgentId, 1_024);
}

async function recordKittenQuest(binding: HostBinding, record: QuestMemory): Promise<boolean> {
  const key = record.idempotencyKey;
  if (recordedKittenQuests.has(key)) return true;
  const uncertain = uncertainKittenQuests.get(key);
  if (uncertain) {
    console.warn(`[athanor] Lineage ${key} requires reconciliation; no write was retried.`);
    return false;
  }
  const existing = kittenQuestWrites.get(key);
  if (existing) return existing;
  if (uncertainKittenQuests.size + kittenQuestWrites.size >= KITTEN_QUEST_WRITE_LIMIT) {
    console.warn("[athanor] Automatic lineage writes are paused: the unresolved-write admission bound is full.");
    return false;
  }

  const writing = (async () => {
    try {
      const result = await writeRustMemory({
        binding,
        room: binding.room,
        title: record.title,
        body: record.body,
        threads: record.threads,
        continues: [],
        supersedes: [],
        signal: undefined,
      });
      const receipt = result as Record<string, unknown>;
      const execution = (receipt.details as { execution?: { write_outcome?: unknown } } | undefined)?.execution;
      if (receipt.code === "outcome_unknown" || receipt.outcome === "unknown" || execution?.write_outcome === "unknown") {
        uncertainKittenQuests.set(key, { binding, receipt });
        console.warn("[athanor] Lineage outcome is unknown; reconcile before retrying.", { key, binding, receipt });
        noteKittenLineageWrite(false);
        return false;
      }
      if (result?.ok !== true) throw new Error("Native lineage write did not succeed");
      recordedKittenQuests.add(key);
      trimOldestSet(recordedKittenQuests, KITTEN_QUEST_WRITE_LIMIT);
      noteKittenLineageWrite(true);
      return true;
    } catch {
      noteKittenLineageWrite(false);
      return false;
    }
  })();
  kittenQuestWrites.set(key, writing);
  try {
    return await writing;
  } finally {
    kittenQuestWrites.delete(key);
  }
}
const adoptedContextSessions = new Set<string>();



// The loader hands the entry the release it actually loaded, so a session can
// report loadedRelease as derived state instead of re-resolving the pointer.
export default function solarisaelHouseProof(pi, release) {
  pi.setLabel("The Athanor");
  registerSubagentRecallProtocol(pi);
  const lessonTtsrInstallWarning = installLessonTtsrBridge(pi);
  const semanticJudgmentShadow = installSemanticJudgmentShadow(pi);
  const contextReceipts = new Map<string, Record<string, unknown>>();
  // Semantic shadow coverage is local; automatic Recall reranking is separately policy-gated.
  pi.registerCommand?.("jev-shadow", {
    description: "Show local Jev shadow coverage for this session",
    handler: (_args, ctx) => {
      ctx.ui.notify(JSON.stringify(semanticJudgmentShadow.getCoverage(), null, 2), "info");
    },
  });
  pi.registerCommand?.("jev-recall", {
    description: "Show the last observed completed Jev Recall calls for this native room and Host lifetime",
    handler: (_args, ctx) => {
      const { room, effectiveRoomDir } = roomContext(ctx.cwd);
      ctx.ui.notify(JSON.stringify(contextReceipts.get(`${room}:${hostSessionIdentity(ctx, effectiveRoomDir)}:recall`) ?? null, null, 2), "info");
    },
  });
  pi.registerCommand?.("jev-lessons", {
    description: "Show the last observed completed Jev lesson calls for this native room and Host lifetime",
    handler: (_args, ctx) => {
      const { room, effectiveRoomDir } = roomContext(ctx.cwd);
      ctx.ui.notify(JSON.stringify(contextReceipts.get(`${room}:${hostSessionIdentity(ctx, effectiveRoomDir)}:lessons`) ?? null, null, 2), "info");
    },
  });
  pi.registerCommand?.("insula", {
    description: "Show the Host's Insula Vitals for the last 15m, 1h, or 24h",
    handler: (args, ctx) => showInsulaCockpit(args, ctx),
  });
  // Only the room's top-level session casts a boat before its handoff; workers keep OMP's document.
  installBoatDoor(pi, {
    session: (ctx) => {
      ctx.getContextUsage?.();
      return capturedAgentSession(String(ctx.sessionManager?.getSessionId?.() ?? ""));
    },
    isTopLevel: (ctx) => {
      const { room, effectiveRoomDir } = roomContext(ctx.cwd);
      return hostSessionIdentity(ctx, effectiveRoomDir) === topLevelSession(room);
    },
    tokensOverBoatLine: async (ctx) => {
      const usage = ctx.getContextUsage?.();
      const settings = pi.pi?.settings;
      if (!usage?.tokens || !usage.contextWindow || !settings?.get?.("compaction.enabled")) return undefined;
      const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
      const binding = { room, spirit, session: hostSessionIdentity(ctx, effectiveRoomDir) };
      return usage.tokens - await boatLineTokens(binding, usage.contextWindow, {
        thresholdTokens: settings.get("compaction.thresholdTokens"),
        thresholdPercent: settings.get("compaction.thresholdPercent"),
        reserveTokens: settings.get("compaction.reserveTokens"),
      });
    },
    room: (ctx) => {
      const { spirit, effectiveRoomDir } = roomContext(ctx.cwd);
      return { dir: effectiveRoomDir, spirit };
    },
  });
  const showReadyFeedback = async (_event, ctx) => {
    const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
    const binding = {
      room,
      spirit,
      session: hostSessionIdentity(ctx, effectiveRoomDir),
    };
    // Worker sessions share session_start, so first-wins adoption protects the
    // current top-level holder. Only an explicit session switch replaces it.
    adoptTopLevelSession(room, binding.session);
    showHouseContextFeedback(ctx, { room, spirit, activities: [] });
    startHallwayKnockDoorman(pi, ctx, binding);
    startChatDoorman(pi, ctx, binding);
    if (topLevelSession(room) === binding.session && !isSubagentSessionContext(ctx) && process.env.ATHANOR_REPLAY_MODE !== "1") {
      try {
        await setGigaEnablement(binding, {
          gigaEnabled: process.env.ATHANOR_GIGA_ENABLED === "1",
          hippocampusEnabled: process.env.ATHANOR_HIPPOCAMPUS_ENABLED === "1",
          replayMode: false,
        });
      } catch {
        ctx.ui?.notify?.("Athanor GIGA enablement is unavailable.", "warning");
      }
    }
  };
  pi.on("session_start", showReadyFeedback);
  pi.on("session_switch", (event, ctx) => {
    const { room, effectiveRoomDir } = roomContext(ctx.cwd);
    const session = hostSessionIdentity(ctx, effectiveRoomDir);
    activeTurnPrompts.delete(activeTurnPromptKey(room, session));
    retireStaleInsulaSessions(room, session);
    registerTopLevelSession(room, session);
    return showReadyFeedback(event, ctx);
  });
  pi.on("session_shutdown", async (_event, ctx) => {
    const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
    const session = hostSessionIdentity(ctx, effectiveRoomDir);
    activeTurnPrompts.delete(activeTurnPromptKey(room, session));
    retireInsulaSession(room, session);
    retireTopLevelSession(room, session);
    await stopHallwayKnockDoorman({ room, spirit, session });
    stopChatDoorman({ room, spirit, session });
  });

  // The turn's own prompt, as the harness names it. OMP emits
  // before_agent_start with the prompt text for a native prompt and for a door
  // message drained from its hidden next-turn queue; the idle agent-initiated
  // path emits nothing. The context hook resolves the turn's origin against
  // this text when it is held and falls back to the latest recognized message
  // when it is not. It lives exactly one turn: agent_end clears it first thing,
  // so a prior user prompt can never outlive its turn and be matched again.
  pi.on("before_agent_start", (event, ctx) => {
    try {
      const { room, effectiveRoomDir } = roomContext(ctx?.cwd);
      const key = activeTurnPromptKey(room, hostSessionIdentity(ctx, effectiveRoomDir));
      const prompt = typeof event?.prompt === "string" ? event.prompt : "";
      if (!prompt) {
        activeTurnPrompts.delete(key);
        return;
      }
      activeTurnPrompts.delete(key);
      activeTurnPrompts.set(key, prompt);
      trimOldestMap(activeTurnPrompts, 128);
    } catch {
      // An unreadable start leaves no active prompt; the context hook falls back.
    }
  });

  // Insula tool lifecycle. These two taps observe and nothing else: they read
  // no content, return nothing, and swallow their own failures, so no verdict
  // or result they sit beside can be changed by an observation.
  pi.on("tool_call", async (event, ctx) => {
    try {
      const toolCallId = String(event?.toolCallId ?? "").trim();
      if (!toolCallId) return;
      const { room, effectiveRoomDir } = roomContext(ctx?.cwd);
      const session = hostSessionIdentity(ctx, effectiveRoomDir);
      const key = insulaToolKey(room, session, toolCallId);
      if (insulaToolSpans.has(key)) return;
      const request = insulaRequestSpans.get(insulaRequestKey(room, session));
      const span = startInsulaSpan({
        room,
        operation: insulaToolOperation(event?.toolName),
        toolCallId,
        traceId: request?.traceId ?? null,
        parentSpanId: request?.spanId ?? null,
      });
      if (!span) return;
      insulaToolSpans.set(key, span);
      trimOldestMap(insulaToolSpans, 512);
    } catch {
      // Observation is never load-bearing.
    }
  });

  pi.on("tool_result", async (event, ctx) => {
    try {
      const toolCallId = String(event?.toolCallId ?? "").trim();
      if (!toolCallId) return;
      const failed = Boolean(event?.isError);
      const { room, effectiveRoomDir } = roomContext(ctx?.cwd);
      const session = hostSessionIdentity(ctx, effectiveRoomDir);
      const key = insulaToolKey(room, session, toolCallId);
      const span = insulaToolSpans.get(key);
      if (span) {
        insulaToolSpans.delete(key);
        endInsulaSpan(span, failed ? "error" : "ok", failed ? "tool_error" : null);
      }
      // A result point is distinct from the call span's end: every completed
      // call gets its own result fact, including the normal paired lifecycle.
      const request = insulaRequestSpans.get(insulaRequestKey(room, session));
      recordInsulaPoint({
        room,
        operation: "tool_result",
        toolCallId,
        traceId: span?.traceId ?? request?.traceId ?? null,
        parentSpanId: span?.spanId ?? request?.spanId ?? null,
        outcomeClass: failed ? "error" : "ok",
        errorClass: failed ? "tool_error" : null,
        scope: "tool_call",
      });
    } catch {
      // Observation is never load-bearing.
    }
  });

  // Chat draft. While the doorman's say is being answered, the assistant text
  // and the tools it uses are mirrored to the Host so the chat surface shows
  // the answer forming. Observation only: a failed report is warned about by
  // the doorman and changes nothing here.
  pi.on("message_start", (event) => {
    noteChatMessageStart(event?.message);
  });
  pi.on("message_update", (event) => {
    noteChatMessageUpdate(event?.message);
  });
  pi.on("message_end", (event) => {
    noteChatMessageEnd(event?.message);
  });
  pi.on("tool_execution_start", (event) => {
    noteChatToolStart(event);
  });
  pi.on("tool_execution_end", (event) => {
    noteChatToolEnd(event);
  });

  // Insula provider lifecycle. The main loop emits turn_start immediately
  // before its provider call. Advisor, side-stream, capture, and cache-refresh
  // traffic has no main turn event, so it never enters this correlation map.
  pi.on("turn_start", async (_event, ctx) => {
    try {
      const { room, session } = insulaSessionBinding(ctx);
      openInsulaRequest(room, session, "provider_replaced");
    } catch {
      // Observation is never load-bearing.
    }
  });

  pi.on("auto_retry_start", async (_event, ctx) => {
    try {
      const { room, session } = insulaSessionBinding(ctx);
      openInsulaRequest(room, session, "provider_retried");
    } catch {
      // Observation is never load-bearing.
    }
  });

  pi.on("turn_end", async (event, ctx) => {
    const response = messageText(event?.message);
    try {
      const settlement = insulaAssistantSettlement(event?.message);
      if (settlement) {
        const { room, session } = insulaSessionBinding(ctx);
        const key = insulaRequestKey(room, session);
        noteInsulaProviderRequestId(insulaRequestSpans.get(key), event?.message?.responseId);
        settleInsulaRequest(
          key,
          settlement.outcomeClass,
          settlement.errorClass,
          settlement,
        );
      }
    } catch {
      // Observation is never load-bearing.
    }
    // An empty assistant turn is exactly what the nonempty hard guard exists
    // to catch, so it must produce a receipt rather than a quiet return. The
    // old early exit left the contract pending and unsettled, which is the
    // one outcome the guard was supposed to make impossible.
    // `emitted` decides whether anything was said; the digest still covers the
    // response exactly as the provider returned it.
    const emitted = Boolean(response.trim());
    try {
      const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
      const session = hostSessionIdentity(ctx, effectiveRoomDir);
      const key = `${room}\0${session}`;
      const pending = pendingPresenceContracts.get(key);
      if (!pending) return;
      if (!emitted && !pending.nonemptyGuardId) {
        console.warn("[athanor] Presence refusal cites no guard: the contract carried no hard nonempty-response guard");
      }
      await settlePresence(
        { room, spirit, session },
        {
          contractId: pending.contractId,
          attempt: 1,
          evaluatedDirectives: pending.directiveIds,
          violations: emitted || !pending.nonemptyGuardId ? [] : [{
            directiveId: pending.nonemptyGuardId,
            reason: "The assistant turn emitted no text.",
          }],
          decision: emitted ? "accept" : "refuse",
          responseDigest: emitted ? responseDigest(response) : null,
        },
        `${pending.contractId}:settle:1`,
      );
      // Only a settled contract may be forgotten. Clearing before the Host
      // answers would lose the one receipt that says what happened.
      pendingPresenceContracts.delete(key);
    } catch (error) {
      console.warn(`[athanor] Presence settlement degraded: ${error instanceof Error ? error.message : String(error)}`);
    }
  });

  pi.on("agent_end", async (event, ctx) => {
    try {
      const { room, session } = insulaSessionBinding(ctx);
      const key = insulaRequestKey(room, session);
      const span = insulaRequestSpans.get(key);
      if (!span) return;
      const messages = Array.isArray(event?.messages) ? event.messages : [];
      const currentAssistant = [...messages].reverse().find((message) =>
        message?.role === "assistant"
        && typeof message?.timestamp === "number"
        && message.timestamp >= span.startedAtEpochMs
      );
      const fallback = insulaAssistantSettlement(currentAssistant);
      if (fallback) {
        noteInsulaProviderRequestId(span, currentAssistant?.responseId);
        settleInsulaRequest(key, fallback.outcomeClass, fallback.errorClass, fallback);
      } else settleInsulaRequest(key, "unknown", "provider_unsettled");
    } catch {
      // Observation is never load-bearing.
    }
  });


  const stopKittenProgress = pi.events?.on?.("task:subagent:progress", (payload: unknown) => {
    if (kittenLineageDisabled() || !payload || typeof payload !== "object") return;
    // Dispatch is where a Docket attempt is still known, so the attempt id is
    // stamped here and travels with the join to settlement.
    const progress = stampAttemptId(payload as Record<string, unknown>) as KittenQuestProgress;
    const joinKey = kittenLifecycleJoinKey(progress);
    noteKittenProgress(progress, joinKey);
    if (!joinKey) return;
    kittenQuestProgress.set(joinKey, progress);
    trimOldestMap(kittenQuestProgress, 1_024);
    const callId = String(progress.parentToolCallId ?? "").trim();
    const room = kittenRoomsByToolCallId.get(callId);
    if (room) kittenRoomsByAgentId.set(joinKey, room);
  });

  const stopKittenLifecycle = pi.events?.on?.("task:subagent:lifecycle", async (payload: unknown) => {
    if (kittenLineageDisabled() || !payload || typeof payload !== "object") return;
    const lifecycle = payload as Record<string, unknown>;
    const id = String(lifecycle.id ?? "").trim();
    const joinKey = kittenLifecycleJoinKey(lifecycle);
    const progress = kittenQuestProgress.get(joinKey);
    noteKittenLifecycle(payload, id, Boolean(progress));
    if (!progress) return;
    const toolCallId = String(lifecycle.parentToolCallId ?? progress.parentToolCallId ?? "").trim();
    const room = kittenRoomsByToolCallId.get(toolCallId)
      || kittenRoomsByAgentId.get(joinKey)
      || kittenRoomsByAgentId.get(id);
    const binding = kittenBindingsByToolCallId.get(toolCallId);
    if (!room || !binding) return;
    let settled = false;
    try {
      const lineage = await settleQuestLifecycle(
        binding,
        toolCallId,
        progress as Record<string, unknown>,
        lifecycle,
        `${toolCallId}:lifecycle`,
      );
      settled = lineage.settled;
      for (const record of lineage.memories) await recordKittenQuest(binding, record);
    } finally {
      // The Host decides when a quest is over; the join map is released on its
      // word, never on a status string read here.
      if (settled) {
        kittenQuestProgress.delete(joinKey);
        kittenRoomsByToolCallId.delete(toolCallId);
        kittenBindingsByToolCallId.delete(toolCallId);
        kittenRoomsByAgentId.delete(joinKey);
        kittenRoomsByAgentId.delete(id);
      }
    }
  });


  pi.on("tool_call", async (event, ctx) => {
    if (event?.toolName !== "task" || kittenLineageDisabled()) return;
    const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
    const binding = {
      room,
      spirit,
      session: hostSessionIdentity(ctx, effectiveRoomDir),
    };
    cacheKittenTaskRoom(room, event.toolCallId, event.input, binding);
  });

  pi.on("tool_call", async (event, ctx) => {
    if (!isMutateTool(event?.toolName)) return;
    try {
      const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
      markToolEvidence({ room, spirit, session: hostSessionIdentity(ctx, effectiveRoomDir) }, {
        paths: mutateToolPaths(event.toolName, event.input),
        cwd: String(ctx?.cwd ?? ""),
      });
    } catch {
      // An unreadable room costs the work hint, never the tool call.
    }
  });
  pi.on("tool_call", async (event, ctx) => {
    try {
      const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
      return await blockLessonRefusal(event, ctx, { room, spirit, session: hostSessionIdentity(ctx, effectiveRoomDir) });
    } catch (error) {
      // tool_call errors fail closed, so a broken guard would refuse every write in the
      // session. The native TTSR interrupt still fires once; the operator sees this degrade.
      const reason = error instanceof Error ? error.message : String(error);
      console.warn(`[athanor] Block lesson guard degraded on ${event?.toolName}: ${reason}`);
      ctx?.ui?.notify?.(`Athanor block lessons unguarded for this call: ${reason}`, "warning");
      return undefined;
    }
  });
  // OMP's matcher decided the fire; the ledger only learns it happened.
  pi.on("ttsr_triggered", async (event, ctx) => {
    if (!Array.isArray(event?.rules)) return;
    try {
      const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
      await recordNativeFires({ room, spirit, session: hostSessionIdentity(ctx, effectiveRoomDir) }, event.rules);
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      console.warn(`[athanor] native lesson fire not recorded: ${reason}`);
    }
  });
  pi.on("message_start", async (event, ctx) => {
    if (event?.message?.customType !== "athanor-hallway-knock") return;
    const knockId = String(event?.message?.details?.knockId ?? "").trim();
    if (!knockId) return;
    const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
    await noteHallwayKnockTurnStart({
      room,
      spirit,
      session: hostSessionIdentity(ctx, effectiveRoomDir),
    }, knockId);
  });

  pi.on("context", async (event, ctx) => {
    const result = await settleAutomaticContextWithinBudget(async (signal, deadline) => {
      const messages = Array.isArray(event?.messages) ? event.messages : [];
      const initial = roomContext(ctx.cwd);
      const session = hostSessionIdentity(ctx, initial.effectiveRoomDir);
      const activePrompt = activeTurnPrompts.get(activeTurnPromptKey(initial.room, session));
      const origin = currentTurnOrigin(messages, activePrompt ?? null);
      const capabilities = {
        automaticRecall: automaticRecallAllowed(ctx) && process.env.ATHANOR_DISABLE_AUTO_RECALL !== "1",
        topLevel: topLevelSession(initial.room) === session && ctx?.agent?.kind !== "sub" && !ctx?.agent?.parentId,
      };
      const nativeUser = Boolean(origin?.native) && capabilities.topLevel;
      const prompt = messageText(origin?.message);
      if (!prompt.trim()) return;
      const turnKeys = turnKeysByMessage(messages);
      const turnId = turnKeys.get(origin?.message);
      if (!turnId) return;
      const warnings: string[] = [];
      const activities: string[] = [];
      let houseState = null;
      try {
        houseState = await applyPromptDirectives(ctx, prompt, nativeUser, signal);
      } catch {
        warnings.push("room state maintenance degraded");
      }
      const { room, spirit, operator, effectiveRoomDir } = roomContext(ctx.cwd);
      const binding = { room, spirit, session };
      startChatDoorman(pi, ctx, binding);
      startHallwayKnockDoorman(pi, ctx, binding);
      const modelDefault = houseState?.modelDefault;
      const modelKey = `${room}:${ctx.cwd || effectiveRoomDir}:${modelDefault?.model || ""}`;
      if (modelDefault?.enabled && modelDefault.model && !modelDefaultsApplied.has(modelKey) && typeof pi.setModel === "function") {
        try {
          if (ctx.models?.resolve?.(modelDefault.model)) {
            await pi.setModel(modelDefault.model);
            modelDefaultsApplied.add(modelKey);
            activities.push(`model default ${modelDefault.model}`);
          } else {
            warnings.push(`model default unavailable: ${modelDefault.model}`);
          }
        } catch {
          warnings.push(`model default failed: ${modelDefault.model}`);
        }
      }
      const activeProject = activeProjectFromEvidence(binding);
      const word = capabilities.topLevel ? classWord(prompt) : null;
      if (word) {
        try {
          await new RecallPolicyHostClient(binding).setRequestedMode(word.mode, `${turnId}:class-word`);
          activities.push(`class word ${word.word}: Recall mode ${word.mode}`);
        } catch (error) {
          warnings.push(`class word ${word.word} not applied: ${error instanceof Error ? error.message : String(error)}`);
        }
      }
      const plan = await planContextLessons(binding, {
        turnId, activeProject, managerAvailable: contextLessonManagerAvailable(ctx), deadline, nativeUser, capabilities,
      }, signal);
      const synced = syncLessonTtsr({ ctx, plan });
      warnings.push(...synced.warnings);
      if (lessonTtsrInstallWarning) warnings.push(lessonTtsrInstallWarning);
      if (synced.active > 0) activities.push(`${synced.active} native lesson guards`);
      let conversation: ConversationCapture | null = null;
      try {
        conversation = await logConversationWindow(binding, effectiveRoomDir, ctx, messages, "context",
          houseState?.operator || operator, houseState?.embodiedSpirit || spirit, process.env.ATHANOR_REPLAY_MODE !== "1");
        ingestGigaLoggedTurnsDetached(ctx, conversation.loggedTurns);
      } catch {
        warnings.push("conversation capture degraded");
      }
      const visibleTurnIds = [...turnKeys.values()];
      const memoSessionKey = `${room}:${session}`;
      const legacy = adoptedContextSessions.has(memoSessionKey)
        ? { status: "absent" as const } : readLegacyContextProposal(effectiveRoomDir, memoSessionKey, new Set(visibleTurnIds));
      const priorPresence = [...messages].reverse().find((message) =>
        message?.customType === "athanor-presence-context" && typeof message?.details?.frameId === "string");
      const existingBlockKinds = messages.map((message) => contextBlockKind(message?.customType))
        .filter((kind): kind is ContextBlockKind => kind !== undefined);
      const previousRequest = lastSettledInsulaRequests.get(insulaRequestKey(room, session));
      const prepared = await prepareContext(binding, {
        turnId, prompt, nativeUser, visibleTurnIds,
        previousRequest: previousRequest ? {
          traceId: previousRequest.traceId,
          spanId: previousRequest.spanId,
          providerRequestId: previousRequest.providerRequestId ?? null,
        } : null,
        history: messages.flatMap((message) => {
          const kind = message?.role === "user" ? "user" : message?.role === "assistant" ? "assistant" : turnKeys.has(message) ? "generated" : null;
          return kind ? [{ kind, text: conversationText(message), turnId: turnKeys.get(message) ?? null }] : [];
        }),
        existingBlockKinds,
        contextCharacters: messages.reduce((sum, message) => sum + messageText(message).length, 0),
        userTurnOrdinal: messages.filter((message) => message?.role === "user").length,
        freshConversation: conversation?.fresh === true,
        activeProject,
        toolEvidenceRevision: toolEvidenceRevision(binding),
        toolEvidenceEpoch: TOOL_EVIDENCE_EPOCH,
        capabilities,
        planToken: plan.token,
        credential: plan.requiresCredential ? await resolveJudgmentCredential(ctx, binding, signal) : undefined,
        legacyMemo: legacy.status === "ready" ? legacy.memo : null,
        legacyRejection: legacy.status === "rejected" ? legacy.reason : null,
        recallTelemetryOverride: process.env.ATHANOR_RECALL_TELEMETRY,
        priorPresence: priorPresence ? {
          frameId: priorPresence.details.frameId,
          frameRendered: String(priorPresence.details.frameRendered ?? ""),
        } : null,
      }, signal);
      if (prepared.nativeOwned) {
        adoptedContextSessions.add(memoSessionKey);
        trimOldestSet(adoptedContextSessions, 128);
      }
      const settlementKey = `${room}\0${session}`;
      if (prepared.presenceSettledContractId
        && pendingPresenceContracts.get(settlementKey)?.contractId === prepared.presenceSettledContractId) {
        pendingPresenceContracts.delete(settlementKey);
      }
      if (prepared.presenceSettlement) {
        pendingPresenceContracts.set(settlementKey, prepared.presenceSettlement);
      }
      const memo = new Map(prepared.turns.map((turn) => [turn.turnId, turn.blocks.map((block) => ({
        role: "custom",
        customType: CONTEXT_BLOCK_TYPES[block.kind],
        content: block.content,
        ...(block.details ? { details: block.details } : {}),
        timestamp: block.timestamp,
        display: false,
        attribution: "agent",
      }))]));
      // enough: the prepared context does not carry the resolved mode, so the door
      // costs one more local Host read per top-level turn. The way up is a mode
      // field on PreparedContext.
      if (capabilities.topLevel) {
        try {
          const resolved = (await new RecallPolicyHostClient(binding).inspect()).recallPolicy.resolvedMode;
          const { lines, problem } = readModeDoorLines(effectiveRoomDir);
          if (problem) warnings.push(`${MODE_DOOR_FILE}: ${problem} The House lines fill the gap.`);
          const door = modeDoor(settlementKey, resolved, lines);
          if (door) {
            memo.set(turnId, [...(memo.get(turnId) ?? []), {
              role: "custom",
              customType: "athanor-mode-door",
              content: door.text,
              display: true,
              details: { from: door.from, to: door.to, word: word?.word ?? null },
              attribution: "agent",
              timestamp: Date.now(),
            }]);
            activities.push(`mode door: ${door.from ?? "start"} → ${door.to}`);
          }
        } catch (error) {
          warnings.push(`mode door unavailable: ${error instanceof Error ? error.message : String(error)}`);
        }
      }
      for (const [grant, coverage] of Object.entries(prepared.coverage ?? {})) {
        contextReceipts.set(`${memoSessionKey}:${grant}`, coverage);
      }
      trimOldestMap(contextReceipts, 256);
      if (prepared.invalidationReason) warnings.push(`Context rebuilt: ${prepared.invalidationReason}`);
      if (prepared.adoptionRejection) warnings.push(`Legacy context adoption refused: ${prepared.adoptionRejection}; source file retained`);
      showHouseContextFeedback(ctx, { room, spirit, activities: [...activities, ...prepared.activities], warnings: [...warnings, ...prepared.warnings] });
      const currentMessages = prepared.invalidationReason
        ? messages.filter((message) => message?.role !== "custom" || !contextBlockKind(message?.customType))
        : messages;
      if (!prepared.replayed) {
        const current = prepared.turns.find((turn) => turn.turnId === turnId);
        const contextCharacters = currentMessages.reduce((total, message) => total + messageText(message).length, 0);
        for (const block of current?.blocks ?? []) {
          recordInsulaPoint({
            room,
            operation: `injection.${block.kind.replace(/-/g, "_")}`,
            outcomeClass: "ok",
            bytesIn: contextCharacters,
            bytesOut: block.content.length,
          });
        }
      }
      return anchorTurnAdditions(currentMessages, turnKeys, memo) ?? (currentMessages !== messages ? { messages: currentMessages } : undefined);
    });
    if (result.status === "settled") return result.value;
    console.warn(`[athanor] Automatic context ${result.status === "timeout" ? "timed out" : "unavailable"}`);
    const messages = Array.isArray(event?.messages) ? event.messages : [];
    const withoutDerived = messages.filter((message) => message?.role !== "custom" || !contextBlockKind(message?.customType));
    if (withoutDerived.length !== messages.length) return { messages: withoutDerived };
  });


  pi.on("session_compact", async (event, ctx) => {
    const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
    const hostSession = hostSessionIdentity(ctx, effectiveRoomDir);
    const summary = event?.compactionEntry?.summary ?? event?.summary;
    try {
      await new RecallPolicyHostClient({
        room,
        spirit,
        session: hostSession,
      }).invalidateAfterCompaction(
        summary,
        `compaction:${String(event?.compactionEntry?.id || event?.id || Bun.hash(String(summary ?? "")))}`,
      );
    } catch (error) {
      console.warn(`[athanor] Recall Policy Host degraded during compaction: ${error instanceof Error ? error.message : String(error)}`);
      ctx.ui?.notify?.("Athanor recall policy did not invalidate after compaction.", "warning");
    }
  });
  pi.on("tool_result", async (event, ctx) => {
    if (event?.toolName !== "task" || kittenLineageDisabled()) return;
    const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
    const toolCallId = String(event.toolCallId ?? "").trim();
    const binding = {
      room,
      spirit,
      session: hostSessionIdentity(ctx, effectiveRoomDir),
    };
    cacheKittenTaskRoom(room, toolCallId, event.input, binding);
    const records = await normalizeQuestMemories(
      binding,
      toolCallId,
      event.input,
      event.details,
      `${toolCallId}:result`,
    );
    for (const record of records) {
      const recorded = await recordKittenQuest(binding, record);
      if (!recorded) {
        ctx.ui?.notify?.("Athanor could not persist subagent lineage.", "warning");
        break;
      }
    }
  });


  pi.on("shutdown", async () => {
    await flushGigaTurns();
    // Close every still-open observation before the bounded writer flush, so a
    // request settles with its usage point and an unfinished tool becomes an
    // explicit cancellation instead of a dangling start.
    retireAllInsulaSpans();
    await closeInsulaWriter();
    stopKittenProgress?.();
    stopKittenLifecycle?.();
  });

  pi.on("agent_end", async (event, ctx) => {
    const { room, spirit, operator, effectiveRoomDir } = roomContext(ctx?.cwd || process.cwd());
    const binding = {
      room,
      spirit,
      session: hostSessionIdentity(ctx, effectiveRoomDir),
    };
    // The turn is over: its prompt must not be matched by the next one.
    activeTurnPrompts.delete(activeTurnPromptKey(room, binding.session));
    try {
      await noteChatTurnEnd(binding, event);
    } catch {
      // The doorman degrades on its own warning cadence.
    }
    try {
      const capture = await logConversationWindow(
        binding,
        effectiveRoomDir,
        ctx,
        event?.messages || [],
        "agent_end",
        operator,
        spirit,
        process.env.ATHANOR_REPLAY_MODE !== "1",
      );
      ingestGigaLoggedTurnsDetached(ctx, capture.loggedTurns);
    } catch {
      ctx.ui?.notify?.("Athanor conversation capture degraded.", "warning");
      // Capture must never perturb the visible OMP turn.
    } finally {
      await noteHallwayKnockTurnEnd(binding);
    }
  });

  registerSolarisaelTools(pi, release);
}

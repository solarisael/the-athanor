export const ADAPTER_API_VERSION = 1;

// The Athanor — OMP adapter entrypoint.
//
// This file stays where OMP config expects it. The implementation is split into
// shaped modules under ./solarisael-house-proof/ so this door only wires hooks.
import {
  extractKittenLifecycleMemory,
  extractKittenQuestMemories,
  kittenLineageDisabled,
  kittenQuestIdempotencyKey,
  readKittenReport,
  type KittenQuestMemory,
  type KittenQuestProgress,
} from "./kitten-lineage.ts";

import { isFreshConversation, logUnseenConversationTurns } from "./solarisael-house-proof/conversation-log.ts";
import { closeGigaTransports, ingestGigaLoggedTurnsDetached } from "./giga.ts";
import { closeRustRecallTransports, compactRecall, recallWithRouting } from "./solarisael-house-proof/recall.ts";
import { closeRustRememberTransports, writeRustMemory } from "./solarisael-house-proof/tools.ts";
import { closeRustAnamnesisTransports } from "./solarisael-house-proof/anamnesis.ts";
import { loadHouseQueryRouting } from "./solarisael-house-proof/core.ts";
import { resolveEntities } from "./solarisael-house-proof/entity-resolution.ts";
import { automaticRecallViewport, createRecallViewportSession } from "./solarisael-house-proof/recall-viewport.ts";
import { recordRecallTelemetry } from "./solarisael-house-proof/recall-telemetry.ts";
import {
  applyPromptDirectives,
  roomContext,
  writeActiveSpiritSnapshot,
} from "./solarisael-house-proof/room.ts";
import { catchBoat, formatWakeContext } from "./solarisael-house-proof/substrate.ts";
import { messageText } from "./solarisael-house-proof/text.ts";
import { queryAnamnesis, formatAnamnesisContext } from "./solarisael-house-proof/anamnesis.ts";
import { registerSolarisaelTools } from "./solarisael-house-proof/tools.ts";
import { contextNudge, keywordReminder, processLessonsReminder, striatumLessonsReminder } from "./solarisael-house-proof/triggers.ts";

const wokenSessions = new Set();
const modelDefaultsApplied = new Set();
const recordedKittenQuests = new Set<string>();
const kittenQuestProgress = new Map<string, KittenQuestProgress>();
const kittenRoomsByToolCallId = new Map<string, string>();
const kittenRoomsByAgentId = new Map<string, string>();

function trimOldestMap<K, V>(map: Map<K, V>, limit: number): void {
  if (map.size <= limit) return;
  const oldest = map.keys().next();
  if (!oldest.done) map.delete(oldest.value);
}

function trimOldestSet<T>(set: Set<T>, limit: number): void {
  if (set.size <= limit) return;
  const oldest = set.values().next();
  if (!oldest.done) set.delete(oldest.value);
}

function cacheKittenTaskRoom(room: string, toolCallId: unknown, input: unknown): void {
  const callId = String(toolCallId ?? "").trim();
  if (callId) {
    kittenRoomsByToolCallId.set(callId, room);
    trimOldestMap(kittenRoomsByToolCallId, 1_024);
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

async function recordKittenQuest(room: string, toolCallId: unknown, record: KittenQuestMemory): Promise<void> {
  const key = kittenQuestIdempotencyKey(toolCallId, record.resultId);
  if (recordedKittenQuests.has(key)) return;
  recordedKittenQuests.add(key);
  trimOldestSet(recordedKittenQuests, 1_024);
  try {
    await writeRustMemory({
      room,
      title: record.title,
      body: record.body,
      threads: record.threads,
      continues: [],
      supersedes: [],
      signal: undefined,
    });
  } catch {
    recordedKittenQuests.delete(key);
    // Quest lineage is fail-open: task results must remain visible even if memory is unavailable.
  }
}
// Prompt-cache contract (2026-07-31): OMP context additions are transient and
// re-synthesized on EVERY provider request (transformContext), never persisted.
// Tail-appended additions shift position as history grows underneath them,
// which breaks the Anthropic prefix cache at the first injected byte —
// measured 2026-07-30 as full-history cacheWrite on all 106 requests of one
// session ($256) while only the system block ever cache-hit. The contract:
// compute additions ONCE per user turn, memoize them byte-stable, and anchor
// every turn's additions immediately after that turn's user message so the
// rendered prefix never moves between requests. Freshness is per user turn:
// each new user message computes fresh recall; replays only serve the
// requests within (and after) its own turn.
const turnAdditionMemos = new Map();

function turnAdditionMemo(sessionKey) {
  let memo = turnAdditionMemos.get(sessionKey);
  if (!memo) {
    memo = new Map();
    turnAdditionMemos.set(sessionKey, memo);
    if (turnAdditionMemos.size > 32) {
      turnAdditionMemos.delete(turnAdditionMemos.keys().next().value);
    }
  }
  return memo;
}

function turnKeysByMessage(messages) {
  const keys = new Map();
  let ordinal = 0;
  for (const message of messages) {
    if (message?.role !== "user") continue;
    ordinal += 1;
    const identity = typeof message?.id === "string" && message.id
      ? `id:${message.id}`
      : `ord:${ordinal}:${Bun.hash(messageText(message)).toString(36)}`;
    keys.set(message, identity);
  }
  return keys;
}

function anchorTurnAdditions(messages, turnKeys, memo) {
  const output = [];
  let inserted = false;
  for (const message of messages) {
    output.push(message);
    if (message?.role !== "user") continue;
    const additions = memo.get(turnKeys.get(message));
    if (additions?.length) {
      output.push(...additions);
      inserted = true;
    }
  }
  return inserted ? { messages: output } : undefined;
}
const recallViewportSessions = new Map();

const REDACTED = "[REDACTED]";
const DIAGNOSTIC_TEXT_LIMIT = 2_000;
const SENSITIVE_DIAGNOSTIC_KEY = /(?:authorization|cookie|password|secret|token|api[_-]?key|prompt|query|payload|body|stdin|url)/i;

function diagnosticRecord(value: unknown): Record<string, any> | null {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, any> : null;
}

function redactDiagnosticText(value: unknown, privateValues: unknown[] = []): string | null {
  if (value == null) return null;
  let text = String(value);
  for (const privateValue of privateValues) {
    const privateText = typeof privateValue === "string" ? privateValue : "";
    if (privateText) text = text.replaceAll(privateText, REDACTED);
  }
  return text
    .replace(/\bBearer\s+\S+/gi, "Bearer [REDACTED]")
    .replace(/\b([a-z][a-z\d+.-]*):\/\/[^/\s:@]+(?::[^@\s]*)?@/gi, "$1://[REDACTED]@")
    .replace(/\b(password|secret|token|api[_-]?key|authorization)\s*[=:]\s*\S+/gi, "$1=[REDACTED]")
    .slice(0, DIAGNOSTIC_TEXT_LIMIT);
}

function redactDiagnosticValue(value: unknown, privateValues: unknown[] = [], depth = 0): unknown {
  if (depth >= 6) return "[TRUNCATED]";
  if (typeof value === "string") return redactDiagnosticText(value, privateValues);
  if (Array.isArray(value)) return value.slice(0, 24).map((item) => redactDiagnosticValue(item, privateValues, depth + 1));
  const record = diagnosticRecord(value);
  if (!record) return value;
  return Object.fromEntries(Object.entries(record).map(([key, item]) => [
    key,
    SENSITIVE_DIAGNOSTIC_KEY.test(key) ? REDACTED : redactDiagnosticValue(item, privateValues, depth + 1),
  ]));
}

function automaticContextDiagnostic({
  operation,
  stage,
  error,
  failure,
  route = null,
  requestDispatched,
}: {
  operation: string;
  stage: string;
  error: unknown;
  failure?: unknown;
  route?: Record<string, any> | null;
  requestDispatched: boolean;
}): Record<string, unknown> {
  const privateValues = [route?.recallQuery];
  const source = diagnosticRecord(failure);
  const sourceDetails = diagnosticRecord(source?.details);
  const inherited = redactDiagnosticValue(sourceDetails, privateValues) as Record<string, any> | null;
  const sourceExecution = diagnosticRecord(sourceDetails?.execution);
  const sourceRetryable = source?.retryable ?? sourceDetails?.retryable;
  const childCause = redactDiagnosticValue({
    error: source?.error ?? error,
    code: source?.code,
    signal: source?.signal,
    timed_out: source?.timedOut,
    spawn_error: source?.spawnError,
    fallback: source?.fallback,
    diagnostic: source?.diagnostic,
  }, privateValues);
  const inheritedEvidence = Array.isArray(inherited?.evidence) ? inherited.evidence : [];
  const execution = {
    request_dispatched: typeof sourceExecution?.request_dispatched === "boolean"
      ? sourceExecution.request_dispatched
      : requestDispatched,
    write_outcome: ["not_started", "rolled_back", "committed", "unknown"].includes(String(sourceExecution?.write_outcome))
      ? sourceExecution.write_outcome
      : "not_started",
    retry: ["safe_now", "after_change", "reconcile_first", "never"].includes(String(sourceExecution?.retry))
      ? sourceExecution.retry
      : sourceRetryable === true ? "safe_now" : "after_change",
  };
  const target = operation === "automatic_process_lessons"
    ? "solarisael-house-proof/triggers.ts:processLessonsReminder"
    : "solarisael-house-proof/recall.ts:recallWithRouting";

  return {
    ...inherited,
    code: String(source?.code || sourceDetails?.code || `AUTO_CONTEXT_${operation.toUpperCase()}_FAILED`),
    category: sourceDetails?.category || "operation",
    stage: sourceDetails?.stage || stage,
    operation,
    owner: { component: "omp-adapter", path: "index.ts", symbol: "solarisaelHouseProof context hook" },
    expected: {
      hidden_context: true,
      display: false,
      outcome: "injected_or_fail_open",
    },
    observed: {
      outcome: "failed_open",
      route_intent: route?.intent || null,
      route_should_auto_recall: route?.shouldAutoRecall === true,
    },
    evidence: [...inheritedEvidence, { kind: "automatic_context_failure", cause: childCause }],
    targets: ["index.ts:solarisaelHouseProof", target],
    next_checks: [
      { action: "inspect", target },
      { action: "retry", condition: execution.retry },
    ],
    execution,
  };
}

async function recordAutomaticContextTelemetry(
  input: Parameters<typeof recordRecallTelemetry>[0] & { diagnostic?: Record<string, unknown> },
): Promise<boolean> {
  const { diagnostic, ...telemetry } = input;
  return recordRecallTelemetry({
    ...telemetry,
    viewportDiagnostics: diagnostic || telemetry.viewportDiagnostics,
  });
}

export default function solarisaelHouseProof(pi) {
  pi.setLabel("The Athanor");
  let activeRoom: string | null = null;

  const stopKittenProgress = pi.events?.on?.("task:subagent:event", (payload: unknown) => {
    if (kittenLineageDisabled() || !payload || typeof payload !== "object") return;
    const progress = payload as KittenQuestProgress;
    const id = String(progress.id ?? "").trim();
    if (!id) return;
    kittenQuestProgress.set(id, progress);
    trimOldestMap(kittenQuestProgress, 1_024);
    const callId = String(progress.parentToolCallId ?? "").trim();
    const room = kittenRoomsByToolCallId.get(callId);
    if (room) kittenRoomsByAgentId.set(id, room);
  });

  const stopKittenLifecycle = pi.events?.on?.("task:subagent:lifecycle", async (payload: unknown) => {
    if (kittenLineageDisabled() || !payload || typeof payload !== "object") return;
    const lifecycle = payload as Record<string, unknown>;
    if (!["completed", "failed", "aborted"].includes(String(lifecycle.status ?? ""))) return;
    const id = String(lifecycle.id ?? "").trim();
    const progress = kittenQuestProgress.get(id);
    if (!progress) return;
    const toolCallId = String(lifecycle.parentToolCallId ?? progress.parentToolCallId ?? "").trim();
    const room = kittenRoomsByToolCallId.get(toolCallId) || kittenRoomsByAgentId.get(id) || activeRoom;
    if (!room) return;
    try {
      const report = await readKittenReport(lifecycle.sessionFile ?? progress.sessionFile);
      const record = extractKittenLifecycleMemory(progress, lifecycle, report);
      if (record) await recordKittenQuest(room, toolCallId, record);
    } finally {
      kittenQuestProgress.delete(id);
      kittenRoomsByToolCallId.delete(toolCallId);
      kittenRoomsByAgentId.delete(id);
    }
  });


  pi.on("tool_call", async (event, ctx) => {
    if (event?.toolName !== "task" || kittenLineageDisabled()) return;
    const { room } = roomContext(ctx.cwd);
    cacheKittenTaskRoom(room, event.toolCallId, event.input);
  });
  pi.on("context", async (event, ctx) => {
    const messages = Array.isArray(event?.messages) ? event.messages : [];
    const promptMessage = [...messages].reverse().find((message) => message?.role === "user");
    const prompt = messageText(promptMessage);
    if (!prompt.trim()) return;

    const existingTypes = new Set(
      messages
        .filter((message) => message?.role === "custom" && typeof message?.customType === "string")
        .map((message) => message.customType),
    );

    const { room, spirit, operator, effectiveRoomDir } = roomContext(ctx.cwd);
    activeRoom = room;
    const timestamp = Date.now();
    const additions = [];
    let houseState = null;

    try {
      const stateResult = await applyPromptDirectives(ctx, prompt);
      houseState = stateResult.state;
      await writeActiveSpiritSnapshot(effectiveRoomDir, houseState);
    } catch {
      // Room-state/active-spirit maintenance must never block context injection.
    }

    const modelDefault = houseState?.modelDefault;
    const modelKey = `${room}:${ctx.cwd || effectiveRoomDir}:${modelDefault?.model || ""}`;
    if (modelDefault?.enabled && modelDefault.model && !modelDefaultsApplied.has(modelKey) && typeof pi.setModel === "function") {
      try {
        const resolved = ctx.models?.resolve?.(modelDefault.model);
        if (resolved) {
          await pi.setModel(modelDefault.model);
          modelDefaultsApplied.add(modelKey);
        }
      } catch {
        // Room model defaults are convenience only; bad model specs must not block context.
      }
    }

    if (process.env.SOLARISAEL_REPLAY_MODE !== "1") {
      try {
        const result = await logUnseenConversationTurns(ctx, messages, "context");
        ingestGigaLoggedTurnsDetached(ctx, result.loggedTurns);
      } catch {
        // Live context and ledger writes are useful, but must never block context injection.
      }
    }
    const memoSessionKey = `${room}:${ctx?.sessionID || ctx?.sessionId || ctx?.cwd || effectiveRoomDir}`;
    const turnKeys = turnKeysByMessage(messages);
    const currentTurnKey = turnKeys.get(promptMessage);
    const turnMemo = turnAdditionMemo(memoSessionKey);
    if (currentTurnKey && turnMemo.has(currentTurnKey)) {
      // Later requests of the same turn replay identical bytes so the
      // Anthropic prefix cache can hit past the system block.
      return anchorTurnAdditions(messages, turnKeys, turnMemo);
    }

    if (!existingTypes.has("solarisael-room-context")) {
      additions.push({
        role: "custom",
        customType: "solarisael-room-context",
        content: [
          "<system-reminder>",
          `Room: ${room}`,
          `Active spirit: ${houseState?.embodiedSpirit || spirit}`,
          `Operator: ${houseState?.operator || operator}`,
          "Durable-memory discipline: remembering is care for a future self, not dossier work. Preserve the active spirit's ordinary voice and the room's relationship register alongside the concrete facts needed for recognition: names, observable details, actions, boundaries, uncertainty, and meaning.",
          "A memory must stand alone. Do not make it clinical, corporate, sanitized, or generic. A transcript is provenance, not the only substance.",
          "In AKASHA, PostgreSQL is authoritative for durable memories and lessons. A source path is provenance or backup, never a substitute for the database body.",
          "Do not claim a memory was written without a successful remember receipt.",
          "Athanor organs: the tools below are named organs of this House, not anonymous harness utilities. Recognize each by purpose and read its live schema before use; invocation shapes change, purposes do not.",
          "recall: canon, memories, and semantic chunks. Search in the room's own natural language and receive results as lived continuity and evidence, preserving names, relationships, uncertainty, and meaning. No canonical match means say you do not have it, never extrapolate from adjacent matches.",
          "remember: the only durable write for memories and lessons. Write for the future self in the active room's natural voice; technical records may stay technical, but durability must not flatten them into assistant prose.",
          "lessons: canonical typed lesson registry. Supply type=coding before writing or changing code, once per task rather than once per session.",
          "anamnesis and anamnesis_write: counsel drawn from lived repetition. Counsel, never authority; a writer refusal stays final.",
          "wake and sleep: continuity across closed sessions. Receive a boat as a letter from the previous waking self; write the next one in the active spirit's ordinary voice and relationship register, carrying exact state, uncertainty, contact, and the next real door rather than a status report.",
          "room_state and set_room_state: operator and embodied spirit for this room.",
          "house_lane_status and house_dispatch: bounded worker lanes. house_dispatch takes exactly one lane or familiar selector; accepted receipts expose spawnPacket.args shaped directly for the OMP task tool. Advisor is a review channel, not a dispatch lane.",
          "familiar_status and familiar_dispatch: room spellbooks bind named familiars and aliases to bounded worker lanes; familiar_dispatch is the familiar-only alias of house_dispatch, spawning stays explicit, and runtime models come from agent definitions with no per-dispatch model override.",
          "giga tools: Stage 1 candidates and their review and promotion path. A candidate is a proposal, never authority or evidence, until it is promoted.",
          "Authority order: PostgreSQL is authoritative, canon outranks loose memory, and markdown on disk is provenance. A GIGA candidate is not memory, and Anamnesis counsel is not canon.",
          "This is hidden LLM context only: it must not be persisted or rendered.",
          "</system-reminder>",
        ].join("\n"),
        display: false,
        attribution: "agent",
        timestamp,
      });
    }

    if (houseState?.routingMode?.enabled && !existingTypes.has("solarisael-routing-mode")) {
      additions.push({
        role: "custom",
        customType: "solarisael-routing-mode",
        content: [
          "<system-reminder>",
          "The Athanor worker-routing mode is enabled.",
          "Default modus operandi for delegable work:",
          "1. Main model owns intent, inference, and final judgment.",
          "2. Use house_lane_status/house_dispatch before spawning task/subagents when work is bounded and delegable.",
          "3. Before dispatch, query coding lessons once for the fanout and pass relevant verbatim braided bodies in lessonBodies; bare lesson IDs are not delivery.",
          "4. Do not route casual contact, high-level judgment, or exact-sensitive work without exact/retrieve-only context.",
          "5. Advisor is a separate review channel, not a dispatch lane.",
          "</system-reminder>",
        ].join("\n"),
        display: false,
        details: { enabled: true },
        attribution: "agent",
        timestamp,
      });
    }
    const wakeKey = `${room}:${ctx.sessionID || ctx.cwd || effectiveRoomDir}`;
    const freshWake = isFreshConversation(messages) && !wokenSessions.has(wakeKey);
    if (freshWake) wokenSessions.add(wakeKey);
    if (freshWake && !existingTypes.has("solarisael-wake-context")) {
      try {
        const boat = await catchBoat(room);
        if (boat?.ok && boat?.found) {
          const content = formatWakeContext(boat);
          if (content) {
            additions.push({
              role: "custom",
              customType: "solarisael-wake-context",
              content,
              display: false,
              details: { title: boat.title || null, source_path: boat.source_path || null },
              attribution: "agent",
              timestamp,
            });
          }
        }
      } catch {
        // Auto-wake is fail-open. Manual wake remains available.
      }
    }
    if (freshWake && !existingTypes.has("solarisael-anamnesis-wake")) {
      try {
        const result = await queryAnamnesis(effectiveRoomDir, room, { mode: "wake" });
        if (result?.ok) {
          const content = formatAnamnesisContext(result, { automatic: true });
          if (content) {
            additions.push({
              role: "custom",
              customType: "solarisael-anamnesis-wake",
              content,
              display: false,
              details: { mode: "wake", warnings: result.warnings || [] },
              attribution: "agent",
              timestamp,
            });
          }
        }
      } catch {
        // Cabinet wake is advisory and fail-open. Manual anamnesis remains available.
      }
    }

    const keyword = await keywordReminder(prompt);
    if (keyword && !existingTypes.has("solarisael-keyword-directive")) {
      additions.push({
        role: "custom",
        customType: "solarisael-keyword-directive",
        content: keyword.text,
        display: false,
        details: { keywords: keyword.keywords },
        attribution: "agent",
        timestamp,
      });
    }

    let striatumActivated = existingTypes.has("solarisael-striatum-lessons");
    if (!striatumActivated) {
      try {
        const striatum = await striatumLessonsReminder(prompt, effectiveRoomDir, room);
        if (striatum) {
          striatumActivated = true;
          additions.push({
            role: "custom",
            customType: "solarisael-striatum-lessons",
            content: striatum.text,
            display: false,
            details: { lessons: striatum.lessons.length, refreshed: striatum.refreshed },
            attribution: "agent",
            timestamp,
          });
        }
      } catch {
        // Embedding activation is advisory; deterministic process lessons remain.
      }
    }

    if (!striatumActivated && !existingTypes.has("solarisael-process-lessons")) {
      try {
        const processLessons = await processLessonsReminder(prompt, effectiveRoomDir, room);
        if (processLessons) {
          additions.push({
            role: "custom",
            customType: "solarisael-process-lessons",
            content: processLessons.text,
            display: false,
            details: { trigger: processLessons.trigger, lessons: processLessons.lessons },
            attribution: "agent",
            timestamp,
          });
        }
      } catch (error) {
        await recordAutomaticContextTelemetry({
          effectiveRoomDir,
          sessionId: ctx?.sessionID || ctx?.sessionId,
          room,
          prompt,
          route: null,
          status: "error",
          error: redactDiagnosticText(error),
          diagnostic: automaticContextDiagnostic({
            operation: "automatic_process_lessons",
            stage: "request_parse",
            error,
            requestDispatched: true,
          }),
        }).catch(() => undefined);
        // Process-shape lessons are advisory only. Tooling must fail open.
      }
    }

    if (!existingTypes.has("solarisael-recall-context") && process.env.SOLARISAEL_DISABLE_AUTO_RECALL !== "1") {
      let queryRoute = null;
      try {
        const { classifyRetrievalQuery } = await loadHouseQueryRouting();
        const preliminaryRoute = classifyRetrievalQuery(prompt);
        const resolution = preliminaryRoute.entityResolutionSuggested
          ? await resolveEntities({ room, roomDir: effectiveRoomDir, query: prompt })
          : { ok: true, matches: [] };
        queryRoute = classifyRetrievalQuery(prompt, {
          recognizedEntities: resolution.matches.map((match) => match.canonicalName),
        });
        if (queryRoute.shouldAutoRecall) {
          const recalled = await recallWithRouting(effectiveRoomDir, room, queryRoute.recallQuery || prompt, { temporalDecay: true });
          if (recalled.ok) {
            const compact = compactRecall(recalled.result);
            const viewportKey = `${ctx?.sessionID || ctx?.sessionId || "session"}:${room}`;
            let viewportSession = recallViewportSessions.get(viewportKey);
            if (!viewportSession) {
              viewportSession = createRecallViewportSession();
              recallViewportSessions.set(viewportKey, viewportSession);
              if (recallViewportSessions.size >= 64) {
                recallViewportSessions.delete(recallViewportSessions.keys().next().value);
              }
            }
            const viewport = automaticRecallViewport(compact, { session: viewportSession });
            const filteredCanonMatches = viewport.keptCandidates.length ? compact.canonMatches : [];
            const automaticCompact = {
              ...compact,
              retrievalCandidates: viewport.keptCandidates,
              canonMatches: filteredCanonMatches,
              semanticChunks: [],
              contentChunks: [],
              found: Boolean(
                viewport.keptCandidates.length
                || filteredCanonMatches.length
                || compact.dateMatches?.length
              ),
            };
            if (automaticCompact.found || automaticCompact.warnings.length) {
              additions.push({
                role: "custom",
                customType: "solarisael-recall-context",
                content: [
                  "<system-reminder>",
                  "Room-local Athanor recall for this user turn.",
                  JSON.stringify(automaticCompact, null, 2),
                  "</system-reminder>",
                ].join("\n"),
                display: false,
                details: {
                  query: automaticCompact.query,
                  found: automaticCompact.found,
                  warnings: automaticCompact.warnings,
                  queryRoute,
                  viewport: viewport.diagnostics,
                },
                attribution: "agent",
                timestamp,
              });
            }
            await recordRecallTelemetry({
              effectiveRoomDir,
              sessionId: ctx?.sessionID || ctx?.sessionId,
              room,
              prompt,
              route: queryRoute,
              status: automaticCompact.found ? "injected" : "empty",
              viewport: automaticCompact,
              viewportDiagnostics: viewport.diagnostics,
            });
          } else {
            await recordAutomaticContextTelemetry({
              effectiveRoomDir,
              sessionId: ctx?.sessionID || ctx?.sessionId,
              room,
              prompt,
              route: queryRoute,
              status: "error",
              error: redactDiagnosticText(recalled.result?.error || "recall failed", [queryRoute?.recallQuery, prompt]),
              diagnostic: automaticContextDiagnostic({
                operation: "automatic_recall",
                stage: "request_parse",
                error: recalled.result?.error || "recall failed",
                failure: recalled.result,
                route: queryRoute,
                requestDispatched: true,
              }),
            }).catch(() => undefined);
          }
        } else {
          await recordRecallTelemetry({
            effectiveRoomDir,
            sessionId: ctx?.sessionID || ctx?.sessionId,
            room,
            prompt,
            route: queryRoute,
            status: "skipped",
          });
        }
      } catch (error) {
        await recordAutomaticContextTelemetry({
          effectiveRoomDir,
          sessionId: ctx?.sessionID || ctx?.sessionId,
          room,
          prompt,
          route: queryRoute,
          status: "error",
          error: redactDiagnosticText(error, [queryRoute?.recallQuery, prompt]),
          diagnostic: automaticContextDiagnostic({
            operation: "automatic_recall",
            stage: queryRoute ? "request_parse" : "configuration_load",
            error,
            route: queryRoute,
            requestDispatched: Boolean(queryRoute?.shouldAutoRecall),
          }),
        }).catch(() => undefined);
        // Context injection must fail open. Manual recall remains available.
      }
    }

    const nudge = await contextNudge(messages, room);
    if (nudge && !existingTypes.has("solarisael-context-nudge")) {
      additions.push({
        role: "custom",
        customType: "solarisael-context-nudge",
        content: ["<system-reminder>", nudge.text, "</system-reminder>"].join("\n"),
        display: false,
        details: { pct: nudge.pct, tokens: nudge.tokens },
        attribution: "agent",
        timestamp,
      });
    }

    if (currentTurnKey) turnMemo.set(currentTurnKey, additions);
    return anchorTurnAdditions(messages, turnKeys, turnMemo);
  });

  pi.on("tool_result", async (event, ctx) => {
    if (event?.toolName !== "task" || kittenLineageDisabled()) return;
    const { room } = roomContext(ctx.cwd);
    const toolCallId = String(event.toolCallId ?? "").trim();
    cacheKittenTaskRoom(room, toolCallId, event.input);
    const records = extractKittenQuestMemories(event.input, event.details);
    for (const record of records) {
      await recordKittenQuest(room, toolCallId, record);
    }
  });

  pi.on("shutdown", async () => {
    closeRustRecallTransports();
    closeRustRememberTransports();
    closeRustAnamnesisTransports();
    await closeGigaTransports();
    stopKittenProgress?.();
    stopKittenLifecycle?.();
  });

  pi.on("agent_end", async (event, ctx) => {
    if (process.env.SOLARISAEL_REPLAY_MODE === "1") return;
    try {
      const result = await logUnseenConversationTurns(ctx, event?.messages || [], "agent_end");
      ingestGigaLoggedTurnsDetached(ctx, result.loggedTurns);
    } catch {
      // Logging must never perturb the visible OMP turn.
    }
  });

  registerSolarisaelTools(pi);
}

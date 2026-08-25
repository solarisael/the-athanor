import { existsSync } from "node:fs";

import { discoverRustExecutable } from "./discovery.ts";
import {
  RustJsonlTransport,
  RustTransportError,
  RustTransportOutcomeUnknownError,
  TransportUnavailableError,
  type JsonObject,
} from "./rust-transport.ts";
import { roomContext } from "./solarisael-house-proof/room.ts";

const GIGA_READ_TIMEOUT_MS = 120_000;
let gigaClosing = false;
const gigaTransports = new Map<string, RustJsonlTransport>();

export const GIGA_OMP_ROOM_BINDING = "omp_room_binding" as const;
export type GigaSafeReviewState = "in_review" | "dismissed" | "unresolved" | "curio" | "expired";
export type GigaCandidate = JsonObject & {
  candidate_id: string;
  event_id: string;
  room: string;
  session_id: string;
  kind: string;
  review_state: string;
  project_keys: string[];
};
export type GigaCandidateListResult = { candidates: GigaCandidate[] };
export type GigaReviewResult = JsonObject & {
  candidate_id: string;
  previous_state: string;
  new_state: string;
  reviewed_at: string;
};
export type GigaPromotionTarget =
  | { kind: "memory"; title: string; body: string; threads: string[] }
  | { kind: "coding_lesson"; title: string; body: string; shape?: string; proof_pattern?: string; trigger_context?: string; language_keys: string[]; technology_keys: string[]; tags: string[] }
  | { kind: "project_lesson"; title: string; body: string; proof_pattern?: string; trigger_context?: string; language_keys: string[]; technology_keys: string[]; tags: string[]; publication_approved: boolean };
export type GigaPromotionRequest = {
  candidate_id: string;
  room: string;
  reviewer_id: string;
  operator_identity: string;
  authorization_basis: typeof GIGA_OMP_ROOM_BINDING;
  target: GigaPromotionTarget;
};
export type GigaPromotionResult = JsonObject;
export type GigaHealthResult = JsonObject & { enabled: boolean; store_healthy: boolean };
export type GigaQueueMaintenanceOperation = "check" | "purge_stuck";
export type GigaQueueMaintenanceResult = JsonObject;
export type GigaReviewRequest = {
  candidate_id: string;
  room: string;
  reviewer_id: string;
  new_state: GigaSafeReviewState;
  reason: string;
  authorization_basis: typeof GIGA_OMP_ROOM_BINDING;
};

type LoggedTurn = {
  role?: unknown;
  text?: unknown;
  sourceID?: unknown;
  contentHash?: unknown;
  sessionID?: unknown;
  sourceTimestamp?: unknown;
  hasStableID?: unknown;
};

type GigaTurnBuffer = { ctx: any; cwd: string; turns: LoggedTurn[] };
const gigaTurnBuffers = new Map<string, GigaTurnBuffer>();


function gigaTransport(cwd: string = process.cwd()): RustJsonlTransport | null {
  if (process.env.SOLARISAEL_GIGA_ENABLED !== "1") return null;
  const executable = discoverRustExecutable();
  if (!executable) return null;
  const trusted = roomContext(cwd);
  const key = `${executable}\0${trusted.room}\0${trusted.effectiveRoomDir}`;
  let transport = gigaTransports.get(key);
  if (!transport) {
    transport = new RustJsonlTransport({
      executable,
      cwd: trusted.effectiveRoomDir,
      env: {
        SOLARISAEL_GIGA_SOURCE_ROOM: trusted.room,
        SOLARISAEL_GIGA_CLAIM_OWNER: "1",
      },
    });
    gigaTransports.set(key, transport);
  }
  return transport;
}

function requireGigaTransport(): RustJsonlTransport {
  if (process.env.SOLARISAEL_GIGA_ENABLED !== "1") {
    throw Object.assign(new Error("GIGA is disabled"), { code: "giga_disabled", retryable: false, details: { enabled: false } });
  }
  const transport = gigaTransport();
  if (!transport) {
    throw Object.assign(new Error("GIGA transport is unavailable"), { code: "giga_transport_unavailable", retryable: true, details: { enabled: true, transport_available: false } });
  }
  return transport;
}

async function requestObject(method: string, params: JsonObject, options: { signal?: AbortSignal; write?: boolean; timeoutMs?: number } = {}): Promise<JsonObject> {
  return await requireGigaTransport().request(method, params, {
    signal: options.signal,
    timeoutMs: options.timeoutMs,
    ...(options.write ? { settleDefinitively: true } : {}),
  }) as JsonObject;
}

export function gigaTransportFailure(error: unknown): { code: string; message: string; retryable: boolean; details?: unknown } {
  if (error instanceof RustTransportError || error instanceof TransportUnavailableError || error instanceof RustTransportOutcomeUnknownError) {
    return { code: error.code, message: error.message, retryable: error.retryable, details: error.details };
  }
  if (error && typeof error === "object") {
    const value = error as { code?: unknown; message?: unknown; retryable?: unknown; details?: unknown };
    if (typeof value.code === "string" && typeof value.message === "string" && typeof value.retryable === "boolean") {
      return { code: value.code, message: value.message, retryable: value.retryable, details: value.details };
    }
  }
  return { code: "giga_transport_failure", message: error instanceof Error ? error.message : "GIGA transport failed", retryable: false };
}

export async function requestGigaCandidateList(room: string, options: { reviewState?: string; limit?: number; signal?: AbortSignal } = {}): Promise<GigaCandidateListResult> {
  return await requestObject("giga_candidate_list", {
    room,
    review_state: options.reviewState ?? null,
    limit: options.limit ?? 50,
  }, { signal: options.signal, timeoutMs: GIGA_READ_TIMEOUT_MS }) as GigaCandidateListResult;
}

export async function requestGigaReview(request: GigaReviewRequest, options: { signal?: AbortSignal } = {}): Promise<GigaReviewResult> {
  return await requestObject("giga_tool_review", request as unknown as JsonObject, { signal: options.signal, write: true }) as GigaReviewResult;
}

export async function requestGigaPromote(request: GigaPromotionRequest, options: { signal?: AbortSignal } = {}): Promise<GigaPromotionResult> {
  return await requestObject("giga_tool_promote", request as unknown as JsonObject, { signal: options.signal, write: true });
}

export async function requestGigaHealth(room: string, options: { signal?: AbortSignal } = {}): Promise<GigaHealthResult> {
  return await requestObject("giga_health", { room }, { signal: options.signal, timeoutMs: GIGA_READ_TIMEOUT_MS }) as GigaHealthResult;
}

export async function requestGigaQueueMaintenance(room: string, operation: GigaQueueMaintenanceOperation, options: { signal?: AbortSignal } = {}): Promise<GigaQueueMaintenanceResult> {
  return await requestObject("giga_queue_maintenance", { room, operation, scope: "room" }, { signal: options.signal, timeoutMs: GIGA_READ_TIMEOUT_MS, write: true });
}

async function ingestLoggedTurns(ctx: any, loggedTurns: LoggedTurn[]): Promise<void> {
  if (gigaClosing || process.env.SOLARISAEL_GIGA_ENABLED !== "1" || loggedTurns.length === 0) return;
  try {
    const trusted = roomContext(ctx?.cwd || process.cwd());
    const transport = gigaTransport(ctx?.cwd || process.cwd());
    if (!transport) return;
    await transport.request("giga_conversation_ingest", {
      room: trusted.room,
      project_keys: process.env.SOLARISAEL_GIGA_PROJECT_KEY ? [process.env.SOLARISAEL_GIGA_PROJECT_KEY] : [],
      turns: loggedTurns.map((turn) => ({
        role: turn.role,
        source_id: turn.sourceID,
        content_hash: turn.contentHash,
        session_id: turn.sessionID,
        timestamp: turn.sourceTimestamp,
        has_stable_id: turn.hasStableID,
      })),
    } as JsonObject);
  } catch {
    // GIGA ingestion is opt-in background work and cannot block ordinary House behavior.
  }
}

function isSubagentSessionContext(ctx: any): boolean {
  try {
    const sessionFile = ctx?.sessionManager?.getSessionFile?.();
    return typeof sessionFile !== "string" || !sessionFile || existsSync(`${path.dirname(sessionFile)}.jsonl`);
  } catch {
    return true;
  }
}

export function ingestGigaLoggedTurnsDetached(ctx: any, loggedTurns: LoggedTurn[]): void {
  if (gigaClosing || process.env.SOLARISAEL_GIGA_ENABLED !== "1" || !Array.isArray(loggedTurns) || loggedTurns.length === 0 || isSubagentSessionContext(ctx)) return;
  const cwd = String(ctx?.cwd || "");
  for (const turn of loggedTurns) {
    const key = `${cwd}\0${String(turn.sessionID ?? "")}`;
    const buffered = gigaTurnBuffers.get(key) ?? { ctx, cwd, turns: [] };
    buffered.ctx = ctx;
    buffered.turns.push(turn);
    gigaTurnBuffers.set(key, buffered);
  }
}

export function flushGigaTurnsDetached(ctx: any): void {
  const cwd = String(ctx?.cwd || "");
  for (const [key, buffered] of [...gigaTurnBuffers]) {
    if (buffered.cwd !== cwd) continue;
    gigaTurnBuffers.delete(key);
    if (buffered.turns.length) void ingestLoggedTurns(buffered.ctx, buffered.turns);
  }
}

// Read-only census of turns still waiting for a flush, for the callers that
// must report what an exit destroys. Buffered turns are a real casualty:
// closeGigaTransports drains them on a graceful shutdown, and a process.exit
// never reaches that door. Counts only - never turn content - and it mutates
// nothing, so reporting can never cost a flush.
export function gigaBufferedTurnCensus(): Array<{ session: string; cwd: string; turns: number }> {
  return [...gigaTurnBuffers]
    .filter(([, buffered]) => buffered.turns.length)
    .map(([key, buffered]) => ({
      session: key.slice(key.indexOf("\0") + 1),
      cwd: buffered.cwd,
      turns: buffered.turns.length,
    }));
}

export async function closeGigaTransports(): Promise<void> {
  const pending = [...gigaTurnBuffers.values()].filter((buffer) => buffer.turns.length);
  gigaTurnBuffers.clear();
  await Promise.allSettled(pending.map((buffer) => ingestLoggedTurns(buffer.ctx, buffer.turns)));
  gigaClosing = true;
  const closing = [...gigaTransports.values()].map((transport) => transport.close());
  gigaTransports.clear();
  await Promise.allSettled(closing);
}

export const __gigaTest = Object.freeze({
  isSubagentSessionContext,
  resetState() {
    gigaClosing = false;
    gigaTransports.clear();
    gigaTurnBuffers.clear();
  },
});

import { existsSync } from "node:fs";
// isSubagentSessionContext reaches for path.dirname inside a try/catch whose
// catch returns "this is a subagent". Without this import that call threw
// ReferenceError on every turn, the catch swallowed it, and every session
// looked like a subagent - so turn ingest was silently dead from b635264 until
// the restart door's production-seam test surfaced it.
import path from "node:path";

import { requestOrgan, OrganError, type OrganOperation } from "./house-proof/organ.ts";
import type { HostBinding } from "./house-proof/host.ts";
type JsonObject = Record<string, unknown>;
import { resolveJudgmentCredential } from "./house-proof/judgment-host.ts";

const GIGA_READ_TIMEOUT_MS = 120_000;

export type GigaEnablement = { gigaEnabled: boolean; hippocampusEnabled: boolean; replayMode: boolean };

export async function setGigaEnablement(binding: HostBinding, enablement: GigaEnablement, options: { signal?: AbortSignal; context?: any; status?: JsonObject } = {}): Promise<JsonObject> {
  const request = { signal: options.signal, write: true };
  const status = options.status ?? await requestOrgan(binding, "giga_set_enablement", enablement, request);

  if (!options.context || (status.credential_required !== true && status.classifier_unavailable !== "classifier_config_invalid")) return status;

  const credential = await resolveJudgmentCredential(options.context, binding, options.signal);

  if (!credential) return { ...status, classifier_unavailable: "credential_unavailable" };

  return requestOrgan(binding, "giga_set_enablement", { ...enablement, credential }, request);
}

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
  reviewer_id: string;
  operator_identity: string;
  authorization_basis: typeof GIGA_OMP_ROOM_BINDING;
  target: GigaPromotionTarget;
};
export type GigaPromotionResult = JsonObject;
export type GigaHealthResult = JsonObject & {
  capture_enabled: boolean;
  classifier_enabled: boolean;
  store_healthy: boolean;
  classifier: JsonObject & { consecutive_failures: number };
};
export type GigaQueueMaintenanceOperation = "check" | "purge_stuck";
export type GigaQueueMaintenanceResult = JsonObject;
export type GigaReviewRequest = {
  candidate_id: string;
  reviewer_id: string;
  new_state: GigaSafeReviewState;
  reason: string;
  authorization_basis: typeof GIGA_OMP_ROOM_BINDING;
};

export type GigaCaptureRequest = {
  gigaEnabled: boolean;
  hippocampusEnabled: boolean;
  replayMode: boolean;
  subagent: boolean;
  projectKeys: string[];
};
export type GigaCaptureReceipt = {
  status: "captured" | "partial" | "failed" | "skipped";
  reason: "replay" | "subagent" | "disabled" | "not_persisted" | null;
  submittedTurns: number;
  acceptedTurns: number;
  duplicateTurns: number;
  ledgerFailures: number;
  eventIds: string[];
  errors: Array<{ code: string; message: string; retryable: boolean; turns: number }>;
  classifierStatus: JsonObject | null;
};

// One credential hand-off at a time: a second one would restart the room worker.
let credentialBootstrap: Promise<void> | undefined;


async function requestObject(binding: HostBinding, method: OrganOperation, params: JsonObject, options: { signal?: AbortSignal; write?: boolean; timeoutMs?: number } = {}): Promise<JsonObject> {
  if (process.env.ATHANOR_GIGA_ENABLED !== "1") {
    throw new OrganError("GIGA is disabled", "giga_disabled", false, { enabled: false });
  }
  return requestOrgan(binding, method, params, options);
}

export function gigaTransportFailure(error: unknown): { code: string; message: string; retryable: boolean; details?: unknown } {
  if (error instanceof OrganError) {
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

export async function requestGigaCandidateList(binding: HostBinding, options: { reviewState?: string; limit?: number; signal?: AbortSignal } = {}): Promise<GigaCandidateListResult> {
  return await requestObject(binding, "giga_candidate_list", {
    review_state: options.reviewState ?? null,
    limit: options.limit ?? 50,
  }, { signal: options.signal, timeoutMs: GIGA_READ_TIMEOUT_MS }) as GigaCandidateListResult;
}

export async function requestGigaReview(binding: HostBinding, request: GigaReviewRequest, options: { signal?: AbortSignal } = {}): Promise<GigaReviewResult> {
  return await requestObject(binding, "giga_tool_review", request as unknown as JsonObject, { signal: options.signal, write: true }) as GigaReviewResult;
}

export async function requestGigaPromote(binding: HostBinding, request: GigaPromotionRequest, options: { signal?: AbortSignal } = {}): Promise<GigaPromotionResult> {
  return await requestObject(binding, "giga_tool_promote", request as unknown as JsonObject, { signal: options.signal, write: true });
}

export async function requestGigaHealth(binding: HostBinding, options: { signal?: AbortSignal } = {}): Promise<GigaHealthResult> {
  return await requestObject(binding, "giga_health", {}, { signal: options.signal, timeoutMs: GIGA_READ_TIMEOUT_MS }) as GigaHealthResult;
}

export async function requestGigaQueueMaintenance(binding: HostBinding, operation: GigaQueueMaintenanceOperation, options: { signal?: AbortSignal } = {}): Promise<GigaQueueMaintenanceResult> {
  return await requestObject(binding, "giga_queue_maintenance", { operation, scope: "room" }, { signal: options.signal, timeoutMs: GIGA_READ_TIMEOUT_MS, write: true });
}

export function isSubagentSessionContext(ctx: any): boolean {
  try {
    const sessionFile = ctx?.sessionManager?.getSessionFile?.();
    return typeof sessionFile !== "string" || !sessionFile || existsSync(`${path.dirname(sessionFile)}.jsonl`);
  } catch {
    return true;
  }
}

// Every capture states its GIGA switches. Replay and subagent sessions opt out
// explicitly; the Host never infers either one.
export function gigaCaptureRequest(ctx: any): GigaCaptureRequest {
  return {
    gigaEnabled: process.env.ATHANOR_GIGA_ENABLED === "1",
    hippocampusEnabled: process.env.ATHANOR_HIPPOCAMPUS_ENABLED === "1",
    replayMode: process.env.ATHANOR_REPLAY_MODE === "1",
    subagent: isSubagentSessionContext(ctx),
    projectKeys: process.env.ATHANOR_GIGA_PROJECT_KEY ? [process.env.ATHANOR_GIGA_PROJECT_KEY] : [],
  };
}

// The Host persisted or refused the window's turn jobs before it replied, so
// nothing waits here. OMP still owns the classifier credential: only it can
// read the Typesafe key. Returns the warning for a partial or failed capture.
export function settleGigaCapture(binding: HostBinding, ctx: any, receipt: GigaCaptureReceipt | null | undefined): string | null {
  if (!receipt) return null;
  if (receipt.classifierStatus?.credential_required === true && !credentialBootstrap) {
    const enablement: GigaEnablement = {
      gigaEnabled: true,
      hippocampusEnabled: process.env.ATHANOR_HIPPOCAMPUS_ENABLED === "1",
      replayMode: false,
    };
    credentialBootstrap = setGigaEnablement(binding, enablement, { context: ctx, status: receipt.classifierStatus })
      .then(() => undefined, (error) => {
        console.warn("[athanor] GIGA captured turns; classifier credential setup failed.", {
          code: gigaTransportFailure(error).code,
        });
      })
      .finally(() => {
        credentialBootstrap = undefined;
      });
  }
  if (receipt.status !== "partial" && receipt.status !== "failed") return null;

  const causes = receipt.errors.map((error) => `${error.code} x${error.turns}`);
  if (receipt.ledgerFailures > 0) causes.push(`giga_source_ledger x${receipt.ledgerFailures}`);
  const missed = receipt.errors.reduce((sum, error) => sum + error.turns, receipt.ledgerFailures);
  const warning = `GIGA capture ${receipt.status}: ${missed} turn(s) not queued (${causes.join(", ")})`;
  console.warn(`[athanor] ${warning}`);
  return warning;
}

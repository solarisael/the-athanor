import {
  decodeHostFailure, failureDetails, failureRetryable, hostCommand, HostRefused, HostUnavailable, sendHostCommand,
  type HostBinding,
} from "./host.ts";

export type OrganOperation =
  | "canon_write" | "canon_read" | "remember" | "paper_boat_sleep" | "paper_boat_wake"
  | "memory_thread_join" | "memory_thread_leave" | "memory_amend"
  | "recall" | "vault_recall" | "anamnesis" | "anamnesis_write"
  | "lesson_query" | "lesson_update" | "lesson_delete" | "lesson_trigger_match" | "lesson_trigger_record"
  | "design_document_query" | "design_document_write" | "entity_resolve"
  | "hallway_create" | "hallway_join" | "hallway_post" | "hallway_read" | "hallway_inbox"
  | "hallway_knock_policy" | "hallway_knock"
  | "quest_post" | "quest_board" | "quest_claim" | "quest_report" | "quest_evidence"
  | "restart_request" | "restart_transition" | "restart_verify" | "restart_status"
  | "giga_set_enablement" | "giga_candidate_list" | "giga_tool_review" | "giga_tool_promote"
  | "giga_health" | "giga_queue_maintenance" | "substrate_health"
  | "orihon" | "unfold" | "orihon_capture" | "orihon_mode" | "orihon_turn" | "orihon_zoom" | "orihon_date"
  | "worker_lifecycle";

export class OrganError extends Error {
  constructor(
    message: string,
    readonly code: string,
    readonly retryable: boolean,
    readonly details?: unknown,
  ) {
    super(message);
    this.name = "OrganError";
  }
}

const RESULT = new Set(["athanor.organ.result"]);

export async function requestOrgan(
  binding: HostBinding,
  operation: OrganOperation,
  params: Record<string, unknown>,
  { signal, timeoutMs = 120_000, write = false, targetScope = "room" }: { signal?: AbortSignal; timeoutMs?: number; write?: boolean; targetScope?: "room" | "house" } = {},
): Promise<Record<string, unknown>> {
  const command = hostCommand(binding, "athanor.organ.call", "organ", {
    organ_request: { operation, params, target_scope: targetScope },
  });
  let response;
  try {
    response = await sendHostCommand(command, RESULT, signal, timeoutMs, { settleDefinitively: write });
  } catch (error) {
    if (error instanceof HostRefused) {
      throw new OrganError(error.message, error.code, error.retryable, {
        ...error.details,
        execution: {
          request_dispatched: error.dispatched,
          write_outcome: "not_started",
          retry: error.retryable ? "safe_now" : "after_change",
        },
      });
    }
    if (error instanceof HostUnavailable) {
      if (write && error.dispatched) {
        throw new OrganError(`Native ${operation} outcome is unknown after dispatch`, "outcome_unknown", false, {
          execution: { request_dispatched: true, write_outcome: "unknown", retry: "reconcile_first" },
          cause: error.message,
        });
      }
      throw new OrganError(error.message, error.code, true, {
        execution: { request_dispatched: error.dispatched, write_outcome: "not_started", retry: "safe_now" },
      });
    }
    throw error;
  }
  if (response.failure !== undefined) {
    const failure = decodeHostFailure(response.failure);
    if (failure) throw new OrganError(failure.message, failure.code, failureRetryable(failure), failureDetails(failure));
  } else if (response.result && typeof response.result === "object" && !Array.isArray(response.result)) {
    return response.result as Record<string, unknown>;
  }
  throw new OrganError(`Native ${operation} returned an invalid response`, write ? "outcome_unknown" : "invalid_response", !write, {
    execution: { request_dispatched: true, write_outcome: write ? "unknown" : "not_started", retry: write ? "reconcile_first" : "safe_now" },
  });
}

export function organFailure(error: unknown): Record<string, unknown> {
  if (error instanceof OrganError) {
    return {
      ok: false, error: error.message, code: error.code, retryable: error.retryable,
      ...(error.details === undefined ? {} : { details: error.details }),
      ...(error.code === "outcome_unknown" ? { outcome: "unknown" } : {}),
    };
  }
  return { ok: false, error: error instanceof Error ? error.message : String(error) };
}

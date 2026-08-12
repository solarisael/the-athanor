import {
  HostUnavailable,
  hostCommand,
  sendHostCommand as sendRawHostCommand,
  type HostBinding,
  type HostCommand,
  type HostResponse,
} from "./host.ts";

export const RECALL_POLICY_MODES = ["auto", "conversation", "work", "quiet"] as const;
export type RequestedRecallMode = typeof RECALL_POLICY_MODES[number];
export type ResolvedRecallMode = "conversation" | "work" | "mixed" | "quiet";
export type PersistedRecallPolicy = {
  requestedMode: RequestedRecallMode;
  resolvedMode: ResolvedRecallMode;
  activeProject: string | null;
  resolutionReason: string;
  lastRefreshReason: string | null;
  lastRefreshAt: string | null;
  workingSetEntries: number;
  recoveryPending: boolean;
  recoveryTerms: string[];
  degraded: string | null;
  updatedAt: string | null;
};
export type RecallPolicyDecision = {
  shouldRecall: boolean;
  clearWorkingSet: boolean;
  query: string;
  queryTerms: string[];
  refreshReason: string | null;
  intent: string;
  resolvedMode: ResolvedRecallMode;
};
export type RecallPolicyHostSnapshot = {
  recallPolicy: PersistedRecallPolicy;
  version: number;
  sequence: number;
  stateHash: string;
};
type QueryRoute = {
  intent: string;
  terms: string[];
  requiredTerms: string[];
  recognizedEntities: string[];
};
type SnapshotEvent = HostResponse & {
  state: PersistedRecallPolicy;
  version: number;
  sequence: number;
  state_hash: string;
  decision?: RecallPolicyDecision;
};

const COMMAND_ACCEPTED = "athanor.recall_policy.command_accepted";
const POLICY_SNAPSHOT = "athanor.recall_policy.snapshot";

export class RecallPolicyHostUnavailable extends Error {
  readonly code = "recall_policy_host_unavailable";
  constructor(message: string) {
    super(message);
    this.name = "RecallPolicyHostUnavailable";
  }
}

function commandEnvelope(
  binding: HostBinding,
  commandType: string,
  payload: Record<string, unknown> = {},
  idempotencyKey?: unknown,
) {
  return hostCommand(binding, commandType, "recall_policy", payload, idempotencyKey);
}

async function send(command: HostCommand): Promise<SnapshotEvent> {
  try {
    return await sendRawHostCommand(command, new Set([COMMAND_ACCEPTED, POLICY_SNAPSHOT])) as SnapshotEvent;
  } catch (error) {
    if (error instanceof HostUnavailable) throw new RecallPolicyHostUnavailable(error.message);
    throw error;
  }
}

function snapshot(event: SnapshotEvent): RecallPolicyHostSnapshot {
  return {
    recallPolicy: event.state,
    version: event.version,
    sequence: event.sequence,
    stateHash: event.state_hash,
  };
}

export class RecallPolicyHostClient {
  constructor(readonly binding: HostBinding) {}

  async inspect(): Promise<RecallPolicyHostSnapshot> {
    return snapshot(await send(commandEnvelope(this.binding, "athanor.recall_policy.subscribe")));
  }

  async setRequestedMode(requestedMode: RequestedRecallMode, idempotencyKey?: unknown) {
    const current = await this.inspect();
    const event = await send(commandEnvelope(
      this.binding,
      "athanor.recall_policy.set_requested_mode",
      {
        base_version: current.version,
        mutations: [{ mutation_type: "field_update", field: "requested_mode", value: requestedMode }],
      },
      idempotencyKey,
    ));
    return snapshot(event);
  }

  async evaluate(input: {
    queryRoute: QueryRoute;
    activeProject?: string | null;
    conversationTokens?: number;
    workingSetPresent: boolean;
    idempotencyKey?: unknown;
  }): Promise<{ decision: RecallPolicyDecision; snapshot: RecallPolicyHostSnapshot }> {
    const event = await send(commandEnvelope(
      this.binding,
      "athanor.recall_policy.evaluate",
      {
        facts: {
          query_route: {
            intent: input.queryRoute.intent,
            terms: input.queryRoute.terms,
            required_terms: input.queryRoute.requiredTerms,
            recognized_entities: input.queryRoute.recognizedEntities,
          },
          active_project: input.activeProject ?? null,
          conversation_tokens: input.conversationTokens ?? 0,
          working_set_present: input.workingSetPresent,
        },
      },
      input.idempotencyKey,
    ));
    return { decision: event.decision!, snapshot: snapshot(event) };
  }

  async completeRefresh(input: {
    queryTerms: string[];
    refreshReason: string;
    entries: number;
    hasWorkingSet: boolean;
    warning?: string;
    idempotencyKey?: unknown;
  }) {
    return snapshot(await send(commandEnvelope(
      this.binding,
      "athanor.recall_policy.complete_refresh",
      {
        refresh: {
          query_terms: input.queryTerms,
          refresh_reason: input.refreshReason,
          entries: input.entries,
          has_working_set: input.hasWorkingSet,
          warning: input.warning ?? null,
        },
      },
      input.idempotencyKey,
    )));
  }

  async failRefresh(reason: unknown, idempotencyKey?: unknown) {
    return snapshot(await send(commandEnvelope(
      this.binding,
      "athanor.recall_policy.fail_refresh",
      { failure_reason: String(reason ?? "") },
      idempotencyKey,
    )));
  }

  async invalidateAfterCompaction(summary: unknown, idempotencyKey?: unknown) {
    return snapshot(await send(commandEnvelope(
      this.binding,
      "athanor.recall_policy.invalidate_after_compaction",
      { compaction_summary: String(summary ?? "") },
      idempotencyKey,
    )));
  }
}

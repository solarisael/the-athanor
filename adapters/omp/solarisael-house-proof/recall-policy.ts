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

// --- hands-on-files evidence -------------------------------------------------
// A session that has edited or written is working, whatever the prompt sounds
// like. This module only remembers the fact and carries it to the Host on the
// next evaluate; the resolution itself stays the Host's judgment.
//
// enough: evidence holds for the session's lifetime; a decay window is the door if work mode overstays casual evenings.

const TOOL_EVIDENCE_SESSION_LIMIT = 256;
const MUTATE_TOOLS = new Set(["edit", "write"]);
const toolEvidenceSessions = new Set<string>();

function evidenceKey(binding: HostBinding): string {
  const room = String(binding?.room ?? "").trim();
  const session = String(binding?.session ?? "").trim();
  return room && session ? `${room}\0${session}` : "";
}

/** Single authority for which tool calls count as hands on files. */
export function isMutateTool(toolName: unknown): boolean {
  return MUTATE_TOOLS.has(String(toolName ?? "").trim());
}

export function markToolEvidence(binding: HostBinding): void {
  const key = evidenceKey(binding);
  if (!key) return;
  // Re-marking refreshes recency, so the session actually working is the last
  // one this bound stash forgets.
  toolEvidenceSessions.delete(key);
  toolEvidenceSessions.add(key);
  if (toolEvidenceSessions.size > TOOL_EVIDENCE_SESSION_LIMIT) {
    const oldest = toolEvidenceSessions.values().next();
    if (!oldest.done) toolEvidenceSessions.delete(oldest.value);
  }
}

export function hasToolEvidence(binding: HostBinding): boolean {
  const key = evidenceKey(binding);
  return key ? toolEvidenceSessions.has(key) : false;
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
    toolEvidence?: boolean;
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
          // Sent only when true: a Host built before this field refuses unknown
          // fact keys, and no evidence is exactly its existing behavior.
          ...(input.toolEvidence ? { tool_evidence: true } : {}),
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

import { createHash } from "node:crypto";
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

type HostBinding = {
  room: string;
  spirit: string;
  session: string;
};

type QueryRoute = {
  intent?: unknown;
  terms?: unknown;
  requiredTerms?: unknown;
  recognizedEntities?: unknown;
};

const HOST_SCHEMA_VERSION = 1;
const DEFAULT_HOST_WS_URL = "ws://127.0.0.1:8787/athanor/v1/ws";
const HOST_TIMEOUT_MS = 3_000;
const COMMAND_ACCEPTED = "athanor.recall_policy.command_accepted";
const POLICY_SNAPSHOT = "athanor.recall_policy.snapshot";
const COMMAND_REFUSED = "athanor.recall_policy.command_refused";
const COMMAND_FAILED = "athanor.recall_policy.command_failed";

export class RecallPolicyHostUnavailable extends Error {
  readonly code = "recall_policy_host_unavailable";

  constructor(message: string) {
    super(message);
    this.name = "RecallPolicyHostUnavailable";
  }
}

function normalizedText(value: unknown): string {
  return String(value ?? "").trim();
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.map(normalizedText).filter(Boolean)
    : [];
}

export function isRequestedRecallMode(value: unknown): value is RequestedRecallMode {
  return typeof value === "string" && RECALL_POLICY_MODES.includes(value as RequestedRecallMode);
}

export function defaultPersistedRecallPolicy(): PersistedRecallPolicy {
  return {
    requestedMode: "auto",
    resolvedMode: "conversation",
    activeProject: null,
    resolutionReason: "default",
    lastRefreshReason: null,
    lastRefreshAt: null,
    workingSetEntries: 0,
    recoveryPending: false,
    recoveryTerms: [],
    degraded: null,
    updatedAt: null,
  };
}

export function normalizePersistedRecallPolicy(value: unknown): PersistedRecallPolicy {
  const source = value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
  const defaults = defaultPersistedRecallPolicy();
  const requested = source.requestedMode ?? source.requested_mode;
  const resolved = normalizedText(source.resolvedMode ?? source.resolved_mode).toLowerCase();
  const workingSetEntries = source.workingSetEntries ?? source.working_set_entries;
  return {
    requestedMode: isRequestedRecallMode(requested) ? requested : defaults.requestedMode,
    resolvedMode: ["conversation", "work", "mixed", "quiet"].includes(resolved)
      ? resolved as ResolvedRecallMode
      : defaults.resolvedMode,
    activeProject: normalizedText(source.activeProject ?? source.active_project) || null,
    resolutionReason: normalizedText(source.resolutionReason ?? source.resolution_reason) || defaults.resolutionReason,
    lastRefreshReason: normalizedText(source.lastRefreshReason ?? source.last_refresh_reason) || null,
    lastRefreshAt: normalizedText(source.lastRefreshAt ?? source.last_refresh_at) || null,
    workingSetEntries: Number.isInteger(workingSetEntries) && Number(workingSetEntries) >= 0
      ? Number(workingSetEntries)
      : 0,
    recoveryPending: (source.recoveryPending ?? source.recovery_pending) === true,
    recoveryTerms: stringArray(source.recoveryTerms ?? source.recovery_terms).slice(0, 12),
    degraded: normalizedText(source.degraded) || null,
    updatedAt: normalizedText(source.updatedAt ?? source.updated_at) || null,
  };
}

function normalizeDecision(value: unknown): RecallPolicyDecision {
  const source = value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
  const resolvedMode = normalizedText(source.resolved_mode);
  if (!["conversation", "work", "mixed", "quiet"].includes(resolvedMode)) {
    throw new RecallPolicyHostUnavailable("Recall Policy Host returned an invalid decision mode");
  }
  return {
    shouldRecall: source.should_recall === true,
    clearWorkingSet: source.clear_working_set === true,
    query: normalizedText(source.query),
    queryTerms: stringArray(source.query_terms),
    refreshReason: normalizedText(source.refresh_reason) || null,
    intent: normalizedText(source.intent) || "general",
    resolvedMode: resolvedMode as ResolvedRecallMode,
  };
}

function commandEnvelope(
  binding: HostBinding,
  commandType: string,
  payload: Record<string, unknown> = {},
  idempotencyKey?: unknown,
) {
  const requestedKey = normalizedText(idempotencyKey);
  const stableKey = requestedKey
    ? `recall-policy:${createHash("sha256").update(requestedKey).digest("hex")}`
    : crypto.randomUUID();
  const messageId = stableKey;
  const now = new Date();
  return {
    schema_version: HOST_SCHEMA_VERSION,
    message_id: messageId,
    house_id: requiredEnvironment("ATHANOR_HOST_HOUSE_ID"),
    sender_room: binding.room,
    sender_spirit: binding.spirit,
    sender_session: binding.session,
    recipient: normalizedText(process.env.ATHANOR_HOST_RECIPIENT) || "house-host",
    command_or_event_type: commandType,
    correlation_id: messageId,
    causation_id: "",
    reply_target: binding.session,
    idempotency_key: stableKey,
    source_record_refs: [],
    scope: `room:${binding.room}:recall_policy`,
    visibility: "operator",
    authority_class: "room_state",
    created_at: now.toISOString(),
    expires_at: new Date(now.getTime() + 30_000).toISOString(),
    max_hops: 1,
    projection_id: "recall_policy",
    ...payload,
  };
}

function requiredEnvironment(name: string): string {
  const value = normalizedText(process.env[name]);
  if (!value) {
    throw new RecallPolicyHostUnavailable(`${name} is required for Recall Policy Host access`);
  }
  return value;
}

function hostUrlForCommand(command: Record<string, unknown>): string {
  const override = normalizedText(process.env.ATHANOR_HOST_WS_URL);
  if (override) return override;
  const configured = normalizedText(process.env.ATHANOR_HOST_ENDPOINTS);
  if (!configured) return DEFAULT_HOST_WS_URL;
  let endpoints: unknown;
  try {
    endpoints = JSON.parse(configured);
  } catch {
    throw new RecallPolicyHostUnavailable("ATHANOR_HOST_ENDPOINTS is not valid JSON");
  }
  const room = normalizedText(command.sender_room);
  const entry = endpoints && typeof endpoints === "object" && !Array.isArray(endpoints)
    ? (endpoints as Record<string, unknown>)[room]
    : null;
  const url = entry && typeof entry === "object" && !Array.isArray(entry)
    ? normalizedText((entry as Record<string, unknown>).url)
    : "";
  if (!url) {
    throw new RecallPolicyHostUnavailable(`no installed Recall Policy Host endpoint exists for room ${room || "<empty>"}`);
  }
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    throw new RecallPolicyHostUnavailable(`installed Recall Policy Host endpoint for room ${room} is invalid`);
  }
  if (parsed.protocol !== "ws:" || !["127.0.0.1", "localhost", "[::1]"].includes(parsed.hostname)) {
    throw new RecallPolicyHostUnavailable(`installed Recall Policy Host endpoint for room ${room} must be loopback WebSocket`);
  }
  return url;
}

async function sendHostCommand(command: Record<string, unknown>): Promise<Record<string, any>> {
  const url = hostUrlForCommand(command);
  const token = requiredEnvironment("ATHANOR_HOST_TOKEN");
  return await new Promise<Record<string, any>>((resolve, reject) => {
    let settled = false;
    let socket: WebSocket;
    const finish = (error: unknown, value?: Record<string, any>) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      try {
        socket?.close();
      } catch {
        // The Host connection may already be gone.
      }
      if (error) reject(error);
      else resolve(value!);
    };
    const timeout = setTimeout(() => {
      finish(new RecallPolicyHostUnavailable(`Recall Policy Host timed out after ${HOST_TIMEOUT_MS}ms`));
    }, HOST_TIMEOUT_MS);
    try {
      const WebSocketConstructor = WebSocket as any;
      socket = new WebSocketConstructor(url, {
        headers: { Authorization: `Bearer ${token}` },
      });
    } catch (error) {
      finish(new RecallPolicyHostUnavailable(`Recall Policy Host connection failed: ${normalizedText(error)}`));
      return;
    }
    socket.addEventListener("open", () => socket.send(JSON.stringify(command)));
    socket.addEventListener("error", () => {
      finish(new RecallPolicyHostUnavailable(`Recall Policy Host is unavailable at ${url}`));
    });
    socket.addEventListener("close", () => {
      finish(new RecallPolicyHostUnavailable("Recall Policy Host closed before replying"));
    });
    socket.addEventListener("message", (event) => {
      let response: Record<string, any>;
      try {
        response = JSON.parse(String(event.data));
      } catch {
        finish(new RecallPolicyHostUnavailable("Recall Policy Host returned malformed JSON"));
        return;
      }
      if (response.correlation_id !== command.message_id) return;
      const kind = normalizedText(response.command_or_event_type);
      if (kind === COMMAND_REFUSED || kind === COMMAND_FAILED) {
        finish(new RecallPolicyHostUnavailable(
          normalizedText(response.reason) || `Recall Policy Host ${kind.endsWith("refused") ? "refused" : "failed"} the command`,
        ));
        return;
      }
      if (kind !== COMMAND_ACCEPTED && kind !== POLICY_SNAPSHOT) return;
      finish(null, response);
    });
  });
}

function snapshotFromEvent(event: Record<string, any>): RecallPolicyHostSnapshot {
  const state = event.state;
  if (!state) throw new RecallPolicyHostUnavailable("Recall Policy Host response omitted state");
  const version = Number(event.version);
  const sequence = Number(event.sequence);
  return {
    recallPolicy: normalizePersistedRecallPolicy(state),
    version: Number.isSafeInteger(version) ? version : 0,
    sequence: Number.isSafeInteger(sequence) ? sequence : 0,
    stateHash: normalizedText(event.state_hash),
  };
}

export class RecallPolicyHostClient {
  readonly binding: HostBinding;

  constructor(binding: HostBinding) {
    this.binding = {
      room: normalizedText(binding.room),
      spirit: normalizedText(binding.spirit),
      session: normalizedText(binding.session),
    };
    if (!this.binding.room || !this.binding.spirit || !this.binding.session) {
      throw new RecallPolicyHostUnavailable("Recall Policy Host binding requires room, spirit, and session");
    }
  }

  async inspect(): Promise<RecallPolicyHostSnapshot> {
    const event = await sendHostCommand(commandEnvelope(
      this.binding,
      "athanor.recall_policy.subscribe",
    ));
    return snapshotFromEvent(event);
  }

  async setRequestedMode(
    requestedMode: RequestedRecallMode,
    idempotencyKey?: unknown,
  ): Promise<RecallPolicyHostSnapshot> {
    if (!isRequestedRecallMode(requestedMode)) {
      throw new RecallPolicyHostUnavailable(`unknown Recall Policy mode: ${normalizedText(requestedMode)}`);
    }
    const current = await this.inspect();
    if (current.recallPolicy.requestedMode === requestedMode) return current;
    const event = await sendHostCommand(commandEnvelope(
      this.binding,
      "athanor.recall_policy.set_requested_mode",
      {
        base_version: current.version,
        mutations: [{
          mutation_type: "field_update",
          field: "requested_mode",
          value: requestedMode,
        }],
      },
      `set:${this.binding.session}:${current.version}:${normalizedText(idempotencyKey) || requestedMode}`,
    ));
    return snapshotFromEvent(event);
  }

  async evaluate(input: {
    queryRoute: QueryRoute;
    activeProject?: unknown;
    conversationTokens?: unknown;
    workingSetPresent: boolean;
    idempotencyKey?: unknown;
  }): Promise<{ decision: RecallPolicyDecision; snapshot: RecallPolicyHostSnapshot }> {
    const event = await sendHostCommand(commandEnvelope(
      this.binding,
      "athanor.recall_policy.evaluate",
      {
        facts: {
          query_route: {
            intent: normalizedText(input.queryRoute.intent),
            terms: stringArray(input.queryRoute.terms),
            required_terms: stringArray(input.queryRoute.requiredTerms),
            recognized_entities: stringArray(input.queryRoute.recognizedEntities),
          },
          active_project: normalizedText(input.activeProject) || null,
          conversation_tokens: Math.max(0, Math.floor(Number(input.conversationTokens) || 0)),
          working_set_present: input.workingSetPresent,
        },
      },
      `evaluate:${this.binding.session}:${normalizedText(input.idempotencyKey) || crypto.randomUUID()}`,
    ));
    if (!event.decision) {
      throw new RecallPolicyHostUnavailable("Recall Policy Host response omitted its decision");
    }
    return {
      decision: normalizeDecision(event.decision),
      snapshot: snapshotFromEvent(event),
    };
  }

  async completeRefresh(input: {
    queryTerms: string[];
    refreshReason: string;
    entries: number;
    hasWorkingSet: boolean;
    warning?: unknown;
    idempotencyKey?: unknown;
  }): Promise<RecallPolicyHostSnapshot> {
    const event = await sendHostCommand(commandEnvelope(
      this.binding,
      "athanor.recall_policy.complete_refresh",
      {
        refresh: {
          query_terms: stringArray(input.queryTerms),
          refresh_reason: normalizedText(input.refreshReason),
          entries: Math.max(0, Math.floor(Number(input.entries) || 0)),
          has_working_set: input.hasWorkingSet,
          warning: normalizedText(input.warning) || null,
        },
      },
      `complete:${this.binding.session}:${normalizedText(input.idempotencyKey) || crypto.randomUUID()}`,
    ));
    return snapshotFromEvent(event);
  }

  async failRefresh(reason: unknown, idempotencyKey?: unknown): Promise<RecallPolicyHostSnapshot> {
    const event = await sendHostCommand(commandEnvelope(
      this.binding,
      "athanor.recall_policy.fail_refresh",
      { failure_reason: normalizedText(reason) || "recall unavailable" },
      `failed:${this.binding.session}:${normalizedText(idempotencyKey) || crypto.randomUUID()}`,
    ));
    return snapshotFromEvent(event);
  }

  async invalidateAfterCompaction(
    summary: unknown,
    idempotencyKey?: unknown,
  ): Promise<RecallPolicyHostSnapshot> {
    const event = await sendHostCommand(commandEnvelope(
      this.binding,
      "athanor.recall_policy.invalidate_after_compaction",
      { compaction_summary: normalizedText(summary) },
      `compact:${this.binding.session}:${normalizedText(idempotencyKey) || crypto.randomUUID()}`,
    ));
    return snapshotFromEvent(event);
  }
}

function boundedWarning(value: unknown): string {
  return normalizedText(value).slice(0, 240);
}

function compactCandidate(candidate: Record<string, unknown>) {
  return {
    source_path: candidate.source_path,
    title: candidate.title,
    heading_path: candidate.heading_path,
    memory_id: candidate.memory_id,
    score: candidate.score,
    term_coverage: candidate.term_coverage,
    matched_terms: Array.isArray(candidate.matched_terms) ? candidate.matched_terms.slice(0, 4) : [],
    reasons: Array.isArray(candidate.reasons) ? candidate.reasons.slice(0, 3) : [],
    excerpt: normalizedText(candidate.excerpt).slice(0, 480),
  };
}

export function compactAutomaticRecallPayload(input: {
  source?: unknown;
  warnings?: unknown;
  retrievalCandidates?: unknown;
  canonMatches?: unknown;
  dateMatches?: unknown;
}) {
  const candidates = Array.isArray(input.retrievalCandidates)
    ? input.retrievalCandidates.filter((entry) => entry && typeof entry === "object").slice(0, 2).map((entry) => compactCandidate(entry as Record<string, unknown>))
    : [];
  const canonMatches = Array.isArray(input.canonMatches)
    ? input.canonMatches.filter((entry) => entry && typeof entry === "object").slice(0, 2).map((entry: Record<string, unknown>) => ({
      termKey: entry.termKey,
      type: entry.type,
      summary: normalizedText(entry.summary).slice(0, 480),
    }))
    : [];
  const dateMatches = Array.isArray(input.dateMatches)
    ? input.dateMatches.filter((entry) => entry && typeof entry === "object").slice(0, 2).map((entry: Record<string, unknown>) => ({
      source_path: entry.source_path,
      title: entry.title,
      dates: Array.isArray(entry.dates) ? entry.dates.slice(0, 4) : [],
      body_excerpt: normalizedText(entry.body_excerpt).slice(0, 480),
    }))
    : [];
  const warnings = Array.isArray(input.warnings) ? input.warnings.map(boundedWarning).filter(Boolean).slice(0, 2) : [];
  return {
    ok: true,
    source: input.source,
    found: Boolean(candidates.length || canonMatches.length || dateMatches.length),
    warnings,
    canonMatches,
    retrievalCandidates: candidates,
    dateMatches,
  };
}

import { existsSync } from "node:fs";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
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

// A mutate tool marks work mode and records only enough path state to name the
// active repository. Lesson selection no longer rides this evidence.
//
// enough: evidence holds for the session's lifetime; add decay if work mode overstays casual use.

const TOOL_EVIDENCE_SESSION_LIMIT = 256;
const EVIDENCE_DIR_LIMIT = 8;
const MUTATE_TOOLS = new Set(["edit", "write"]);
const EDIT_SECTION_HEADER = /^\[([^#\r\n]+)#[0-9A-F]{4}\]$/;
const INTERNAL_URI = /^[a-z][a-z0-9+.-]*:\/\//i;

type SessionWorkEvidence = { dirs: Set<string> };
const toolEvidenceSessions = new Map<string, SessionWorkEvidence>();

export type ToolTouch = { paths?: string[]; cwd?: string };

function evidenceKey(binding: HostBinding): string {
  const room = String(binding?.room ?? "").trim();
  const session = String(binding?.session ?? "").trim();
  return room && session ? `${room}\0${session}` : "";
}

/** Single authority for which tool calls count as hands on files. */
export function isMutateTool(toolName: unknown): boolean {
  return MUTATE_TOOLS.has(String(toolName ?? "").trim());
}

export function mutateToolPaths(toolName: unknown, input: unknown): string[] {
  const name = String(toolName ?? "").trim();
  const args = input && typeof input === "object" && !Array.isArray(input)
    ? input as Record<string, unknown>
    : {};
  if (name === "write") {
    const filePath = String(args.path ?? "").trim();
    if (!filePath || (INTERNAL_URI.test(filePath) && !/^[A-Za-z]:[\\/]/.test(filePath))) return [];
    return [filePath];
  }
  if (name !== "edit" || typeof args.input !== "string") return [];
  const paths: string[] = [];
  for (const line of args.input.split(/\r?\n/)) {
    const match = EDIT_SECTION_HEADER.exec(line.trim());
    if (match && !paths.includes(match[1])) paths.push(match[1]);
  }
  return paths;
}

export function markToolEvidence(binding: HostBinding, touch?: ToolTouch): void {
  const key = evidenceKey(binding);
  if (!key) return;
  const evidence = toolEvidenceSessions.get(key) ?? { dirs: new Set<string>() };
  toolEvidenceSessions.delete(key);
  toolEvidenceSessions.set(key, evidence);
  if (toolEvidenceSessions.size > TOOL_EVIDENCE_SESSION_LIMIT) {
    const oldest = toolEvidenceSessions.keys().next();
    if (!oldest.done) toolEvidenceSessions.delete(oldest.value);
  }
  const cwd = String(touch?.cwd ?? "").trim();
  for (const raw of touch?.paths ?? []) {
    const filePath = String(raw ?? "").trim();
    if (!filePath) continue;
    if ((isAbsolute(filePath) || cwd) && evidence.dirs.size < EVIDENCE_DIR_LIMIT) {
      evidence.dirs.add(dirname(resolve(cwd || ".", filePath)));
    }
  }
}

export function hasToolEvidence(binding: HostBinding): boolean {
  const key = evidenceKey(binding);
  return key ? toolEvidenceSessions.has(key) : false;
}

// Worktrees keep `.git` as a file, not a directory; existsSync covers both.
const repoNameCache = new Map<string, string | null>();

function repoName(startDir: string): string | null {
  const cached = repoNameCache.get(startDir);
  if (cached !== undefined) return cached;
  let name: string | null = null;
  let dir = startDir;
  for (let depth = 0; depth < 12; depth += 1) {
    if (existsSync(join(dir, ".git"))) {
      name = basename(dir).toLowerCase() || null;
      break;
    }
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  if (repoNameCache.size > 256) repoNameCache.clear();
  repoNameCache.set(startDir, name);
  return name;
}

export function activeProjectFromEvidence(binding: HostBinding): string | null {
  const key = evidenceKey(binding);
  const evidence = key ? toolEvidenceSessions.get(key) : undefined;
  if (!evidence) return null;
  for (const dir of [...evidence.dirs].reverse()) {
    const project = repoName(dir);
    if (project) return project;
  }
  return null;
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

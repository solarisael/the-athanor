import { readFileSync } from "node:fs";
import path from "node:path";
import { HostUnavailable, hostCommand, sendHostCommand, type HostBinding } from "./host.ts";
import { AUTOMATIC_CONTEXT_IO_TIMEOUT_MS } from "./constants.ts";

export const CONTEXT_BLOCK_TYPES = {
  "room-context": "athanor-room-context",
  "routing-mode": "athanor-routing-mode",
  "wake-context": "athanor-wake-context",
  "anamnesis-wake": "athanor-anamnesis-wake",
  "keyword-directive": "athanor-keyword-directive",
  "hallway-bell": "athanor-hallway-bell",
  "recall-context": "athanor-recall-context",
  "presence-context": "athanor-presence-context",
} as const;

export type ContextBlockKind = keyof typeof CONTEXT_BLOCK_TYPES;
export type ContextBlock = { kind: ContextBlockKind; content: string; details: Record<string, unknown> | null; timestamp: number };
export type PreparedTurn = { turnId: string; blocks: ContextBlock[] };
export type NativeLessonPlan = {
  token: string;
  requiresCredential: boolean;
  turnId: string;
  rules: Array<{ rule: Record<string, unknown>; block: boolean }>;
  warnings: string[];
};
export type PreparedContext = {
  turns: PreparedTurn[];
  replayed: boolean;
  nativeOwned: boolean;
  adoptionRejection?: "unpaired-surrogate" | null;
  adoption: "native" | "preserved" | "rebuilt";
  invalidationReason: string | null;
  activities: string[];
  warnings: string[];
  coverage?: Record<string, Record<string, unknown>>;
  presenceSettlement?: { contractId: string; directiveIds: string[]; nonemptyGuardId: string | null } | null;
  presenceSettledContractId?: string | null;
};
export type ContextPrepareRequest = {
  turnId: string;
  prompt: string;
  nativeUser: boolean;
  visibleTurnIds: string[];
  history: Array<{ kind: "user" | "assistant" | "generated"; text: string; turnId: string | null }>;
  existingBlockKinds: ContextBlockKind[];
  contextCharacters: number;
  userTurnOrdinal: number;
  freshConversation: boolean;
  activeProject: string | null;
  toolEvidenceRevision: number;
  toolEvidenceEpoch: string;
  previousRequest?: { traceId: string; spanId: string; providerRequestId: string | null } | null;
  capabilities: { automaticRecall: boolean; topLevel: boolean };
  planToken: string;
  credential?: string;
  legacyMemo: { version: number; turns: PreparedTurn[] } | null;
  recallTelemetryOverride?: string;
  legacyRejection?: "unpaired-surrogate" | null;
  priorPresence: { frameId: string; frameRendered: string } | null;
};

export function contextBlockKind(customType: unknown): ContextBlockKind | undefined {
  return (Object.keys(CONTEXT_BLOCK_TYPES) as ContextBlockKind[]).find((kind) => CONTEXT_BLOCK_TYPES[kind] === customType);
}

function hasUnpairedUnicode(value: unknown): boolean {
  if (typeof value === "string") {
    return Array.from(value).some((character) => {
      const code = character.charCodeAt(0);
      return character.length === 1 && code >= 0xd800 && code <= 0xdfff;
    });
  }
  if (value && typeof value === "object") {
    return Object.entries(value).some(([key, child]) => hasUnpairedUnicode(key) || hasUnpairedUnicode(child));
  }
  return false;
}

type LegacyContextProposal =
  | { status: "absent" }
  | { status: "ready"; memo: { version: number; turns: PreparedTurn[] } }
  | { status: "rejected"; reason: "unpaired-surrogate" };

export function readLegacyContextProposal(roomDir: string, sessionKey: string, visibleTurns: Set<string>): LegacyContextProposal {
  const file = path.join(roomDir, ".omp", "runtime", "turn-additions", `${Bun.hash(sessionKey).toString(36)}.json`);
  let value: any;
  try {
    value = JSON.parse(readFileSync(file, "utf8"));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return { status: "absent" };
    throw error;
  }
  if (value?.version !== 1 || !value.turns || typeof value.turns !== "object" || Array.isArray(value.turns)) {
    throw new HostUnavailable("Unsupported legacy context memo");
  }
  const turns: PreparedTurn[] = [];
  for (const [turnId, additions] of Object.entries(value.turns)) {
    if (!visibleTurns.has(turnId)) continue;
    if (!Array.isArray(additions)) throw new HostUnavailable("Invalid legacy context turn");
    if (hasUnpairedUnicode(turnId) || hasUnpairedUnicode(additions)) {
      return { status: "rejected", reason: "unpaired-surrogate" };
    }
    const blocks = additions.map((addition): ContextBlock => {
      const customType = String(addition?.customType ?? "").replace(/^solarisael-/, "athanor-");
      const kind = contextBlockKind(customType);
      if (!kind || typeof addition.content !== "string" || !Number.isFinite(addition.timestamp)) {
        throw new HostUnavailable("Invalid legacy context block");
      }
      return { kind, content: addition.content, details: addition.details ?? null, timestamp: addition.timestamp };
    });
    turns.push({ turnId, blocks });
  }
  // The legacy file remains provenance. Only the Host writes prepared turns.
  return { status: "ready", memo: { version: 1, turns } };
}

export async function planContextLessons(binding: HostBinding, request: {
  turnId: string; activeProject: string | null; managerAvailable: boolean; deadline: number;
  nativeUser: boolean; capabilities: ContextPrepareRequest["capabilities"];
}, signal?: AbortSignal): Promise<NativeLessonPlan> {
  const response = await sendHostCommand(
    hostCommand(binding, "athanor.context.lesson_plan", "context", { context_prepare: request }),
    new Set(["athanor.context.lesson_planned"]), signal, AUTOMATIC_CONTEXT_IO_TIMEOUT_MS,
  );
  const result = response.result as NativeLessonPlan;
  if (!result || typeof result.token !== "string" || typeof result.requiresCredential !== "boolean" || !Array.isArray(result.rules)) {
    throw new HostUnavailable("Native lesson plan omitted its token or rules");
  }
  return result;
}

export async function prepareContext(binding: HostBinding, request: ContextPrepareRequest, signal?: AbortSignal): Promise<PreparedContext> {
  const response = await sendHostCommand(
    hostCommand(binding, "athanor.context.prepare", "context", { context_prepare: request }),
    new Set(["athanor.context.prepared"]), signal, AUTOMATIC_CONTEXT_IO_TIMEOUT_MS,
  );
  const result = response.result as PreparedContext;
  if (!result || !Array.isArray(result.turns) || typeof result.replayed !== "boolean") {
    throw new HostUnavailable("Native context response omitted its prepared turns");
  }
  for (const turn of result.turns) {
    if (!request.visibleTurnIds.includes(turn.turnId) || !Array.isArray(turn.blocks)) {
      throw new HostUnavailable("Native context response carries an unknown turn");
    }
    for (const block of turn.blocks) {
      if (!Object.hasOwn(CONTEXT_BLOCK_TYPES, block.kind) || typeof block.content !== "string") {
        throw new HostUnavailable("Native context response carries an invalid block");
      }
    }
  }
  return result;
}

export type RecallViewport = {
  keptCandidates: unknown[];
  suppressions: Array<{ identity: string; reason: string }>;
  diagnostics: { kept: number; suppressed: number; reasons: Record<string, number> };
  presentation: Record<string, any>;
};

export async function applyRecallViewport(binding: HostBinding, result: Record<string, unknown>, mode: "automatic" | "manual", idempotencyKey?: string, signal?: AbortSignal): Promise<RecallViewport> {
  const response = await sendHostCommand(
    hostCommand(binding, "athanor.context.viewport", "context", { recall_result: result, context_viewport_mode: mode }, idempotencyKey),
    new Set(["athanor.context.viewported"]), signal,
  );
  return response.result as RecallViewport;
}

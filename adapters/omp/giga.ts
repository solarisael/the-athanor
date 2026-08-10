import { createHash } from "node:crypto";
import { homedir } from "node:os";
import path from "node:path";
import { existsSync } from "node:fs";
import { readFile, readdir } from "node:fs/promises";

import { discoverRustExecutable } from "./discovery.ts";
import {
  RustJsonlTransport,
  RustTransportError,
  RustTransportOutcomeUnknownError,
  TransportUnavailableError,
  type JsonObject,
} from "./rust-transport.ts";
import { roomContext } from "./solarisael-house-proof/room.ts";

const GIGA_EVENT_SCHEMA_VERSION = 1;
const SHA256_PATTERN = /^[a-f0-9]{64}$/;
const RFC3339_PATTERN = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/;
const GIGA_SHUTDOWN_WAIT_MS = 5_000;
const GIGA_READ_TIMEOUT_MS = 120_000;
const GIGA_MAX_SOURCE_COUNT = 8;
const GIGA_MAX_SOURCE_BYTES = 8_000;
const GIGA_MAX_WINDOW_BYTES = 24_000;
const GIGA_PROMOTABLE_KINDS = new Set(["memory", "coding_lesson", "project_lesson"] as const);


const gigaProcesses = new Map<string, Promise<void>>();
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
  source_refs: GigaSourceRef[];
  project_keys: string[];
  scope: JsonObject;
};

export type GigaCandidateListResult = {
  candidates: GigaCandidate[];
};

export type GigaReviewResult = {
  candidate_id: string;
  previous_state: string;
  new_state: string;
  reviewed_at: string;
};

export type GigaPromotionTarget =
  | { kind: "memory"; payload: { title: string; body: string; threads: string[] } }
  | {
      kind: "coding_lesson";
      payload: {
        title: string;
        body: string;
        shape: string | null;
        proof_pattern: string | null;
        trigger_context: string | null;
        language_keys: string[];
        technology_keys: string[];
        tags: string[];
      };
    }
  | {
      kind: "project_lesson";
      payload: {
        title: string;
        body: string;
        project: string;
        proof_pattern: string | null;
        trigger_context: string | null;
        language_keys: string[];
        technology_keys: string[];
        tags: string[];
      };
    };

type GigaPromotionRequestBase = {
  candidate_id: string;
  room: string;
  reviewer_id: string;
  operator_identity: string;
  authorization_basis: typeof GIGA_OMP_ROOM_BINDING;
  source_refs: GigaSourceRef[];
  reviewed_at: string;
};

export type GigaPromotionRequest = GigaPromotionRequestBase & (
  | {
      target: Extract<GigaPromotionTarget, { kind: "memory" }>;
      publication_consent: null;
    }
  | {
      target: Extract<GigaPromotionTarget, { kind: "coding_lesson" }>;
      publication_consent: null;
    }
  | {
      target: Extract<GigaPromotionTarget, { kind: "project_lesson" }>;
      publication_consent: { operator_approved: true; reviewer_approved: true };
    }
);

type GigaPromotionReceiptBase = {
  candidate_id: string;
  review_state: "promoted";
  durable: true;
  authority: "full";
  warnings: string[];
  reviewer_id: string;
  operator_identity: string;
  reviewed_at: string;
  committed_at: string;
};

export type GigaPromotionResult =
  | (GigaPromotionReceiptBase & { kind: "memory"; memory_id: number; room: string })
  | (GigaPromotionReceiptBase & { kind: "coding_lesson"; coding_lesson_id: number; scope: string })
  | (GigaPromotionReceiptBase & { kind: "project_lesson"; project_lesson_id: number; project: string });

export type GigaHealthResult = {
  enabled: boolean;
  store_healthy: boolean;
  queue_depth: number;
  oldest_queue_age_seconds: number | null;
  processed_count: number;
  failed_count: number;
  candidates_by_kind_state: Array<{ kind: string; review_state: string; count: number }>;
  classifier: {
    provider_type: string;
    model: string;
    model_digest: string;
    prompt_version: string;
    endpoint_scope: string;
    last_error_class: string | null;
    last_error_at: string | null;
    consecutive_failures: number;
  };
};
export type GigaQueueMaintenanceOperation = "check" | "purge_stuck";

export type GigaQueueMaintenanceResult = {
  ok: true;
  operation: GigaQueueMaintenanceOperation;
  scope: "room";
  room: string;
  eligible_events: number;
  blocked_events: number;
  deleted_events: number;
  deleted_attempts: number;
  preserved_candidates: number;
  before: Array<{ queue_state: string; count: number }>;
  after: Array<{ queue_state: string; count: number }>;
};


export type GigaReviewRequest = {
  candidate_id: string;
  reviewer_id: string;
  previous_state: string;
  new_state: GigaSafeReviewState;
  reason: string;
  authorization_basis: typeof GIGA_OMP_ROOM_BINDING;
  source_refs: GigaSourceRef[];
  promotion_target: null;
  merge_target: null;
  merge_source_candidates: [];
  resonance: null;
  reviewed_at: string;
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

export type GigaSourceRef = {
  source_type: "turn";
  source_id: string;
  role: "user" | "assistant";
  timestamp: string;
  content_hash: string;
  scope: {
    room: string;
    project: string | null;
    visibility: "private";
    publication_review_required: true;
  };
  range: null;
};

export type GigaConversationWindow = {
  event_schema_version: 1;
  event_id: string;
  event_type: "conversation_window";
  room: string;
  session_id: string;
  project_keys: string[];
  source_refs: GigaSourceRef[];
  lifecycle: Record<string, never>;
  created_at: string;
};


type ResolvedGigaSource = {
  source: GigaSourceRef;
  text: string;
};


function sha256(value: string): string {
  return createHash("sha256").update(value, "utf8").digest("hex");
}


function isObject(value: unknown): value is JsonObject {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function hasExactKeys(value: JsonObject, keys: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function isRfc3339(value: unknown): value is string {
  return typeof value === "string" && RFC3339_PATTERN.test(value) && !Number.isNaN(Date.parse(value));
}


function isNonemptyBoundedString(value: unknown, maximum: number): value is string {
  return typeof value === "string" && value.trim() === value && value.length > 0 && value.length <= maximum;
}

function isNonblankBoundedString(value: unknown, maximum: number): value is string {
  return typeof value === "string" && value.trim().length > 0 && value.length <= maximum;
}



function configuredGigaProjectKeys(): string[] {
  const configured = process.env.SOLARISAEL_GIGA_PROJECT_KEY;
  if (configured === undefined || configured === "") return [];
  if (
    configured.trim() !== configured
    || configured.length > 160
    || /[\u0000-\u001f\u007f]/.test(configured)
  ) {
    throw new TypeError("Invalid SOLARISAEL_GIGA_PROJECT_KEY");
  }
  return [configured];
}

export function buildGigaConversationWindow(ctx: any, loggedTurns: LoggedTurn[]): GigaConversationWindow | null {
  if (!Array.isArray(loggedTurns) || loggedTurns.length === 0) return null;
  const { room } = roomContext(ctx?.cwd || process.cwd());
  const projectKeys = configuredGigaProjectKeys();
  const selectedTurns = loggedTurns.slice(-GIGA_MAX_SOURCE_COUNT);
  const stableTurns = selectedTurns.map((turn) => {
    const sourceID = typeof turn?.sourceID === "string" ? turn.sourceID : "";
    const contentHash = typeof turn?.contentHash === "string" ? turn.contentHash.toLowerCase() : "";
    const sessionID = typeof turn?.sessionID === "string" ? turn.sessionID : "";
    const timestamp = typeof turn?.sourceTimestamp === "string" ? turn.sourceTimestamp : "";
    const role = turn?.role;
    if (
      turn?.hasStableID !== true
      || !sourceID.trim()
      || sourceID.trim() !== sourceID
      || !sessionID.trim()
      || sessionID.trim() !== sessionID
      || (role !== "user" && role !== "assistant")
      || !isRfc3339(timestamp)
      || !SHA256_PATTERN.test(contentHash)
    ) return null;
    return { sourceID, contentHash, sessionID, role, timestamp };
  });
  if (stableTurns.some((turn) => turn === null)) return null;

  const exactTurns = stableTurns as Array<{
    sourceID: string;
    contentHash: string;
    sessionID: string;
    role: "user" | "assistant";
    timestamp: string;
  }>;
  const sessionID = exactTurns[0].sessionID;
  if (
    exactTurns.some((turn) => turn.sessionID !== sessionID)
    || new Set(exactTurns.map((turn) => turn.sourceID)).size !== exactTurns.length
  ) return null;
  const identity = JSON.stringify([
    GIGA_EVENT_SCHEMA_VERSION,
    room,
    sessionID,
    exactTurns.map((turn) => [turn.sourceID, turn.contentHash]),
  ]);
  const eventID = sha256(identity);
  const project = projectKeys[0] ?? null;
  return {
    event_schema_version: GIGA_EVENT_SCHEMA_VERSION,
    event_id: eventID,
    event_type: "conversation_window",
    room,
    session_id: sessionID,
    project_keys: projectKeys,
    source_refs: exactTurns.map((turn) => ({
      source_type: "turn",
      source_id: turn.sourceID,
      role: turn.role,
      timestamp: turn.timestamp,
      content_hash: turn.contentHash,
      scope: {
        room,
        project,
        visibility: "private",
        publication_review_required: true,
      },
      range: null,
    })),
    lifecycle: {},
    created_at: exactTurns[exactTurns.length - 1].timestamp,
  };
}


function gigaWorkerError(name: string, retryable: boolean): Error & { retryable: boolean } {
  const error = Object.assign(new Error(name), { retryable });
  error.name = name;
  return error;
}


function parseGigaSourceRef(value: unknown, room: string, project: string | null): GigaSourceRef {
  if (!isObject(value) || !hasExactKeys(value, [
    "source_type",
    "source_id",
    "role",
    "timestamp",
    "content_hash",
    "scope",
    "range",
  ])) {
    throw gigaWorkerError("GigaSourceValidationError", false);
  }
  if (
    value.source_type !== "turn"
    || !isNonemptyBoundedString(value.source_id, 512)
    || (value.role !== "user" && value.role !== "assistant")
    || !isRfc3339(value.timestamp)
    || typeof value.content_hash !== "string"
    || !SHA256_PATTERN.test(value.content_hash)
    || value.range !== null
    || !isObject(value.scope)
    || !hasExactKeys(value.scope, ["room", "project", "visibility", "publication_review_required"])
    || value.scope.room !== room
    || value.scope.project !== project
    || value.scope.visibility !== "private"
    || value.scope.publication_review_required !== true
  ) {
    throw gigaWorkerError("GigaSourceValidationError", false);
  }
  return value as unknown as GigaSourceRef;
}


function ledgerConversationDirectory(spirit: string): string {
  const root = path.resolve(homedir(), ".local", "operators", "vessel", "state");
  const directory = path.resolve(root, spirit, "conversations");
  if (directory !== root && !directory.startsWith(`${root}${path.sep}`)) {
    throw gigaWorkerError("GigaLedgerPathError", false);
  }
  return directory;
}

async function resolveSourcesFromLedger(
  ctx: any,
  room: string,
  sessionId: string,
  sourceRefs: GigaSourceRef[],
): Promise<ResolvedGigaSource[]> {
  const trusted = roomContext(ctx?.cwd || process.cwd());
  if (trusted.room !== room) throw gigaWorkerError("GigaCrossRoomSourceError", false);
  const wanted = new Set(sourceRefs.map((source) => source.source_id));
  if (wanted.size !== sourceRefs.length) throw gigaWorkerError("GigaSourceValidationError", false);
  const directory = ledgerConversationDirectory(trusted.spirit);
  let names: string[];
  try {
    names = (await readdir(directory, { withFileTypes: true }))
      .filter((entry) => entry.isFile() && /^\d{4}-\d{2}-\d{2}\.jsonl$/.test(entry.name))
      .map((entry) => entry.name)
      .sort();
  } catch {
    throw gigaWorkerError("GigaLedgerUnavailableError", true);
  }
  const matches = new Map<string, Array<{ text: string; hash: string }>>();
  for (const name of names) {
    let contents: string;
    try {
      contents = await readFile(path.join(directory, name), "utf8");
    } catch {
      throw gigaWorkerError("GigaLedgerUnavailableError", true);
    }
    for (const line of contents.split("\n")) {
      if (!line) continue;
      let entry: unknown;
      try {
        entry = JSON.parse(line);
      } catch {
        continue;
      }
      if (
        !isObject(entry)
        || !wanted.has(String(entry.messageID ?? ""))
        || String(entry.sessionID ?? "") !== sessionId
        || (entry.role !== "user" && entry.role !== "assistant")
        || typeof entry.text !== "string"
      ) continue;
      const sourceId = String(entry.messageID);
      const expectedRole = sourceRefs.find((source) => source.source_id === sourceId)?.role;
      if (entry.role !== expectedRole) continue;
      const text = entry.text.trim();
      const hash = sha256(text);
      const values = matches.get(sourceId) ?? [];
      values.push({ text, hash });
      matches.set(sourceId, values);
    }
  }

  let totalBytes = 0;
  return sourceRefs.map((source) => {
    const values = matches.get(source.source_id) ?? [];
    const distinctHashes = new Set(values.map((value) => value.hash));
    if (
      values.length === 0
      || distinctHashes.size !== 1
      || !distinctHashes.has(source.content_hash)
    ) {
      throw gigaWorkerError("GigaSourceHashMismatchError", false);
    }
    const text = values[0].text;
    const sourceBytes = Buffer.byteLength(text, "utf8");
    totalBytes += sourceBytes;
    if (sourceBytes > GIGA_MAX_SOURCE_BYTES || totalBytes > GIGA_MAX_WINDOW_BYTES) {
      throw gigaWorkerError("GigaSourceWindowTooLargeError", false);
    }
    return { source, text };
  });
}

export async function resolveGigaSourceRefsFromLedger(
  ctx: any,
  room: string,
  sessionId: string,
  sourceRefs: unknown[],
  projectKeys: string[],
): Promise<GigaSourceRef[]> {
  if (
    !isNonemptyBoundedString(room, 160)
    || !isNonemptyBoundedString(sessionId, 512)
    || !Array.isArray(projectKeys)
    || projectKeys.length > 1
    || !projectKeys.every((key) => isNonemptyBoundedString(key, 160))
    || !Array.isArray(sourceRefs)
    || sourceRefs.length === 0
    || sourceRefs.length > GIGA_MAX_SOURCE_COUNT
  ) {
    throw gigaWorkerError("GigaSourceValidationError", false);
  }
  const project = projectKeys[0] ?? null;
  const parsed = sourceRefs.map((source) => parseGigaSourceRef(source, room, project));
  const resolved = await resolveSourcesFromLedger(ctx, room, sessionId, parsed);
  return resolved.map((source) => source.source);
}



function gigaTransport(cwd: string = process.cwd()): RustJsonlTransport | null {
  if (process.env.SOLARISAEL_GIGA_ENABLED !== "1") return null;
  const executable = discoverRustExecutable();
  if (!executable) return null;
  const trusted = roomContext(cwd);
  const ledgerDirectory = ledgerConversationDirectory(trusted.spirit);
  const key = `${executable}\0${trusted.room}\0${ledgerDirectory}`;
  let transport = gigaTransports.get(key);
  if (!transport) {
    transport = new RustJsonlTransport({
      executable,
      cwd: trusted.effectiveRoomDir,
      env: {
        SOLARISAEL_GIGA_SOURCE_LEDGER_DIR: ledgerDirectory,
        SOLARISAEL_GIGA_SOURCE_ROOM: trusted.room,
      },
    });
    gigaTransports.set(key, transport);
  }
  return transport;
}

function requireGigaTransport(): RustJsonlTransport {
  if (process.env.SOLARISAEL_GIGA_ENABLED !== "1") {
    throw Object.assign(new Error("GIGA is disabled"), {
      code: "giga_disabled",
      retryable: false,
      details: { enabled: false },
    });
  }
  const transport = gigaTransport();
  if (!transport) {
    throw Object.assign(new Error("GIGA transport is unavailable"), {
      code: "giga_transport_unavailable",
      retryable: true,
      details: { enabled: true, transport_available: false },
    });
  }
  return transport;
}

function objectResult(value: unknown, method: string): JsonObject {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw Object.assign(new Error(`Invalid ${method} response`), {
      code: "giga_invalid_response",
      retryable: false,
      details: { method },
    });
  }
  return value as JsonObject;
}

export function gigaTransportFailure(error: unknown): {
  code: string;
  message: string;
  retryable: boolean;
  details?: unknown;
} {
  if (
    error instanceof RustTransportError
    || error instanceof TransportUnavailableError
    || error instanceof RustTransportOutcomeUnknownError
  ) {
    return {
      code: error.code,
      message: error.message,
      retryable: error.retryable,
      details: error.details,
    };
  }
  if (error && typeof error === "object") {
    const candidate = error as { code?: unknown; message?: unknown; retryable?: unknown; details?: unknown };
    if (
      (candidate.code === "giga_disabled"
        || candidate.code === "giga_transport_unavailable"
        || candidate.code === "giga_invalid_response")
      && typeof candidate.message === "string"
      && typeof candidate.retryable === "boolean"
    ) {
      return {
        code: candidate.code,
        message: candidate.message,
        retryable: candidate.retryable,
        ...(candidate.details === undefined ? {} : { details: candidate.details }),
      };
    }
  }
  return {
    code: "giga_transport_failure",
    message: "GIGA transport failed",
    retryable: false,
  };
}

export async function requestGigaCandidateList(
  room: string,
  options: { reviewState?: string; limit?: number; signal?: AbortSignal } = {},
): Promise<GigaCandidateListResult> {
  const limit = options.limit ?? 50;
  if (!room.trim() || !Number.isInteger(limit) || limit < 1 || limit > 200) {
    throw new TypeError("Invalid GIGA candidate list request");
  }
  const response = objectResult(await requireGigaTransport().request("giga_candidate_list", {
    room,
    review_state: options.reviewState ?? null,
    limit,
  }, { signal: options.signal, timeoutMs: GIGA_READ_TIMEOUT_MS }), "giga_candidate_list");
  if (!Array.isArray(response.candidates)) {
    throw Object.assign(new Error("Invalid giga_candidate_list response"), {
      code: "giga_invalid_response",
      retryable: false,
      details: { method: "giga_candidate_list" },
    });
  }
  const candidates = response.candidates.map((value) => {
    const candidate = objectResult(value, "giga_candidate_list");
    if (
      !isNonemptyBoundedString(candidate.candidate_id, 512)
      || !isNonemptyBoundedString(candidate.event_id, 512)
      || typeof candidate.room !== "string"
      || !isNonemptyBoundedString(candidate.session_id, 512)
      || typeof candidate.kind !== "string"
      || typeof candidate.review_state !== "string"
      || !Array.isArray(candidate.source_refs)
      || !Array.isArray(candidate.project_keys)
      || !isObject(candidate.scope)
    ) {
      throw Object.assign(new Error("Invalid giga_candidate_list response"), {
        code: "giga_invalid_response",
        retryable: false,
        details: { method: "giga_candidate_list" },
      });
    }
    return candidate as GigaCandidate;
  });
  return { candidates };
}

export async function requestGigaReview(
  request: GigaReviewRequest,
  options: { signal?: AbortSignal } = {},
): Promise<GigaReviewResult> {
  if (
    !request.candidate_id.trim()
    || !request.reviewer_id.trim()
    || !request.previous_state.trim()
    || !request.reason.trim()
    || request.authorization_basis !== GIGA_OMP_ROOM_BINDING
    || !Array.isArray(request.source_refs)
    || request.source_refs.length === 0
    || request.promotion_target !== null
    || request.merge_target !== null
    || !Array.isArray(request.merge_source_candidates)
    || request.merge_source_candidates.length !== 0
    || request.resonance !== null
    || !RFC3339_PATTERN.test(request.reviewed_at)
    || !["in_review", "dismissed", "unresolved", "curio", "expired"].includes(request.new_state)
  ) {
    throw new TypeError("Invalid safe GIGA review request");
  }
  const payload: GigaReviewRequest = {
    candidate_id: request.candidate_id,
    reviewer_id: request.reviewer_id,
    previous_state: request.previous_state,
    new_state: request.new_state,
    reason: request.reason,
    authorization_basis: GIGA_OMP_ROOM_BINDING,
    source_refs: request.source_refs,
    promotion_target: null,
    merge_target: null,
    merge_source_candidates: [],
    resonance: null,
    reviewed_at: request.reviewed_at,
  };
  const response = objectResult(
    await requireGigaTransport().request("giga_review", payload as unknown as JsonObject, {
      signal: options.signal,
      settleDefinitively: true,
    }),
    "giga_review",
  );
  if (
    response.candidate_id !== request.candidate_id
    || response.previous_state !== request.previous_state
    || response.new_state !== request.new_state
    || typeof response.reviewed_at !== "string"
  ) {
    throw Object.assign(new Error("Invalid giga_review response"), {
      code: "giga_invalid_response",
      retryable: false,
      details: { method: "giga_review" },
    });
  }
  return response as GigaReviewResult;
}

function boundedUniqueNonblankTextArray(value: unknown, maximumItems: number, maximumLength: number): value is string[] {
  return Array.isArray(value)
    && value.length <= maximumItems
    && value.every((item) => isNonblankBoundedString(item, maximumLength))
    && new Set(value).size === value.length;
}

function validPromotionTarget(target: GigaPromotionTarget): boolean {
  if (!GIGA_PROMOTABLE_KINDS.has(target.kind as never) || !isObject(target.payload)) return false;
  const payload = target.payload as unknown as JsonObject;
  if (!isNonblankBoundedString(payload.title, 240) || !isNonblankBoundedString(payload.body, 32_000)) {
    return false;
  }
  if (target.kind === "memory") {
    return hasExactKeys(payload, ["title", "body", "threads"])
      && boundedUniqueNonblankTextArray(payload.threads, 64, 240);
  }
  if (!hasExactKeys(payload, target.kind === "coding_lesson"
    ? ["title", "body", "shape", "proof_pattern", "trigger_context", "language_keys", "technology_keys", "tags"]
    : ["title", "body", "project", "proof_pattern", "trigger_context", "language_keys", "technology_keys", "tags"])) {
    return false;
  }
  if (
    !boundedUniqueNonblankTextArray(payload.tags, 64, 240)
    || !boundedUniqueNonblankTextArray(payload.language_keys, 64, 64)
    || !boundedUniqueNonblankTextArray(payload.technology_keys, 64, 64)
    || !(payload.proof_pattern === null || isNonblankBoundedString(payload.proof_pattern, 2_000))
    || !(payload.trigger_context === null || isNonblankBoundedString(payload.trigger_context, 2_000))
  ) return false;
  if (target.kind === "coding_lesson") {
    return payload.shape === null || isNonblankBoundedString(payload.shape, 240);
  }
  return isNonemptyBoundedString(payload.project, 160);
}

export async function requestGigaPromote(
  request: GigaPromotionRequest,
  options: { signal?: AbortSignal } = {},
): Promise<GigaPromotionResult> {
  if (
    !isNonemptyBoundedString(request.candidate_id, 512)
    || !isNonemptyBoundedString(request.room, 160)
    || !isNonemptyBoundedString(request.reviewer_id, 160)
    || !isNonemptyBoundedString(request.operator_identity, 160)
    || request.authorization_basis !== GIGA_OMP_ROOM_BINDING
    || !Array.isArray(request.source_refs)
    || request.source_refs.length === 0
    || request.source_refs.length > GIGA_MAX_SOURCE_COUNT
    || !validPromotionTarget(request.target)
    || !isRfc3339(request.reviewed_at)
    || (request.target.kind === "project_lesson"
      ? !isObject(request.publication_consent)
        || !hasExactKeys(request.publication_consent, ["operator_approved", "reviewer_approved"])
        || request.publication_consent.operator_approved !== true
        || request.publication_consent.reviewer_approved !== true
      : request.publication_consent !== null)
  ) {
    throw new TypeError("Invalid GIGA promotion request");
  }
  const response = objectResult(
    await requireGigaTransport().request("giga_promote", request as unknown as JsonObject, {
      signal: options.signal,
      settleDefinitively: true,
    }),
    "giga_promote",
  );
  const commonKeys = [
    "kind",
    "candidate_id",
    "review_state",
    "durable",
    "authority",
    "warnings",
    "reviewer_id",
    "operator_identity",
    "reviewed_at",
    "committed_at",
  ];
  const variantKeys = response.kind === "memory"
    ? ["memory_id", "room"]
    : response.kind === "coding_lesson"
      ? ["coding_lesson_id", "scope"]
      : response.kind === "project_lesson"
        ? ["project_lesson_id", "project"]
        : [];
  const durableID = response.kind === "memory"
    ? response.memory_id
    : response.kind === "coding_lesson"
      ? response.coding_lesson_id
      : response.project_lesson_id;
  const expectedProject = request.target.kind === "project_lesson"
    ? request.target.payload.project
    : null;
  const variantMatches = response.kind === "memory"
    ? response.room === request.room
    : response.kind === "coding_lesson"
      ? isNonemptyBoundedString(response.scope, 160)
      : response.kind === "project_lesson"
        && response.project === expectedProject;
  if (
    response.kind !== request.target.kind
    || !hasExactKeys(response, [...commonKeys, ...variantKeys])
    || response.candidate_id !== request.candidate_id
    || response.review_state !== "promoted"
    || !Number.isSafeInteger(durableID)
    || (durableID as number) <= 0
    || response.durable !== true
    || response.authority !== "full"
    || !Array.isArray(response.warnings)
    || !response.warnings.every((warning) => typeof warning === "string")
    || response.reviewer_id !== request.reviewer_id
    || response.operator_identity !== request.operator_identity
    || response.reviewed_at !== request.reviewed_at
    || !isRfc3339(response.committed_at)
    || !variantMatches
  ) {
    throw Object.assign(new Error("Invalid giga_promote response"), {
      code: "giga_invalid_response",
      retryable: false,
      details: { method: "giga_promote" },
    });
  }
  return response as unknown as GigaPromotionResult;
}

export async function requestGigaHealth(
  room: string,
  options: { signal?: AbortSignal } = {},
): Promise<GigaHealthResult> {
  const response = objectResult(
    await requireGigaTransport().request("giga_health", { room }, {
      signal: options.signal,
      timeoutMs: GIGA_READ_TIMEOUT_MS,
    }),
    "giga_health",
  );
  if (
    typeof response.enabled !== "boolean"
    || typeof response.store_healthy !== "boolean"
    || typeof response.queue_depth !== "number"
    || !(response.oldest_queue_age_seconds === null || typeof response.oldest_queue_age_seconds === "number")
    || typeof response.processed_count !== "number"
    || typeof response.failed_count !== "number"
    || !hasExactKeys(response.classifier, [
      "provider_type",
      "model",
      "model_digest",
      "prompt_version",
      "endpoint_scope",
      "last_error_class",
      "last_error_at",
      "consecutive_failures",
    ])
    || typeof response.classifier.provider_type !== "string"
    || typeof response.classifier.model !== "string"
    || typeof response.classifier.model_digest !== "string"
    || typeof response.classifier.prompt_version !== "string"
    || typeof response.classifier.endpoint_scope !== "string"
    || !(response.classifier.last_error_class === null
      || typeof response.classifier.last_error_class === "string")
    || !(response.classifier.last_error_at === null
      || typeof response.classifier.last_error_at === "string")
    || typeof response.classifier.consecutive_failures !== "number"
  ) {
    throw Object.assign(new Error("Invalid giga_health response"), {
      code: "giga_invalid_response",
      retryable: false,
      details: { method: "giga_health" },
    });
  }
  return response as GigaHealthResult;
}

export async function requestGigaQueueMaintenance(
  room: string,
  operation: GigaQueueMaintenanceOperation,
  options: { signal?: AbortSignal } = {},
): Promise<GigaQueueMaintenanceResult> {
  if (!room.trim() || !["check", "purge_stuck"].includes(operation)) {
    throw new TypeError("Invalid GIGA queue maintenance request");
  }
  const response = objectResult(
    await requireGigaTransport().request("giga_queue_maintenance", {
      room,
      operation,
      scope: "room",
    }, {
      signal: options.signal,
      timeoutMs: GIGA_READ_TIMEOUT_MS,
    }),
    "giga_queue_maintenance",
  );
  const validCount = (value: unknown) => Number.isSafeInteger(value) && Number(value) >= 0;
  const validStateCounts = (value: unknown) => Array.isArray(value) && value.every((entry) =>
    isObject(entry)
    && hasExactKeys(entry, ["queue_state", "count"])
    && typeof entry.queue_state === "string"
    && validCount(entry.count)
  );
  if (
    !hasExactKeys(response, [
      "ok",
      "operation",
      "scope",
      "room",
      "eligible_events",
      "blocked_events",
      "deleted_events",
      "deleted_attempts",
      "preserved_candidates",
      "before",
      "after",
    ])
    || response.ok !== true
    || response.operation !== operation
    || response.scope !== "room"
    || response.room !== room
    || !validCount(response.eligible_events)
    || !validCount(response.blocked_events)
    || !validCount(response.deleted_events)
    || !validCount(response.deleted_attempts)
    || !validCount(response.preserved_candidates)
    || !validStateCounts(response.before)
    || !validStateCounts(response.after)
  ) {
    throw Object.assign(new Error("Invalid giga_queue_maintenance response"), {
      code: "giga_invalid_response",
      retryable: false,
      details: { method: "giga_queue_maintenance" },
    });
  }
  return response as GigaQueueMaintenanceResult;
}


// Substrate giga_process is single-attempt (07-30 starvation fix): a retryable
// classifier failure returns outcome "retry" instead of looping inside the
// worker. The adapter owns the retry cadence so the transport stays responsive
// between attempts. The substrate's GIGA_MAX_EVENT_ATTEMPTS cap ends the loop:
// at cap the event transitions to failed and outcome stops being "retry".
const GIGA_PROCESS_RETRY_DELAY_MS = 15_000;
const GIGA_PROCESS_MAX_ADAPTER_INVOCATIONS = 12;

async function runGigaProcess(
  transport: { request(method: string, params: JsonObject): Promise<unknown> },
  eventID: string,
): Promise<void> {
  for (let invocation = 0; invocation < GIGA_PROCESS_MAX_ADAPTER_INVOCATIONS; invocation += 1) {
    const result = await transport.request("giga_process", { event_id: eventID });
    const outcome = (result as JsonObject | null)?.outcome;
    if (gigaClosing || outcome !== "retry") return;
    await new Promise((resolve) => setTimeout(resolve, GIGA_PROCESS_RETRY_DELAY_MS));
    if (gigaClosing) return;
  }
}
function trackGigaProcess(eventID: string, request: Promise<unknown>): void {
  const tracked = request.then(() => undefined, () => undefined);
  gigaProcesses.set(eventID, tracked);
  void tracked.finally(() => {
    if (gigaProcesses.get(eventID) === tracked) gigaProcesses.delete(eventID);
  });
}

async function ingestLoggedTurns(ctx: any, loggedTurns: LoggedTurn[]): Promise<void> {
  if (gigaClosing || process.env.SOLARISAEL_GIGA_ENABLED !== "1" || !loggedTurns.length) return;
  try {
    const event = buildGigaConversationWindow(ctx, loggedTurns);
    if (!event) return;
    const transport = gigaTransport(ctx?.cwd || process.cwd());
    if (!transport) return;
    const response = objectResult(
      await transport.request("giga_event_ingest", event as unknown as JsonObject),
      "giga_event_ingest",
    );
    if (
      !hasExactKeys(response, ["event_id", "accepted", "duplicate"])
      || response.event_id !== event.event_id
      || typeof response.accepted !== "boolean"
      || typeof response.duplicate !== "boolean"
      || (!response.accepted && !response.duplicate)
    ) return;
    if (!gigaClosing && !gigaProcesses.has(event.event_id)) {
      trackGigaProcess(
        event.event_id,
        runGigaProcess(transport, event.event_id),
      );
    }
  } catch {
    // GIGA is opt-in background work and must never block ordinary House behavior.
  }
}

// enough: in-memory turn buffer, dropped on hard crash. giga_process now reloads
// stored source references and exact ledger text from event_id alone.
// Danger: check both caps BEFORE appending — an over-deep or cross-session buffer loses
// turns with no error. Project lesson 130 (the-athanor) carries the caps and the why.
const GIGA_TURN_BUFFER_BYTES = 18_000;

type GigaTurnBuffer = { ctx: any; cwd: string; turns: LoggedTurn[]; bytes: number };

const gigaTurnBuffers = new Map<string, GigaTurnBuffer>();

function appendGigaTurn(ctx: any, turn: LoggedTurn): void {
  const cwd = String(ctx?.cwd || "");
  const session = typeof turn.sessionID === "string" ? turn.sessionID : "";
  const key = `${cwd}\u0000${session}`;
  const buffered = gigaTurnBuffers.get(key) ?? { ctx, cwd, turns: [], bytes: 0 };
  buffered.ctx = ctx;
  const bytes = typeof turn.text === "string" ? Buffer.byteLength(turn.text, "utf8") : 0;
  if (
    buffered.turns.length >= GIGA_MAX_SOURCE_COUNT
    || (buffered.turns.length > 0 && buffered.bytes + bytes > GIGA_TURN_BUFFER_BYTES)
  ) {
    void ingestLoggedTurns(buffered.ctx, buffered.turns.splice(0));
    buffered.bytes = 0;
  }
  buffered.turns.push(turn);
  buffered.bytes += bytes;
  gigaTurnBuffers.set(key, buffered);
}

// Stage 1 decision 7 (HIPPOCAMPUS.md §28): GIGA ingests only a verified main
// session. Subagent output re-enters the ledger through the main agent's turn,
// so typed task/subagent events stay deferred and their raw windows are
// excluded here.
//
// Detection mirrors session-manager's child-transcript shape: a subagent file
// lives at `<parentStem>/<agentId>.jsonl`, so its directory plus `.jsonl` names
// an existing parent session file. Main sessions sit flat. Missing session
// metadata is not authority to spawn background work: fail closed.
function isSubagentSessionContext(ctx: any): boolean {
  try {
    const sessionFile = ctx?.sessionManager?.getSessionFile?.();
    if (typeof sessionFile !== "string" || !sessionFile) return true;
    return existsSync(`${path.dirname(sessionFile)}.jsonl`);
  } catch {
    return true;
  }
}

export function ingestGigaLoggedTurnsDetached(ctx: any, loggedTurns: LoggedTurn[]): void {
  if (
    gigaClosing
    || process.env.SOLARISAEL_GIGA_ENABLED !== "1"
    || !Array.isArray(loggedTurns)
    || !loggedTurns.length
    || isSubagentSessionContext(ctx)
  ) return;
  for (const turn of loggedTurns) appendGigaTurn(ctx, turn);
}

export function flushGigaTurnsDetached(ctx: any): void {
  const cwd = String(ctx?.cwd || "");
  for (const [key, buffered] of [...gigaTurnBuffers]) {
    if (buffered.cwd !== cwd) continue;
    gigaTurnBuffers.delete(key);
    if (buffered.turns.length) void ingestLoggedTurns(buffered.ctx, buffered.turns);
  }
}

export async function closeGigaTransports(): Promise<void> {
  const pendingBuffers = [...gigaTurnBuffers.values()].filter((buffer) => buffer.turns.length);
  gigaTurnBuffers.clear();
  await Promise.allSettled(pendingBuffers.map((buffer) => ingestLoggedTurns(buffer.ctx, buffer.turns)));
  gigaClosing = true;
  const active = [...gigaProcesses.values()];
  if (active.length) {
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      await Promise.race([
        Promise.allSettled(active),
        new Promise<void>((resolve) => {
          timer = setTimeout(resolve, GIGA_SHUTDOWN_WAIT_MS);
        }),
      ]);
    } finally {
      if (timer) clearTimeout(timer);
    }
  }
  const closing: Promise<void>[] = [];
  for (const [executable, transport] of gigaTransports) {
    gigaTransports.delete(executable);
    closing.push(transport.close());
  }
  await Promise.allSettled(closing);
  gigaProcesses.clear();
}

export const __gigaTest = Object.freeze({
  trackGigaProcess,
  isSubagentSessionContext,
  resetState() {
    gigaClosing = false;
    gigaProcesses.clear();
    gigaTransports.clear();
  },
});

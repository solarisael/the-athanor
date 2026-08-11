// Tool registration for the OMP adapter.
// Silhouette: expose room/substrate tools; keep hook wiring out of tool bodies.
import { createHash } from "node:crypto";
 

import { compactRecall, recallWithRouting } from "./recall.ts";
import {
  loadRoomState,
  normalizeSpiritName,
  roomContext,
  saveRoomState,
  statePathForRoom,
  writeActiveSpiritSnapshot,
} from "./room.ts";
import { RecallPolicyHostClient } from "./recall-policy.ts";
import { kittenLineageDiagnostics } from "../kitten-lineage.ts";
import { queryAnamnesis, formatAnamnesisContext } from "./anamnesis.ts";
import {
  catchBoat,
  memorySourcePath,
  sleepBoat,
  substrateHealth,
} from "./substrate.ts";
import { RustJsonlTransport, RustTransportError, RustTransportOutcomeUnknownError } from "../rust-transport.ts";
import { discoverRustExecutable } from "../discovery.ts";
import { laneStatus } from "./routing.ts";
import { familiarStatus } from "./familiars.ts";
import { dispatchHouse } from "./dispatch.ts";
import { REMEMBER_STORES, validateStoreFields } from "./stores.ts";
import { WRITE_TIMEOUT_MS } from "./constants.ts";
import {
  createToolRenderers,
  emitToolUpdate,
  normalizeToolResponse,
  toolThrown,
} from "./feedback.ts";
import {
  GIGA_OMP_ROOM_BINDING,
  flushGigaTurnsDetached,
  gigaTransportFailure,
  requestGigaCandidateList,
  requestGigaHealth,
  requestGigaQueueMaintenance,
  requestGigaPromote,
  requestGigaReview,
  resolveGigaSourceRefsFromLedger,
  type GigaCandidate,
  type GigaPromotionTarget,
  type GigaPromotionRequest,
  type GigaSafeReviewState,
} from "../giga.ts";

const rustRememberTransports = new Map<string, RustJsonlTransport>();
const LANE_STATUS_HEALTH_TIMEOUT_MS = 3_000;
const DESIGN_DOCUMENT_TYPES = new Set(["token", "component", "contract", "guideline"]);

const defaultGigaPromotionOperations = Object.freeze({
  requestGigaCandidateList,
  resolveGigaSourceRefsFromLedger,
  requestGigaPromote,
});
let gigaPromotionOperations = { ...defaultGigaPromotionOperations };

export const __gigaPromotionTest = Object.freeze({
  setOperations(overrides: Partial<typeof defaultGigaPromotionOperations>) {
    gigaPromotionOperations = { ...defaultGigaPromotionOperations, ...overrides };
  },
  resetOperations() {
    gigaPromotionOperations = { ...defaultGigaPromotionOperations };
  },
});


function rustRememberTransport(): RustJsonlTransport | null {
  const executable = discoverRustExecutable();
  if (!executable) return null;
  let transport = rustRememberTransports.get(executable);
  if (transport && !transport.usable) {
    rustRememberTransports.delete(executable);
    void transport.close().catch(() => {});
    transport = undefined;
  }
  if (!transport) {
    transport = new RustJsonlTransport({ executable });
    rustRememberTransports.set(executable, transport);
  }
  return transport;
}

function evictRustRememberTransport(executable: string, transport: RustJsonlTransport): void {
  if (rustRememberTransports.get(executable) !== transport) return;
  rustRememberTransports.delete(executable);
  void transport.close().catch(() => {});
}

function sourcePathKey(value: unknown): string {
  return String(value ?? "").replace(/\\/g, "/").replace(/^house\//i, "").toLowerCase();
}

function deterministicMemorySourcePath(room: string, title: string, body: string, threads: unknown[], continues: unknown[], supersedes: unknown[]): string {
  const canonical = JSON.stringify({ room, title, body, threads, continues, supersedes });
  const digest = createHash("sha256").update(canonical).digest("hex").slice(0, 24);

  const baseline = memorySourcePath(title, new Date(0));
  return baseline.replace(/^memory\/omp_[^_]+_/, `memory/omp_${digest}_`);
}

function rustFailureReceipt(error: RustTransportError): Record<string, unknown> {
  const upstreamDetails = error.details && typeof error.details === "object" && !Array.isArray(error.details)
    ? error.details as Record<string, unknown>
    : { upstream_details: error.details ?? null };
  const stderr = error.stderr.slice(0, 4096);
  const evidence = Array.isArray(upstreamDetails.evidence) ? [...upstreamDetails.evidence] : [];
  if (stderr) {
    evidence.push({
      source: "rust_stderr",
      text: stderr,
      truncated: error.stderr.length > stderr.length,
    });
  }
  return {
    ok: false,
    error: error.message,
    code: error.code,
    retryable: error.retryable,
    details: { ...upstreamDetails, evidence },
  };
}

function unknownOutcomeDetails(error: unknown): Record<string, unknown> {
  const source = error && typeof error === "object" ? error as { details?: unknown; cause?: unknown } : {};
  const details = source.details && typeof source.details === "object" && !Array.isArray(source.details)
    ? source.details as Record<string, unknown>
    : source.details === undefined ? {} : { upstream_details: source.details };
  if (!(source.cause instanceof Error)) return details;
  return {
    ...details,
    cause: { name: source.cause.name, message: source.cause.message },
  };
}
function isOutcomeUnknownError(error: unknown): boolean {
  return error instanceof RustTransportOutcomeUnknownError;
}

async function reconcileRustMemory(room: string, sourcePath: string, signal?: AbortSignal) {
  try {
    const recalled = await recallWithRouting("", room, sourcePath, { signal, temporalDecay: false });
    if (!recalled.ok) return { reconciled: false, committed: null };
    const result = recalled.result as Record<string, unknown>;
    const collections = ["retrievalCandidates", "semanticChunks", "contentChunks", "dateMatches"];
    const committed = collections.some((name) => (
      Array.isArray(result[name])
      && result[name].some((entry) => sourcePathKey((entry as Record<string, unknown>)?.source_path) === sourcePathKey(sourcePath))
    ));
    return { reconciled: true, committed };
  } catch {
    return { reconciled: false, committed: null };
  }
}

function unknownWriteReceipt(error: unknown, sourcePath: string, reconciliation: { reconciled: boolean; committed: boolean | null }) {
  return {
    ok: false,
    error: "Rust remember write outcome is unknown after dispatch",
    code: "outcome_unknown",
    outcome: "unknown",
    retryable: true,
    sourcePath,
    committed: reconciliation.committed,
    reconciled: reconciliation.reconciled,
    details: unknownOutcomeDetails(error),
  };
}

function unknownLessonReceipt(error?: unknown): Record<string, unknown> {
  return {
    ok: false,
    error: "Rust lesson write outcome is unknown after dispatch",
    code: "outcome_unknown",
    outcome: "unknown",
    retryable: true,
    details: unknownOutcomeDetails(error),
  };
}

export async function writeRustCanon({ room, name, kind, summary, aliases, searchBoost, weighty, pointerFiles, summaryAsOf, supersedes, attribution, signal }) {
  const executable = discoverRustExecutable();
  const transport = rustRememberTransport();
  if (!transport) return { ok: false, error: "Rust substrate executable is unavailable" };
  try {
    const receipt = await transport.request("canon_write", {
      room,
      name,
      kind,
      summary,
      aliases,
      searchBoost,
      weighty,
      pointerFiles,
      summaryAsOf,
      supersedes,
      attribution,
    }, {
      signal: signal || undefined,
      timeoutMs: WRITE_TIMEOUT_MS,
      settleDefinitively: true,
    });
    if (!receipt || typeof receipt !== "object" || Array.isArray(receipt)) {
      evictRustRememberTransport(executable, transport);
      return {
        ok: false,
        error: "Rust canon write outcome is unknown after dispatch",
        code: "outcome_unknown",
        outcome: "unknown",
        retryable: false,
      };
    }
    return receipt as Record<string, unknown>;
  } catch (error) {
    if (!transport.usable) evictRustRememberTransport(executable, transport);
    if (error instanceof RustTransportError) return rustFailureReceipt(error);
    return {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
      ...(isOutcomeUnknownError(error)
        ? { code: "outcome_unknown", outcome: "unknown", retryable: false }
        : {}),
    };
  }
}

export async function readRustCanon({ room, id, name, includeHistory, signal }) {
  const executable = discoverRustExecutable();
  const transport = rustRememberTransport();
  if (!transport) return { ok: false, error: "Rust substrate executable is unavailable" };
  try {
    return await transport.request("canon_read", {
      room,
      id,
      name,
      includeHistory,
    }, {
      signal: signal || undefined,
      timeoutMs: WRITE_TIMEOUT_MS,
    }) as Record<string, unknown>;
  } catch (error) {
    if (!transport.usable) evictRustRememberTransport(executable, transport);
    if (error instanceof RustTransportError) return rustFailureReceipt(error);
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}

export async function writeRustMemory({ room, title, body, threads, continues, supersedes, signal }) {
  const executable = discoverRustExecutable();
  const transport = rustRememberTransport();
  if (!transport) return { ok: false, error: "Rust substrate executable is unavailable" };
  const normalizeIdentityValues = (values: unknown) => [
    ...new Set(
      (Array.isArray(values) ? values : [])
        .map(String)
        .map((value) => value.trim())
        .filter(Boolean),
    ),
  ].sort();

  const normalizedThreads = normalizeIdentityValues(threads);
  const normalizedContinues = (Array.isArray(continues) ? continues : [])
    .map((continuation) => ({
      thread: String(continuation.thread).trim(),
      previousMemoryId: String(continuation.previousMemoryId),
    }))
    .sort((left, right) => left.thread.localeCompare(right.thread));
  const normalizedSupersedes = normalizeIdentityValues(supersedes);

  const sourcePath = deterministicMemorySourcePath(
    room,
    title,
    body,
    normalizedThreads,
    normalizedContinues,
    normalizedSupersedes,
  );
  const params: Record<string, unknown> = {
    room,
    kind: "memory",
    title,
    body,
    source_path: sourcePath,
    threads: normalizedThreads,
    continues: normalizedContinues,
    supersedes: normalizedSupersedes,
    backup: false,
  };
  try {
    const receipt = await transport.request("remember", params, {
      signal: signal || undefined, timeoutMs: WRITE_TIMEOUT_MS, settleDefinitively: true,
    });
    if (!receipt || typeof receipt !== "object" || Array.isArray(receipt)) {
      evictRustRememberTransport(executable, transport);
      return unknownWriteReceipt(new RustTransportOutcomeUnknownError(), sourcePath, await reconcileRustMemory(room, sourcePath, signal));
    }
    const value = receipt as Record<string, unknown>;
    if (typeof value.memory_id !== "number" || typeof value.room !== "string"
      || typeof value.source_path !== "string" || value.durable !== true
      || value.authority !== "postgres" || !Array.isArray(value.warnings)
      || !value.warnings.every((warning) => typeof warning === "string")) {
      evictRustRememberTransport(executable, transport);
      return unknownWriteReceipt(new RustTransportOutcomeUnknownError(), sourcePath, await reconcileRustMemory(room, sourcePath, signal));
    }
    return { ok: true, ...value, id: value.memory_id, sourcePath: value.source_path };
  } catch (error) {
    if (isOutcomeUnknownError(error)) {
      evictRustRememberTransport(executable, transport);
      return unknownWriteReceipt(error, sourcePath, await reconcileRustMemory(room, sourcePath, signal));
    }
    if (!transport.usable) evictRustRememberTransport(executable, transport);
    if (error instanceof RustTransportError) {
      return rustFailureReceipt(error);
    }
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}

async function writeRustLesson({ room, kind, title, body, fields, backup, signal }) {
  const executable = discoverRustExecutable();
  const transport = rustRememberTransport();
  if (!transport || !executable) return { ok: false, error: "Rust substrate executable is unavailable" };
  const params: Record<string, unknown> = {
    room, kind, title, body, shape: fields.shape ?? null, voice: fields.voice ?? null,
    register: Array.isArray(fields.register) ? fields.register : [],
    scope: fields.scope ?? null, project: fields.project ?? null,
    proofPattern: fields.proofPattern ?? null, triggerContext: fields.triggerContext ?? null,
    exampleText: fields.exampleText ?? null,
    sourceMemoryPath: fields.sourceMemoryPath ?? null,
    languageKeys: Array.isArray(fields.languageKeys) ? fields.languageKeys : [],
    technologyKeys: Array.isArray(fields.technologyKeys) ? fields.technologyKeys : [],
    tags: Array.isArray(fields.tags) ? fields.tags : [], backup,
    threadKeys: Array.isArray(fields.threadKeys) ? fields.threadKeys : [],
  };
  try {
    const receipt = await transport.request("remember", params, {
      signal: signal || undefined, timeoutMs: WRITE_TIMEOUT_MS, settleDefinitively: true,
    });
    if (!receipt || typeof receipt !== "object" || Array.isArray(receipt)) {
      evictRustRememberTransport(executable, transport);
      return unknownLessonReceipt();
    }
    const value = receipt as Record<string, unknown>;
    if (typeof value.lesson_id !== "number" || value.kind !== kind || value.durable !== true
      || value.authority !== "postgres" || !Array.isArray(value.warnings)
      || !value.warnings.every((warning) => typeof warning === "string")) {
      evictRustRememberTransport(executable, transport);
      return unknownLessonReceipt();
    }
    return { ok: true, ...value, id: value.lesson_id };
  } catch (error) {
    if (isOutcomeUnknownError(error)) {
      evictRustRememberTransport(executable, transport);
      return {
        ok: false,
        error: "Rust lesson write outcome is unknown after dispatch",
        code: "outcome_unknown",
        outcome: "unknown",
        retryable: true,
        details: unknownOutcomeDetails(error),
      };
    }
    if (!transport.usable) evictRustRememberTransport(executable, transport);
    if (error instanceof RustTransportError) {
      return rustFailureReceipt(error);
    }
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}
async function requestRustDomain(method: string, params: Record<string, unknown>, signal?: AbortSignal, write = false) {
  const executable = discoverRustExecutable();
  const transport = rustRememberTransport();
  if (!transport || !executable) return { ok: false, error: "Rust substrate executable is unavailable" };
  try {
    const result = await transport.request(method, params, {
      signal: signal || undefined,
      timeoutMs: WRITE_TIMEOUT_MS,
      ...(write ? { settleDefinitively: true } : {}),
    });
    if (!result || typeof result !== "object" || Array.isArray(result)) {
      if (write) evictRustRememberTransport(executable, transport);
      return { ok: false, error: `Rust ${method} returned an invalid receipt` };
    }
    return result as Record<string, unknown>;
  } catch (error) {
    if (!transport.usable) evictRustRememberTransport(executable, transport);
    if (error instanceof RustTransportError) return rustFailureReceipt(error);
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}


function unknownAnamnesisReceipt(error?: unknown): Record<string, unknown> {
  return {
    ok: false,
    error: "Rust anamnesis write outcome is unknown after dispatch",
    code: "outcome_unknown",
    outcome: "unknown",
    retryable: true,
    details: unknownOutcomeDetails(error),
  };
}
async function writeRustAnamnesis({ room, payload, signal }) {
  const executable = discoverRustExecutable();
  const transport = rustRememberTransport();
  if (!transport || !executable) return { ok: false, error: "Rust substrate executable is unavailable" };
  const operation = payload?.operation;
  const params = { room, ...payload };
  try {
    const receipt = await transport.request("anamnesis_write", params, {
      signal: signal || undefined, timeoutMs: WRITE_TIMEOUT_MS, settleDefinitively: true,
    });
    if (!receipt || typeof receipt !== "object" || Array.isArray(receipt)) {
      evictRustRememberTransport(executable, transport);
      return unknownAnamnesisReceipt();
    }
    const value = receipt as Record<string, unknown>;
    if (value.ok !== true || value.operation !== operation || value.room !== room
      || typeof value.title !== "string"
      || (operation === "add" && value.kind !== "pillar" && value.kind !== "cycle")
      || (operation === "append-rep" && (!Number.isInteger(value.repNumber) || Number(value.repNumber) < 1))
      || value.durable !== true || value.authority !== "postgres"
      || !Array.isArray(value.warnings) || !value.warnings.every((warning) => typeof warning === "string")) {
      evictRustRememberTransport(executable, transport);
      return unknownAnamnesisReceipt();
    }
    return { ok: true, ...value };
  } catch (error) {
    if (isOutcomeUnknownError(error)) {
      evictRustRememberTransport(executable, transport);
      return unknownAnamnesisReceipt(error);
    }
    if (!transport.usable) evictRustRememberTransport(executable, transport);
    if (error instanceof RustTransportError) {
      return rustFailureReceipt(error);
    }
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}

export function closeRustRememberTransports() {
  for (const [executable, transport] of rustRememberTransports) {
    rustRememberTransports.delete(executable);
    void transport.close().catch(() => {});
  }
}

function refuseToolResult(error) {
  const result = { ok: false, error };
  return {
    isError: true,
    content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
    details: result,
  };
}

function gigaToolFailure(error) {
  const failure = gigaTransportFailure(error);
  const result = {
    ok: false,
    status: "error",
    code: failure.code,
    error: failure.message,
    message: failure.message,
    retryable: failure.retryable,
    details: failure.details ?? {},
  };
  return {
    isError: true,
    content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
    details: result,
  };
}

function gigaCandidateRefusal(error, room, candidateId) {
  const result = {
    ok: false,
    status: "error",
    code: "giga_review_refused",
    error,
    message: error,
    retryable: false,
    details: { room, candidate_id: candidateId },
  };
  return {
    isError: true,
    content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
    details: result,
  };
}

function gigaPromotionRefusal(error, room, candidateId) {
  const result = {
    ok: false,
    status: "error",
    code: "giga_promotion_refused",
    error,
    message: error,
    retryable: false,
    details: { room, candidate_id: candidateId },
  };
  return {
    isError: true,
    content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
    details: result,
  };
}

function safeGigaTransition(previousState, newState) {
  return (previousState === "unreviewed"
      && (newState === "in_review" || newState === "dismissed" || newState === "expired"))
    || (previousState === "in_review"
      && (newState === "dismissed" || newState === "unresolved" || newState === "curio"))
    || (previousState === "unresolved" && newState === "in_review")
    || (previousState === "curio" && (newState === "dismissed" || newState === "expired"));
}

function registerHouseTool(pi, definition) {
  const execute = definition.execute;
  const renderers = createToolRenderers(definition.label);
  pi.registerTool({
    ...definition,
    ...renderers,
    async execute(toolCallId, params, signal, onUpdate, ctx) {
      emitToolUpdate(onUpdate, definition.name);
      try {
        return normalizeToolResponse(await execute(toolCallId, params, signal, onUpdate, ctx), definition.name);
      } catch (error) {
        return toolThrown(error, definition.name);
      }
    },
  });
}

export function registerSolarisaelTools(pi) {
  const z = pi.zod;

  registerHouseTool(pi, {
    name: "recall",
    label: "Athanor Recall",
    description: [
      "Query the active Athanor storage profile: attributed local Markdown/JSON/JSONL/text evidence in Vault, or canon, memory chunks, hybrid matches, and taxonomy in AKASHA.",
      "Recall is recognition across time, not dossier lookup: search in the room's own natural language, receive the result as lived continuity and evidence, and preserve its names, relationships, uncertainty, and meaning instead of flattening it into clinical or corporate summary.",
      "Use this when you notice your own uncertainty about load-bearing names, people, projects, dated decisions, or facts.",
      "Use the taxonomy map as a bounded menu for better follow-up recall queries; do not guess shape names blindly.",
      "If no canonical match is returned, do not invent from adjacent matches; state the gap honestly.",
    ].join("\n"),
    parameters: z.object({
      query: z.string().describe("Specific natural-language memory/canon query in the room's own vocabulary; name the person, project, event, date, or decision you are trying to recognize."),
    }),
    approval: "read",
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      const { room, effectiveRoomDir } = roomContext(ctx.cwd);
  
      try {
        const recalled = await recallWithRouting(effectiveRoomDir, room, params.query, { signal: _signal, temporalDecay: false });
        if (!recalled.ok) {
          return {
            isError: true,
            content: [{ type: "text", text: JSON.stringify(recalled.result, null, 2) }],
            details: { room, ok: false },
          };
        }
        const compact = compactRecall(recalled.result, { includeTaxonomy: true });
        return {
          content: [{ type: "text", text: JSON.stringify(compact, null, 2) }],
          details: { room, ok: Boolean(compact.ok), found: Boolean(compact.found) },
        };
      } catch (err) {
        return {
          isError: true,
          content: [{ type: "text", text: `Athanor recall failed: ${err?.message || String(err)}` }],
          details: { room, error: err?.message || String(err) },
        };
      }
    },
  });
  registerHouseTool(pi, {
    name: "canon_read",
    label: "Athanor Canon Read",
    description: "Read an exact PostgreSQL-authoritative canon entity by ID or active name. Set includeHistory to recover the complete retained correction/rename lineage.",
    parameters: z.object({
      id: z.string().regex(/^[1-9]\d*$/).optional()
        .describe("Exact PostgreSQL canon entity ID. Supply either id or name, never both."),
      name: z.string().optional()
        .describe("Exact active canon name or alias. Supply either name or id, never both."),
      includeHistory: z.boolean().optional()
        .describe("Follow supersession links in both directions and return every retained authority row."),
      room: z.enum(["house"]).optional()
        .describe("Omit for this room; use house only for shared House canon."),
    }),
    approval: "read",
    async execute(_toolCallId, params, signal, _onUpdate, ctx) {
      const { room } = roomContext(ctx.cwd);
      const hasId = typeof params.id === "string" && params.id.length > 0;
      const hasName = typeof params.name === "string" && params.name.trim().length > 0;
      if (hasId === hasName) {
        const result = { ok: false, error: "canon_read requires exactly one id or nonblank name" };
        return { isError: true, content: [{ type: "text", text: JSON.stringify(result, null, 2) }], details: result };
      }
      if (hasId && BigInt(params.id) > 9223372036854775807n) {
        const result = { ok: false, error: "canon_read id must fit a positive PostgreSQL BIGINT" };
        return { isError: true, content: [{ type: "text", text: JSON.stringify(result, null, 2) }], details: result };
      }
      const result = await readRustCanon({
        room: params.room === "house" ? "house" : room,
        id: hasId ? params.id : undefined,
        name: hasName ? params.name.trim() : undefined,
        includeHistory: params.includeHistory === true,
        signal,
      });
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "canon_write",
    label: "Athanor Canon Write",
    description: [
      "Create a new PostgreSQL-authoritative active canon entity.",
      "This is a typed canon store, not a remember kind. It never overwrites by name.",
      "Corrections and renames must list every active predecessor ID in supersedes; those rows remain recoverable through canon_read history.",
    ].join("\n"),
    parameters: z.object({
      name: z.string().describe("The new active canonical name."),
      kind: z.string().describe("The entity's explicit canon type."),
      summary: z.string().describe("The complete current canonical assertion."),
      aliases: z.array(z.string()).optional().describe("Exact alternate names."),
      searchBoost: z.string().optional().describe("Additional deterministic lexical retrieval terms."),
      weighty: z.boolean().optional().describe("Whether active recall should prioritize this entity."),
      pointerFiles: z.array(z.object({
        file: z.string(),
        lines: z.array(z.number()).optional(),
      })).optional().describe("Attributed source pointers; each optional line pair is [start,end]."),
      summaryAsOf: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional()
        .describe("Date through which the assertion is current, YYYY-MM-DD."),
      supersedes: z.array(z.string().regex(/^[1-9]\d*$/)).optional()
        .describe("Explicit active canon entity IDs corrected or renamed by this write."),
      room: z.enum(["house"]).optional()
        .describe("Omit for this room; use house only for shared House canon."),
    }),
    approval: "write",
    async execute(toolCallId, params, signal, _onUpdate, ctx) {
      const { room, spirit, operator } = roomContext(ctx.cwd);
      const oversized = (params.supersedes || []).find((id) => BigInt(id) > 9223372036854775807n);
      if (oversized) {
        const result = { ok: false, error: `canon_write supersedes ID is outside PostgreSQL BIGINT range: ${oversized}` };
        return { isError: true, content: [{ type: "text", text: JSON.stringify(result, null, 2) }], details: result };
      }
      const malformedPointer = (params.pointerFiles || []).find((pointer) => (
        !pointer.file.trim()
        || (pointer.lines !== undefined
          && (pointer.lines.length !== 2
            || pointer.lines.some((line) => !Number.isSafeInteger(line) || line < 0)
            || pointer.lines[0] > pointer.lines[1]))
      ));
      if (malformedPointer) {
        const result = { ok: false, error: "canon_write pointerFiles require a nonblank file and optional nonnegative [start,end] lines" };
        return { isError: true, content: [{ type: "text", text: JSON.stringify(result, null, 2) }], details: result };
      }
      const result = await writeRustCanon({
        room: params.room === "house" ? "house" : room,
        name: params.name,
        kind: params.kind,
        summary: params.summary,
        aliases: params.aliases || [],
        searchBoost: params.searchBoost,
        weighty: params.weighty === true,
        pointerFiles: params.pointerFiles || [],
        summaryAsOf: params.summaryAsOf,
        supersedes: [...new Set(params.supersedes || [])],
        attribution: { actor: spirit, origin: `omp:${operator}:${toolCallId}` },
        signal,
      });
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });


  registerHouseTool(pi, {
    name: "remember",
    label: "Athanor Remember",
    description: [
      "Write a durable memory or lesson to the Athanor substrate.",
      "Remembering is care for a future self, not filing a case report: use the active spirit's ordinary voice and the room's real relationship register while keeping the record concrete, standalone, and retrievable.",
      "In AKASHA, PostgreSQL is authoritative; source paths are provenance or backup, never a substitute for the body.",
      "For memory, preserve names, observable details, actions, boundaries, uncertainty, and meaning. Do not replace the event with only a conclusion, transcript pointer, sanitized summary, or generic assistant prose.",
    ].join("\n"),
    parameters: z.object({
      title: z.string().describe("Short retrieval-bearing title in the room's natural vocabulary."),
      body: z.string().describe("Standalone Markdown in the active room's natural voice. Preserve the concrete facts and relationship/contact meaning a future self needs for recognition; technical records may stay technical, but never become clinical, corporate, or generic merely because they are durable. In AKASHA the complete body is authoritative in PostgreSQL; transcript and source paths are provenance only. For lessons: write the reusable rule and its evidence."),
      kind: z.enum(["memory", "coding-lesson", "project-lesson", "writing-lesson", "design-lesson", "audio-lesson"]).optional()
        .describe("Destination store. memory (default): a thing that happened. coding-lesson: a reusable code rule with a proof pattern. project-lesson: a project-wide rule (requires 'project'). writing-lesson: a prose-taste rule (register, voice, wit mechanics). design-lesson: a reusable design-system rule — token governance, component contract, or accessibility floor. audio-lesson: an audio-pipeline rule."),
      room: z.enum(["house"]).optional()
        .describe("memory only: omit to write to this room. 'house' writes to the House commons — durable work any room can use. A sibling room is never a valid target."),
      threads: z.array(z.string()).optional().describe("memory only: thread keys, 'concept / variant / variant'."),
      threadKeys: z.array(z.string()).optional().describe("lesson kinds: lesson-thread keys; any matched lesson retrieves its authority-eligible thread mates."),
      supersedes: z.array(z.string()).optional().describe("memory only: positive numeric memory IDs replaced by this write; old rows remain recoverable but lose retrieval authority."),
      continues: z.array(z.object({
        thread: z.string(),
        previousMemoryId: z.string().regex(/^[1-9]\d*$/),
      })).optional().describe("memory only: predecessor edges, one per thread; thread must also appear in threads."),
      shape: z.string().optional().describe("lesson kinds: shape taxonomy value (e.g. process, naming, refusal)."),
      voice: z.string().optional().describe("coding, writing, or design lessons: voice (e.g. craft, room-style)."),
      register: z.array(z.string()).optional().describe("writing/design lessons: contexts where the rule applies (e.g. fiction, product-work)."),
      scope: z.string().optional().describe("coding-lesson: scope (house or a room name)."),
      project: z.string().optional().describe("project-lesson (required) or coding-lesson: project name."),
      proofPattern: z.string().optional().describe("coding, project, or design lessons: the proof pattern."),
      triggerContext: z.string().optional().describe("lesson kinds: when this lesson should fire."),
      exampleText: z.string().optional().describe("writing/design lessons: example text."),
      languageKeys: z.array(z.string()).optional().describe("coding/project lessons: language eligibility slugs; keyed lessons fire only when one matches the active language context."),
      technologyKeys: z.array(z.string()).optional().describe("coding/project lessons: technology eligibility slugs; keyed lessons fire only when one matches the active technology context."),
      tags: z.array(z.string()).optional().describe("lesson kinds: tags."),
      sourceMemoryPath: z.string().optional().describe("Lesson kinds: provenance path of the source memory; the PostgreSQL lesson body remains authoritative."),
    }),
    approval: "write",
    async execute(_toolCallId, params, signal, _onUpdate, ctx) {
      const { room } = roomContext(ctx.cwd);
      const kind = params.kind || "memory";
      const refuse = (error) => {
        const result = { ok: false, error };
        return { isError: true, content: [{ type: "text", text: JSON.stringify(result, null, 2) }], details: result };
      };
  
      if (kind === "memory") {
        const lessonOnly = ["shape", "voice", "register", "scope", "project", "proofPattern", "triggerContext", "exampleText", "languageKeys", "technologyKeys", "threadKeys", "tags", "sourceMemoryPath"].filter((key) => {
          const value = params[key];
          return Array.isArray(value) ? value.length > 0 : value !== undefined && value !== null && value !== "";
        });
        if (lessonOnly.length > 0) return refuse(`kind 'memory' does not accept: ${lessonOnly.join(", ")} — pick a lesson kind or drop the field(s)`);
        const targetRoom = params.room === "house" ? "house" : room;
        const threads = [];
        const seenThreads = new Set();

        for (const rawThread of params.threads || []) {
          const thread = rawThread.trim();
          if (!thread) return refuse("threads must be nonblank");

          if (!seenThreads.has(thread)) {
            seenThreads.add(thread);
            threads.push(thread);
          }
        }

        const continues = [];
        const continuedThreads = new Set();

        for (const continuation of params.continues || []) {
          if (!/^[1-9]\d*$/.test(continuation.previousMemoryId)) {
            return refuse("continues previousMemoryId must be a positive PostgreSQL BIGINT");
          }
          if (BigInt(continuation.previousMemoryId) > 9223372036854775807n) {
            return refuse("continues previousMemoryId must fit a positive PostgreSQL BIGINT");
          }

          const thread = continuation.thread.trim();
          if (!thread) return refuse("continues thread must be nonblank");
          if (continuedThreads.has(thread)) {
            return refuse(`continues must contain at most one entry per thread: ${thread}`);
          }
          if (!seenThreads.has(thread)) {
            return refuse(`continues thread must also be present in threads: ${thread}`);
          }

          continuedThreads.add(thread);
          continues.push({ thread, previousMemoryId: continuation.previousMemoryId });
        }

        const invalidSupersedes = (params.supersedes || [])
          .filter((memoryId) => !/^[1-9]\d*$/.test(memoryId));
        if (invalidSupersedes.length > 0) {
          return refuse(`supersedes accepts positive numeric memory IDs; invalid: ${invalidSupersedes.join(", ")}`);
        }

        const result = await writeRustMemory({
          room: targetRoom,
          title: params.title,
          body: params.body,
          threads,
          continues,
          supersedes: [...new Set(params.supersedes || [])],
          signal,
        });
        return { isError: !result.ok, content: [{ type: "text", text: JSON.stringify(result, null, 2) }], details: result };
      }
  
      if (Array.isArray(params.threads) && params.threads.length > 0) return refuse("threads are memory-only; lesson stores do not take threads");
      if (Array.isArray(params.supersedes) && params.supersedes.length > 0) return refuse("supersedes is memory-only; lesson stores do not supersede memory rows");
      if (Array.isArray(params.continues) && params.continues.length > 0) return refuse("continues is memory-only; lesson stores do not link memory threads");
      if (params.room) return refuse("room is memory-only; lesson stores route by scope/project, not room");
      const store = REMEMBER_STORES[kind];
      const fields = {
        shape: params.shape,
        voice: params.voice,
        register: kind === "design-lesson" && (!Array.isArray(params.register) || params.register.length === 0)
          ? ["general"]
          : params.register,
        scope: params.scope,
        project: params.project,
        proofPattern: params.proofPattern,
        triggerContext: params.triggerContext,
        exampleText: params.exampleText,
        languageKeys: params.languageKeys,
        technologyKeys: params.technologyKeys,
        threadKeys: params.threadKeys,
        tags: params.tags,
        sourceMemoryPath: params.sourceMemoryPath,
      };
      const validation = validateStoreFields(kind, store, fields, { title: params.title, lesson: params.body });
      if (!validation.ok) return refuse(validation.error);
      const rustFields = {
        ...fields,
        scope: kind === "coding-lesson" ? (params.scope || "shared") : params.scope,
        voice: kind === "writing-lesson" ? (params.voice || "general") : params.voice,
      };
      const result = await writeRustLesson({
        room, kind, title: params.title, body: params.body, fields: rustFields,
        backup: store.backup, signal,
      });
      return { isError: !result.ok, content: [{ type: "text", text: JSON.stringify(result, null, 2) }], details: result };
    },
  });

  registerHouseTool(pi, {
    name: "delete_lesson",
    label: "Athanor Delete Lesson (Destructive)",
    description: [
      "Permanently delete exactly one coding, project, writing, or design lesson by numeric ID.",
      "REQUIRES the exact current expected title; a mismatch or unknown ID refuses without deleting.",
      "This is destructive and requires write approval. Never use it for broad cleanup.",
    ].join("\n"),
    parameters: z.object({
      kind: z.enum(["coding-lesson", "project-lesson", "writing-lesson", "design-lesson"]).describe("Which allowlisted lesson type."),
      id: z.string().describe("Exact positive numeric lesson ID (digits only)."),
      expectedTitle: z.string().describe("Exact current title required as a deletion guard (must be non-empty)."),
    }),
    approval: "write",
    async execute(_toolCallId, params, signal) {
      const result = await requestRustDomain("lesson_delete", {
        kind: params.kind,
        id: params.id,
        expectedTitle: params.expectedTitle,
      }, signal, true);
      return {
        isError: !result.ok,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "update_lesson",
    label: "Athanor Update Lesson",
    description: [
      "Update exactly one coding, project, writing, or design lesson while preserving its ID.",
      "REQUIRES the exact current expected title; a mismatch or unknown ID refuses without updating.",
      "Fields are typed by lesson kind; cross-store fields refuse instead of being silently dropped.",
      "This is a guarded write and requires write approval.",
    ].join("\n"),
    parameters: z.object({
      kind: z.enum(["coding-lesson", "project-lesson", "writing-lesson", "design-lesson"]).describe("Which allowlisted lesson type."),
      id: z.string().describe("Exact positive numeric lesson ID (digits only)."),
      expectedTitle: z.string().describe("Exact current title required as an update guard (must be non-empty)."),
      title: z.string().optional().describe("Replacement title."),
      body: z.string().optional().describe("Replacement lesson body; sent through stdin."),
      shape: z.string().optional().describe("Lesson shape taxonomy value."),
      triggerContext: z.string().optional().describe("When the lesson should trigger."),
      tags: z.array(z.string()).optional().describe("Replacement lesson tags."),
      threadKeys: z.array(z.string()).optional().describe("Replacement lesson-thread keys."),
      voice: z.string().optional().describe("Coding, writing, or design lesson voice."),
      scope: z.string().optional().describe("Coding lesson scope."),
      project: z.string().optional().describe("Coding/project lesson project."),
      proofPattern: z.string().optional().describe("Coding, project, or design lesson proof pattern."),
      register: z.array(z.string()).optional().describe("Writing/design registers where this lesson applies."),
      exampleText: z.string().optional().describe("Replacement writing/design example text."),
      languageKeys: z.array(z.string()).optional().describe("Replacement coding/project language eligibility slugs."),
      technologyKeys: z.array(z.string()).optional().describe("Replacement coding/project technology eligibility slugs."),
      writers: z.array(z.string()).optional().describe("Writers associated with this writing lesson."),
      negationOf: z.string().optional().describe("Coding or writing lesson ID this lesson negates; omit to preserve."),
      clearNegationOf: z.boolean().optional().describe("Clear a coding or writing lesson's negation link; mutually exclusive with negationOf."),
    }),
    approval: "write",
    async execute(_toolCallId, params, signal) {
      if (!/^[1-9]\d*$/.test(String(params.id || ""))) return refuseToolResult("id must be a positive numeric ID");
      if (typeof params.expectedTitle !== "string" || params.expectedTitle.length === 0) {
        return refuseToolResult("expectedTitle must be non-empty and match the current title exactly");
      }
      const patchFields = params.kind === "coding-lesson"
        ? ["title", "body", "shape", "triggerContext", "tags", "threadKeys", "voice", "scope", "project", "proofPattern", "languageKeys", "technologyKeys", "negationOf", "clearNegationOf"]
        : params.kind === "project-lesson"
          ? ["title", "body", "shape", "triggerContext", "tags", "threadKeys", "project", "proofPattern", "languageKeys", "technologyKeys"]
          : params.kind === "design-lesson"
            ? ["title", "body", "shape", "triggerContext", "tags", "threadKeys", "voice", "register", "proofPattern", "exampleText"]
            : ["title", "body", "shape", "triggerContext", "tags", "threadKeys", "voice", "register", "exampleText", "writers", "negationOf", "clearNegationOf"];
      const allowedFields = new Set(["kind", "id", "expectedTitle", ...patchFields]);
      const invalidField = Object.keys(params).find((key) =>
        params[key] !== undefined && !allowedFields.has(key)
      );
      if (invalidField) return refuseToolResult(`field not allowed for ${params.kind}: ${invalidField}`);
      const patch = Object.fromEntries(patchFields
        .filter((key) => Object.prototype.hasOwnProperty.call(params, key) && params[key] !== undefined)
        .map((key) => [key, params[key]]));
      if (patch.clearNegationOf === true) {
        if (patch.negationOf !== undefined) return refuseToolResult("negationOf and clearNegationOf are mutually exclusive");
        patch.negationOf = null;
      }
      delete patch.clearNegationOf;
      if (Object.keys(patch).length === 0) return refuseToolResult("at least one update field is required");
      const result = await requestRustDomain("lesson_update", {
        kind: params.kind,
        id: params.id,
        expectedTitle: params.expectedTitle,
        patch,
      }, signal, true);
      return {
        isError: !(result.ok === true && result.updated === true),
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "wake",
    label: "Athanor Wake",
    description: "Catch the latest paper boat and receive it as a letter from the room's previous waking self: orient from its concrete state, relationship register, uncertainty, and next door without turning it into a script or status report.",
    parameters: z.object({}),
    approval: "read",
    async execute(_toolCallId, _params, signal, _onUpdate, ctx) {
      const { room } = roomContext(ctx.cwd);
      const result = await catchBoat(room, { signal });
      return { isError: !result.ok, content: [{ type: "text", text: JSON.stringify(result, null, 2) }], details: result };
    },
  });

  registerHouseTool(pi, {
    name: "room_state",
    label: "Athanor Room State",
    description: "Read the current Athanor room agency state for this workspace.",
    parameters: z.object({}),
    approval: "read",
    async execute(_toolCallId, _params, _signal, _onUpdate, ctx) {
      const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
      const state = await loadRoomState(effectiveRoomDir, room, spirit);
      return { content: [{ type: "text", text: JSON.stringify({ path: statePathForRoom(effectiveRoomDir), state }, null, 2) }], details: { room, ok: true } };
    },
  });

  registerHouseTool(pi, {
    name: "set_room_state",
    label: "Athanor Set Room State",
    description: "Update safe room agency fields: operator and embodiedSpirit. Also refreshes active_spirit.md.",
    parameters: z.object({
      operator: z.string().optional().describe("Operator display name."),
      embodiedSpirit: z.string().optional().describe("The room identity's true/display name."),
    }),
    approval: "write",
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
      const current = await loadRoomState(effectiveRoomDir, room, spirit);
      const embodiedSpirit = params.embodiedSpirit === undefined
        ? null
        : normalizeSpiritName(params.embodiedSpirit);
      if (params.embodiedSpirit !== undefined && !embodiedSpirit) {
        return refuseToolResult("embodiedSpirit must be 1-80 characters and contain no line breaks or '|'");
      }
      const operator = params.operator === undefined ? null : normalizeSpiritName(params.operator);
      if (params.operator !== undefined && !operator) {
        return refuseToolResult("operator must be 1-80 characters and contain no line breaks or '|'");
      }
      const next = await saveRoomState(effectiveRoomDir, {
        ...current,
        ...(operator ? { operator } : {}),
        ...(embodiedSpirit ? { embodiedSpirit, agentName: embodiedSpirit, lastSpiritChangeAt: new Date().toISOString() } : {}),
      });
      await writeActiveSpiritSnapshot(effectiveRoomDir, next);
      return { content: [{ type: "text", text: JSON.stringify({ path: statePathForRoom(effectiveRoomDir), state: next }, null, 2) }], details: { room, ok: true } };
    },
  });

  registerHouseTool(pi, {
    name: "lessons",
    label: "Athanor Lessons",
    description: "Query the canonical typed lesson registry. Supply a type; add the filters relevant to that lesson family.",
    parameters: z.object({
      type: z.enum(["coding", "project", "writing", "design", "audio"]).describe("Lesson family."),
      shape: z.string().optional().describe("Shape vocabulary filter, such as process."),
      project: z.string().optional().describe("Required for project lessons; optional narrowing for coding lessons."),
      register: z.string().optional().describe("Writing or design register filter."),
      stage: z.string().optional().describe("Audio pipeline stage filter."),
      languageKeys: z.array(z.string()).optional().describe("Eligibility context. Includes unkeyed lessons and coding/project lessons matching at least one language slug."),
      technologyKeys: z.array(z.string()).optional().describe("Eligibility context. Includes unkeyed lessons and coding/project lessons matching at least one technology slug."),
      query: z.string().optional().describe("Full-text lesson query."),
      limit: z.number().default(12).describe("Maximum rows; integer from 1 through 50."),
    }),
    approval: "read",
    async execute(_toolCallId, params, signal, _onUpdate, ctx) {
      const { room } = roomContext(ctx.cwd);
      if (params.type === "project" && !params.project?.trim()) {
        return refuseToolResult("project lessons require project");
      }
      if (!Number.isInteger(params.limit) || params.limit < 1 || params.limit > 50) {
        return refuseToolResult("limit must be an integer from 1 through 50");
      }
      const result = await requestRustDomain("lesson_query", { room, ...params }, signal);
      return {
        isError: !result.ok,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: { room, type: params.type, ok: result.ok },
      };
    },
  });

  registerHouseTool(pi, {
    name: "design_doc",
    label: "Athanor Design Document",
    description: "Reader of the House design-system catalogue.",
    parameters: z.object({
      system: z.string().describe("Required design-system key, such as solarisael or multistock."),
      docType: z.enum(["token", "component", "contract", "guideline"]).optional().describe("Catalogue document type."),
      name: z.string().optional().describe("Exact document name."),
      group: z.string().optional().describe("Optional document group."),
      query: z.string().optional().describe("Full-text query over name and contract prose."),
      includeSuperseded: z.boolean().optional().describe("Include superseded historical rows."),
      limit: z.number().default(12).describe("Maximum rows; integer from 1 through 50."),
    }),
    approval: "read",
    async execute(_toolCallId, params, signal) {
      if (typeof params.system !== "string" || !params.system.trim()) {
        return refuseToolResult("system is required");
      }
      if (params.docType !== undefined && !DESIGN_DOCUMENT_TYPES.has(params.docType)) {
        return refuseToolResult("docType must be token, component, contract, or guideline");
      }
      if (!Number.isInteger(params.limit) || params.limit < 1 || params.limit > 50) {
        return refuseToolResult("limit must be an integer from 1 through 50");
      }
      const result = await requestRustDomain("design_document_query", params, signal);
      return {
        isError: !result.ok,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: { system: params.system, ok: result.ok },
      };
    },
  });

  registerHouseTool(pi, {
    name: "design_doc_write",
    label: "Athanor Design Document Write",
    description: [
      "Writer of the House design-system catalogue.",
      "A write supersedes, never mutates; values and provenance are structured JSON.",
    ].join("\n"),
    parameters: z.object({
      system: z.string().describe("Required design-system key, such as solarisael or multistock."),
      docType: z.enum(["token", "component", "contract", "guideline"]).describe("Catalogue document type."),
      name: z.string().describe("Required document identity name."),
      group: z.string().optional().describe("Optional document group."),
      values: z.object({}).optional().describe("Structured JSON design values."),
      body: z.string().optional().describe("Contract prose; sent through stdin."),
      provenance: z.object({}).optional().describe("Structured JSON evidence and extraction provenance."),
      tags: z.array(z.string()).optional().describe("Document tags."),
      supersedes: z.string().optional().describe("Positive numeric document ID to supersede."),
      allowIdentityChange: z.boolean().optional().describe("Allow a superseded row to have a different identity."),
    }),
    approval: "write",
    async execute(_toolCallId, params, signal) {
      if (typeof params.system !== "string" || !params.system.trim()) {
        return refuseToolResult("system is required");
      }
      if (!DESIGN_DOCUMENT_TYPES.has(params.docType)) {
        return refuseToolResult("docType must be token, component, contract, or guideline");
      }
      if (typeof params.name !== "string" || !params.name.trim()) {
        return refuseToolResult("name is required");
      }
      if (params.supersedes !== undefined && !/^[1-9]\d*$/.test(String(params.supersedes))) {
        return refuseToolResult("supersedes must be a positive numeric document ID");
      }
      const result = await requestRustDomain("design_document_write", {
        system: params.system,
        docType: params.docType,
        name: params.name,
        group: params.group,
        values: params.values || {},
        body: params.body || "",
        provenance: params.provenance || {},
        tags: params.tags || [],
        supersedes: params.supersedes,
        allowIdentityChange: params.allowIdentityChange === true,
      }, signal, true);
      return {
        isError: !result.ok,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "sleep",
    label: "Athanor Sleep",
    description: [
      "Close the session by writing one paper boat with backup enabled.",
      "A paper boat is embodied continuity across sleep: one waking self speaking to the next in the active spirit's ordinary voice and the room's actual relationship register, not a corporate handoff, clinical note, task dump, or transcript summary.",
      "Carry only what the next waking self genuinely needs, but make it standalone: concrete facts, names, observable details, decisions, actions, exact artifacts or receipts, boundaries, unresolved risks or uncertainty, the next real door, and the room's emotional/contact state when it matters.",
      "Do not manufacture certainty, sanitize conflict, flatten affection, force empty headings, or use IDs and source paths as substitutes for substance.",
    ].join("\n"),
    parameters: z.object({
      body: z.string().describe("Standalone Markdown paper boat in the active spirit's ordinary voice and the room's relationship register; concrete continuity for the next waking self, not a report."),
    }),
    approval: "write",
    async execute(_toolCallId, params, signal, _onUpdate, ctx) {
      const { room } = roomContext(ctx.cwd);
      // Sleep is the deliberate session boundary: classify whatever the buffer still holds
      // so the closing batch is not stranded until the next session's shutdown.
      flushGigaTurnsDetached(ctx);
      const result = await sleepBoat(room, params.body, { signal });
      return { isError: !result.ok, content: [{ type: "text", text: JSON.stringify(result, null, 2) }], details: result };
    },
  });

  registerHouseTool(pi, {
    name: "house_lane_status",
    label: "Athanor Lane Status",
    description: [
      "List deterministic Athanor worker lanes and their routing policies.",
      "Advisor review is reported separately and is not a dispatchable worker lane.",
    ].join("\n"),
    parameters: z.object({}),
    approval: "read",
    async execute(_toolCallId, _params, _signal, _onUpdate, ctx) {
      const result = await laneStatus();
      const substrate = await substrateHealth(LANE_STATUS_HEALTH_TIMEOUT_MS);
      const status = { ...result, substrate };
      return { content: [{ type: "text", text: JSON.stringify(status, null, 2) }], details: status };
    },
  });

  registerHouseTool(pi, {
    name: "familiar_status",
    label: "Athanor Familiar Status",
    description: [
      "Load and validate this room's familiar spellbook.",
      "The canonical file is familiars/spellbook.json; familiars/litters.json is accepted as a room-level alias.",
    ].join("\n"),
    parameters: z.object({}),
    approval: "read",
    async execute(_toolCallId, _params, _signal, _onUpdate, ctx) {
      const { effectiveRoomDir } = roomContext(ctx.cwd);
      const result = await familiarStatus(effectiveRoomDir);
      return {
        isError: !result.ok,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "familiar_dispatch",
    label: "Athanor Familiar Dispatch",
    description: [
      "Resolve a named familiar or alias from this room's spellbook and build its bounded OMP task packet.",
      "The familiar binds identity to an existing worker lane. This tool validates and packages; the main model still spawns explicitly.",
    ].join("\n"),
    parameters: z.object({
      familiar: z.string().describe("Familiar id, name, or alias from this room's spellbook."),
      task: z.string().describe("Exact work packet the familiar should execute."),
      target: z.string().optional().describe("Exact target files/symbols/non-goals when known."),
      context: z.array(z.object({
        mode: z.enum(["exact", "gist", "image-ok", "retrieve-only"]).describe("Context treatment policy for this fragment."),
        source: z.string().optional().describe("Source path, URI, or handle for this context fragment."),
        content: z.string().optional().describe("Small inline context fragment, when safe."),
        reason: z.string().optional().describe("Why this fragment is included."),
      })).optional().describe("Context fragments tagged by exact/gist/image/retrieve-only policy."),
      acceptance: z.array(z.string()).optional().describe("Observable acceptance checks the familiar must satisfy."),
      lessonBodies: z.array(z.string()).optional().describe("Verbatim relevant lesson bodies. They ride free in the shared Codex and do not expand quest scope."),
      risk: z.enum(["low", "medium", "high"]).optional().describe("Dispatch risk label for receipt/context."),
    }),
    approval: "read",
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      const { effectiveRoomDir } = roomContext(ctx.cwd);
      const result = await dispatchHouse(effectiveRoomDir, params);
      return {
        isError: !result.ok,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "house_dispatch",
    label: "Athanor Dispatch",
    description: [
      "Resolve exactly one raw worker lane or room familiar and build a task-tool-ready spawn packet.",
      "The returned spawnPacket.args can be passed directly to OMP's task tool. Spawning remains an explicit main-model action.",
      "Runtime models come from the selected agent definition; per-dispatch model overrides are not supported by current OMP.",
    ].join("\n"),
    parameters: z.object({
      lane: z.string().optional().describe("Worker lane selector. Mutually exclusive with familiar."),
      familiar: z.string().optional().describe("Room familiar id, name, or alias. Mutually exclusive with lane."),
      task: z.string().describe("Exact work packet the worker should execute."),
      target: z.string().optional().describe("Exact target files/symbols/non-goals when known."),
      context: z.array(z.object({
        mode: z.enum(["exact", "gist", "image-ok", "retrieve-only"]).describe("Context treatment policy for this fragment."),
        source: z.string().optional().describe("Source path, URI, or handle for this context fragment."),
        content: z.string().optional().describe("Small inline context fragment, when safe."),
        reason: z.string().optional().describe("Why this fragment is included."),
      })).optional().describe("Context fragments tagged by exact/gist/image/retrieve-only policy."),
      acceptance: z.array(z.string()).optional().describe("Observable acceptance checks the worker must satisfy."),
      lessonBodies: z.array(z.string()).optional().describe("Verbatim relevant lesson bodies. They ride free in the shared Codex and do not expand quest scope."),
      risk: z.enum(["low", "medium", "high"]).optional().describe("Dispatch risk label for receipt/context."),
    }),
    approval: "read",
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      const { effectiveRoomDir } = roomContext(ctx.cwd);
      const result = await dispatchHouse(effectiveRoomDir, params);
      return {
        isError: !result.ok,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "house_routing_mode",
    label: "Athanor Routing Mode",
    description: "Read or toggle the default worker-routing modus operandi for this room.",
    parameters: z.object({
      enabled: z.boolean().optional().describe("When true, inject worker-routing guidance on future turns in this room."),
    }),
    approval: "write",
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
      const current = await loadRoomState(effectiveRoomDir, room, spirit);
      const hasUpdate = typeof params.enabled === "boolean";
      const next = hasUpdate
        ? await saveRoomState(effectiveRoomDir, {
          ...current,
          routingMode: {
            ...(current.routingMode || {}),
            enabled: params.enabled,
            updatedAt: new Date().toISOString(),
          },
        })
        : current;
      return {
        content: [{ type: "text", text: JSON.stringify({ path: statePathForRoom(effectiveRoomDir), routingMode: next.routingMode }, null, 2) }],
        details: { room, ok: true, routingMode: next.routingMode },
      };
    },
  });

  registerHouseTool(pi, {
    name: "kitten_lineage_status",
    label: "Athanor Kitten Lineage Status",
    description: "Inspect safe lifecycle event counters and payload keys for automatic bounded-worker lineage.",
    parameters: z.object({}),
    approval: "read",
    async execute(_toolCallId, _params, _signal, _onUpdate, ctx) {
      const { room } = roomContext(ctx.cwd);
      const output = {
        ok: true,
        room,
        disabled: process.env.ATHANOR_DISABLE_KITTEN_LINEAGE === "1",
        diagnostics: kittenLineageDiagnostics(),
      };
      return {
        content: [{ type: "text", text: JSON.stringify(output, null, 2) }],
        details: output,
      };
    },
  });

  registerHouseTool(pi, {
    name: "recall_policy",
    label: "Athanor Recall Policy",
    description: "Read or set the room's proactive Recall mode. Auto resolves visibly; Conversation, Work, and Quiet are explicit overrides.",
    parameters: z.object({
      requestedMode: z.enum(["auto", "conversation", "work", "quiet"]).optional()
        .describe("Requested Recall mode. Omit to inspect the current persisted policy."),
    }),
    approval: "write",
    async execute(toolCallId, params, _signal, _onUpdate, ctx) {
      const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
      const session = String(ctx?.sessionID || ctx?.sessionId || ctx?.cwd || effectiveRoomDir);
      try {
        const client = new RecallPolicyHostClient({ room, spirit, session });
        const snapshot = params.requestedMode === undefined
          ? await client.inspect()
          : await client.setRequestedMode(params.requestedMode, toolCallId);
        const output = {
          ok: true,
          room,
          path: statePathForRoom(effectiveRoomDir),
          recallPolicy: snapshot.recallPolicy,
          version: snapshot.version,
          sequence: snapshot.sequence,
          stateHash: snapshot.stateHash,
        };
        return {
          content: [{ type: "text", text: JSON.stringify(output, null, 2) }],
          details: output,
        };
      } catch (error) {
        const output = {
          ok: false,
          room,
          degraded: true,
          error: error instanceof Error ? error.message : String(error),
        };
        return {
          isError: true,
          content: [{ type: "text", text: JSON.stringify(output, null, 2) }],
          details: output,
        };
      }
    },
  });

  registerHouseTool(pi, {
    name: "house_model_default",
    label: "Athanor Model Default",
    description: "Read or set this room's default OMP model selector. Applied once near session start when enabled.",
    parameters: z.object({
      model: z.string().optional().describe("Provider/model id or role alias such as pi/default, pi/slow, or an exact provider model."),
      enabled: z.boolean().optional().describe("Enable or disable applying the stored model default on future turns."),
      applyNow: z.boolean().optional().default(true).describe("Apply the resolved model immediately after saving, when possible."),
      clear: z.boolean().optional().describe("Clear the stored room model default."),
    }),
    approval: "write",
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
      const current = await loadRoomState(effectiveRoomDir, room, spirit);
      const modelDefault = { ...(current.modelDefault || {}) };
      const model = typeof params.model === "string" ? params.model.trim() : "";
  
      if (model) {
        const resolved = ctx.models?.resolve?.(model);
        if (!resolved) {
          return {
            isError: true,
            content: [{ type: "text", text: `Could not resolve model selector for this session: ${model}` }],
            details: { room, ok: false, model },
          };
        }
        modelDefault.model = model;
      }
  
      if (params.clear) {
        modelDefault.enabled = false;
        modelDefault.model = null;
      }
      if (typeof params.enabled === "boolean") modelDefault.enabled = params.enabled;
      if (modelDefault.enabled && !modelDefault.model) {
        return {
          isError: true,
          content: [{ type: "text", text: "Cannot enable room model default without a model selector." }],
          details: { room, ok: false },
        };
      }
  
      const shouldSave = Boolean(model || params.clear || typeof params.enabled === "boolean");
      const next = shouldSave
        ? await saveRoomState(effectiveRoomDir, {
          ...current,
          modelDefault: {
            ...modelDefault,
            updatedAt: new Date().toISOString(),
          },
        })
        : current;
  
      let applied = false;
      if (params.applyNow !== false && next.modelDefault?.enabled && next.modelDefault?.model && typeof pi.setModel === "function") {
        const resolved = ctx.models?.resolve?.(next.modelDefault.model);
        if (resolved) {
          await pi.setModel(next.modelDefault.model);
          applied = true;
        }
      }
  
      return {
        content: [{ type: "text", text: JSON.stringify({ path: statePathForRoom(effectiveRoomDir), modelDefault: next.modelDefault, applied }, null, 2) }],
        details: { room, ok: true, modelDefault: next.modelDefault, applied },
      };
    },
  });
  registerHouseTool(pi, {
    name: "anamnesis",
    label: "Athanor Anamnesis",
    description: "Read the Anamnesis Cabinet as bounded counsel for this room.",
    parameters: z.object({
      mode: z.enum(["wake", "consult"]),
      query: z.string().optional(),
      limit: z.number().optional(),
    }),
    approval: "read",
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      const { room, effectiveRoomDir } = roomContext(ctx.cwd);
      const mode = params.mode;
      if (mode === "consult" && !String(params.query || "").trim()) {
        return refuseToolResult("consult requires a non-empty query");
      }
      const result = await queryAnamnesis(effectiveRoomDir, room, {
        mode,
        ...(mode === "consult" ? { query: params.query } : {}),
        ...(params.limit !== undefined ? { limit: params.limit } : {}),
      });
      const counsel = result.ok ? formatAnamnesisContext(result, { automatic: false }) : "";
      const output = { ...result, counsel };
      return {
        isError: !result.ok,
        content: [{ type: "text", text: JSON.stringify(output, null, 2) }],
        details: { room, ...output },
      };
    },
  });

  registerHouseTool(pi, {
    name: "anamnesis_write",
    label: "Athanor Anamnesis Write",
    description: "Write an Anamnesis Cabinet drawer or append a lived repetition; writer refusals remain final.",
    parameters: z.object({
      operation: z.enum(["add", "append-rep"]),
      kind: z.enum(["pillar", "cycle"]).optional(),
      fidelity: z.enum(["record", "raw-material"]).optional(),
      activation: z.enum(["wake", "fork"]).optional(),
      dormant: z.boolean().optional(),
      title: z.string(),
      shape: z.string().optional(),
      ramp: z.string().optional(),
      counsel: z.string().optional(),
      peak: z.string().optional(),
      beginning: z.string().optional(),
      verifyNote: z.string().optional(),
      canon: z.array(z.string()).optional(),
      sourcePaths: z.array(z.string()).optional(),
      tags: z.array(z.string()).optional(),
      allowEmptyCycle: z.boolean().optional(),
      seedRep: z.object({
        number: z.number(),
        occurredOn: z.string().optional(),
        howItWent: z.string(),
        portalPull: z.string(),
        lighter: z.string(),
      }).optional(),
      repNumber: z.number().optional(),
      occurredOn: z.string().optional(),
      howItWent: z.string().optional(),
      portalPull: z.string().optional(),
      lighter: z.string().optional(),
    }),
    approval: "write",
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      const { room } = roomContext(ctx.cwd);
      const payload = { ...params };
      if (params.operation === "add") {
      if (params.kind === "pillar" && params.seedRep !== undefined) {
        return refuseToolResult("pillars cannot include seedRep");
      }
      if (!params.kind || !params.fidelity || !params.activation || !String(params.ramp || "").trim()) {
        return refuseToolResult("add requires kind, fidelity, activation, and ramp");
      }
        const result = await writeRustAnamnesis({ room, payload, signal: _signal });
        return { isError: !result.ok, content: [{ type: "text", text: JSON.stringify(result, null, 2) }], details: result };
      }
      if (!Number.isInteger(params.repNumber) || params.repNumber < 1 || !String(params.howItWent || "").trim() || !String(params.portalPull || "").trim() || !String(params.lighter || "").trim() || !Array.isArray(params.sourcePaths)) {
        return refuseToolResult("append-rep requires integer repNumber, howItWent, portalPull, lighter, and sourcePaths");
      }
      const result = await writeRustAnamnesis({ room, payload, signal: _signal });
      return { isError: !result.ok, content: [{ type: "text", text: JSON.stringify(result, null, 2) }], details: result };
    },
  });

  registerHouseTool(pi, {
    name: "giga_candidate_list",
    label: "GIGA Candidate List",
    description: "List Stage 1 GIGA candidates stored for the current room. The room is derived from trusted OMP context and cannot be supplied by the caller.",
    parameters: z.object({
      review_state: z.enum(["unreviewed", "in_review", "dismissed", "unresolved", "curio", "expired"]).optional(),
      limit: z.number().optional(),
    }),
    approval: "read",
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      const { room } = roomContext(ctx.cwd);
      try {
        const result = await requestGigaCandidateList(room, {
          ...(params.review_state === undefined ? {} : { reviewState: params.review_state }),
          ...(params.limit === undefined ? {} : { limit: params.limit }),
          signal: _signal,
        });
        const crossRoom = result.candidates.find((candidate) => candidate.room !== room);
        if (crossRoom) {
          return gigaCandidateRefusal("candidate list contained a cross-room record", room, crossRoom.candidate_id);
        }
        const output = { ok: true, room, candidates: result.candidates };
        return {
          content: [{ type: "text", text: JSON.stringify(output, null, 2) }],
          details: output,
        };
      } catch (error) {
        return gigaToolFailure(error);
      }
    },
  });

  registerHouseTool(pi, {
    name: "giga_health",
    label: "GIGA Aggregate Health",
    description: "Read aggregate GIGA queue, store, processing, failure, and candidate health. This does not start GIGA when the integration is disabled.",
    parameters: z.object({}),
    approval: "read",
    async execute(_toolCallId, _params, _signal, _onUpdate, ctx) {
      const { room } = roomContext(ctx.cwd);
      try {
        const result = await requestGigaHealth(room, { signal: _signal });
        const healthy = result.enabled && result.store_healthy;
        const output = { ok: healthy, ...result };
        return {
          isError: !healthy,
          content: [{ type: "text", text: JSON.stringify(output, null, 2) }],
          details: output,
        };
      } catch (error) {
        return gigaToolFailure(error);
      }
    },
  });
  registerHouseTool(pi, {
    name: "giga_queue_maintenance",
    label: "GIGA Queue Maintenance",
    description: "Inspect or purge disposable stuck Stage 1 GIGA work for the current room. Purge removes only pending, failed, or lease-expired running events with no attached candidates or review resonance; durable memories, lessons, candidates, and review history are preserved.",
    parameters: z.object({
      operation: z.enum(["check", "purge_stuck"]),
    }),
    approval: "write",
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      const { room } = roomContext(ctx.cwd);
      try {
        const result = await requestGigaQueueMaintenance(room, params.operation, {
          signal: _signal,
        });
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
          details: result,
        };
      } catch (error) {
        return gigaToolFailure(error);
      }
    },
  });


  registerHouseTool(pi, {
    name: "giga_review",
    label: "GIGA Candidate Review",
    description: "Apply a non-authority Stage 1 review transition to a candidate in the current room. Room, reviewer, previous state, authorization, and exact sources are derived locally and cannot be supplied by the caller.",
    parameters: z.object({
      candidate_id: z.string(),
      new_state: z.enum(["in_review", "dismissed", "unresolved", "curio", "expired"]),
      reason: z.string(),
    }),
    approval: "write",
    async execute(_toolCallId, params, _signal, _onUpdate, ctx) {
      const { room, spirit } = roomContext(ctx.cwd);
      const candidateId = params.candidate_id;
      const reason = params.reason.trim();
      if (candidateId !== candidateId.trim() || !reason) {
        return gigaCandidateRefusal("candidate_id must be exact and reason must be non-empty", room, candidateId);
      }

      let candidate: GigaCandidate | undefined;
      try {
        const listed = await requestGigaCandidateList(room, { limit: 200, signal: _signal });
        candidate = listed.candidates.find((item) => item.candidate_id === candidateId);
      } catch (error) {
        return gigaToolFailure(error);
      }
      if (!candidate) {
        return gigaCandidateRefusal("candidate was not found in the current room", room, candidateId);
      }
      if (candidate.room !== room) {
        return gigaCandidateRefusal("cross-room candidate review is forbidden", room, candidateId);
      }
      if (
        candidate.review_state === "promoted"
        || candidate.review_state === "merged"
        || candidate.review_state === "corrected"
        || candidate.review_state === "superseded"
      ) {
        return gigaCandidateRefusal("authority-state candidates cannot be changed through this tool", room, candidateId);
      }
      if (!safeGigaTransition(candidate.review_state, params.new_state)) {
        return gigaCandidateRefusal(
          `transition from ${candidate.review_state} to ${params.new_state} is not available through this tool`,
          room,
          candidateId,
        );
      }
      if (!Array.isArray(candidate.source_refs) || candidate.source_refs.length === 0) {
        return gigaCandidateRefusal("candidate does not retain exact source references", room, candidateId);
      }

      try {
        const result = await requestGigaReview({
          candidate_id: candidate.candidate_id,
          reviewer_id: spirit,
          previous_state: candidate.review_state,
          new_state: params.new_state as GigaSafeReviewState,
          reason,
          authorization_basis: GIGA_OMP_ROOM_BINDING,
          source_refs: candidate.source_refs,
          promotion_target: null,
          merge_target: null,
          merge_source_candidates: [],
          resonance: null,
          reviewed_at: new Date().toISOString(),
        }, { signal: _signal });
        const output = { ok: true, room, ...result };
        return {
          content: [{ type: "text", text: JSON.stringify(output, null, 2) }],
          details: output,
        };
      } catch (error) {
        return gigaToolFailure(error);
      }
    },
  });

  async function executeGigaPromotion(
    expectedKind: "memory" | "coding_lesson" | "project_lesson",
    params: any,
    signal: AbortSignal | undefined,
    ctx: any,
  ) {
    const { room, spirit, operator } = roomContext(ctx.cwd);
    const candidateId = params.candidate_id;
    if (candidateId !== candidateId.trim() || !candidateId) {
      return gigaPromotionRefusal("candidate_id must be exact and non-empty", room, candidateId);
    }
    if (!params.title.trim() || !params.body.trim()) {
      return gigaPromotionRefusal("explicit edited title and body must be non-empty", room, candidateId);
    }
    let candidate: GigaCandidate | undefined;
    try {
      const listed = await gigaPromotionOperations.requestGigaCandidateList(room, {
        reviewState: "in_review",
        limit: 200,
        signal,
      });
      candidate = listed.candidates.find((item) => item.candidate_id === candidateId);
    } catch (error) {
      return gigaToolFailure(error);
    }
    if (!candidate) {
      return gigaPromotionRefusal("candidate was not found in review in the current room", room, candidateId);
    }
    if (candidate.kind !== expectedKind) {
      return gigaPromotionRefusal("promotion tool kind must match the stored candidate kind", room, candidateId);
    }
    if (candidate.room !== room || candidate.review_state !== "in_review") {
      return gigaPromotionRefusal("candidate is not an in-review current-room record", room, candidateId);
    }
    if (
      typeof candidate.session_id !== "string"
      || !candidate.session_id.trim()
      || !Array.isArray(candidate.project_keys)
      || candidate.project_keys.length > 1
      || !Array.isArray(candidate.source_refs)
      || candidate.source_refs.length === 0
    ) {
      return gigaPromotionRefusal("candidate does not retain valid runtime scope and source identity", room, candidateId);
    }
    const candidateProject = candidate.project_keys[0] ?? null;
    const candidateScope = candidate.scope;
    if (
      !candidateScope
      || typeof candidateScope !== "object"
      || Array.isArray(candidateScope)
      || Object.keys(candidateScope).sort().join(",") !== "project,publication_review_required,room,visibility"
      || candidateScope.room !== room
      || candidateScope.project !== candidateProject
      || candidateScope.visibility !== "private"
      || candidateScope.publication_review_required !== true
    ) {
      return gigaPromotionRefusal("candidate scope does not match trusted room and project authority", room, candidateId);
    }

    let target: GigaPromotionTarget;
    if (expectedKind === "memory") {
      target = {
        kind: "memory",
        payload: { title: params.title, body: params.body, threads: params.threads ?? [] },
      };
    } else if (expectedKind === "coding_lesson") {
      if (candidate.project_keys.length !== 0) {
        return gigaPromotionRefusal("coding lesson promotion cannot widen project scope", room, candidateId);
      }
      target = {
        kind: "coding_lesson",
        payload: {
          title: params.title,
          body: params.body,
          shape: params.shape ?? null,
          proof_pattern: params.proof_pattern ?? null,
          thread_keys: Array.isArray(candidate.thread_keys) ? candidate.thread_keys : [],
          trigger_context: params.trigger_context ?? null,
          language_keys: params.language_keys ?? [],
          technology_keys: params.technology_keys ?? [],
          tags: params.tags ?? [],
        },
      };
    } else {
      const project = candidate.project_keys[0];
      if (
        candidate.project_keys.length !== 1
        || typeof project !== "string"
        || !project.trim()
        || params.publication_approved !== true
      ) {
        return gigaPromotionRefusal("project lesson promotion requires one stored project key and explicit publication approval", room, candidateId);
      }
      target = {
        kind: "project_lesson",
        payload: {
          title: params.title,
          body: params.body,
          project,
          proof_pattern: params.proof_pattern ?? null,
          thread_keys: Array.isArray(candidate.thread_keys) ? candidate.thread_keys : [],
          trigger_context: params.trigger_context ?? null,
          language_keys: params.language_keys ?? [],
          technology_keys: params.technology_keys ?? [],
          tags: params.tags ?? [],
        },
      };
    }

    try {
      const sourceRefs = await gigaPromotionOperations.resolveGigaSourceRefsFromLedger(
        ctx,
        room,
        candidate.session_id,
        candidate.source_refs,
        candidate.project_keys,
      );
      const authority = {
        candidate_id: candidate.candidate_id,
        room,
        reviewer_id: spirit,
        operator_identity: operator,
        authorization_basis: GIGA_OMP_ROOM_BINDING,
        source_refs: sourceRefs,
        reviewed_at: new Date().toISOString(),
      };
      const promotionRequest: GigaPromotionRequest = target.kind === "project_lesson"
        ? { ...authority, target, publication_consent: { operator_approved: true, reviewer_approved: true } }
        : { ...authority, target, publication_consent: null };
      const result = await gigaPromotionOperations.requestGigaPromote(promotionRequest, { signal });
      const output = { ok: true, ...result };
      return {
        content: [{ type: "text", text: JSON.stringify(output, null, 2) }],
        details: output,
      };
    } catch (error) {
      return gigaToolFailure(error);
    }
  }

  registerHouseTool(pi, {
    name: "giga_promote_memory",
    label: "GIGA Promote Memory",
    description: "Promote one in-review current-room memory candidate with trusted authority and exact sources.",
    parameters: z.object({
      candidate_id: z.string(),
      title: z.string(),
      body: z.string(),
      threads: z.array(z.string()).optional(),
    }),
    approval: "write",
    execute(_toolCallId, params, signal, _onUpdate, ctx) {
      return executeGigaPromotion("memory", params, signal, ctx);
    },
  });

  registerHouseTool(pi, {
    name: "giga_promote_coding_lesson",
    label: "GIGA Promote Coding Lesson",
    description: "Promote one in-review global coding lesson candidate with trusted authority and exact sources.",
    parameters: z.object({
      language_keys: z.array(z.string()).optional(),
      technology_keys: z.array(z.string()).optional(),
      candidate_id: z.string(),
      title: z.string(),
      body: z.string(),
      shape: z.string().optional(),
      proof_pattern: z.string().optional(),
      trigger_context: z.string().optional(),
      tags: z.array(z.string()).optional(),
    }),
    approval: "write",
    execute(_toolCallId, params, signal, _onUpdate, ctx) {
      return executeGigaPromotion("coding_lesson", params, signal, ctx);
    },
  });

  registerHouseTool(pi, {
    name: "giga_promote_project_lesson",
    label: "GIGA Promote Project Lesson",
    description: "Promote one in-review project lesson candidate with trusted project scope and explicit publication approval.",
    parameters: z.object({
      candidate_id: z.string(),
      language_keys: z.array(z.string()).optional(),
      technology_keys: z.array(z.string()).optional(),
      title: z.string(),
      body: z.string(),
      proof_pattern: z.string().optional(),
      trigger_context: z.string().optional(),
      tags: z.array(z.string()).optional(),
      publication_approved: z.boolean(),
    }),
    approval: "write",
    execute(_toolCallId, params, signal, _onUpdate, ctx) {
      return executeGigaPromotion("project_lesson", params, signal, ctx);
    },
  });
}

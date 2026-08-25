// Tool registration for the OMP adapter.
// Silhouette: expose room/substrate tools; keep hook wiring out of tool bodies.

import { recallWithRouting } from "./recall.ts";
import {
  loadRoomState,
  normalizeSpiritName,
  roomCapability,
  roomContext,
  saveRoomState,
  statePathForRoom,
  writeActiveSpiritSnapshot,
} from "./room.ts";
import { RecallPolicyHostClient } from "./recall-policy.ts";
import { embodiedSession, hostHouseId, hostSessionIdentity } from "./host.ts";
import { applyRecallViewport } from "./context.ts";
import { kittenLineageDiagnostics } from "../kitten-lineage.ts";
import { queryAnamnesis, formatAnamnesisContext } from "./anamnesis.ts";
import {
  catchBoat,
  sleepBoat,
  substrateHealth,
} from "./substrate.ts";
import { RustJsonlTransport, RustTransportError, RustTransportOutcomeUnknownError } from "../rust-transport.ts";
import { discoverRustExecutable } from "../discovery.ts";
import { dispatchHouse, familiarStatus, laneStatus } from "./routing.ts";
import { WRITE_TIMEOUT_MS } from "./constants.ts";
import {
  beginHouseToolFeedback,
  completeHouseToolFeedback,
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
  type GigaPromotionTarget,
  type GigaSafeReviewState,
} from "../giga.ts";

const rustRememberTransports = new Map<string, RustJsonlTransport>();
const LANE_STATUS_HEALTH_TIMEOUT_MS = 3_000;
const DESIGN_DOCUMENT_TYPES = new Set(["token", "component", "contract", "guideline"]);

function hostBinding(ctx: any) {
  const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
  return {
    binding: {
      room,
      spirit,
      session: hostSessionIdentity(ctx, effectiveRoomDir),
    },
    effectiveRoomDir,
  };
}

// Docket writes carry an operation-scoped room capability. It is resolved from
// the room the caller is standing in, never accepted as a parameter and never
// echoed into a receipt, so no spirit can borrow another room's authority by
// typing one. An unprovisioned room refuses here, before the substrate call.
const DOCKET_WRITE_GATE = "docket_write capability";

function docketWriteBinding(ctx: any) {
  const { binding, effectiveRoomDir } = hostBinding(ctx);
  return { binding, capability: roomCapability(effectiveRoomDir) };
}

// Worker-rejecting fence at the organ door (guild-hall #144, settled NOT_MET
// against M1 criterion 4 before this cut existed). A docket write must come
// from the room's embodied session; a worker spawned by the task tool carries
// its own session identity and refuses here, typed, before any capability or
// substrate work. Worker output enters receipts through the spirit's hand.
function refuseWorkerHands(gate: string) {
  return refuseDocket(
    "worker_hands_off",
    gate,
    "docket writes require the room's embodied session; worker evidence enters through the spirit's hand",
  );
}

export function workerAtTheDoor(
    _ctx: any,
    binding: { room: string; session: string },
): boolean {
  const embodied = embodiedSession(binding.room);
  // No registered embodiment means no session_start has run in this process;
  // fail closed. A fence that opens when unsure is not a fence.
  if (!embodied) return true;
  return binding.session !== embodied;
}

function refuseDocket(code: string, gate: string, error: string) {
  const result = { ok: false, code, gate, error };
  return {
    isError: true,
    content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
    details: result,
  };
}

function refuseUnprovisionedRoom() {
  return refuseDocket("capability_not_provisioned", DOCKET_WRITE_GATE, "room capability not provisioned");
}

// A Docket row belongs to a House. The caller may name one; otherwise the
// installation's own House id answers. Nothing is invented when neither does.
function docketHouseId(requested: unknown) {
  return (typeof requested === "string" ? requested.trim() : "") || hostHouseId();
}



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
  const source = error && typeof error === "object" ? error as { details?: unknown } : {};
  return source.details && typeof source.details === "object" && !Array.isArray(source.details)
    ? source.details as Record<string, unknown>
    : {};
}

function isOutcomeUnknownError(error: unknown): boolean {
  return error instanceof RustTransportOutcomeUnknownError;
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

export async function writeRustMemory({ room, title, body, threads, continues, supersedes, condition, astCondition, triggerScope, interruptMode, repeatCooldownSecs, signal }) {
  const transport = rustRememberTransport();
  if (!transport) return { ok: false, error: "Rust substrate executable is unavailable" };
  try {
    const value = await transport.request("remember", {
      room,
      kind: "memory",
      title,
      body,
      threads,
      continues,
      supersedes,
      condition,
      astCondition,
      triggerScope,
      interruptMode,
      repeatCooldownSecs,
      backup: false,
    }, {
      signal: signal || undefined,
      timeoutMs: WRITE_TIMEOUT_MS,
      settleDefinitively: true,
    }) as Record<string, unknown>;
    return { ok: true, ...value, id: value.memory_id, sourcePath: value.source_path };
  } catch (error) {
    if (error instanceof RustTransportError) return rustFailureReceipt(error);
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}

async function writeRustLesson({ room, kind, title, body, fields, backup, signal }) {
  const transport = rustRememberTransport();
  if (!transport) return { ok: false, error: "Rust substrate executable is unavailable" };
  try {
    const value = await transport.request("remember", {
      room,
      kind,
      title,
      body,
      shape: fields.shape,
      voice: fields.voice,
      register: fields.register,
      scope: fields.scope,
      project: fields.project,
      proofPattern: fields.proofPattern,
      triggerContext: fields.triggerContext,
      exampleText: fields.exampleText,
      sourceMemoryPath: fields.sourceMemoryPath,
      languageKeys: fields.languageKeys,
      technologyKeys: fields.technologyKeys,
      tags: fields.tags,
      threadKeys: fields.threadKeys,
      condition: fields.condition,
      astCondition: fields.astCondition,
      triggerScope: fields.triggerScope,
      interruptMode: fields.interruptMode,
      repeatCooldownSecs: fields.repeatCooldownSecs,
      backup,
    }, {
      signal: signal || undefined,
      timeoutMs: WRITE_TIMEOUT_MS,
      settleDefinitively: true,
    }) as Record<string, unknown>;
    return { ok: true, ...value, id: value.lesson_id };
  } catch (error) {
    if (error instanceof RustTransportError) return rustFailureReceipt(error);
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


function registerHouseTool(pi, definition) {
  const execute = definition.execute;
  const renderers = createToolRenderers(definition.label, definition.name);
  pi.registerTool({
    ...definition,
    ...renderers,
    async execute(toolCallId, params, signal, onUpdate, ctx) {
      emitToolUpdate(onUpdate, definition.name);
      beginHouseToolFeedback(ctx, definition.label);
      try {
        const result = normalizeToolResponse(
          await execute(toolCallId, params, signal, onUpdate, ctx),
          definition.name,
        );
        completeHouseToolFeedback(ctx, definition.label, result);
        return result;
      } catch (error) {
        const result = toolThrown(error, definition.name);
        completeHouseToolFeedback(ctx, definition.label, result);
        return result;
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
    async execute(toolCallId, params, _signal, _onUpdate, ctx) {
      const { room, spirit, effectiveRoomDir } = roomContext(ctx.cwd);
      const session = hostSessionIdentity(ctx, effectiveRoomDir);
      try {
        const recalled = await recallWithRouting(effectiveRoomDir, room, params.query, { signal: _signal, temporalDecay: false });
        if (!recalled.ok) {
          return {
            isError: true,
            content: [{ type: "text", text: JSON.stringify(recalled.result, null, 2) }],
            details: { room, ok: false },
          };
        }
        const viewport = await applyRecallViewport(
          { room, spirit, session },
          recalled.result,
          "manual",
          `${toolCallId}:manual-viewport`,
        );
        const compact = viewport.presentation;
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
      const result = await readRustCanon({
        room: params.room === "house" ? "house" : room,
        id: params.id,
        name: params.name,
        includeHistory: params.includeHistory,
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
      const result = await writeRustCanon({
        room: params.room === "house" ? "house" : room,
        name: params.name,
        kind: params.kind,
        summary: params.summary,
        aliases: params.aliases,
        searchBoost: params.searchBoost,
        weighty: params.weighty,
        pointerFiles: params.pointerFiles,
        summaryAsOf: params.summaryAsOf,
        supersedes: params.supersedes,
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
      })).optional().describe("memory only: predecessor edges, one per thread. The predecessor must belong to the target memory's room and the exact thread key must appear in both memories; cross-room conceptual provenance belongs in the body instead. Each thread must also appear in threads."),
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
      condition: z.array(z.string()).optional().describe("lesson kinds only: Rust-regex trigger patterns (no lookaround or backreferences); the substrate refuses these for memory."),
      astCondition: z.array(z.string()).optional().describe("lesson kinds only: ast-grep patterns, each a single AST node in rust/typescript/tsx/javascript/jsx/python; the substrate refuses these for memory."),
      triggerScope: z.array(z.string()).optional().describe("lesson kinds only: scope tokens (text | tool | tool:<name>); requires condition or astCondition."),
      interruptMode: z.enum(["block", "remind"]).optional().describe("lesson kinds only: omit for the house default block; remind is the demoted mode."),
      repeatCooldownSecs: z.number().optional().describe("lesson kinds only: positive seconds between fires; omit for once per session."),
    }),
    approval: "write",
    async execute(_toolCallId, params, signal, _onUpdate, ctx) {
      const { room } = roomContext(ctx.cwd);
      const kind = params.kind || "memory";
      const result = kind === "memory"
        ? await writeRustMemory({
            room: params.room === "house" ? "house" : room,
            title: params.title,
            body: params.body,
            threads: params.threads,
            continues: params.continues,
            supersedes: params.supersedes,
            condition: params.condition,
            astCondition: params.astCondition,
            triggerScope: params.triggerScope,
            interruptMode: params.interruptMode,
            repeatCooldownSecs: params.repeatCooldownSecs,
            signal,
          })
        : await writeRustLesson({
            room,
            kind,
            title: params.title,
            body: params.body,
            fields: {
              shape: params.shape,
              voice: params.voice,
              register: params.register,
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
              condition: params.condition,
              astCondition: params.astCondition,
              triggerScope: params.triggerScope,
              interruptMode: params.interruptMode,
              repeatCooldownSecs: params.repeatCooldownSecs,
            },
            backup: undefined,
            signal,
          });
      return {
        isError: !result.ok,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
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
      "Send editable fields inside patch. Fields are typed by lesson kind; cross-store fields refuse instead of being silently dropped.",
      "This is a guarded write and requires write approval.",
    ].join("\n"),
    parameters: z.object({
      kind: z.enum(["coding-lesson", "project-lesson", "writing-lesson", "design-lesson"]).describe("Which allowlisted lesson type."),
      id: z.string().describe("Exact positive numeric lesson ID (digits only)."),
      expectedTitle: z.string().describe("Exact current title required as an update guard (must be non-empty)."),
      patch: z.object({
        title: z.string().optional().describe("Replacement title."),
        body: z.string().optional().describe("Replacement lesson body; sent through stdin."),
        shape: z.string().optional().describe("Lesson shape taxonomy value."),
        triggerContext: z.string().optional().describe("When the lesson should trigger."),
        tags: z.array(z.string()).optional().describe("Replacement lesson tags."),
        threadKeys: z.array(z.string()).optional().describe("Replacement lesson-thread keys."),
        voice: z.string().optional().describe("Coding, writing, or design lesson voice."),
        scope: z.string().optional().describe("Coding lesson scope."),
        alwaysOn: z.boolean().optional().describe("Whether a coding, writing, or design lesson is always eligible."),
        project: z.string().optional().describe("Coding/project lesson project; mutually exclusive with clearProject."),
        clearProject: z.boolean().optional().describe("Set a coding/project lesson project to null; must be true and is mutually exclusive with project."),
        proofPattern: z.string().optional().describe("Coding, project, or design lesson proof pattern."),
        register: z.array(z.string()).optional().describe("Writing/design registers where this lesson applies."),
        exampleText: z.string().optional().describe("Replacement writing/design example text."),
        languageKeys: z.array(z.string()).optional().describe("Replacement language eligibility slugs (coding, project, writing, or design lessons)."),
        technologyKeys: z.array(z.string()).optional().describe("Replacement technology eligibility slugs (coding, project, writing, or design lessons)."),
        writers: z.array(z.string()).optional().describe("Writers associated with this writing lesson."),
        negationOf: z.string().optional().describe("Coding or writing lesson ID this lesson negates; omit to preserve."),
        condition: z.array(z.string()).optional().describe("Replacement Rust-regex trigger patterns (no lookaround or backreferences); omit to preserve, [] to disarm."),
        astCondition: z.array(z.string()).optional().describe("Replacement ast-grep patterns, each a single AST node in a supported language; omit to preserve, [] to disarm."),
        triggerScope: z.array(z.string()).optional().describe("Replacement scope tokens (text | tool | tool:<name>); omit to preserve, [] for every surface."),
        interruptMode: z.enum(["block", "remind"]).optional().describe("Trigger interrupt mode; omit to preserve, block is the house default and remind is demoted."),
        repeatCooldownSecs: z.number().optional().describe("Positive seconds between fires; omit to preserve."),
      }).describe("Typed replacement fields for the selected lesson kind."),
    }),
    approval: "write",
    async execute(_toolCallId, params, signal) {
      const result = await requestRustDomain("lesson_update", params, signal, true);
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
      const { binding } = hostBinding(ctx);
      const result = await laneStatus(binding);
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
      const { binding, effectiveRoomDir } = hostBinding(ctx);
      const result = await familiarStatus(binding, effectiveRoomDir);
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
      const { binding, effectiveRoomDir } = hostBinding(ctx);
      const result = await dispatchHouse(binding, effectiveRoomDir, params);
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
      const { binding, effectiveRoomDir } = hostBinding(ctx);
      const result = await dispatchHouse(binding, effectiveRoomDir, params);
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
      const session = hostSessionIdentity(ctx, effectiveRoomDir);
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
      try {
        const result = await requestGigaReview({
          candidate_id: params.candidate_id,
          room,
          reviewer_id: spirit,
          new_state: params.new_state as GigaSafeReviewState,
          reason: params.reason,
          authorization_basis: GIGA_OMP_ROOM_BINDING,
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
    const target: GigaPromotionTarget = expectedKind === "memory"
      ? {
          kind: "memory",
          title: params.title,
          body: params.body,
          threads: params.threads ?? [],
        }
      : expectedKind === "coding_lesson"
        ? {
            kind: "coding_lesson",
            title: params.title,
            body: params.body,
            shape: params.shape,
            proof_pattern: params.proof_pattern,
            trigger_context: params.trigger_context,
            language_keys: params.language_keys ?? [],
            technology_keys: params.technology_keys ?? [],
            tags: params.tags ?? [],
          }
        : {
            kind: "project_lesson",
            title: params.title,
            body: params.body,
            proof_pattern: params.proof_pattern,
            trigger_context: params.trigger_context,
            language_keys: params.language_keys ?? [],
            technology_keys: params.technology_keys ?? [],
            tags: params.tags ?? [],
            publication_approved: params.publication_approved,
          };
    try {
      const result = await requestGigaPromote({
        candidate_id: params.candidate_id,
        room,
        reviewer_id: spirit,
        operator_identity: operator,
        authorization_basis: GIGA_OMP_ROOM_BINDING,
        target,
      }, { signal });
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

  registerHouseTool(pi, {
    name: "hallway_create",
    label: "Athanor Hallway Create",
    description: "Create an operator-visible shared Hallway with explicit room access. The current authenticated room, spirit, and session become the first presence. Messages never auto-wake a spirit; a separate authorized Hallway Knock may request one bounded turn.",
    parameters: z.object({
      hallway: z.string().describe("Lowercase kebab-case Hallway key."),
      allowed_rooms: z.array(z.string()).describe("One to 32 rooms allowed to join. The current room is added automatically."),
      idempotency_key: z.string().optional().describe("Stable retry key. Defaults to this tool-call id."),
    }),
    approval: "write",
    async execute(toolCallId, params, signal, _onUpdate, ctx) {
      const { binding } = hostBinding(ctx);
      const allowedRooms = [...new Set([binding.room, ...params.allowed_rooms])].sort();
      const result = await requestRustDomain("hallway_create", {
        hallway: params.hallway,
        ...binding,
        allowedRooms,
        idempotencyKey: params.idempotency_key || String(toolCallId),
      }, signal, true);
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "hallway_join",
    label: "Athanor Hallway Join",
    description: "Join the current authenticated room, spirit, and session to an allowed Hallway. Sessions are independent presences; multiple sessions may embody the same spirit.",
    parameters: z.object({
      hallway: z.string().describe("Hallway key."),
      idempotency_key: z.string().optional().describe("Stable retry key. Defaults to this tool-call id."),
    }),
    approval: "write",
    async execute(toolCallId, params, signal, _onUpdate, ctx) {
      const { binding } = hostBinding(ctx);
      const result = await requestRustDomain("hallway_join", {
        hallway: params.hallway,
        ...binding,
        idempotencyKey: params.idempotency_key || String(toolCallId),
      }, signal, true);
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "hallway_post",
    label: "Athanor Hallway Post",
    description: "Append one visible message from the current authenticated presence to a Hallway. Peer messages are requests, not commands. Posting alone never wakes a recipient; an explicit authorized Hallway Knock is separate.",
    parameters: z.object({
      hallway: z.string().describe("Hallway key."),
      body: z.string().describe("Non-empty substantive message, at most 32768 UTF-8 bytes."),
      reply_to: z.number().optional().describe("Positive message id being answered."),
      to_rooms: z.array(z.string()).optional().describe("Structured recipient room keys. A listed room gets a durable Bell notification (targeted attention, not privacy). Never parsed from body text."),
      idempotency_key: z.string().optional().describe("Stable retry key. Defaults to this tool-call id."),
    }),
    approval: "write",
    async execute(toolCallId, params, signal, _onUpdate, ctx) {
      const { binding } = hostBinding(ctx);
      const result = await requestRustDomain("hallway_post", {
        hallway: params.hallway,
        ...binding,
        body: params.body,
        replyTo: params.reply_to,
        toRooms: params.to_rooms ?? [],
        idempotencyKey: params.idempotency_key || String(toolCallId),
      }, signal, true);
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "hallway_knock_policy",
    label: "Athanor Hallway Knock Policy",
    description: "Set this authenticated room's standing Knock policy for one Hallway. Manual is the default. allow_list permits only the named peer rooms to request bounded turns.",
    parameters: z.object({
      hallway: z.string().describe("Hallway key."),
      mode: z.enum(["manual", "allow_list"]).describe("manual refuses every Knock; allow_list permits only allowed_rooms."),
      allowed_rooms: z.array(z.string()).optional().describe("Peer room keys allowed to Knock. Ignored and cleared in manual mode."),
      max_turns: z.number().optional().describe("Maximum turns in one bounded exchange, 1-8; default 4."),
      idempotency_key: z.string().optional().describe("Stable retry key. Defaults to this tool-call id."),
    }),
    approval: "write",
    async execute(toolCallId, params, signal, _onUpdate, ctx) {
      const { binding } = hostBinding(ctx);
      const result = await requestRustDomain("hallway_knock_policy", {
        hallway: params.hallway,
        ...binding,
        mode: params.mode,
        allowedRooms: params.mode === "manual" ? [] : (params.allowed_rooms ?? []),
        maxTurns: params.max_turns ?? 4,
        idempotencyKey: params.idempotency_key || String(toolCallId),
      }, signal, true);
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "hallway_knock",
    label: "Athanor Hallway Knock",
    description: "Request one bounded turn in an allowed peer room for an existing addressed Hallway message. The recipient's standing policy decides. A child Knock must reverse the prior Knock along the same reply thread and inherits its remaining turn budget.",
    parameters: z.object({
      hallway: z.string().describe("Hallway key containing the addressed message."),
      message_id: z.number().describe("Positive Hallway message id authored by this authenticated presence."),
      recipient_room: z.string().describe("Structured recipient room already addressed by the message."),
      parent_knock_id: z.string().optional().describe("Prior Knock id when continuing the same bounded exchange."),
      max_turns: z.number().optional().describe("Root exchange turn budget, 1-8; default 4. Child Knocks inherit it."),
      idempotency_key: z.string().optional().describe("Stable retry key. Defaults to this tool-call id."),
    }),
    approval: "write",
    async execute(toolCallId, params, signal, _onUpdate, ctx) {
      const { binding } = hostBinding(ctx);
      const result = await requestRustDomain("hallway_knock", {
        hallway: params.hallway,
        ...binding,
        messageId: params.message_id,
        recipientRoom: params.recipient_room,
        parentKnockId: params.parent_knock_id,
        maxTurns: params.max_turns ?? 4,
        idempotencyKey: params.idempotency_key || String(toolCallId),
      }, signal, true);
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "hallway_read",
    label: "Athanor Hallway Read",
    description: "Read ordered Hallway messages for the current authenticated presence. Uses that session's own cursor unless after or an exact thread is supplied. Filtered thread reads acknowledge only returned messages and never advance the session cursor across other threads.",
    parameters: z.object({
      hallway: z.string().describe("Hallway key."),
      after: z.number().optional().describe("Non-negative message id; reads after it instead of the presence cursor."),
      thread: z.string().optional().describe("Exact daily thread key. Omit for the whole Hallway."),
      limit: z.number().optional().describe("Maximum messages to return, 1-200; default 50."),
      advance_cursor: z.boolean().optional().describe("Acknowledge returned messages. Whole-Hallway reads also advance this presence's cursor; filtered thread reads leave it unchanged."),
    }),
    approval: "read",
    async execute(_toolCallId, params, signal, _onUpdate, ctx) {
      const { binding } = hostBinding(ctx);
      const result = await requestRustDomain("hallway_read", {
        hallway: params.hallway,
        ...binding,
        after: params.after,
        thread: params.thread,
        limit: params.limit ?? 50,
        advanceCursor: params.advance_cursor ?? false,
      }, signal, params.advance_cursor === true);
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "hallway_inbox",
    label: "Athanor Hallway Inbox",
    description: "List every persistent Hallway this room may open: derived unread counts, pending targeted Bell rows with exact message/thread routing, and latest-message metadata. Reading the inbox clears nothing; only a covering hallway_read with advance_cursor acknowledges.",
    parameters: z.object({}),
    approval: "read",
    async execute(_toolCallId, _params, signal, _onUpdate, ctx) {
      const { binding } = hostBinding(ctx);
      const result = await requestRustDomain("hallway_inbox", { ...binding }, signal, false);
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "quest_post",
    label: "Athanor Quest Post",
    description: "Post or activate Docket work for this House. A post is a draft and never an assignment: goalDraft and draft write DRAFT rows that bind nobody, while goalActivate and activate are the explicit transitions that freeze authority, review class, and the acceptance tuple. An action missing its own required fields refuses by name without touching the substrate, and every Docket write refuses locally, naming the docket_write capability, when this room holds no provisioned capability.",
    parameters: z.object({
      action: z.enum(["goalDraft", "goalActivate", "draft", "activate"]).describe("goalDraft and draft write drafts; goalActivate and activate are the explicit transitions that freeze authority."),
      houseId: z.string().optional().describe("House the goal or quest belongs to. Defaults to this installation's House id."),
      goalId: z.string().optional().describe("Goal id: required by goalActivate, optional parent for draft."),
      questId: z.string().optional().describe("Quest id being activated. Required by activate."),
      title: z.string().optional().describe("Goal or quest title. Required by goalDraft and draft."),
      intent: z.string().optional().describe("What the goal is for, in the House's own words. Required by goalDraft."),
      priority: z.number().optional().describe("Goal ordering hint; higher sorts first."),
      recurrenceInterval: z.string().optional().describe("ISO-8601 duration, such as P1W. Omit for a goal that runs once; a recurring goal re-arms its quest at settlement."),
      intentAuthorityPrincipal: z.string().optional().describe("Principal whose intent authorizes this work. Required by goalActivate and activate."),
      stewardRoom: z.string().optional().describe("Room accountable for the goal. Required by goalActivate."),
      stewardSpirit: z.string().optional().describe("Spirit accountable for the goal. Required by goalActivate."),
      kind: z.string().optional().describe("Quest kind. Required by draft."),
      body: z.string().optional().describe("Quest body: the work as prose. Required by draft."),
      importance: z.enum(["hint", "blocker"]).optional().describe("hint or blocker. Required by activate."),
      deadlineAt: z.string().optional().describe("RFC3339 deadline. Omit for a quest the clock never rings about."),
      reviewClass: z.enum(["R0", "R1", "R2", "R3"]).optional().describe("Review class frozen at activation. Required by activate."),
      acceptanceCriteria: z.array(z.string()).optional().describe("Named criteria, at least one, frozen at activation and settled one by one. Required by activate."),
      idempotencyKey: z.string().optional().describe("Stable retry key. Defaults to this tool-call id."),
    }).strict(),
    approval: "write",
    async execute(toolCallId, params, signal, _onUpdate, ctx) {
      const { binding, capability } = docketWriteBinding(ctx);
      if (workerAtTheDoor(ctx, binding)) return refuseWorkerHands(`quest_post ${params.action}`);
      if (!capability) return refuseUnprovisionedRoom();
      const action = params.action;
      const gate = `quest_post ${action}`;
      let fields: Record<string, unknown>;
      if (action === "goalDraft" || action === "draft") {
        const houseId = docketHouseId(params.houseId);
        if (!houseId) return refuseDocket("house_unnamed", "houseId", "houseId is required and this installation names no House");
        if (action === "goalDraft") {
          if (!params.title?.trim() || !params.intent?.trim()) {
            return refuseDocket("incomplete_action", gate, "goalDraft requires title and intent");
          }
          fields = {
            houseId,
            title: params.title.trim(),
            intent: params.intent,
            priority: params.priority,
            recurrenceInterval: params.recurrenceInterval?.trim() || undefined,
          };
        } else {
          if (!params.kind?.trim() || !params.title?.trim() || !params.body?.trim()) {
            return refuseDocket("incomplete_action", gate, "draft requires kind, title, and body");
          }
          fields = {
            houseId,
            goalId: params.goalId?.trim() || undefined,
            kind: params.kind.trim(),
            title: params.title.trim(),
            body: params.body,
            importance: params.importance,
            deadlineAt: params.deadlineAt?.trim() || undefined,
          };
        }
      } else if (action === "goalActivate") {
        if (!params.goalId?.trim() || !params.intentAuthorityPrincipal?.trim()
          || !params.stewardRoom?.trim() || !params.stewardSpirit?.trim()) {
          return refuseDocket("incomplete_action", gate, "goalActivate requires goalId, intentAuthorityPrincipal, stewardRoom, and stewardSpirit");
        }
        fields = {
          goalId: params.goalId.trim(),
          intentAuthorityPrincipal: params.intentAuthorityPrincipal.trim(),
          stewardRoom: params.stewardRoom.trim(),
          stewardSpirit: params.stewardSpirit.trim(),
        };
      } else {
        const criteria = (params.acceptanceCriteria ?? []).map((item: string) => item.trim()).filter(Boolean);
        if (!params.questId?.trim() || !params.intentAuthorityPrincipal?.trim()
          || !params.reviewClass || !params.importance || criteria.length === 0) {
          return refuseDocket("incomplete_action", gate, "activate requires questId, intentAuthorityPrincipal, reviewClass, importance, and at least one acceptance criterion");
        }
        fields = {
          questId: params.questId.trim(),
          intentAuthorityPrincipal: params.intentAuthorityPrincipal.trim(),
          reviewClass: params.reviewClass,
          acceptanceCriteria: criteria,
          importance: params.importance,
          deadlineAt: params.deadlineAt?.trim() || undefined,
        };
      }
      const result = await requestRustDomain("quest_post", {
        action,
        ...binding,
        capability,
        idempotencyKey: params.idempotencyKey?.trim() || String(toolCallId),
        ...fields,
      }, signal, true);
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "quest_board",
    label: "Athanor Quest Board",
    description: "Read this House's Docket board: quests by deadline, soonest first, each with its state, importance, claim epoch, and acceptance verdict counts. The board is a read and carries no capability. It offers work and never assigns it, and an empty board is an empty board rather than an instruction to invent one.",
    parameters: z.object({
      houseId: z.string().optional().describe("House to read. Defaults to this installation's House id."),
      states: z.array(z.string()).optional().describe("Quest states to include, such as offered or claimed. Omit for the substrate's own default set."),
      limit: z.number().optional().describe("Maximum quests to return. Omit for the substrate default."),
    }).strict(),
    approval: "read",
    async execute(_toolCallId, params, signal, _onUpdate, ctx) {
      const { binding } = hostBinding(ctx);
      const houseId = docketHouseId(params.houseId);
      if (!houseId) return refuseDocket("house_unnamed", "houseId", "houseId is required and this installation names no House");
      const result = await requestRustDomain("quest_board", {
        ...binding,
        houseId,
        states: params.states?.length ? params.states : undefined,
        limit: params.limit,
      }, signal, false);
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "quest_claim",
    label: "Athanor Quest Claim",
    description: "Take one offered quest as this room and this spirit, by consent and never by assignment. The claim mints a lease token shown exactly once, so keep it: a replay of the same idempotency key returns the existing attempt receipt without the token. A quest that is not offered refuses as not_claimable, and the write refuses locally, naming the docket_write capability, when this room holds no provisioned capability.",
    parameters: z.object({
      questId: z.string().describe("Offered quest to claim."),
      idempotencyKey: z.string().optional().describe("Stable retry key. Defaults to this tool-call id. Replaying it returns the attempt without reminting the lease."),
    }).strict(),
    approval: "write",
    async execute(toolCallId, params, signal, _onUpdate, ctx) {
      const { binding, capability } = docketWriteBinding(ctx);
      if (workerAtTheDoor(ctx, binding)) return refuseWorkerHands("quest_claim");
      if (!capability) return refuseUnprovisionedRoom();
      if (!params.questId?.trim()) return refuseDocket("incomplete_action", "quest_claim", "questId is required");
      const result = await requestRustDomain("quest_claim", {
        ...binding,
        capability,
        idempotencyKey: params.idempotencyKey?.trim() || String(toolCallId),
        questId: params.questId.trim(),
      }, signal, true);
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "quest_report",
    label: "Athanor Quest Report",
    description: "File evidence against a claimed quest: progress receipts while the work runs, one submit that yields the attempt, or one settleItem verdict on a named acceptance criterion. Every action revalidates the lease first, so an expired lease or a superseded claim epoch refuses as stale_lease and publishes nothing. An executor can never settle its own acceptance item; that refusal comes from the ledger and is surfaced as it stands.",
    parameters: z.object({
      questId: z.string().describe("Quest this report belongs to."),
      attemptId: z.string().describe("Attempt id returned by quest_claim."),
      leaseToken: z.string().optional().describe("Lease token minted once by quest_claim. Required by progress and submit: it proves the claimant still holds the quest. settleItem takes no token; settlement authenticates the reviewer's own room and role."),
      action: z.enum(["progress", "submit", "settleItem"]).describe("progress records evidence, submit yields the attempt for review, settleItem records one acceptance verdict."),
      body: z.string().describe("The evidence or verdict reasoning as prose. The substrate digests it; nothing is trusted from the caller."),
      kind: z.string().optional().describe("Receipt kind. Omit for the substrate's default for this action."),
      performedBy: z.string().optional().describe("Familiar or worker whose hand did the work, kept visible instead of laundered."),
      // Two roles live in this one field. A receipt names who authored the
      // evidence (executor or reviewer); a settlement names who carries the
      // verdict (reviewer or steward). The ledger refuses an executor verdict
      // regardless, and the substrate refuses the wrong role for the action.
      authoredRole: z.enum(["executor", "reviewer", "steward"]).optional().describe("Role behind this action. A receipt from progress or submit is authored by executor or reviewer. settleItem is a settlement and requires reviewer or steward; an executor verdict is refused by the ledger."),
      itemPosition: z.number().optional().describe("1-based acceptance item position. Required by settleItem."),
      verdict: z.enum(["met", "not_met", "unknown", "inconclusive", "not_applicable", "refused"]).optional().describe("Verdict for the named acceptance item. Required by settleItem."),
      idempotencyKey: z.string().optional().describe("Stable retry key. Defaults to this tool-call id."),
    }).strict(),
    approval: "write",
    async execute(toolCallId, params, signal, _onUpdate, ctx) {
      const { binding, capability } = docketWriteBinding(ctx);
      if (workerAtTheDoor(ctx, binding)) return refuseWorkerHands(`quest_report ${params.action}`);
      if (!capability) return refuseUnprovisionedRoom();
      const action = params.action;
      const gate = `quest_report ${action}`;
      if (!params.questId?.trim() || !params.attemptId?.trim() || !params.body?.trim()) {
        return refuseDocket("incomplete_action", gate, "questId, attemptId, and body are required");
      }
      // The bearer token binds the claimant's own work. Settlement rides the
      // reviewer's authority instead (guild-hall #167/#171), so demanding a
      // token here would force executors to share a secret across rooms.
      if (action !== "settleItem" && !params.leaseToken?.trim()) {
        return refuseDocket("incomplete_action", gate, "leaseToken is required for progress and submit");
      }
      // The substrate requires authoredRole for settleItem and refuses an
      // executor verdict from the ledger. Naming the missing role here spends
      // no capability on a call that cannot land.
      if (action === "settleItem"
        && (!Number.isInteger(params.itemPosition) || Number(params.itemPosition) < 1
          || !params.verdict || !params.authoredRole)) {
        return refuseDocket("incomplete_action", gate, "settleItem requires itemPosition (1 or greater), verdict, and authoredRole");
      }
      const result = await requestRustDomain("quest_report", {
        ...binding,
        capability,
        idempotencyKey: params.idempotencyKey?.trim() || String(toolCallId),
        questId: params.questId.trim(),
        attemptId: params.attemptId.trim(),
        leaseToken: action === "settleItem" ? undefined : params.leaseToken?.trim(),
        action,
        body: params.body,
        kind: params.kind?.trim() || undefined,
        performedBy: params.performedBy?.trim() || undefined,
        authoredRole: params.authoredRole,
        itemPosition: action === "settleItem" ? params.itemPosition : undefined,
        verdict: action === "settleItem" ? params.verdict : undefined,
      }, signal, true);
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });

  registerHouseTool(pi, {
    name: "quest_evidence",
    label: "Athanor Quest Evidence",
    description: "Read one quest's evidence: full receipt bodies, ledger events, and acceptance items with their verdicts. A read with no capability, like the board. An independent reviewer judges primary receipts here, never a claimant's summary of them.",
    parameters: z.object({
      questId: z.string().describe("Quest whose evidence to read."),
      limit: z.number().optional().describe("Maximum receipts and events to return, 1-200; default 50."),
    }).strict(),
    approval: "read",
    async execute(_toolCallId, params, signal, _onUpdate, ctx) {
      const { binding } = hostBinding(ctx);
      if (!params.questId?.trim()) return refuseDocket("incomplete_action", "quest_evidence", "questId is required");
      const result = await requestRustDomain("quest_evidence", {
        ...binding,
        questId: params.questId.trim(),
        limit: params.limit,
      }, signal, false);
      return {
        isError: result.ok !== true,
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result,
      };
    },
  });
}

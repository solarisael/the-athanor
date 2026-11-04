// Worker lifecycle: task-lineage notebooks, individual worker wakes, and the
// 40% wrap / 60% handoff-only / retire transition for task-tool workers.
//
// The Host `worker_lifecycle` organ owns every transition, threshold, and
// ownership rule. This module only translates OMP events into those requests
// and enforces the answers locally: it enrolls task spawns, binds a child to
// its reservation, renders the role lessons the Host delivers with each named
// familiar's wake, gates the child's model requests and tools on persisted
// lifecycle state, persists terminal yields before they leave, and reports
// retirements back to the parent. A Host failure blocks the unsafe step; it
// never lets a child run untracked.

import { createHash, randomBytes, randomUUID } from "node:crypto";
import { hostSessionIdentity, type HostBinding } from "./host.ts";
import { OrganError, organFailure, requestOrgan } from "./organ.ts";
import { requireRoomBinding, roomContext } from "./room.ts";
import { contextBlockKind } from "./context.ts";
import { conversationText } from "./text.ts";
import { parentContextRequest } from "./recall.ts";
import { registerHouseTool } from "./tools.ts";

export const WORKER_LIFECYCLE_ENTRY = "athanor-worker-lifecycle";
export const WORKER_WAKE_MESSAGE = "athanor-worker-wake";
export const WORKER_NOTICE_MESSAGE = "athanor-worker-lifecycle-notice";
const FAMILIAR_LESSONS_MESSAGE = "athanor-familiar-lessons";

// Each lifecycle request stays far below OMP's 30 s extension handler budget:
// a timed-out tool_call handler blocks, but a timed-out context handler is
// skipped, so the gate must finish (or abort) on its own clock.
const LIFECYCLE_TIMEOUT_MS = 6_000;
const SETTLEMENT_WAIT_MS = 10_000;
const EXCERPT_LIMIT = 1_200;
const MESSAGE_EXCERPT_LIMIT = 2_000;
const HANDOFF_REQUEST_LIMIT = 2;
const READ_LIMIT_MAX = 50;
const REGISTRY_LIMIT = 1_024;
// The Host refuses reports above 256 KiB. A larger final report is stored as
// an explicitly clipped excerpt; the full text stays in the native transcript.
const REPORT_DATA_LIMIT = 200_000;
const SECTIONS_LIMIT = 40_000;

const MARKER_HINT = "athanor-worker-enrollment";
const MARKER_LINE = /^\[athanor-worker-enrollment:v1:([0-9a-f]{64})\]$/;
const MARKER_ANY = /\[?athanor-worker-enrollment[A-Za-z0-9:_-]*\]?/g;
const MARKER_REDACTED = "[athanor-worker-enrollment redacted]";

// Room continuity belongs to the room's own spirit. A worker that opened one
// of these would read or rewrite the parent's identity, Presence, or Bell.
const ROOM_CONTINUITY_TOOLS = new Set([
  "wake", "sleep", "room_state", "set_room_state", "anamnesis", "anamnesis_write",
  "hallway_create", "hallway_join", "hallway_post", "hallway_read", "hallway_inbox",
  "hallway_knock", "hallway_knock_policy", "recall_policy", "house_model_default",
  "house_routing_mode", "request_restart",
]);

// Room-derived blocks a worker request must never carry, whichever path
// placed them in the transcript.
const ROOM_DERIVED_MESSAGES = new Set(["athanor-mode-door", "athanor-hallway-knock"]);

export type LifecycleState = "working" | "wrapping" | "handoff_only" | "retired";
export type WorkerTaskOrigin = { id: string; batch: string; agent: string };
const LIFECYCLE_STATES = new Set<LifecycleState>(["working", "wrapping", "handoff_only", "retired"]);
const NOTEBOOK_KINDS = ["note", "message", "evidence", "briefing"] as const;

type Predecessor = { bookId: string; instanceId: string };

type Reservation = {
  requestId: string;
  swarmKey: string;
  slotKey: string;
  agentName: string;
  task: string;
  token: string;
  binding: HostBinding;
  predecessor?: Predecessor;
  successor: boolean;
  consumed: boolean;
  released?: boolean;
  swarmId?: string;
  bookId?: string;
  instanceId?: string;
};

type WorkerState = {
  token: string;
  binding: HostBinding;
  swarmId: string;
  bookId: string;
  instanceId: string;
  state: LifecycleState;
  retired: boolean;
  handoffComplete: boolean;
  handoffRequests: number;
  remainingRequests: number;
  measurementUnavailable: string | null;
  checkpointFailures: number;
  notebookGaps: number;
  sections: unknown[];
  lastAssistantText: string;
  openOperations: Map<string, string>;
};

type SessionState = {
  key: string;
  reservations: Map<string, Reservation>;
  worker: WorkerState | null;
  workerFailure: string | null;
  /** Kept outside transcript compaction and injected once per provider context. */
  roleLessons?: string | undefined;
};

type SlotRecord = {
  key: string;
  toolCallId: string;
  index: number;
  reservation: Reservation;
  started: boolean;
  settled: boolean;
  agentId?: string;
  sessionFile?: string;
  deliver: (toolCallId: string, text: string) => void;
  persist: (data: Record<string, unknown>) => void;
};

type ToolCallBlock = { block: true; reason: string };

// Extension factories are rebound per session (parent and every child) without
// re-evaluating modules, so these registries are shared by every registration
// in the process. Each is keyed by an identity no other session can produce.
const sessions = new Map<string, SessionState>();
const slotRegistry = new Map<string, SlotRecord>();
const enrolledCalls = new Set<string>();
const tokenRooms = new Map<string, { room: string; spirit: string }>();
const handledLifecycle = new WeakSet<object>();
const pendingNotices = new Map<string, string[]>();
// Task calls whose native result is still pending in this process. Notices
// for them ride the result; any other notice (late background or cold-revived
// settlement) goes to the parent as its own message.
const awaitingResult = new Set<string>();
// Fallback slot lookups for lifecycle events that arrive without the native
// parentToolCallId (cold persisted revive): by agent id and by transcript.
const slotsByAgent = new Map<string, string>();
const slotsBySession = new Map<string, string>();
const slotsByToken = new Map<string, string>();
const settlements = new Map<string, Promise<void>[]>();

// Eval's nested task results are absent from model history; enrollment still owns their native IDs.
export function workerTaskOrigins(parentSession: string): WorkerTaskOrigin[] {
  const origins: WorkerTaskOrigin[] = [];

  for (const slot of slotRegistry.values()) {
    if (!slot.started || !slot.agentId || slot.reservation.binding.session !== parentSession) continue;

    origins.push({ id: slot.agentId, batch: slot.toolCallId, agent: slot.reservation.agentName });
  }

  return origins;
}

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : value == null ? "" : String(value).trim();
}

function trimOldest<K, V>(map: Map<K, V>, limit = REGISTRY_LIMIT): void {
  while (map.size > limit) {
    const oldest = map.keys().next();
    if (oldest.done) return;
    map.delete(oldest.value);
  }
}

function trimOldestSet<T>(set: Set<T>, limit = REGISTRY_LIMIT): void {
  while (set.size > limit) {
    const oldest = set.values().next();
    if (oldest.done) return;
    set.delete(oldest.value);
  }
}

function sha256(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

/** A bounded, nonempty idempotency key; long native ids are hashed, never cut. */
export function lifecycleRequestId(...parts: string[]): string {
  const raw = parts.map(text).join(":");
  return raw.length <= 160 ? raw : `${text(parts[0])}:${sha256(raw).slice(0, 48)}`;
}

// ---------------------------------------------------------------------------
// Enrollment marker
// ---------------------------------------------------------------------------

export function mintEnrollmentToken(): string {
  return randomBytes(32).toString("hex");
}

export function enrollmentMarker(token: string): string {
  return `[${MARKER_HINT}:v1:${token}]`;
}

export type EnrollmentMarker =
  | { kind: "absent" }
  | { kind: "valid"; token: string; text: string }
  | { kind: "malformed" };

/**
 * Exactly one marker line, alone on its line, and no other trace of the
 * marker vocabulary. Anything that merely resembles a marker is malformed and
 * never authority.
 */
export function parseEnrollmentMarker(value: unknown): EnrollmentMarker {
  const source = typeof value === "string" ? value : "";
  const occurrences = source.split(MARKER_HINT).length - 1;
  if (occurrences === 0) return { kind: "absent" };
  if (occurrences !== 1) return { kind: "malformed" };
  const lines = source.split(/\r?\n/);
  const index = lines.findIndex((line) => MARKER_LINE.test(line));
  if (index < 0) return { kind: "malformed" };
  const token = MARKER_LINE.exec(lines[index])![1];
  const remaining = [...lines.slice(0, index), ...lines.slice(index + 1)].join("\n").trimEnd();
  return { kind: "valid", token, text: remaining };
}

export function redactEnrollment(value: string): string {
  return value.replace(MARKER_ANY, MARKER_REDACTED);
}

function redactValue(value: unknown): unknown {
  if (value === undefined) return undefined;
  const json = JSON.stringify(value);
  return json === undefined ? undefined : JSON.parse(redactEnrollment(json));
}

export function boundedExcerpt(value: unknown, limit = EXCERPT_LIMIT): { text: string; clipped: boolean; chars: number } {
  let raw: string;
  if (typeof value === "string") raw = value;
  else {
    try {
      raw = JSON.stringify(value) ?? "";
    } catch {
      raw = String(value);
    }
  }
  const clean = redactEnrollment(raw);
  return clean.length <= limit
    ? { text: clean, clipped: false, chars: clean.length }
    : { text: clean.slice(0, limit), clipped: true, chars: clean.length };
}

function fitted(value: unknown, limit: number): unknown {
  const json = JSON.stringify(value) ?? "";
  if (json.length <= limit) return value;
  return { clipped: true, excerpt: boundedExcerpt(json, limit).text, chars: json.length, note: "Full value remains in the worker transcript." };
}

function contentText(content: unknown): string {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content
    .map((part) => (part && typeof part === "object" && typeof (part as any).text === "string" ? (part as any).text : ""))
    .filter(Boolean)
    .join("\n");
}

// ---------------------------------------------------------------------------
// Host requests
// ---------------------------------------------------------------------------

async function lifecycleRequest(
  binding: HostBinding,
  action: string,
  params: Record<string, unknown>,
  { write = true }: { write?: boolean } = {},
): Promise<Record<string, any>> {
  const result = await requestOrgan(binding, "worker_lifecycle", { action, ...params }, {
    timeoutMs: LIFECYCLE_TIMEOUT_MS,
    write,
  });
  if (result.ok !== true) {
    throw new OrganError(`worker_lifecycle ${action} answered without ok`, write ? "outcome_unknown" : "invalid_response", false, {
      action,
      execution: { request_dispatched: true, write_outcome: write ? "unknown" : "not_started", retry: "reconcile_first" },
    });
  }
  return result;
}

function requireStrings(action: string, value: Record<string, any>, keys: string[]): void {
  for (const key of keys) {
    if (typeof value[key] !== "string" || !value[key]) {
      throw new OrganError(`worker_lifecycle ${action} omitted ${key}`, "outcome_unknown", false, {
        action,
        execution: { request_dispatched: true, write_outcome: "unknown", retry: "reconcile_first" },
      });
    }
  }
}

function lifecycleState(action: string, value: unknown): LifecycleState {
  if (typeof value === "string" && LIFECYCLE_STATES.has(value as LifecycleState)) return value as LifecycleState;
  throw new OrganError(`worker_lifecycle ${action} returned an unknown state`, "outcome_unknown", false, {
    action,
    execution: { request_dispatched: true, write_outcome: "unknown", retry: "reconcile_first" },
  });
}

function errorText(error: unknown): string {
  const failure = organFailure(error);
  return `${failure.error}${failure.code ? ` (${failure.code})` : ""}`;
}

// ---------------------------------------------------------------------------
// Session state, persisted through appendEntry
// ---------------------------------------------------------------------------

function sessionBinding(ctx: any): HostBinding {
  return requireRoomBinding(ctx).binding;
}

function sessionKey(ctx: any): string {
  return hostSessionIdentity(ctx);
}

// An ordinary harness session (no attached or launched home) keeps OMP's own
// task and worker behaviour: the House worker protocol never engages. A
// selected home that is unreachable still counts as a House session, so its
// gates fail closed instead of silently reverting to ordinary behaviour.
function ordinarySession(ctx: any): boolean {
  try {
    return roomContext(ctx) === null;
  } catch {
    return false;
  }
}

function emptyWorker(token: string, binding: HostBinding, ids: { swarmId: string; bookId: string; instanceId: string }): WorkerState {
  return {
    token,
    binding,
    ...ids,
    state: "working",
    retired: false,
    handoffComplete: false,
    handoffRequests: 0,
    remainingRequests: HANDOFF_REQUEST_LIMIT,
    measurementUnavailable: null,
    checkpointFailures: 0,
    notebookGaps: 0,
    sections: [],
    lastAssistantText: "",
    openOperations: new Map(),
  };
}

type SlotOwner = Pick<SlotRecord, "deliver" | "persist">;

function registerSlot(record: SlotRecord): void {
  slotRegistry.set(record.key, record);
  slotsByToken.set(sha256(record.reservation.token), record.key);
  if (record.agentId) slotsByAgent.set(record.agentId, record.key);
  if (record.sessionFile) slotsBySession.set(record.sessionFile, record.key);
  trimOldest(slotRegistry);
  trimOldest(slotsByToken);
  trimOldest(slotsByAgent);
  trimOldest(slotsBySession);
}

/** Link a slot to the child that actually started for it; both fallbacks resolve to the slot afterwards. */
function linkSlot(record: SlotRecord, agentId: string, sessionFile: string): boolean {
  const changed = !record.started || (agentId && record.agentId !== agentId) || (sessionFile && record.sessionFile !== sessionFile);
  record.started = true;
  if (agentId) record.agentId = agentId;
  if (sessionFile) record.sessionFile = sessionFile;
  registerSlot(record);
  return Boolean(changed);
}

function restore(state: SessionState, ctx: any, owner: SlotOwner): void {
  let entries: unknown[] = [];
  try {
    const listed = ctx?.sessionManager?.getEntries?.();
    entries = Array.isArray(listed) ? listed : [];
  } catch (error) {
    // Without the entries this session cannot prove its own reservations or
    // bound worker, so a child fails closed through workerFailure below.
    console.warn(`[athanor] worker lifecycle could not read session entries: ${error instanceof Error ? error.message : String(error)}`);
    if (ctx?.agent?.kind === "sub" && text(ctx.agent.id) !== "advisor") state.workerFailure = "session entries unreadable";
    return;
  }
  for (const entry of entries as Array<Record<string, any>>) {
    if (entry?.type !== "custom" || entry.customType !== WORKER_LIFECYCLE_ENTRY) continue;
    const data = entry.data as Record<string, any> | undefined;
    if (!data || typeof data !== "object") continue;
    switch (data.stage) {
      case "pending": {
        const reservation = data.reservation as Reservation | undefined;
        if (reservation?.requestId && reservation.token) {
          state.reservations.set(reservation.requestId, { ...reservation, consumed: Boolean(reservation.consumed) });
          tokenRooms.set(sha256(reservation.token), { room: reservation.binding.room, spirit: reservation.binding.spirit });
        }
        break;
      }
      case "reserved": {
        const reservation = state.reservations.get(text(data.requestId));
        if (reservation) Object.assign(reservation, { swarmId: data.swarmId, bookId: data.bookId, instanceId: data.instanceId });
        break;
      }
      case "released": {
        const reservation = state.reservations.get(text(data.requestId));
        if (reservation) reservation.released = true;
        break;
      }
      case "consumed": {
        const reservation = state.reservations.get(text(data.requestId));
        if (reservation) reservation.consumed = true;
        break;
      }
      case "enrolled-call": {
        const toolCallId = text(data.toolCallId);
        if (!toolCallId) break;
        enrolledCalls.add(toolCallId);
        for (const slot of Array.isArray(data.slots) ? data.slots : []) {
          const reservation = state.reservations.get(text(slot?.requestId));
          const index = Number(slot?.index);
          if (reservation?.instanceId && Number.isInteger(index)) {
            registerSlot({ key: `${toolCallId}:${index}`, toolCallId, index, reservation, started: false, settled: false, ...owner });
          }
        }
        break;
      }
      case "slot-started": {
        const record = slotRegistry.get(`${text(data.toolCallId)}:${Number(data.index)}`);
        if (record) linkSlot(record, text(data.agentId), text(data.sessionFile));
        break;
      }
      case "slot-settled": {
        const record = slotRegistry.get(`${text(data.toolCallId)}:${Number(data.index)}`);
        if (record) record.settled = true;
        break;
      }
      case "bound": {
        const binding = data.binding as HostBinding;
        state.worker = emptyWorker(text(data.token), binding, {
          swarmId: text(data.swarmId), bookId: text(data.bookId), instanceId: text(data.instanceId),
        });
        state.worker.state = LIFECYCLE_STATES.has(data.state) ? data.state : "working";
        break;
      }
      case "state": {
        if (state.worker && LIFECYCLE_STATES.has(data.state)) state.worker.state = data.state;
        break;
      }
      case "retired": {
        if (state.worker) {
          state.worker.state = "retired";
          state.worker.retired = true;
          state.worker.handoffComplete = data.handoffComplete === true;
        }
        break;
      }
      default:
        break;
    }
  }
  trimOldest(slotRegistry);
  trimOldestSet(enrolledCalls);
}

/**
 * Whether this hook serves a worker session. OMP routes the House advisor's
 * own calls through the advised session with `{kind:"sub", id:"advisor"}`; in
 * the room's session that is the room's own review channel, not a worker, but
 * inside an enrolled (or refused) child it inherits the child's gates.
 */
export function isWorkerSession(ctx: any): boolean {
  if (ctx?.agent?.kind !== "sub") return false;
  if (text(ctx.agent.id) !== "advisor") return true;
  const state = sessions.get(sessionKey(ctx));
  if (state) return Boolean(state.worker || state.workerFailure);
  // Not yet restored in this process: a bound child left its binding entry.
  try {
    const entries = ctx?.sessionManager?.getEntries?.();
    return Array.isArray(entries) && entries.some((entry: any) => entry?.type === "custom"
      && entry.customType === WORKER_LIFECYCLE_ENTRY && entry.data?.stage === "bound");
  } catch (error) {
    // Unreadable entries cannot prove this is the room's own advisor: gate it.
    console.warn(`[athanor] advisor session entries unreadable: ${error instanceof Error ? error.message : String(error)}`);
    return true;
  }
}

// ---------------------------------------------------------------------------
// Guidance text
// ---------------------------------------------------------------------------

function askParent(ctx: any): string {
  return parentContextRequest({ agent: { kind: "sub", name: "worker", parentId: ctx?.agent?.parentId } }) ?? "";
}

const HANDOFF_CONTENTS = "completed work, evidence (paths, commands, results), pending side effects, blockers, and the exact next action";

function wrappingNotice(): string {
  return [
    "Athanor worker lifecycle: this worker has used at least 40% of its context window.",
    "Start wrapping up: finish the current step, avoid new investigation branches, and prepare to yield.",
    `Your final report must carry ${HANDOFF_CONTENTS}.`,
  ].join("\n");
}

function handoffNotice(remaining: number): string {
  return [
    "Athanor worker lifecycle: HANDOFF-ONLY. This worker has used at least 60% of its context window.",
    "Every investigation, edit, and spawn tool is now refused. Do not call them.",
    `Call yield now with one terminal report (omit type, or a string type) carrying ${HANDOFF_CONTENTS}.`,
    "Keep the caller's output schema: put the handoff inside the data it already requires.",
    `Model requests left before this worker retires with an incomplete handoff (a request about to be sent counts): ${remaining}.`,
  ].join("\n");
}

function retiredNotice(worker: WorkerState): string {
  return `Athanor worker lifecycle: instance ${worker.instanceId} of book ${worker.bookId} is retired and cannot resume work. A successor must be spawned by the parent.`;
}

function measurementNotice(reason: string): string {
  return [
    `Athanor worker lifecycle: context usage could not be measured (${reason}).`,
    "Work tools are refused until measurement returns, because an unmeasured worker may already be past its handoff line.",
    `If this repeats, yield your current report with ${HANDOFF_CONTENTS}.`,
  ].join("\n");
}

type FamiliarLessons = { tags: string[]; lessons: Array<{ id: number; title: string; lesson: string }> };

/**
 * The role lessons the Host delivered with this wake, or null when the Host
 * states that this worker is no named familiar. The Host selects them on
 * every wake and refuses one without them. An answer that omits the field
 * comes from a Host older than this contract and is refused, as is a
 * malformed delivery, so a familiar never works bare.
 */
function requireFamiliarLessons(opened: Record<string, any>): FamiliarLessons | null {
  const delivered = opened.familiarLessons;
  if (delivered === null) return null;
  const failure = (message: string) => new OrganError(message, "outcome_unknown", false, {
    action: "open",
    execution: { request_dispatched: true, write_outcome: "unknown", retry: "reconcile_first" },
  });
  if (delivered === undefined) throw failure("worker_lifecycle open omitted familiarLessons; the Host predates the role-lesson contract");
  const { tags, lessons } = delivered;
  const whole = Array.isArray(tags) && tags.length > 0 && tags.every((tag: unknown) => typeof tag === "string" && tag !== "")
    && Array.isArray(lessons) && lessons.length > 0
    && lessons.every((lesson: any) => Number.isSafeInteger(lesson?.id) && typeof lesson?.title === "string" && typeof lesson?.lesson === "string" && lesson.lesson.trim() !== "");
  if (!whole) throw failure("worker_lifecycle open delivered malformed familiar lessons");
  return { tags, lessons };
}

function familiarLessonsText(delivered: FamiliarLessons): string {
  return [
    `Role lessons for this familiar (${delivered.tags.join(", ")}), delivered whole by the House at this wake. They govern how you work; they do not widen the assignment. Lesson ids: ${delivered.lessons.map((lesson) => lesson.id).join(", ")}.`,
    ...delivered.lessons.map((lesson) => `[Lesson ${lesson.id}] ${lesson.title}\n${lesson.lesson}`),
  ].join("\n\n");
}

function wakeMessage(opened: Record<string, any>): string {
  const pending = Array.isArray(opened.pendingOperations) ? opened.pendingOperations : [];
  const wake = opened.wake ?? {};
  return [
    `Athanor worker wake (${text(wake.kind)}): book ${opened.bookId}, instance ${opened.instanceId}, state ${opened.state}.`,
    text(wake.text),
    pending.length
      ? `Operations recorded as started without an end (verify their effects before repeating them):\n${pending.map((operation: unknown) => `- ${boundedExcerpt(operation, 400).text}`).join("\n")}`
      : "",
    "Lifecycle: at 40% context you will be asked to wrap up; at 60% only one terminal yield carrying your handoff is allowed, then this instance retires.",
    "worker_notebook reads and appends this task book only. Room continuity (wake, Anamnesis, Hallway, Presence) is not yours.",
  ].filter(Boolean).join("\n\n");
}

function retirementNotice(record: SlotRecord, status: string, settled: { state: string; handoffComplete: boolean }, agentId: string): string {
  const { bookId, instanceId } = record.reservation;
  const who = `Worker ${agentId || record.reservation.agentName} (book ${bookId}, instance ${instanceId})`;
  if (settled.handoffComplete) {
    return [
      `${who} retired with a complete handoff (native status ${status}).`,
      "Retirement does not mean the assignment is finished: read its report before deciding.",
      `To continue, call worker_notebook ${JSON.stringify({ action: "successor", bookId, instanceId })} and pass the returned packet to task. Nothing is spawned or reserved until you do.`,
    ].join("\n");
  }
  return [
    `${who} retired with an INCOMPLETE handoff (native status ${status}).`,
    "A successor is refused until the handoff is supplied. Prerequisites:",
    `1. worker_notebook ${JSON.stringify({ action: "inspect", bookId })} and worker_notebook ${JSON.stringify({ action: "read", bookId })} to review what was recorded (the transcript stays as provenance).`,
    `2. worker_notebook ${JSON.stringify({ action: "repair", bookId, instanceId, report: "<the handoff you can attest to>" })} to append your attributed handoff.`,
    `3. Then worker_notebook ${JSON.stringify({ action: "successor", bookId, instanceId })}.`,
  ].join("\n");
}

function customMessage(customType: string, content: string): Record<string, unknown> {
  return { role: "custom", customType, content, display: false, attribution: "agent", timestamp: Date.now() };
}

/** Remove room-derived blocks and enrollment markers from a worker's provider request. */
export function scrubWorkerMessages(messages: any[]): any[] {
  const scrubbed: any[] = [];
  for (const message of messages) {
    if (message?.role === "custom" && message.customType === FAMILIAR_LESSONS_MESSAGE) continue;
    if (message?.role === "custom" && (contextBlockKind(message.customType) || ROOM_DERIVED_MESSAGES.has(message.customType))) continue;
    if (message?.role === "user" && JSON.stringify(message.content ?? "").includes(MARKER_HINT)) {
      const content = typeof message.content === "string"
        ? stripMarker(message.content)
        : Array.isArray(message.content)
          ? message.content.map((part: any) => (part?.type === "text" && typeof part.text === "string" ? { ...part, text: stripMarker(part.text) } : part))
          : message.content;
      scrubbed.push({ ...message, content });
      continue;
    }
    scrubbed.push(message);
  }
  return scrubbed;
}

function stripMarker(value: string): string {
  const parsed = parseEnrollmentMarker(value);
  return parsed.kind === "valid" ? parsed.text : redactEnrollment(value);
}

export function childRoomToolRefusal(event: any, ctx: any): ToolCallBlock | undefined {
  if (ctx?.agent?.kind !== "sub") return undefined;
  const name = text(event?.toolName);
  let target = ROOM_CONTINUITY_TOOLS.has(name) ? name : "";
  if (!target && name === "write") {
    const device = /^xd:\/\/([a-z_]+)(?:[/?#]|$)/i.exec(text(event?.input?.path));
    if (device && ROOM_CONTINUITY_TOOLS.has(device[1].toLowerCase())) target = device[1].toLowerCase();
  }
  if (!target) return undefined;
  return {
    block: true,
    reason: [
      `${target} is the room's own continuity and is closed to worker sessions: it would install the parent's identity and history, not yours.`,
      askParent(ctx),
    ].join("\n"),
  };
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

export function installWorkerLifecycle(pi: any): void {
  const deliver: SlotRecord["deliver"] = (toolCallId, notice) => {
    if (awaitingResult.has(toolCallId)) {
      const queued = pendingNotices.get(toolCallId) ?? [];
      queued.push(notice);
      pendingNotices.set(toolCallId, queued);
      trimOldest(pendingNotices);
      return;
    }
    try {
      pi.sendMessage({ customType: WORKER_NOTICE_MESSAGE, content: notice, display: true }, { deliverAs: "aside" });
    } catch (error) {
      console.warn(`[athanor] worker retirement notice was not delivered: ${error instanceof Error ? error.message : String(error)}\n${notice}`);
    }
  };

  function stateFor(ctx: any): SessionState {
    const key = sessionKey(ctx);
    let state = sessions.get(key);
    if (!state) {
      state = { key, reservations: new Map(), worker: null, workerFailure: null };
      sessions.set(key, state);
      trimOldest(sessions);
      restore(state, ctx, { deliver, persist: persistSoft });
    }
    return state;
  }

  function persist(data: Record<string, unknown>): void {
    pi.appendEntry(WORKER_LIFECYCLE_ENTRY, data);
  }

  // A lost state entry only weakens cold-revive recovery; the Host keeps the
  // authoritative state, so the gap is reported and work continues.
  function persistSoft(data: Record<string, unknown>): void {
    try {
      persist(data);
    } catch (error) {
      console.warn(`[athanor] worker lifecycle entry ${String(data.stage)} not persisted: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  function markRetired(worker: WorkerState, handoffComplete: boolean, reason: string, settled: boolean): void {
    worker.state = "retired";
    worker.retired = true;
    worker.handoffComplete = handoffComplete;
    persistSoft({ stage: "retired", handoffComplete, reason, settled });
  }

  function workerReport(worker: WorkerState, extra: Record<string, unknown> = {}): Record<string, unknown> {
    return {
      ...extra,
      ...(worker.sections.length ? { sections: fitted(redactValue(worker.sections), SECTIONS_LIMIT) } : {}),
      ...(worker.lastAssistantText.trim() ? { lastTurnText: boundedExcerpt(worker.lastAssistantText, MESSAGE_EXCERPT_LIMIT) } : {}),
      ...(worker.notebookGaps ? { notebookGaps: worker.notebookGaps } : {}),
      ...(worker.openOperations.size ? { unresolvedOperations: [...worker.openOperations.entries()].map(([operationId, tool]) => ({ operationId, tool })) } : {}),
    };
  }

  /** Record an incomplete handoff and retire. A failed record still retires locally (fail closed). */
  async function settleIncomplete(ctx: any, worker: WorkerState, reason: string): Promise<void> {
    try {
      const settled = await lifecycleRequest(worker.binding, "settle", {
        requestId: lifecycleRequestId("settle", worker.instanceId, "failed", reason),
        bookId: worker.bookId,
        instanceId: worker.instanceId,
        outcome: "failed",
        report: workerReport(worker, { reason }),
        ...(text(ctx?.sessionManager?.getSessionFile?.()) ? { transcriptPath: text(ctx.sessionManager.getSessionFile()) } : {}),
      });
      markRetired(worker, settled.handoffComplete === true, reason, true);
    } catch (error) {
      console.warn(`[athanor] worker ${worker.instanceId} incomplete handoff not recorded (${reason}): ${errorText(error)}; the parent's lifecycle settlement must reconcile it.`);
      markRetired(worker, false, reason, false);
    }
  }

  // ---- parent: enrollment ------------------------------------------------

  async function prepareSlot(reservation: Reservation): Promise<Reservation> {
    const prepared = await lifecycleRequest(reservation.binding, "prepare", {
      requestId: reservation.requestId,
      swarmKey: reservation.swarmKey,
      slotKey: reservation.slotKey,
      agentName: reservation.agentName,
      task: reservation.task,
      token: reservation.token,
      ...(reservation.predecessor ? { predecessor: reservation.predecessor } : {}),
    });
    requireStrings("prepare", prepared, ["swarmId", "bookId", "instanceId"]);
    if (prepared.token !== reservation.token) {
      throw new OrganError("worker_lifecycle prepare echoed a different token", "outcome_unknown", false, { action: "prepare" });
    }
    reservation.swarmId = prepared.swarmId;
    reservation.bookId = prepared.bookId;
    reservation.instanceId = prepared.instanceId;
    persist({
      stage: "reserved", requestId: reservation.requestId,
      swarmId: prepared.swarmId, bookId: prepared.bookId, instanceId: prepared.instanceId, state: prepared.state,
    });
    return reservation;
  }

  function reserve(state: SessionState, fields: Omit<Reservation, "token" | "consumed">): Reservation {
    // A released reservation was retired by its own refused spawn; a later
    // attempt for the same slot takes the next generation instead of reusing it.
    let requestId = fields.requestId;
    for (let generation = 1; ; generation++) {
      const existing = state.reservations.get(requestId);
      if (!existing) break;
      if (!existing.released) return existing;
      requestId = lifecycleRequestId(fields.requestId, `r${generation}`);
    }
    const reservation: Reservation = { ...fields, requestId, token: mintEnrollmentToken(), consumed: false };
    // Persist before the Host sees the token: a retry of this request must
    // reuse this exact identity rather than mint another.
    persist({ stage: "pending", reservation });
    state.reservations.set(reservation.requestId, reservation);
    tokenRooms.set(sha256(reservation.token), { room: reservation.binding.room, spirit: reservation.binding.spirit });
    trimOldest(tokenRooms);
    return reservation;
  }

  async function enrollTaskCall(event: any, ctx: any, state: SessionState): Promise<Record<string, unknown> | undefined> {
    const toolCallId = text(event?.toolCallId);
    if (!toolCallId) return { block: true, reason: "Worker enrollment refused: the task call carries no tool call id, so its workers could not be tracked." };
    const input = event?.input;
    if (!input || typeof input !== "object" || Array.isArray(input)) return undefined;
    const batch = Array.isArray(input.tasks);
    const slots: any[] = batch ? input.tasks : [input];
    const binding = state.worker ? { ...state.worker.binding } : sessionBinding(ctx);

    const plans: Array<{ index: number; reservation: Reservation; marker: boolean }> = [];
    for (let index = 0; index < slots.length; index++) {
      const slot = slots[index] ?? {};
      const parsed = parseEnrollmentMarker(slot.task);
      if (parsed.kind === "malformed") {
        return { block: true, reason: `Worker enrollment refused: task ${index} contains text resembling an enrollment marker. Markers are machine-issued by worker_notebook successor; remove the text or use the exact packet.` };
      }
      if (parsed.kind === "valid") {
        const reservation = [...state.reservations.values()].find((candidate) => candidate.successor && candidate.token === parsed.token);
        if (!reservation || !reservation.instanceId) {
          return { block: true, reason: `Worker enrollment refused: task ${index} carries an enrollment marker this session never issued. Call worker_notebook successor for a fresh packet; no new book is substituted.` };
        }
        if (reservation.consumed) {
          return { block: true, reason: `Worker enrollment refused: the successor packet for book ${reservation.bookId} was already spawned. Inspect the book instead of spawning it twice.` };
        }
        plans.push({ index, reservation, marker: false });
        continue;
      }
      const reservation = reserve(state, {
        requestId: lifecycleRequestId("prepare", toolCallId, String(index)),
        swarmKey: toolCallId,
        slotKey: String(index),
        // OMP reports a child's agent definition name lowercased; the Host
        // binds open.agentName to this exact value.
        agentName: text(slot.agent).toLowerCase() || "task",
        task: String(slot.task ?? ""),
        binding,
        successor: false,
      });
      plans.push({ index, reservation, marker: true });
    }

    const prepared = await Promise.allSettled(
      plans.map((plan) => (plan.marker && !plan.reservation.instanceId ? prepareSlot(plan.reservation) : Promise.resolve(plan.reservation))),
    );
    const failures = prepared.flatMap((outcome, position) => (outcome.status === "rejected" ? [`task ${plans[position].index}: ${errorText(outcome.reason)}`] : []));
    if (failures.length) {
      // Release every sibling that did reserve, so no unclaimed instance stays
      // active; a release that fails is named for the parent to reconcile.
      const unreleased: string[] = [];
      await Promise.all(prepared.map(async (outcome, position) => {
        const reservation = plans[position].reservation;
        if (outcome.status !== "fulfilled" || !plans[position].marker || !reservation.instanceId) return;
        try {
          await lifecycleRequest(binding, "settle", {
            requestId: lifecycleRequestId("settle", reservation.instanceId, "aborted", "spawn-refused"),
            bookId: reservation.bookId, instanceId: reservation.instanceId, outcome: "aborted",
            report: { reason: "spawn refused before the worker started", failures },
          });
          reservation.released = true;
          persistSoft({ stage: "released", requestId: reservation.requestId });
        } catch (error) {
          unreleased.push(`${reservation.instanceId}: ${errorText(error)}`);
        }
      }));
      return {
        block: true,
        reason: [
          "Worker enrollment failed, so no worker was spawned (an untracked worker is never started).",
          ...failures,
          ...(unreleased.length ? ["Reservations that could not be released (inspect them):", ...unreleased] : []),
          "Released reservations are not reused; a retry prepares fresh ones, while unreleased reservations are reused by a retry of this same call.",
        ].join("\n"),
      };
    }

    const nextSlots = slots.map((slot, index) => {
      const plan = plans.find((candidate) => candidate.index === index)!;
      if (!plan.marker) return slot;
      return { ...slot, task: `${String(slot?.task ?? "").trimEnd()}\n\n${enrollmentMarker(plan.reservation.token)}` };
    });
    try {
      for (const plan of plans) {
        if (!plan.marker) {
          plan.reservation.consumed = true;
          persist({ stage: "consumed", requestId: plan.reservation.requestId });
        }
      }
      persist({ stage: "enrolled-call", toolCallId, slots: plans.map((plan) => ({ index: plan.index, requestId: plan.reservation.requestId })) });
    } catch (error) {
      return { block: true, reason: `Worker enrollment could not be persisted in this session (${error instanceof Error ? error.message : String(error)}); no worker was spawned. Retry the same call.` };
    }
    enrolledCalls.add(toolCallId);
    trimOldestSet(enrolledCalls);
    awaitingResult.add(toolCallId);
    trimOldestSet(awaitingResult);
    for (const plan of plans) {
      registerSlot({
        key: `${toolCallId}:${plan.index}`, toolCallId, index: plan.index, reservation: plan.reservation,
        started: false, settled: false, deliver, persist: persistSoft,
      });
    }
    return { input: batch ? { ...input, tasks: nextSlots } : { ...input, task: nextSlots[0].task } };
  }

  // ---- parent: lifecycle settlement -------------------------------------

  function trackSettlement(record: SlotRecord, settlement: Promise<void>): void {
    const waiting = settlements.get(record.toolCallId) ?? [];
    waiting.push(settlement);
    settlements.set(record.toolCallId, waiting);
    trimOldest(settlements);
  }

  async function settleSlot(record: SlotRecord, status: string, agentId: string, sessionFile: string): Promise<void> {
    const reservation = record.reservation;
    try {
      const settled = await lifecycleRequest(reservation.binding, "settle", {
        requestId: lifecycleRequestId("settle", reservation.instanceId!, status, randomUUID()),
        bookId: reservation.bookId,
        instanceId: reservation.instanceId,
        outcome: status,
        ...(sessionFile ? { transcriptPath: sessionFile } : {}),
      });
      const result = { state: lifecycleState("settle", settled.state), handoffComplete: settled.handoffComplete === true };
      if (result.state === "retired" && !record.settled) {
        record.settled = true;
        record.persist({ stage: "slot-settled", toolCallId: record.toolCallId, index: record.index });
        record.deliver(record.toolCallId, retirementNotice(record, status, result, agentId));
      }
    } catch (error) {
      record.deliver(record.toolCallId, [
        `Worker lifecycle settlement for book ${reservation.bookId}, instance ${reservation.instanceId} (native status ${status}) failed: ${errorText(error)}.`,
        `Its lifecycle state is unconfirmed. Run worker_notebook ${JSON.stringify({ action: "inspect", bookId: reservation.bookId })} before relying on or continuing it.`,
      ].join("\n"));
    }
  }

  /** Resolve the slot a native lifecycle payload belongs to, with fallbacks for cold revives. */
  function lifecycleSlot(event: Record<string, any>): SlotRecord | undefined {
    const toolCallId = text(event.parentToolCallId);
    const index = Number(event.index);
    if (toolCallId) {
      const direct = slotRegistry.get(`${toolCallId}:${Number.isInteger(index) && index >= 0 ? index : 0}`);
      if (direct) return direct;
    }
    const byAgent = slotsByAgent.get(text(event.id));
    if (byAgent) return slotRegistry.get(byAgent);
    const bySession = slotsBySession.get(text(event.sessionFile));
    return bySession ? slotRegistry.get(bySession) : undefined;
  }

  pi.events?.on?.("task:subagent:lifecycle", (payload: unknown) => {
    if (!payload || typeof payload !== "object" || handledLifecycle.has(payload)) return;
    const event = payload as Record<string, any>;
    const status = text(event.status);
    const record = lifecycleSlot(event);
    if (!record || !record.reservation.instanceId) return;
    handledLifecycle.add(payload);
    const agentId = text(event.id);
    const sessionFile = text(event.sessionFile);
    if (status === "started" || status === "running") {
      if (linkSlot(record, agentId, sessionFile)) {
        record.persist({ stage: "slot-started", toolCallId: record.toolCallId, index: record.index, agentId, sessionFile });
      }
      return;
    }
    if (status !== "completed" && status !== "failed" && status !== "aborted") return;
    if (linkSlot(record, agentId, sessionFile)) {
      record.persist({ stage: "slot-started", toolCallId: record.toolCallId, index: record.index, agentId, sessionFile });
    }
    trackSettlement(record, settleSlot(record, status, agentId, sessionFile));
  });

  /**
   * A task call can return without starting a child (preflight refusal,
   * approval denial, per-slot spawn failure). Its reservations would stay
   * active forever, so each one is cancelled and the parent is told exactly
   * how to recover.
   */
  async function cancelUnstartedSlots(toolCallId: string, event: any): Promise<string[]> {
    // OMP acknowledges queued background jobs before their started events.
    // That acknowledgement transfers ownership; it is not a spawn failure.
    if (event?.details?.async?.state === "running") return [];
    const progress = Array.isArray(event?.details?.progress) ? event.details.progress : [];
    const results = Array.isArray(event?.details?.results) ? event.details.results : [];
    const notices: string[] = [];
    for (const record of slotRegistry.values()) {
      if (record.toolCallId !== toolCallId || record.started || record.settled || !record.reservation.instanceId) continue;
      const result = results.find((candidate: any) => Number(candidate?.index) === record.index);
      const queued = progress.find((candidate: any) => Number(candidate?.index) === record.index);
      if (queued && text(queued.id) && ["pending", "running", "started"].includes(text(queued.status))) continue;
      const failed = result?.aborted === true || Boolean(result?.error)
        || (Number.isFinite(result?.exitCode) && result.exitCode !== 0)
        || ["failed", "aborted", "cancelled"].includes(text(result?.status));
      if (!event?.isError && !failed) continue;
      const { bookId, instanceId, predecessor } = record.reservation;
      try {
        const settled = await lifecycleRequest(record.reservation.binding, "settle", {
          requestId: lifecycleRequestId("settle", instanceId!, "aborted", "never-started"),
          bookId, instanceId, outcome: "aborted",
          report: { reason: "the task call returned without starting this worker", ...(result?.error ? { error: boundedExcerpt(result.error).text } : {}) },
        });
        record.settled = true;
        record.persist({ stage: "slot-settled", toolCallId, index: record.index });
        const lines = [`Task slot ${record.index} never started a worker. Its reservation (book ${bookId}, instance ${instanceId}) was cancelled (state ${text(settled.state)}).`];
        if (predecessor) {
          lines.push(
            "This was a successor reservation; the cancelled instance is now the book's latest. To continue the same book:",
            `1. worker_notebook ${JSON.stringify({ action: "repair", bookId, instanceId, report: "<the predecessor's handoff, which you attest to>" })}`,
            `2. worker_notebook ${JSON.stringify({ action: "successor", bookId, instanceId })}, then task with the returned packet.`,
          );
        } else {
          lines.push("Nothing was spawned for it; retrying the task call creates a fresh book.");
        }
        notices.push(lines.join("\n"));
      } catch (error) {
        notices.push([
          `Task slot ${record.index} never started a worker, and cancelling its reservation (book ${bookId}, instance ${instanceId}) failed: ${errorText(error)}.`,
          `It may still count as active. Run worker_notebook ${JSON.stringify({ action: "inspect", bookId })} and reconcile before disposing its swarm.`,
        ].join("\n"));
      }
    }
    return notices;
  }

  async function taskResultNotices(toolCallId: string): Promise<string[]> {
    const waiting = settlements.get(toolCallId) ?? [];
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timedOut = await Promise.race([
      Promise.allSettled(waiting).then(() => false),
      new Promise<boolean>((resolve) => {
        timer = setTimeout(() => resolve(true), SETTLEMENT_WAIT_MS);
      }),
    ]);
    if (timer) clearTimeout(timer);
    settlements.delete(toolCallId);
    awaitingResult.delete(toolCallId);
    const notices = pendingNotices.get(toolCallId) ?? [];
    pendingNotices.delete(toolCallId);
    if (timedOut) notices.push("Worker lifecycle settlement is still running; its retirement notice will arrive separately.");
    return notices;
  }

  // ---- child: open / resume ---------------------------------------------

  async function openWorker(ctx: any, state: SessionState, token: string, resume: WorkerState | null): Promise<Record<string, any>> {
    const known = resume?.binding ?? tokenRooms.get(sha256(token));
    const own = sessionBinding(ctx);
    const binding: HostBinding = known ? { room: known.room, spirit: known.spirit, session: own.session } : own;
    const transcriptPath = text(ctx?.sessionManager?.getSessionFile?.());
    const opened = await lifecycleRequest(binding, "open", {
      requestId: resume
        ? lifecycleRequestId("resume", resume.instanceId, randomUUID())
        : lifecycleRequestId("open", sha256(token).slice(0, 32), binding.session),
      token,
      agentId: text(ctx?.agent?.id),
      agentName: text(ctx?.agent?.name),
      // A Host predating role lessons refuses this field, so no familiar wakes bare.
      acceptsFamiliarLessons: true,
      ...(transcriptPath ? { transcriptPath } : {}),
    });
    requireStrings("open", opened, ["swarmId", "bookId", "instanceId"]);
    // Validate before the binding is persisted: a restored binding must never lack its lessons.
    const lessons = requireFamiliarLessons(opened);
    const openedState = lifecycleState("open", opened.state);
    const worker = resume ?? emptyWorker(token, binding, { swarmId: opened.swarmId, bookId: opened.bookId, instanceId: opened.instanceId });
    if (resume && (resume.bookId !== opened.bookId || resume.instanceId !== opened.instanceId)) {
      throw new OrganError("worker_lifecycle open answered for a different instance", "outcome_unknown", false, { action: "open" });
    }
    worker.state = openedState;
    worker.handoffRequests = Number(opened.handoffRequests) || 0;
    worker.remainingRequests = Math.max(0, HANDOFF_REQUEST_LIMIT - worker.handoffRequests);
    if (!resume) {
      persist({
        stage: "bound", token, binding, state: openedState,
        swarmId: opened.swarmId, bookId: opened.bookId, instanceId: opened.instanceId,
      });
    } else {
      persistSoft({ stage: "state", state: openedState });
    }
    state.worker = worker;
    state.workerFailure = null;
    state.roleLessons = lessons ? familiarLessonsText(lessons) : undefined;
    // Children run in the parent's process: tell the parent's slot which child
    // took it, so a lifecycle event without a parentToolCallId still settles.
    const slotKey = slotsByToken.get(sha256(token));
    const record = slotKey ? slotRegistry.get(slotKey) : undefined;
    if (record && linkSlot(record, text(ctx?.agent?.id), transcriptPath)) {
      record.persist({ stage: "slot-started", toolCallId: record.toolCallId, index: record.index, agentId: record.agentId ?? "", sessionFile: record.sessionFile ?? "" });
    }
    if (openedState === "retired") markRetired(worker, worker.handoffComplete, "opened retired", true);
    return opened;
  }

  function wakeReply(opened: Record<string, any>): { message: Record<string, unknown> } {
    return { message: { customType: WORKER_WAKE_MESSAGE, content: wakeMessage(opened), display: false } };
  }

  pi.on("before_agent_start", async (event: any, ctx: any) => {
    if (!isWorkerSession(ctx) || ordinarySession(ctx)) return undefined;
    const state = stateFor(ctx);
    try {
      if (state.worker) {
        if (state.worker.retired) return { message: { customType: WORKER_NOTICE_MESSAGE, content: retiredNotice(state.worker), display: false } };
        const opened = await openWorker(ctx, state, state.worker.token, state.worker);
        if (state.worker.retired) return { message: { customType: WORKER_NOTICE_MESSAGE, content: retiredNotice(state.worker), display: false } };
        return wakeReply(opened);
      }
      const parsed = parseEnrollmentMarker(event?.prompt);
      if (parsed.kind !== "valid") {
        state.workerFailure = parsed.kind === "absent"
          ? "the worker has no enrollment; ask the parent to spawn an enrolled task"
          : "the assignment carries a malformed enrollment marker";
        return { message: { customType: WORKER_NOTICE_MESSAGE, content: `Athanor worker lifecycle refused this worker: ${state.workerFailure}.`, display: false } };
      }
      const opened = await openWorker(ctx, state, parsed.token, null);
      return wakeReply(opened);
    } catch (error) {
      if (state.worker && error instanceof OrganError && error.code === "instance_retired") {
        markRetired(state.worker, state.worker.handoffComplete, "resume refused: instance retired", true);
        return { message: { customType: WORKER_NOTICE_MESSAGE, content: retiredNotice(state.worker), display: false } };
      }
      state.workerFailure = `worker open failed: ${errorText(error)}`;
      console.warn(`[athanor] ${state.workerFailure}`);
      return { message: { customType: WORKER_NOTICE_MESSAGE, content: `Athanor worker lifecycle could not bind this worker (${state.workerFailure}). Work is refused; the parent will see the failure. The task book and its recorded entries are kept: after the cause is fixed, the parent continues the book with worker_notebook repair, then successor.`, display: false } };
    }
  });

  // ---- child: per-request gate -----------------------------------------

  type Gate = { abort: true; notice: string } | { abort: false; notice: string | null };

  function applyObservation(worker: WorkerState, observed: Record<string, any>): void {
    const previous = worker.state;
    worker.state = lifecycleState("observe", observed.state);
    worker.handoffRequests = Number(observed.handoffRequests) || 0;
    worker.remainingRequests = Math.max(0, Number(observed.remainingRequests) || 0);
    worker.measurementUnavailable = null;
    if (worker.state !== previous) persistSoft({ stage: "state", state: worker.state });
  }

  function observeParams(ctx: any, worker: WorkerState, requestStarted: boolean): Record<string, unknown> | string {
    const usage = ctx?.getContextUsage?.();
    const tokens = Number(usage?.tokens);
    const contextWindow = Number(usage?.contextWindow);
    if (!Number.isFinite(tokens) || tokens < 0 || !Number.isFinite(contextWindow) || contextWindow <= 0) {
      return "OMP reported no context usage";
    }
    return {
      requestId: lifecycleRequestId("observe", worker.instanceId, randomUUID()),
      bookId: worker.bookId,
      tokens: Math.round(tokens),
      contextWindow: Math.round(contextWindow),
      requestStarted,
    };
  }

  /**
   * Context runs for every conversion of the transcript, including live
   * steering of a response already in flight, so it measures and latches
   * (requestStarted:false) but never spends the handoff request budget.
   */
  async function contextGate(ctx: any, state: SessionState): Promise<Gate> {
    if (state.workerFailure || !state.worker) {
      return { abort: true, notice: `Athanor worker lifecycle refused this request: ${state.workerFailure || "worker not enrolled"}.` };
    }
    const worker = state.worker;
    if (worker.retired) return { abort: true, notice: retiredNotice(worker) };
    const params = observeParams(ctx, worker, false);
    let unavailable = typeof params === "string" ? params : "";
    if (typeof params !== "string") {
      try {
        applyObservation(worker, await lifecycleRequest(worker.binding, "observe", params));
      } catch (error) {
        if (error instanceof OrganError && error.code === "instance_retired") {
          markRetired(worker, worker.handoffComplete, "retired by the Host", true);
          return { abort: true, notice: retiredNotice(worker) };
        }
        unavailable = `observe failed: ${errorText(error)}`;
      }
    }
    if (unavailable) {
      // Work tools stay refused until a measurement succeeds; the model may
      // still answer and yield, which is the safe direction.
      worker.measurementUnavailable = unavailable;
      return { abort: false, notice: measurementNotice(unavailable) };
    }
    if (worker.state === "retired") {
      markRetired(worker, worker.handoffComplete, "retired by the Host", true);
      return { abort: true, notice: retiredNotice(worker) };
    }
    if (worker.state === "handoff_only") return { abort: false, notice: handoffNotice(worker.remainingRequests) };
    if (worker.state === "wrapping") return { abort: false, notice: wrappingNotice() };
    return { abort: false, notice: null };
  }

  /**
   * Fired once per actual provider dispatch. Only here does a handoff-only
   * worker spend one of its two final requests; anything this gate cannot
   * prove safe is aborted before the payload leaves.
   */
  async function providerGate(ctx: any, state: SessionState): Promise<boolean> {
    if (state.workerFailure || !state.worker || state.worker.retired) return false;
    const worker = state.worker;
    if (worker.state !== "handoff_only") return true;
    const params = observeParams(ctx, worker, true);
    if (typeof params === "string") {
      await settleIncomplete(ctx, worker, `handoff request could not be counted: ${params}`);
      return false;
    }
    try {
      const observed = await lifecycleRequest(worker.binding, "observe", params);
      applyObservation(worker, observed);
      if (observed.retireRequired === true) {
        await settleIncomplete(ctx, worker, "handoff request budget exhausted");
        return false;
      }
      if (worker.state === "retired") {
        markRetired(worker, worker.handoffComplete, "retired by the Host", true);
        return false;
      }
      return true;
    } catch (error) {
      if (error instanceof OrganError && error.code === "instance_retired") {
        markRetired(worker, worker.handoffComplete, "retired by the Host", true);
        return false;
      }
      await settleIncomplete(ctx, worker, `handoff request could not be counted: ${errorText(error)}`);
      return false;
    }
  }

  pi.on("session_start", (_event: any, ctx: any) => {
    if (ordinarySession(ctx)) return;
    // Restore persisted reservations and slot links before any lifecycle event
    // for them can arrive (a cold-revived child settles through these).
    stateFor(ctx);
  });

  pi.on("context", async (event: any, ctx: any) => {
    if (!isWorkerSession(ctx) || ordinarySession(ctx)) return undefined;
    const messages = Array.isArray(event?.messages) ? event.messages : [];
    try {
      const state = stateFor(ctx);
      const scrubbed = scrubWorkerMessages(messages);
      if (state.roleLessons) scrubbed.unshift(customMessage(FAMILIAR_LESSONS_MESSAGE, state.roleLessons));
      const gate = await contextGate(ctx, state);
      if (gate.abort) {
        ctx.abort();
        return { messages: [customMessage(WORKER_NOTICE_MESSAGE, gate.notice)] };
      }
      return { messages: gate.notice ? [...scrubbed, customMessage(WORKER_NOTICE_MESSAGE, gate.notice)] : scrubbed };
    } catch (error) {
      // A gate that cannot decide must not let the request through.
      console.warn(`[athanor] worker lifecycle gate failed closed: ${error instanceof Error ? error.message : String(error)}`);
      ctx?.abort?.();
      return { messages: [customMessage(WORKER_NOTICE_MESSAGE, "Athanor worker lifecycle gate failed; this request was aborted.")] };
    }
  });

  pi.on("before_provider_request", async (_event: any, ctx: any) => {
    if (!isWorkerSession(ctx) || ordinarySession(ctx)) return undefined;
    let allowed = false;
    try {
      allowed = await providerGate(ctx, stateFor(ctx));
    } catch (error) {
      console.warn(`[athanor] worker provider gate failed closed: ${error instanceof Error ? error.message : String(error)}`);
    }
    if (!allowed) ctx.abort();
    return undefined;
  });

  pi.on("before_subagent_spawn", (event: any, ctx: any) => {
    if (event?.invocationKind !== "eval" || ordinarySession(ctx)) return undefined;
    return {
      block: true,
      reason: "Athanor worker lifecycle: eval agent()/workpool spawns cannot carry a worker enrollment, and an unenrolled worker is refused on its first request. Spawn this worker with the task tool instead; it is enrolled automatically.",
    };
  });

  // ---- child: tool gate and terminal-yield checkpoint -------------------

  async function workerToolGate(event: any, ctx: any, state: SessionState): Promise<ToolCallBlock | undefined> {
    const toolName = text(event?.toolName);
    if (state.workerFailure || !state.worker) {
      return { block: true, reason: `Athanor worker lifecycle refused this tool: ${state.workerFailure || "worker not enrolled"}. ${askParent(ctx)}` };
    }
    const worker = state.worker;
    if (worker.retired) return { block: true, reason: retiredNotice(worker) };
    // Yield is authorized here and persisted from its validated result.
    if (toolName === "yield") return undefined;
    if (worker.state === "handoff_only") {
      if (toolName === "worker_notebook" && text(event?.input?.action) === "inspect") return undefined;
      return { block: true, reason: handoffNotice(worker.remainingRequests) };
    }
    if (worker.measurementUnavailable) return { block: true, reason: measurementNotice(worker.measurementUnavailable) };
    if (toolName === "worker_notebook") return undefined;
    const operationId = text(event?.toolCallId);
    try {
      await lifecycleRequest(worker.binding, "append", {
        requestId: lifecycleRequestId("tool-start", worker.instanceId, operationId),
        bookId: worker.bookId,
        kind: "tool-start",
        operationId,
        body: { tool: toolName, input: boundedExcerpt(event?.input) },
      });
      worker.openOperations.set(operationId, toolName);
      return undefined;
    } catch (error) {
      return { block: true, reason: `The task book could not record this tool start (${errorText(error)}), so the tool was not run: an unrecorded side effect would be invisible to a successor.` };
    }
  }

  /**
   * Persist a yield OMP has already validated, before its result is
   * published. Rejected yields (isError, or details without a native status)
   * never reach the book. A checkpoint failure turns the result into an error
   * with empty details, so the executor cannot accept a report the book lacks.
   */
  async function checkpointYield(event: any, ctx: any, worker: WorkerState): Promise<Record<string, unknown> | undefined> {
    if (event?.isError === true) return undefined;
    const details = event?.details && typeof event.details === "object" ? event.details as Record<string, any> : null;
    if (!details || (details.status !== "success" && details.status !== "aborted")) return undefined;
    const type = details.type;
    const incremental = Array.isArray(type) && type.length > 0 && details.status === "success" && details.complete !== true;
    if (incremental) {
      worker.sections.push({ type, ...(details.data !== undefined ? { data: details.data } : {}) });
      return undefined;
    }
    const lastTurn = details.useLastTurn === true && worker.sections.length === 0 ? worker.lastAssistantText.trim() : "";
    const report = workerReport(worker, {
      status: details.status,
      type: type ?? null,
      ...(details.data !== undefined ? { data: fitted(redactValue(details.data), REPORT_DATA_LIMIT) } : {}),
      ...(typeof details.error === "string" ? { error: redactEnrollment(details.error) } : {}),
      ...(details.useLastTurn === true ? { useLastTurn: true } : {}),
      ...(details.schemaOverridden === true ? { schemaOverridden: true } : {}),
      // Native yield resolves a data-less typed terminal to the last assistant
      // text; that text is the report, so it is stored whole (within the cap).
      ...(lastTurn ? { lastTurnReport: fitted(redactEnrollment(lastTurn), REPORT_DATA_LIMIT) } : {}),
    });
    try {
      const checkpoint = await lifecycleRequest(worker.binding, "checkpoint", {
        requestId: lifecycleRequestId("checkpoint", worker.instanceId, text(event?.toolCallId) || randomUUID()),
        bookId: worker.bookId,
        report,
      });
      worker.checkpointFailures = 0;
      worker.state = lifecycleState("checkpoint", checkpoint.state);
      if (worker.state === "retired") markRetired(worker, checkpoint.handoffComplete === true, "terminal yield", true);
      else persistSoft({ stage: "state", state: worker.state });
      worker.sections = [];
      return undefined;
    } catch (failure) {
      worker.checkpointFailures += 1;
      let message = `Your yield was valid but could not be persisted to the task book (${errorText(failure)}). Submit the same yield once more; a second failure retires this worker with an incomplete handoff.`;
      if (worker.checkpointFailures >= 2) {
        await settleIncomplete(ctx, worker, `terminal yield could not be checkpointed: ${errorText(failure)}`);
        ctx?.abort?.();
        message = `${retiredNotice(worker)} The final report could not be persisted; the transcript is kept for the parent.`;
      }
      return { content: [{ type: "text", text: message }], details: {}, isError: true };
    }
  }

  pi.on("tool_call", async (event: any, ctx: any) => {
    if (ordinarySession(ctx)) return undefined;
    const worker = isWorkerSession(ctx);
    if (worker) {
      const roomRefusal = childRoomToolRefusal(event, ctx);
      if (roomRefusal) return roomRefusal;
    }
    const state = stateFor(ctx);
    if (worker) {
      const gate = await workerToolGate(event, ctx, state);
      if (gate) return gate;
    }
    if (text(event?.toolName) === "task") return enrollTaskCall(event, ctx, state);
    return undefined;
  });

  pi.on("tool_result", async (event: any, ctx: any) => {
    if (ordinarySession(ctx)) return;
    const toolCallId = text(event?.toolCallId);
    if (isWorkerSession(ctx)) {
      const worker = stateFor(ctx).worker;
      if (worker && !worker.retired && text(event?.toolName) === "yield") return checkpointYield(event, ctx, worker);
      const tool = worker?.openOperations.get(toolCallId);
      if (worker && tool !== undefined) {
        try {
          await lifecycleRequest(worker.binding, "append", {
            requestId: lifecycleRequestId("tool-end", worker.instanceId, toolCallId),
            bookId: worker.bookId,
            kind: "tool-end",
            operationId: toolCallId,
            body: { tool, isError: event?.isError === true, output: boundedExcerpt(contentText(event?.content)) },
          });
          worker.openOperations.delete(toolCallId);
        } catch (error) {
          // The start stays unresolved in the book, which is the honest record.
          worker.notebookGaps += 1;
          console.warn(`[athanor] worker ${worker.instanceId} tool end not recorded for ${toolCallId}: ${errorText(error)}`);
        }
      }
    }
    if (text(event?.toolName) !== "task" || !enrolledCalls.has(toolCallId)) return undefined;
    const notices = await taskResultNotices(toolCallId);
    notices.push(...await cancelUnstartedSlots(toolCallId, event));
    if (!notices.length) return undefined;
    const content = Array.isArray(event?.content) ? event.content : [];
    return { content: [...content, { type: "text", text: `\n\n[Athanor worker lifecycle]\n${notices.join("\n\n")}` }] };
  });

  pi.on("assistant_message", (event: any, ctx: any) => {
    if (!isWorkerSession(ctx) || ordinarySession(ctx)) return undefined;
    const worker = stateFor(ctx).worker;
    if (!worker || worker.retired) return undefined;
    const said = conversationText(event?.message);
    if (!said.trim()) return undefined;
    worker.lastAssistantText = said;
    const recorded = lifecycleRequest(worker.binding, "append", {
      requestId: lifecycleRequestId("message", worker.instanceId, randomUUID()),
      bookId: worker.bookId,
      kind: "message",
      body: { role: "assistant", text: boundedExcerpt(said, MESSAGE_EXCERPT_LIMIT) },
    });
    // Recorded beside the turn so tool dispatch never waits on the notebook;
    // a missed message is counted into the checkpoint report as a gap.
    recorded.catch((error) => {
      worker.notebookGaps += 1;
      console.warn(`[athanor] worker ${worker.instanceId} message not recorded: ${errorText(error)}`);
    });
    return undefined;
  });

  pi.on("session_before_compact", (_event: any, ctx: any) => {
    if (!isWorkerSession(ctx) || ordinarySession(ctx)) return undefined;
    const worker = stateFor(ctx).worker;
    // Compaction would spend an uncounted model request after the handoff
    // line and could not clear the latch anyway.
    if (worker && (worker.retired || worker.state === "handoff_only")) return { cancel: true };
    return undefined;
  });

  pi.on("agent_end", async (event: any, ctx: any) => {
    if (!isWorkerSession(ctx) || event?.willContinue === true || ordinarySession(ctx)) return;
    const worker = stateFor(ctx).worker;
    // OMP's yield reminders run after the loop ends and may still use an
    // unspent handoff request; only a spent budget retires here. Native
    // failures are settled by the parent from the lifecycle event.
    if (worker && !worker.retired && worker.state === "handoff_only" && worker.remainingRequests === 0) {
      await settleIncomplete(ctx, worker, "handoff requests exhausted without a persisted terminal yield");
    }
  });

  // ---- worker_notebook ---------------------------------------------------

  const z = pi.zod;
  registerHouseTool(pi, {
    name: "worker_notebook",
    label: "Athanor Worker Notebook",
    description: [
      "Read and write task-lineage notebooks for task-tool workers.",
      "Workers: read/append/inspect act on your own task book only; your identity comes from your enrollment, never from parameters.",
      "Parents: inspect your swarms, read a worker's book, append evidence/briefing, accept a report into durable retained evidence, list retained evidence of a swarm (also after disposal), repair an incomplete handoff with your own attributed report, prepare an explicit successor packet, or dispose a whole settled swarm.",
      "successor returns a ready task packet; nothing spawns until you call task with it.",
    ].join("\n"),
    parameters: z.object({
      action: z.enum(["read", "append", "inspect", "accept", "retained", "dispose", "successor", "repair"]).describe("Notebook action."),
      bookId: z.string().optional().describe("Task book id. Workers may omit it to mean their own book."),
      instanceId: z.string().optional().describe("Worker instance id (successor, repair)."),
      swarmId: z.string().optional().describe("Swarm id (dispose, retained)."),
      after: z.number().optional().describe("read: return entries after this sequence."),
      cursor: z.string().optional().describe("retained: nextCursor from the previous page."),
      limit: z.number().optional().describe("read/retained: 1-50 entries."),
      kind: z.enum(NOTEBOOK_KINDS).optional().describe("append: note or message (workers), evidence or briefing (parents)."),
      body: z.unknown().optional().describe("append/accept: JSON body."),
      report: z.unknown().optional().describe("repair: the complete handoff the parent attests to."),
      discardReason: z.string().optional().describe("dispose: explicit reason for books without accepted evidence."),
      agent: z.string().optional().describe("successor: agent definition; defaults to the predecessor's agent. Required when that agent is unknown."),
      name: z.string().optional().describe("successor: optional task name."),
      task: z.string().optional().describe("successor: assignment text; defaults to continuing from the predecessor's handoff."),
      solutionSpace: z.string().optional().describe("successor: solution space for the task packet."),
    }),
    approval: "write",
    async execute(toolCallId: string, params: Record<string, any>, _signal: unknown, _onUpdate: unknown, ctx: any) {
      const state = stateFor(ctx);
      const worker = state.worker;
      const action = text(params?.action);
      const refuse = (error: string) => ({ isError: true, content: [{ type: "text", text: JSON.stringify({ ok: false, error }, null, 2) }], details: { ok: false, error } });
      const answer = (result: Record<string, unknown>) => ({ isError: result.ok !== true, content: [{ type: "text", text: JSON.stringify(result, null, 2) }], details: result });
      if (isWorkerSession(ctx) && (state.workerFailure || !worker)) return refuse(`worker lifecycle unavailable: ${state.workerFailure || "worker not enrolled"}`);
      if (worker?.retired) return refuse(retiredNotice(worker));
      if (worker?.state === "handoff_only" && action !== "inspect") return refuse("handoff-only: only inspect is allowed; yield your terminal handoff.");
      const binding = worker ? worker.binding : sessionBinding(ctx);
      const requestId = lifecycleRequestId(action, toolCallId);
      const requested = text(params?.bookId);
      try {
        switch (action) {
          case "read": {
            const bookId = requested || worker?.bookId || "";
            if (!bookId) return refuse("read requires bookId");
            const limit = Math.min(READ_LIMIT_MAX, Math.max(1, Math.trunc(Number(params.limit) || 20)));
            const after = Number.isFinite(Number(params.after)) ? Math.max(0, Math.trunc(Number(params.after))) : undefined;
            return answer(await lifecycleRequest(binding, "read", { requestId, bookId, limit, ...(after !== undefined ? { after } : {}) }, { write: false }));
          }
          case "append": {
            if (worker && requested && requested !== worker.bookId) return refuse("a worker appends to its own task book only");
            const bookId = worker?.bookId || requested;
            if (!bookId) return refuse("append requires bookId");
            const kind = text(params.kind) || (worker ? "note" : "evidence");
            if (worker && kind !== "note" && kind !== "message") return refuse("workers append note or message entries");
            if (!worker && kind !== "evidence" && kind !== "briefing") return refuse("parents append evidence or briefing entries");
            if (params.body === undefined) return refuse("append requires body");
            return answer(await lifecycleRequest(binding, "append", { requestId, bookId, kind, body: redactValue(params.body) }));
          }
          case "inspect": {
            const bookId = worker ? worker.bookId : requested;
            return answer(await lifecycleRequest(binding, "inspect", { requestId, ...(bookId ? { bookId } : {}) }, { write: false }));
          }
          case "accept": {
            if (!requested) return refuse("accept requires bookId");
            if (worker?.bookId === requested) return refuse("a worker cannot accept its own book; acceptance belongs to the owning parent");
            if (params.body === undefined) return refuse("accept requires a self-contained body");
            return answer(await lifecycleRequest(binding, "accept", { requestId, bookId: requested, body: redactValue(params.body) }));
          }
          case "retained": {
            if (worker) return refuse("retained evidence belongs to the owning parent");
            const swarmId = text(params.swarmId);
            if (!swarmId) return refuse("retained requires swarmId");
            const limit = Math.min(READ_LIMIT_MAX, Math.max(1, Math.trunc(Number(params.limit) || 20)));
            const cursor = text(params.cursor);
            return answer(await lifecycleRequest(binding, "retained", { requestId, swarmId, limit, ...(cursor ? { after: cursor } : {}) }, { write: false }));
          }
          case "dispose": {
            const swarmId = text(params.swarmId);
            if (!swarmId) return refuse("dispose requires swarmId");
            if (worker?.swarmId === swarmId) return refuse("a worker cannot dispose its own swarm");
            return answer(await lifecycleRequest(binding, "dispose", {
              requestId, swarmId, ...(text(params.discardReason) ? { discardReason: text(params.discardReason) } : {}),
            }));
          }
          case "repair": {
            const instanceId = text(params.instanceId);
            if (!requested || !instanceId) return refuse("repair requires bookId and instanceId");
            if (worker?.bookId === requested) return refuse("a worker cannot repair its own book; repair is the owning parent's attestation");
            if (params.report === undefined || params.report === null || params.report === "") return refuse("repair requires a nonempty report");
            return answer(await lifecycleRequest(binding, "repair", { requestId, bookId: requested, instanceId, report: redactValue(params.report) }));
          }
          case "successor": {
            const instanceId = text(params.instanceId);
            if (!requested || !instanceId) return refuse("successor requires bookId and instanceId");
            if (worker?.bookId === requested) return refuse("a worker cannot prepare its own successor; the owning parent does");
            const assignment = text(params.task)
              || `Continue task book ${requested} as the successor of worker instance ${instanceId}. Your wake carries the predecessor's handoff; read further entries with worker_notebook read before repeating any recorded side effect.`;
            if (assignment.includes(MARKER_HINT)) return refuse("successor task text must not contain enrollment marker text");
            const successorRequest = lifecycleRequestId("successor", requested, instanceId);
            const prior = state.reservations.get(successorRequest);
            if (prior?.consumed) return refuse(`the successor packet for ${instanceId} was already spawned; inspect book ${requested}`);
            const agentName = text(params.agent).toLowerCase() || prior?.agentName
              || [...state.reservations.values()].find((candidate) => candidate.instanceId === instanceId)?.agentName;
            if (!agentName) return refuse("the predecessor's agent is unavailable; inspect the book and name the successor agent explicitly");
            const reservation = reserve(state, {
              requestId: successorRequest,
              swarmKey: `successor:${requested}`,
              slotKey: instanceId,
              agentName,
              task: assignment,
              binding,
              predecessor: { bookId: requested, instanceId },
              successor: true,
            });
            if (!reservation.instanceId) await prepareSlot(reservation);
            const args = {
              ...(reservation.agentName !== "task" ? { agent: reservation.agentName } : {}),
              ...(text(params.name) ? { name: text(params.name) } : {}),
              task: `${reservation.task}\n\n${enrollmentMarker(reservation.token)}`,
              solutionSpace: text(params.solutionSpace) || "Continue from the predecessor's handoff in the same task book; verify recorded pending side effects before repeating them.",
            };
            return answer({
              ok: true,
              swarmId: reservation.swarmId,
              bookId: reservation.bookId,
              instanceId: reservation.instanceId,
              predecessor: reservation.predecessor,
              spawnPacket: { tool: "task", args },
              guidance: "Call task with spawnPacket.args unchanged (in batch mode, place them as one entry of tasks). The marker line is machine-issued and single-use; this packet is returned again until it is spawned.",
            });
          }
          default:
            return refuse(`unknown action ${action}`);
        }
      } catch (error) {
        return answer(organFailure(error));
      }
    },
  });
}

// A process restart empties these in-process registries; tests use this to
// prove recovery comes from persisted session entries alone (as __gigaTest).
export const __workerLifecycleTest = Object.freeze({
  restartProcess() {
    sessions.clear();
    slotRegistry.clear();
    enrolledCalls.clear();
    tokenRooms.clear();
    pendingNotices.clear();
    awaitingResult.clear();
    settlements.clear();
    slotsByAgent.clear();
    slotsBySession.clear();
    slotsByToken.clear();
  },
});

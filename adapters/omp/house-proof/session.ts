// Portable session: one OMP conversation attached to one House home.
//
// The Host owns coordination (crates/host/src/portable.rs). This module owns
// the conversation's side of it: the operator's `/spirit` selection, kept in
// the conversation's own custom entries; the live registration of this exact
// conversation and process instance; the home's identity context; and the
// controls the Host delivers to this instance.
//
// Selection is keyed by conversation, never by cwd or process: two
// conversations in one workspace hold independent homes, and a worker
// resolves through the conversation that spawned it. Only the operator's
// `/spirit` command changes a selection; no tool and no prompt can.

import path from "node:path";
import { existsSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { hostSessionIdentity } from "./host.ts";
import { homeLaunchRoom, isValidRoomKey } from "./room.ts";
import { flushGigaTurns } from "../giga.ts";
import { PortableSessionError, sessionRequest } from "./session-transport.ts";
import { captureTernTerminal, ternLauncher, type Launcher, type TernTerminal } from "./tern-capture.ts";
import { confirmPaperBoat, exitGuard } from "./exit-guards.ts";
import { capturedAgentSession } from "./lesson-ttsr.ts";

export const SELECTION_ENTRY = "athanor-home-selection";
const HOME_CONTEXT_TYPE = "athanor-home-identity";
const CONTINUATION_TYPE = "athanor-restart-continuation";
const POLL_MS = 1_000;
const WARNING_COOLDOWN_MS = 60_000;
const KEEPER_INSTANCE_ENV = "ATHANOR_KEEPER_INSTANCE_ID";
// Controls this adapter can apply through the OMP extension API. `restart`
// is offered only when a launcher can relaunch this exact conversation.
const BASE_CONTROLS = ["deliver", "interrupt", "compact", "close"] as const;
const COMMAND_KINDS = ["deliver", "interrupt", "compact", "close", "restart"] as const;
const KEEPER_EXIT_CODE = 87;

type Json = Record<string, any>;

export type Attachment = {
  key: string;
  room: string;
  // "selected": the operator's `/spirit` choice; refusal is explicit when it
  // cannot attach. "home": the existing launch inside a room directory; it
  // keeps working from local files when the Host is unreachable.
  source: "selected" | "home";
  status: "pending" | "attached" | "failed";
  error: string | null;
  instanceId: string;
  conversationId: string;
  sessionFile: string;
  workspace: string;
  spirit: string;
  operator: string;
  homeDir: string;
  context: { revision: string; text: string } | null;
  controls: string[];
  chatDelivery: boolean;
  restart: { supported: boolean; launcher: string | null; reason?: string } | null;
  heartbeatSecs: number;
  terminal: TernTerminal | null;
  terminalReason: string | null;
  launcher: Launcher | null;
  launcherReason: string | null;
  // Set once a Host-issued restart or close is acknowledged: the shutdown
  // that follows must not detach, because the Host is watching this exact
  // instance exit.
  exiting: boolean;
  connected: boolean;
};

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : value == null ? "" : String(value).trim();
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function pathKey(file: string): string {
  const resolved = path.resolve(file);
  return process.platform === "win32" ? resolved.toLowerCase() : resolved;
}

// A subagent writes `<parent>/<agentId>.jsonl` inside its parent's artifacts
// directory (OMP session-manager resolveBreadcrumbToInteractiveRoot). Walking
// up to the interactive root names the conversation a worker belongs to.
function rootSessionFile(sessionFile: string): string {
  let current = path.resolve(sessionFile);
  for (let depth = 0; depth < 8; depth++) {
    const parent = `${path.dirname(current)}.jsonl`;
    if (!existsSync(parent)) return current;
    current = parent;
  }
  return current;
}

function sessionFileOf(ctx: any): string {
  try {
    return text(ctx?.sessionManager?.getSessionFile?.());
  } catch {
    return "";
  }
}

function conversationKey(ctx: any): string | null {
  const file = sessionFileOf(ctx);
  if (file) return `file:${pathKey(rootSessionFile(file))}`;
  try {
    return `id:${hostSessionIdentity(ctx)}`;
  } catch {
    return null;
  }
}

function isSubagent(ctx: any): boolean {
  return ctx?.agent?.kind === "sub";
}

export function persistedSelection(ctx: any): string | null | undefined {
  let branch: any[] = [];
  try {
    branch = ctx?.sessionManager?.getBranch?.() ?? [];
  } catch {
    return undefined;
  }
  for (let index = branch.length - 1; index >= 0; index--) {
    const entry = branch[index];
    if (entry?.type !== "custom" || entry.customType !== SELECTION_ENTRY) continue;
    const room = entry.data?.room;
    if (room === null) return null;
    return isValidRoomKey(room) ? room : undefined;
  }
  return undefined;
}

const attachments = new Map<string, Attachment>();
// Conversations whose latest persisted selection is an explicit `/spirit off`.
// That null is authoritative until the next explicit `/spirit <room>`: neither
// a launch cwd nor a projected ATHANOR_CLIENT_ROOM_DIR re-attaches a home.
const homeOff = new Set<string>();

export function sessionAttachment(ctx: any): Attachment | null {
  const key = conversationKey(ctx);
  return key ? attachments.get(key) ?? null : null;
}

export function homeExplicitlyOff(ctx: any): boolean {
  const key = conversationKey(ctx);
  return key ? homeOff.has(key) : false;
}

// Workers report their parent conversation's instance.
export function sessionInstanceId(ctx: any): string | null {
  const attachment = sessionAttachment(ctx);
  return attachment?.status === "attached" ? attachment.instanceId : null;
}

// Process instance identity. The integrated keeper names the instance it
// launched; it is spent on the first home this process attaches and on no
// other. A retired instance id is never reused.
const keeperInstance = text(process.env[KEEPER_INSTANCE_ENV]) || null;
let keeperInstanceRoom: string | null = null;
const roomInstances = new Map<string, string>();
// Rooms whose instance in this process the Host retired (instance_stale):
// detached elsewhere, or superseded by a verified restart successor. No
// automatic path (restore, heartbeat, poll) registers this process there
// again; only the operator's explicit `/spirit <room>` may.
const retiredByHost = new Set<string>();

function instanceFor(room: string): string {
  const known = roomInstances.get(room);
  if (known) return known;
  const id = keeperInstance && keeperInstanceRoom === null ? keeperInstance : randomUUID();
  if (id === keeperInstance) keeperInstanceRoom = room;
  roomInstances.set(room, id);
  return id;
}

function retireInstance(room: string, instanceId: string): void {
  if (roomInstances.get(room) === instanceId) roomInstances.delete(room);
}

type ChangeListener = (ctx: any) => unknown;
const changeListeners = new WeakMap<object, ChangeListener[]>();
const shutdownDetachListeners = new WeakMap<object, ChangeListener[]>();

export function onAttachmentChanged(pi: any, listener: ChangeListener): void {
  const listeners = changeListeners.get(pi) ?? [];
  listeners.push(listener);
  changeListeners.set(pi, listeners);
}

export function onBeforeShutdownDetach(pi: any, listener: ChangeListener): void {
  const listeners = shutdownDetachListeners.get(pi) ?? [];
  listeners.push(listener);
  shutdownDetachListeners.set(pi, listeners);
}

async function announceChange(pi: any, ctx: any): Promise<void> {
  for (const listener of changeListeners.get(pi) ?? []) {
    try {
      await listener(ctx);
    } catch (error) {
      console.warn(`[athanor] attachment change listener failed: ${message(error)}`);
    }
  }
}

function identity(attachment: Attachment): Json {
  return { instanceId: attachment.instanceId, conversationId: attachment.conversationId };
}

// `explicit` marks the operator's own `/spirit <room>`: only that path may
// replace an instance the Host retired with a newly minted one.
type AttachOptions = { chatDelivery: boolean; explicit?: boolean; freshInstanceTried?: boolean };

async function attachConversation(
  pi: any,
  ctx: any,
  room: string,
  source: Attachment["source"],
  options: AttachOptions,
): Promise<Attachment> {
  const key = conversationKey(ctx);
  if (!key) throw new PortableSessionError("harness_session_missing", "the harness supplied no session identity", 0);
  const conversationId = hostSessionIdentity(ctx);
  const sessionFile = sessionFileOf(ctx);
  if (!sessionFile || !path.isAbsolute(sessionFile)) {
    throw new PortableSessionError("session_file_missing", "this conversation has no session file, so the Host cannot address or resume it", 0);
  }
  const instanceId = instanceFor(room);
  const keeperLaunch = keeperInstance !== null && instanceId === keeperInstance;
  const { terminal, reason: terminalReason } = await captureTernTerminal(sessionFile);
  let launcher: Launcher | null = null;
  let launcherReason: string | null = null;
  if (keeperLaunch) {
    launcher = { kind: "keeper", keeperInstance: instanceId };
  } else if (terminal) {
    ({ launcher, reason: launcherReason } = await ternLauncher());
  } else {
    launcherReason = terminalReason;
  }
  const controls = [...BASE_CONTROLS, ...(launcher ? ["restart"] : [])];
  const body: Json = {
    action: "attach",
    conversationId,
    instanceId,
    workspace: path.resolve(String(ctx?.cwd || process.cwd())),
    harness: "omp",
    sessionFile: path.resolve(sessionFile),
    processId: process.pid,
    controls,
    chatDelivery: options.chatDelivery,
    ...(terminal ? { terminal } : {}),
    ...(launcher ? { launcher } : {}),
  };
  let result: Json;
  try {
    result = await sessionRequest(room, body);
  } catch (error) {
    // A retired instance id never comes back (wire: instance_stale). Only the
    // operator's explicit attach mints a new instance; automatic paths stop.
    if (error instanceof PortableSessionError && error.code === "instance_stale") {
      retireInstance(room, instanceId);
      retiredByHost.add(room);
      if (!options.explicit || options.freshInstanceTried) throw error;
      retiredByHost.delete(room);
      return attachConversation(pi, ctx, room, source, { ...options, freshInstanceTried: true });
    }
    // Ownership of room chat is explicit: an attach never steals it. The
    // home launch asks for it and settles for a plain attachment when another
    // live instance already delivers chat.
    if (!(error instanceof PortableSessionError) || error.code !== "chat_owner_taken" || !options.chatDelivery) throw error;
    result = await sessionRequest(room, { ...body, chatDelivery: false });
  }
  const attachment: Attachment = {
    key,
    room: text(result.room) || room,
    source,
    status: "attached",
    error: null,
    instanceId: text(result.instanceId) || instanceId,
    conversationId: text(result.conversationId) || conversationId,
    sessionFile: body.sessionFile,
    workspace: text(result.workspace) || body.workspace,
    spirit: text(result.spirit),
    operator: text(result.operator) || "Operator",
    homeDir: text(result.homeDir),
    context: result.context && typeof result.context.text === "string"
      ? { revision: text(result.context.revision), text: result.context.text }
      : null,
    controls: Array.isArray(result.controls) ? result.controls.map(String) : [],
    chatDelivery: result.chatDelivery === true,
    restart: result.restart && typeof result.restart === "object"
      ? { supported: result.restart.supported === true, launcher: text(result.restart.launcher) || null, ...(result.restart.reason ? { reason: text(result.restart.reason) } : {}) }
      : null,
    heartbeatSecs: Number.isFinite(result.heartbeatSecs) && result.heartbeatSecs > 0 ? Number(result.heartbeatSecs) : 10,
    terminal,
    terminalReason,
    launcher,
    launcherReason,
    exiting: false,
    connected: true,
  };
  if (!attachment.spirit || !attachment.homeDir || !path.isAbsolute(attachment.homeDir)) {
    throw new PortableSessionError("invalid_response", "the Host attach result named no spirit or home directory", 200);
  }
  if (hostIdentityRequired(attachment) && !attachment.context?.text) {
    throw new PortableSessionError("invalid_response", `the Host attach result carried no identity context for the ${source === "selected" ? "selected" : "projected"} home`, 200);
  }
  retiredByHost.delete(attachment.room);
  attachments.set(key, attachment);
  queueRestartContinuation(pi, ctx, result.restartSuccessor);
  return attachment;
}

async function detachAttachment(attachment: Attachment, reason: string): Promise<void> {
  attachments.delete(attachment.key);
  if (attachment.status !== "attached" || attachment.exiting) return;
  try {
    await sessionRequest(attachment.room, { action: "detach", ...identity(attachment), reason });
  } catch (error) {
    console.warn(`[athanor] detach from ${attachment.room} failed: ${message(error)}`);
  }
  retireInstance(attachment.room, attachment.instanceId);
}

function failedAttachment(ctx: any, room: string, source: Attachment["source"], error: unknown): Attachment {
  const key = conversationKey(ctx)!;
  let conversationId = "";
  try {
    conversationId = hostSessionIdentity(ctx);
  } catch {
    conversationId = "";
  }
  const attachment: Attachment = {
    key, room, source, status: "failed", error: message(error),
    instanceId: roomInstances.get(room) ?? "", conversationId, sessionFile: sessionFileOf(ctx),
    workspace: path.resolve(String(ctx?.cwd || process.cwd())), spirit: "", operator: "", homeDir: "",
    context: null, controls: [], chatDelivery: false, restart: null, heartbeatSecs: 10,
    terminal: null, terminalReason: null, launcher: null, launcherReason: null, exiting: false, connected: false,
  };
  attachments.set(key, attachment);
  return attachment;
}

// The successor of a Host-driven restart learns it from its own attach
// result and owes itself one turn: the reason the restart was asked for.
const continuedRestarts = new Set<string>();

function queueRestartContinuation(pi: any, ctx: any, successor: any): void {
  const restartId = text(successor?.restartId);
  if (!restartId || continuedRestarts.has(restartId) || typeof pi?.sendMessage !== "function") return;
  continuedRestarts.add(restartId);
  const reason = text(successor?.reason);
  const mode = text(successor?.mode) === "fresh" ? "fresh" : "resume";
  pi.sendMessage({
    customType: CONTINUATION_TYPE,
    content: [
      "<athanor-attention>",
      `You restarted yourself (restart ${restartId}, mode ${mode}); the Host relaunched you in a new Tern tab and verified it.`,
      reason ? `Reason given: ${reason}` : "No reason was recorded.",
      mode === "fresh"
        ? "This is a new conversation. Your paper boat is the letter from the session that restarted; orient from it, then continue what the reason names. Nobody typed this turn."
        : "Your history is intact; continue what the reason names. Nobody typed this turn.",
      "</athanor-attention>",
    ].join("\n"),
    display: true,
    attribution: "agent",
    details: { restartId, reason, mode, predecessorInstanceId: text(successor?.predecessorInstanceId) || null },
  }, { deliverAs: "nextTurn", triggerTurn: true });
  ctx?.ui?.notify?.("Athanor restart continuation queued for the next turn.", "info");
}

type Runtime = {
  pi: any;
  ctx: any;
  key: string | null;
  pollTimer: any;
  heartbeatTimer: any;
  polling: boolean;
  handled: Map<string, "applied" | "refused" | "failed">;
  holding: Map<string, Json>;
  // close/restart commands whose `applied` acknowledgement got no observed
  // answer. The Host may already have armed the exit and may stop listing
  // them, so they are re-acknowledged from here, never re-decided.
  pendingApplied: Map<string, PendingExit>;
  lastWarningAt: number;
};

function warn(runtime: Runtime, text: string): void {
  const now = Date.now();
  if (now - runtime.lastWarningAt < WARNING_COOLDOWN_MS) return;
  runtime.lastWarningAt = now;
  console.warn(`[athanor] portable session: ${text}`);
}

function current(runtime: Runtime): Attachment | null {
  if (!runtime.key) return null;
  const attachment = attachments.get(runtime.key);
  return attachment?.status === "attached" ? attachment : null;
}

async function refreshContext(attachment: Attachment, revision: string): Promise<void> {
  if (!revision || attachment.context?.revision === revision) return;
  const result = await sessionRequest(attachment.room, { action: "context", ...identity(attachment) });
  if (result.context && typeof result.context.text === "string") {
    attachment.context = { revision: text(result.context.revision), text: result.context.text };
  }
}

// A lapsed lease or a Host that forgot this instance re-attaches with the
// same identity. A retired instance is superseded: it stops acting at once.
async function recoverLease(runtime: Runtime, attachment: Attachment, error: PortableSessionError): Promise<void> {
  if (error.code === "instance_stale") {
    attachment.status = "failed";
    attachment.connected = false;
    attachment.error = "this instance was retired by the Host (detached or superseded by a verified restart)";
    retireInstance(attachment.room, attachment.instanceId);
    retiredByHost.add(attachment.room);
    stopTimers(runtime);
    runtime.ctx?.ui?.notify?.(`Athanor home ${attachment.room}: ${attachment.error}.`, "error");
    return;
  }
  if (["lease_expired", "instance_unknown", "conversation_mismatch"].includes(error.code)) {
    try {
      await attachConversation(runtime.pi, runtime.ctx, attachment.room, attachment.source, { chatDelivery: attachment.chatDelivery });
    } catch (reattach) {
      attachment.connected = false;
      warn(runtime, `re-attach to ${attachment.room} failed: ${message(reattach)}`);
    }
    return;
  }
  attachment.connected = false;
  warn(runtime, `${attachment.room}: ${error.message}`);
}

async function heartbeat(runtime: Runtime): Promise<void> {
  const attachment = current(runtime);
  if (!attachment || attachment.exiting) return;
  try {
    const result = await sessionRequest(attachment.room, { action: "heartbeat", ...identity(attachment) });
    attachment.connected = true;
    attachment.chatDelivery = result.chatDelivery === true;
    // The Host may verify a restart successor after answering its attach.
    queueRestartContinuation(runtime.pi, runtime.ctx, result.restartSuccessor);
    await refreshContext(attachment, text(result.contextRevision));
  } catch (error) {
    if (error instanceof PortableSessionError) await recoverLease(runtime, attachment, error);
    else warn(runtime, message(error));
  }
}

async function acknowledge(attachment: Attachment, commandId: string, outcome: "applied" | "refused" | "failed", detail?: string): Promise<void> {
  await sessionRequest(attachment.room, {
    action: "acknowledge",
    ...identity(attachment),
    commandId,
    outcome,
    ...(detail ? { detail: detail.slice(0, 1024) } : {}),
  });
}

async function applyCommand(runtime: Runtime, attachment: Attachment, command: Json): Promise<void> {
  const commandId = text(command.commandId);
  const kind = text(command.kind);
  const payload = command.payload && typeof command.payload === "object" ? command.payload : {};
  const ctx = runtime.ctx;
  const settle = async (outcome: "applied" | "refused" | "failed", detail?: string) => {
    await acknowledge(attachment, commandId, outcome, detail);
    runtime.handled.set(commandId, outcome);
    runtime.holding.delete(commandId);
  };
  switch (kind) {
    case "deliver": {
      const body = text(payload.text);
      if (!body || typeof runtime.pi?.sendUserMessage !== "function") return settle("refused", "nothing deliverable");
      runtime.pi.sendUserMessage(body, { deliverAs: payload.mode === "steer" ? "steer" : "followUp" });
      return settle("applied");
    }
    case "interrupt": {
      if (typeof ctx?.abort !== "function") return settle("refused", "interrupt is unavailable in this harness context");
      ctx.abort();
      return settle("applied");
    }
    case "compact": {
      if (typeof ctx?.compact !== "function") return settle("refused", "compaction is unavailable in this harness context");
      try {
        await ctx.compact(text(payload.instructions) || undefined);
        return settle("applied");
      } catch (error) {
        return settle("failed", message(error));
      }
    }
    case "close":
    case "restart": {
      // A command already being re-acknowledged is never re-decided.
      if (runtime.pendingApplied.has(commandId)) return;
      {
        if (kind === "restart" && !attachment.launcher) return settle("refused", attachment.launcherReason || "no launcher can relaunch this conversation");
        if (typeof ctx?.shutdown !== "function") return settle("refused", "cooperative shutdown is unavailable in this harness context");
        const guard = exitGuard(ctx);
        if (guard && "refuse" in guard) return settle("refused", guard.refuse);
        // OMP saves a conversation lazily (first assistant message), and the
        // extension API exposes no flush. A resume relaunch of an unsaved file
        // dies with "Session not found", so it refuses while the process lives.
        if (!guard && kind === "restart" && payload.mode !== "fresh" && !existsSync(attachment.sessionFile)) {
          return settle("refused", "this conversation is not saved to disk yet, so a resume relaunch could not find it");
        }
        if (!guard && kind === "restart" && payload.mode === "fresh") {
          const boat = await confirmPaperBoat(ctx);
          if (!boat.ok) return settle("refused", `${boat.code}: ${boat.error}`);
        }
        if (guard && "hold" in guard) {
          runtime.holding.set(commandId, command);
          return;
        }
      }
      return commitExit(runtime, {
        commandId, kind, room: attachment.room,
        instanceId: attachment.instanceId, conversationId: attachment.conversationId,
        keeper: kind === "restart" && attachment.launcher?.kind === "keeper",
      });
    }
    default:
      return settle("refused", `unknown control ${kind || "<empty>"}`);
  }
}

// An exit whose `applied` acknowledgement the Host may already hold. The
// identity is frozen at decision time: it is re-acknowledged only under the
// binding it was decided for, never under a later conversation.
type PendingExit = { commandId: string; kind: string; room: string; instanceId: string; conversationId: string; keeper: boolean };

const ACK_ATTEMPTS = 5;

// The Host arms the exit before it answers `applied`, so a lost answer leaves
// the outcome unknown, not refused. Re-acknowledging the same command and
// outcome is idempotent on the Host. Exit only after an observed 200, and only
// while this runtime still holds the conversation the exit was decided for.
async function commitExit(runtime: Runtime, exit: PendingExit): Promise<void> {
  runtime.pendingApplied.set(exit.commandId, exit);
  let acknowledged = false;
  for (let attempt = 0; attempt < ACK_ATTEMPTS && !acknowledged; attempt++) {
    try {
      await sessionRequest(exit.room, {
        action: "acknowledge", instanceId: exit.instanceId, conversationId: exit.conversationId,
        commandId: exit.commandId, outcome: "applied",
      });
      acknowledged = true;
    } catch (error) {
      // Transport loss, a 5xx, or a Host error marked retryable (the Host
      // re-checks the native intent, so a replay never arms twice).
      const ambiguous = error instanceof PortableSessionError
        && (error.status === 0 || error.status >= 500 || error.detail?.retryable === true);
      if (!ambiguous) {
        // A definitive answer: the Host did not take this exit. Stay running.
        runtime.pendingApplied.delete(exit.commandId);
        throw error;
      }
      await new Promise((resolve) => setTimeout(resolve, 1_000 * (attempt + 1)));
    }
  }
  // Still unknown: the entry stays, the next poll re-acknowledges it, and
  // conversation switches stay blocked meanwhile.
  if (!acknowledged) return;
  runtime.pendingApplied.delete(exit.commandId);
  runtime.handled.set(exit.commandId, "applied");
  const live = runtime.key ? attachments.get(runtime.key) : null;
  if (!live || live.instanceId !== exit.instanceId || live.conversationId !== exit.conversationId) {
    // The conversation changed underneath an unpreventable switch. Exiting
    // now would terminate a conversation that never asked to exit.
    runtime.ctx?.ui?.notify?.(`Athanor ${exit.kind} ${exit.commandId} was acknowledged for conversation ${exit.conversationId}, but this session now holds another conversation; it stays running and the restart outcome is unknown.`, "error");
    return;
  }
  live.exiting = true;
  stopTimers(runtime);
  // Buffered GIGA turns are pending work an exit would destroy.
  await flushGigaTurns().catch(() => undefined);
  // The keeper reads exit code 87 as "relaunch me"; Tern and close exit normally.
  if (exit.keeper) setTimeout(() => process.exit(KEEPER_EXIT_CODE), 0);
  else {
    runtime.ctx?.shutdown?.();
    if (runtime.ctx?.mode === "rpc") void finishIdleRpcExit(runtime, exit);
  }
}

const RPC_EXIT_GRACE_MS = 2_000;
const RPC_EXIT_WAIT_MS = 120_000;

// OMP RPC mode honors ctx.shutdown() only when a serial input frame or a
// tracked command settles (rpc-mode onShutdown just latches a flag), so an exit
// decided on this adapter's timer while the session is idle never ran. After a
// grace in which that native path may still fire, this runs the same
// AgentSession.dispose() OMP's own RPC shutdown runs, then ends the process.
// It never interrupts a turn, queued work or running async jobs, and acts only
// for the exact conversation the exit was acknowledged for. The TUI keeps its
// own graceful shutdown; this path is RPC-only.
async function finishIdleRpcExit(runtime: Runtime, exit: PendingExit): Promise<void> {
  const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  const fail = (detail: string) => runtime.ctx?.ui?.notify?.(`Athanor ${exit.kind} ${exit.commandId}: ${detail}; the Host sees no exit and will fail the outcome by its deadline.`, "error");
  const until = Date.now() + RPC_EXIT_WAIT_MS;
  await sleep(RPC_EXIT_GRACE_MS);
  while (Date.now() < until) {
    const ctx = runtime.ctx;
    const live = runtime.key ? attachments.get(runtime.key) : null;
    if (!live?.exiting || live.instanceId !== exit.instanceId || live.conversationId !== exit.conversationId) {
      return fail("the session no longer holds the conversation this exit was acknowledged for");
    }
    const guard = exitGuard(ctx);
    if (guard && "refuse" in guard) return fail(guard.refuse);
    if (!guard) {
      // getContextUsage refreshes the bridge's capture of the live session.
      ctx?.getContextUsage?.();
      const sessionId = text(ctx?.sessionManager?.getSessionId?.());
      const session = sessionId ? capturedAgentSession(sessionId) : null;
      if (!session || typeof session.dispose !== "function") {
        return fail("the live OMP session is not captured, so it cannot be disposed before exit");
      }
      try {
        await session.dispose();
      } catch (error) {
        // As OMP's own RPC shutdown: a dispose failure still ends the process, nonzero.
        process.stderr.write(`Athanor ${exit.kind}: OMP session dispose failed: ${message(error)}\n`);
        process.exit(1);
      }
      process.exit(0);
    }
    await sleep(POLL_MS);
  }
  fail("the session never became idle within the exit wait");
}

// Blocks every conversation change while an exit is pending or taken.
function exitBlocksSwitch(runtime: Runtime): string | null {
  const live = runtime.key ? attachments.get(runtime.key) : null;
  if (live?.exiting) return "this session acknowledged an exit and is shutting down";
  if (runtime.pendingApplied.size) return "an exit acknowledgement is pending with the Athanor Host; the conversation cannot change until it settles";
  return null;
}

async function poll(runtime: Runtime): Promise<void> {
  if (runtime.polling) return;
  const attachment = current(runtime);
  if (!attachment || attachment.exiting) return;
  runtime.polling = true;
  try {
    const result = await sessionRequest(attachment.room, { action: "poll", ...identity(attachment) });
    attachment.connected = true;
    await refreshContext(attachment, text(result.contextRevision));
    for (const pending of [...runtime.pendingApplied.values()]) {
      try {
        await commitExit(runtime, pending);
      } catch (error) {
        warn(runtime, `applied command ${pending.commandId}: ${message(error)}`);
      }
      if (current(runtime)?.exiting) return;
    }
    const commands = Array.isArray(result.commands) ? result.commands : [];
    for (const command of commands) {
      const commandId = text(command?.commandId);
      if (!commandId) continue;
      if (runtime.pendingApplied.has(commandId)) continue;
      const settled = runtime.handled.get(commandId);
      try {
        // A settled command still listed means the acknowledgement was lost:
        // repeat the acknowledgement, never the effect.
        if (settled) await acknowledge(attachment, commandId, settled);
        // A control issued for another conversation never lands on this one,
        // even though the process instance survived a conversation switch.
        else if (text(command.targetConversationId) !== attachment.conversationId) {
          await acknowledge(attachment, commandId, "refused", `issued for conversation ${text(command.targetConversationId) || "<unnamed>"}, not ${attachment.conversationId}`);
          runtime.handled.set(commandId, "refused");
          runtime.holding.delete(commandId);
        } else await applyCommand(runtime, attachment, command);
      } catch (error) {
        warn(runtime, `command ${commandId} (${text(command?.kind)}): ${message(error)}`);
      }
      if (attachment.exiting) return;
    }
  } catch (error) {
    if (error instanceof PortableSessionError) await recoverLease(runtime, attachment, error);
    else warn(runtime, message(error));
  } finally {
    runtime.polling = false;
  }
}

function stopTimers(runtime: Runtime): void {
  for (const timer of [runtime.pollTimer, runtime.heartbeatTimer]) {
    if (!timer) continue;
    try {
      if (typeof runtime.ctx?.clearTimer === "function") runtime.ctx.clearTimer(timer);
      else clearInterval(timer);
    } catch {
      // Already cleared by session_shutdown.
    }
  }
  runtime.pollTimer = null;
  runtime.heartbeatTimer = null;
}

function startTimers(runtime: Runtime, attachment: Attachment): void {
  stopTimers(runtime);
  const every = (callback: () => void, ms: number) => typeof runtime.ctx?.setInterval === "function"
    ? runtime.ctx.setInterval(callback, ms)
    : setInterval(callback, ms);
  runtime.pollTimer = every(() => void poll(runtime), POLL_MS);
  runtime.heartbeatTimer = every(() => void heartbeat(runtime), attachment.heartbeatSecs * 1000);
}

// The same home refreshes the same instance with the new conversation, which
// keeps the keeper launcher; any other outcome detaches the previous one.
function bindRuntime(runtime: Runtime, ctx: any): Attachment | null {
  const key = conversationKey(ctx);
  runtime.ctx = ctx;
  let previous: Attachment | null = null;
  if (runtime.key && runtime.key !== key) {
    previous = attachments.get(runtime.key) ?? null;
    stopTimers(runtime);
  }
  runtime.key = key;
  return previous;
}

// Settles every held close/restart of the conversation being left with an
// explicit refusal, so no exit queued for one conversation can fire after
// this instance rebinds to another.
async function refuseHeld(runtime: Runtime, attachment: Attachment, detail: string): Promise<void> {
  for (const commandId of [...runtime.holding.keys()]) {
    runtime.holding.delete(commandId);
    try {
      await acknowledge(attachment, commandId, "refused", detail);
      runtime.handled.set(commandId, "refused");
    } catch (error) {
      warn(runtime, `held command ${commandId} could not be refused: ${message(error)}`);
    }
  }
}

async function restore(runtime: Runtime, ctx: any): Promise<void> {
  // An acknowledged exit is final for this process: it never rebinds.
  const held = runtime.key ? attachments.get(runtime.key) : null;
  if (held?.exiting) return;
  const previous = bindRuntime(runtime, ctx);
  if (previous?.status === "attached") await refuseHeld(runtime, previous, "conversation switched before the session went idle");
  const key = runtime.key;
  if (!key) {
    if (previous) await detachAttachment(previous, "conversation switched");
    return;
  }
  const selection = persistedSelection(ctx);
  // An explicit `/spirit off` (persisted null) outranks every launch room.
  if (selection === null) homeOff.add(key);
  else homeOff.delete(key);
  const launch = selection === null ? null : (() => {
    try {
      return homeLaunchRoom(ctx?.cwd);
    } catch {
      return null;
    }
  })();
  const room = selection ?? launch?.room ?? null;
  if (previous) {
    if (previous.room === room && previous.status === "attached") attachments.delete(previous.key);
    else await detachAttachment(previous, "conversation switched");
  }
  if (room && retiredByHost.has(room)) {
    const source: Attachment["source"] = selection ? "selected" : "home";
    failedAttachment(ctx, room, source, `the Host retired this process's ${room} instance; run /spirit ${room} to attach as a new instance`);
    ctx?.ui?.notify?.(`Athanor home ${room}: the Host retired this process's instance (a successor or a detach replaced it). Run /spirit ${room} to attach again.`, "warning");
    return;
  }
  const existing = attachments.get(key);
  if (!room) {
    if (existing) await detachAttachment(existing, "no home selected");
    return;
  }
  const source: Attachment["source"] = selection ? "selected" : "home";
  if (existing?.status === "attached" && existing.room === room && existing.source === source) {
    startTimers(runtime, existing);
    return;
  }
  if (existing && existing.room !== room) await detachAttachment(existing, "home changed");
  try {
    // A refresh of the same home keeps the chat ownership the previous
    // registration held; a home launch asks for it, a selection does not.
    const keepsChat = previous?.room === room ? previous.chatDelivery : source === "home";
    const attachment = await attachConversation(runtime.pi, ctx, room, source, { chatDelivery: keepsChat });
    if (selection === undefined) {
      runtime.pi.appendEntry(SELECTION_ENTRY, { room: attachment.room });
    }
    startTimers(runtime, attachment);
    // A second session of the same home attaches without taking room chat
    // over; the owner keeps it until it releases (/spirit chat on to claim).
    if (keepsChat && !attachment.chatDelivery) {
      let owner = "another live instance";
      try {
        owner = text((await sessionRequest(room, { action: "status" })).chatDeliveryOwner) || owner;
      } catch {
        // Without status the notice still names the conflict.
      }
      ctx?.ui?.notify?.(`Athanor home ${room} attached; room chat stays with ${owner}.`, "info");
    }
  } catch (error) {
    const failed = failedAttachment(ctx, room, source, error);
    if (source === "selected") {
      ctx?.ui?.notify?.(`Athanor home ${room} could not attach: ${failed.error}. Execution stays refused until /spirit ${room} succeeds or /spirit off detaches.`, "error");
    } else if (hostIdentityRequired(failed)) {
      ctx?.ui?.notify?.(`Athanor home ${room} was projected into this folder but could not attach: ${failed.error}. Its identity is not loaded here, so execution stays refused.`, "error");
    } else {
      ctx?.ui?.notify?.(`Athanor Host session registration for ${room} is unavailable: ${failed.error}. The home launch continues without Host session controls.`, "warning");
    }
  }
}

// A home launch whose harness starts in the home reads its hand-owned
// AGENTS.md natively. A launch projected through ATHANOR_CLIENT_ROOM_DIR into
// another cwd (a Host restart into the project) does not, so its identity comes
// from the Host exactly as a /spirit selection's does, and it fails closed too.
function hostIdentityRequired(attachment: Attachment): boolean {
  if (attachment.source === "selected") {
    return !attachment.homeDir || pathKey(attachment.homeDir) !== pathKey(attachment.workspace);
  }
  let roomDir: string | null = null;
  try {
    roomDir = homeLaunchRoom(attachment.workspace)?.roomDir ?? null;
  } catch {
    // An invalid projection already refused the launch room upstream.
    return false;
  }
  if (!roomDir) return false;
  const workspace = path.resolve(attachment.workspace);
  const home = path.resolve(roomDir);
  return process.platform === "win32" ? workspace.toLowerCase() !== home.toLowerCase() : workspace !== home;
}

// Workers of a conversation whose selected (or projected) home is unavailable
// refuse too: they never fall back to running as an ordinary session.
function selectionRefusal(ctx: any): string | null {
  const attachment = sessionAttachment(ctx);
  if (!attachment || attachment.status === "attached" || !hostIdentityRequired(attachment)) return null;
  if (attachment.source === "home") {
    return `Athanor refuses to run this turn: this harness was launched for home ${attachment.room} outside that home, and the Host identity for it is unavailable (${attachment.error || "not attached"}). Restart the Athanor Host or relaunch inside the home.`;
  }
  return `Athanor refuses to run this turn: the selected home ${attachment.room} is unavailable (${attachment.error || "not attached"}). Run /spirit ${attachment.room} to attach again or /spirit off to continue as an ordinary session.`;
}

function contextBlock(attachment: Attachment): string {
  return [
    `<athanor-home room="${attachment.room}" spirit="${attachment.spirit}" revision="${attachment.context!.revision}">`,
    "This conversation is attached to an Athanor home outside the current workspace.",
    "The workspace and its project instructions stay authoritative for project work; this block is who you are.",
    attachment.context!.text,
    "</athanor-home>",
  ].join("\n");
}

type ReleaseRooms = Record<string, { spirit?: string }> | undefined;

function statusLines(ctx: any, rooms: ReleaseRooms): string[] {
  const attachment = sessionAttachment(ctx);
  const lines: string[] = [];
  if (!attachment) {
    lines.push("No home attached: this is an ordinary harness session.");
  } else {
    lines.push(`Home ${attachment.room} (${attachment.source}): ${attachment.status}${attachment.error ? ` - ${attachment.error}` : ""}`);
    if (attachment.status === "attached") {
      lines.push(`Spirit ${attachment.spirit}; operator ${attachment.operator}; home ${attachment.homeDir}`);
      lines.push(`Instance ${attachment.instanceId}; conversation ${attachment.conversationId}; ${attachment.connected ? "connected" : "disconnected"}`);
      lines.push(`Controls ${attachment.controls.join(", ") || "none"}; room chat ${attachment.chatDelivery ? "delivered here" : "not delivered here"}`);
      lines.push(`Restart ${attachment.restart?.supported ? `supported (${attachment.restart.launcher})` : `unsupported (${attachment.restart?.reason || attachment.launcherReason || "no launcher"})`}`);
      lines.push(`Terminal ${attachment.terminal ? `tern session ${attachment.terminal.session} pane ${attachment.terminal.pane}` : attachment.terminalReason || "none"}`);
    }
  }
  const configured = Object.entries(rooms ?? {});
  lines.push(configured.length
    ? `Configured homes: ${configured.map(([room, identity]) => `${room} (${text(identity?.spirit) || "?"})`).join(", ")}`
    : "Configured homes: unavailable without the installed client projection; name the room key.");
  return lines;
}

export function installPortableSession(pi: any, release?: { rooms?: ReleaseRooms } | null): void {
  const rooms = release?.rooms;
  const runtime: Runtime = {
    pi, ctx: null, key: null, pollTimer: null, heartbeatTimer: null, polling: false,
    handled: new Map(), holding: new Map(), pendingApplied: new Map(), lastWarningAt: 0,
  };

  const restoreMain = async (_event: unknown, ctx: any) => {
    if (isSubagent(ctx)) return;
    await restore(runtime, ctx);
  };
  // A pending or taken exit belongs to the conversation it was decided for;
  // OMP's supported pre-switch hooks cancel any change of conversation.
  const blockSwitch = (_event: unknown, ctx: any) => {
    if (isSubagent(ctx)) return undefined;
    const reason = exitBlocksSwitch(runtime);
    if (!reason) return undefined;
    ctx?.ui?.notify?.(`Athanor blocked the conversation change: ${reason}.`, "error");
    return { cancel: true };
  };
  pi.on("session_before_switch", blockSwitch);
  pi.on("session_before_branch", blockSwitch);
  pi.on("session_before_tree", blockSwitch);
  pi.on("session_start", restoreMain);
  pi.on("session_switch", restoreMain);
  pi.on("session_branch", restoreMain);
  pi.on("session_tree", restoreMain);
  // Compaction keeps the selection entry on the branch; only a conversation
  // that lost its live registration needs to restore it.
  pi.on("session_compact", async (_event: unknown, ctx: any) => {
    if (isSubagent(ctx)) return;
    if (!sessionAttachment(ctx) || runtime.key !== conversationKey(ctx)) await restore(runtime, ctx);
  });
  pi.on("session_shutdown", async (_event: unknown, ctx: any) => {
    for (const listener of shutdownDetachListeners.get(pi) ?? []) {
      try {
        await listener(ctx);
      } catch (error) {
        console.warn(`[athanor] pre-shutdown detach listener failed: ${message(error)}`);
      }
    }
    if (isSubagent(ctx)) return;
    stopTimers(runtime);
    const attachment = runtime.key ? attachments.get(runtime.key) : null;
    if (attachment) await detachAttachment(attachment, "session shutdown");
  });

  // A selected home that cannot attach refuses execution. Slash commands
  // still pass so `/spirit` can repair or detach.
  pi.on("input", (event: any, ctx: any) => {
    if (text(event?.text).startsWith("/")) return undefined;
    const refusal = selectionRefusal(ctx);
    if (!refusal) return undefined;
    ctx?.ui?.notify?.(refusal, "error");
    return { handled: true };
  });

  pi.on("before_agent_start", (_event: any, ctx: any) => {
    if (isSubagent(ctx)) return undefined;
    const refusal = selectionRefusal(ctx);
    if (refusal) {
      ctx?.ui?.notify?.(refusal, "error");
      ctx?.abort?.();
      return undefined;
    }
  });

  // Idle continuations do not emit before_agent_start. Context runs for every
  // provider request, using the same trusted custom-message lane as Presence.
  pi.on("context", (event: any, ctx: any) => {
    if (isSubagent(ctx)) return undefined;
    const original = Array.isArray(event?.messages) ? event.messages : [];
    const messages = original.filter((entry: any) => entry?.role !== "custom" || entry.customType !== HOME_CONTEXT_TYPE);
    const attachment = sessionAttachment(ctx);
    if (attachment?.status === "attached" && attachment.context?.text && hostIdentityRequired(attachment)) {
      messages.push({
        role: "custom",
        customType: HOME_CONTEXT_TYPE,
        content: contextBlock(attachment),
        display: false,
        attribution: "agent",
        timestamp: Date.now(),
        details: { room: attachment.room, revision: attachment.context.revision },
      });
      return { messages };
    }
    return messages.length === original.length ? undefined : { messages };
  });

  // The fail-closed gate. OMP swallows handler errors (runner emitError), so
  // a throw cannot refuse; aborting the run's signal here, before the provider
  // sends the payload, is the same gate the worker lifecycle uses. It covers
  // every turn origin: typed input, injected chat says, follow-ups, and workers.
  pi.on("before_provider_request", (_event: any, ctx: any) => {
    const refusal = selectionRefusal(ctx);
    if (!refusal) return undefined;
    ctx?.ui?.notify?.(refusal, "error");
    ctx?.abort?.();
    return undefined;
  });

  // Held close/restart commands settle once the session is idle.
  pi.on("agent_end", async () => {
    const attachment = current(runtime);
    if (!attachment || runtime.holding.size === 0) return;
    for (const command of [...runtime.holding.values()]) {
      try {
        await applyCommand(runtime, attachment, command);
      } catch (error) {
        warn(runtime, `held command ${text(command.commandId)}: ${message(error)}`);
      }
      if (attachment.exiting) return;
    }
  });

  pi.registerCommand?.("spirit", {
    description: "Attach this conversation to an Athanor home (/spirit <room>), detach (/spirit off), show status (/spirit), or claim room chat (/spirit chat on|off)",
    getArgumentCompletions: (prefix: string) => {
      const options = ["off", "status", "chat on", "chat off", ...Object.keys(rooms ?? {})];
      return options.filter((option) => option.startsWith(prefix.trim())).map((option) => ({ value: option, label: option }));
    },
    handler: async (args: string, ctx: any) => {
      const words = text(args).split(/\s+/).filter(Boolean);
      const verb = (words[0] ?? "status").toLowerCase();
      const blocked = exitBlocksSwitch(runtime);
      if (blocked && verb !== "status") {
        ctx.ui.notify(`/spirit ${verb} refused: ${blocked}.`, "error");
        return;
      }
      const switched = bindRuntime(runtime, ctx);
      if (switched) await detachAttachment(switched, "conversation switched");
      if (verb === "status") {
        ctx.ui.notify(statusLines(ctx, rooms).join("\n"), "info");
        return;
      }
      if (verb === "chat") {
        const attachment = sessionAttachment(ctx);
        if (attachment?.status !== "attached") {
          ctx.ui.notify("Room chat delivery needs an attached home.", "error");
          return;
        }
        const claim = (words[1] ?? "").toLowerCase() === "on";
        try {
          const result = await sessionRequest(attachment.room, { action: "chatDelivery", ...identity(attachment), claim });
          attachment.chatDelivery = result.chatDelivery === true;
          ctx.ui.notify(`Room chat for ${attachment.room} is ${attachment.chatDelivery ? "delivered here" : `delivered by ${text(result.owner) || "no instance"}`}.`, "info");
        } catch (error) {
          ctx.ui.notify(`Room chat delivery change refused: ${message(error)}`, "error");
        }
        return;
      }
      if (verb === "off" || verb === "none") {
        const attachment = sessionAttachment(ctx);
        if (attachment) await detachAttachment(attachment, "operator detached");
        pi.appendEntry(SELECTION_ENTRY, { room: null });
        await restore(runtime, ctx);
        await announceChange(pi, ctx);
        ctx.ui.notify(statusLines(ctx, rooms)[0], "info");
        return;
      }
      if (!isValidRoomKey(verb)) {
        ctx.ui.notify(`"${verb}" is not a room key. ${statusLines(ctx, rooms).at(-1)}`, "error");
        return;
      }
      const previous = sessionAttachment(ctx);
      try {
        const attachment = await attachConversation(pi, ctx, verb, "selected", {
          chatDelivery: previous?.room === verb ? previous.chatDelivery : false,
          explicit: true,
        });
        if (previous && previous !== attachment && previous.room !== attachment.room) {
          if (previous.status === "attached") {
            try {
              await sessionRequest(previous.room, { action: "detach", ...identity(previous), reason: "operator selected another home" });
            } catch (error) {
              console.warn(`[athanor] detach from ${previous.room} failed: ${message(error)}`);
            }
            retireInstance(previous.room, previous.instanceId);
          }
        }
        // Persisted only after the Host authorized the home, so a refused
        // attach never replaces a working selection.
        pi.appendEntry(SELECTION_ENTRY, { room: attachment.room });
        const selectedKey = conversationKey(ctx);
        if (selectedKey) homeOff.delete(selectedKey);
        startTimers(runtime, attachment);
        await announceChange(pi, ctx);
        ctx.ui.notify(`Attached to ${attachment.room} as ${attachment.spirit}. The workspace stays ${attachment.workspace}.${existsSync(attachment.sessionFile) ? "" : " The selection is saved with this conversation once OMP saves its first turn."}`, "info");
      } catch (error) {
        ctx.ui.notify(`Athanor home ${verb} refused attachment: ${message(error)}${previous ? `; this conversation stays attached to ${previous.room}` : ""}.`, "error");
      }
    },
  });
}

export function requireAttached(ctx: any): Attachment {
  const attachment = sessionAttachment(ctx);
  if (!attachment) throw new PortableSessionError("no_home_attached", "this session has no attached home; the operator attaches one with /spirit <room>", 0);
  if (attachment.status !== "attached") {
    throw new PortableSessionError("selected_home_unavailable", `the home ${attachment.room} is not attached: ${attachment.error || "pending"}`, 0);
  }
  return attachment;
}

export async function homeResource(ctx: any, request: Json, signal?: AbortSignal): Promise<Json> {
  const attachment = requireAttached(ctx);
  return await sessionRequest(attachment.room, { ...request, ...identity(attachment) }, signal);
}

export async function homeSessionStatus(ctx: any, signal?: AbortSignal): Promise<Json> {
  const attachment = requireAttached(ctx);
  return await sessionRequest(attachment.room, { action: "status" }, signal);
}

export type SessionCommandKind = typeof COMMAND_KINDS[number];
export const SESSION_COMMAND_KINDS = COMMAND_KINDS;

export async function issueSessionCommand(
  ctx: any,
  targetInstanceId: string,
  targetConversationId: string,
  kind: SessionCommandKind,
  payload: Json,
  commandId: string,
  signal?: AbortSignal,
): Promise<Json> {
  const attachment = requireAttached(ctx);
  return await sessionRequest(attachment.room, {
    action: "command",
    commandId,
    targetInstanceId,
    targetConversationId,
    issuerInstanceId: attachment.instanceId,
    kind,
    payload,
  }, signal);
}

export async function sessionCommandStatus(ctx: any, commandId: string, signal?: AbortSignal): Promise<Json> {
  const attachment = requireAttached(ctx);
  return await sessionRequest(attachment.room, { action: "commandStatus", commandId }, signal);
}

export const __sessionTest = Object.freeze({
  attachments,
  conversationKey,
  rootSessionFile,
  reset() {
    attachments.clear();
    roomInstances.clear();
    continuedRestarts.clear();
    keeperInstanceRoom = null;
  },
});

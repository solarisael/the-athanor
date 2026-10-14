// The adapter's restart door.
//
// The Host owns restart coordination (crates/host/src/portable.rs): it
// records the intent with this instance as the requester, arms exiting,
// delivers a restart control to this exact instance, watches the process exit
// through its own handle, and relaunches it in a new Tern tab or through the
// integrated keeper. No second supervisor lives here.
//
// request_restart asks the Host to restart this instance and reports what the
// exit costs; session.ts applies the delivered control behind exit-guards.ts.
// A keeper-relaunched successor still verifies itself with the keeper's
// successor proof (the Host sends keeper successors no restartSuccessor); a
// Tern successor is verified by the Host when its new instance attaches.

import path from "node:path";
import { existsSync, readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { roomBinding } from "./room.ts";
import { type HostBinding } from "./host.ts";
import type { OrganOperation } from "./organ.ts";
import { topLevelSession } from "./top-level-session-fence.ts";
import { issueSessionCommand, requireAttached, type Attachment } from "./session.ts";
import { PortableSessionError } from "./session-transport.ts";
import { confirmPaperBoat } from "./exit-guards.ts";

type DomainReceipt = Record<string, unknown>;

type DomainRequest = (
  binding: HostBinding,
  method: OrganOperation,
  params: Record<string, unknown>,
  signal?: AbortSignal,
  write?: boolean,
) => Promise<DomainReceipt>;

type LoadedRelease = {
  releaseId?: string | null;
  previousReleaseId?: string | null;
} | null | undefined;

// One row per session that still holds turns waiting for a GIGA flush.
type GigaBufferCensus = { session?: unknown; cwd?: unknown; turns?: unknown };

type IssueCommand = (
  ctx: any,
  targetInstanceId: string,
  targetConversationId: string,
  kind: "restart",
  payload: Record<string, unknown>,
  commandId: string,
  signal?: AbortSignal,
) => Promise<Record<string, unknown>>;

export type RestartDoorDeps = {
  // tools.ts's existing requestRustDomain seam (restart_status, restart_verify).
  requestDomain: DomainRequest;
  // The house tool registrar, so this tool wears the same feedback renderers.
  registerTool: (definition: Record<string, unknown>) => void;
  // Threaded from configureInstalledAthanor through the adapter entry.
  release?: LoadedRelease;
  // giga.ts's read-only buffered-turn census.
  gigaBuffers?: () => GigaBufferCensus[] | null;
  // The keeper's successor proof is verified with the room's restart_verify
  // capability; injected in tests so proving the fence needs no real secret.
  verifyCapability?: (effectiveRoomDir: string) => string | null;
  restartIntentId?: () => string | null;
  restartSuccessorProof?: () => string | null;
  isEmbodied?: (room: string, session: string) => boolean;
  // This conversation's live Host registration, and the Host command seam.
  attachment?: (ctx: any) => Attachment;
  // Reads the room's latest paper boat; a read, never a consume.
  latestBoat?: (binding: HostBinding, options?: { signal?: AbortSignal }) => Promise<Record<string, unknown>>;
  issueCommand?: IssueCommand;
};

const VERIFY_CAPABILITY_ENV = "ATHANOR_RESTART_VERIFY_CAPABILITY";
const VERIFY_CAPABILITY_FILENAME = "restart-verify-capability";
const RESTART_INTENT_ENV = "ATHANOR_RESTART_INTENT_ID";
const RESTART_SUCCESSOR_PROOF_ENV = "ATHANOR_RESTART_SUCCESSOR_PROOF";

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function refuse(code: string, error: string, extra: Record<string, unknown> = {}) {
  const result = { ok: false, tool: "request_restart", code, error, ...extra };
  return {
    isError: true,
    content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
    details: result,
  };
}

function report(result: Record<string, unknown>) {
  return {
    content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
    details: result,
  };
}

function successorNotice(ctx: any, message: string, level: "info" | "warning"): void {
  if (ctx?.hasUI && typeof ctx?.ui?.notify === "function") {
    ctx.ui.notify(message, level);
  } else {
    console.warn(`[athanor] ${message}`);
  }
}

function readCapability(
  effectiveRoomDir: string,
  environKey: string,
  filename: string,
  environ: NodeJS.ProcessEnv = process.env,
): string | null {
  const configured = String(environ[environKey] || "").trim();
  if (configured) return configured;
  try {
    return readFileSync(path.join(effectiveRoomDir, ".omp", "runtime", filename), "utf8").trim() || null;
  } catch {
    return null;
  }
}

// The contract fixes the intent's fields (id, state, mode, sessionId,
// deadlines) but not the envelope restart_status wraps them in, so the door
// reads the shapes the substrate can honestly return and treats anything else
// as no pending intent rather than inventing one.
function pendingIntent(receipt: DomainReceipt): Record<string, unknown> | null {
  const candidate = receipt?.intent ?? receipt?.pending ?? (receipt?.intentId ? receipt : null);
  if (!candidate || typeof candidate !== "object" || Array.isArray(candidate)) return null;
  const intent = candidate as Record<string, unknown>;
  return text(intent.intentId ?? intent.id) ? intent : null;
}

// The harness's own async-job door: ExtensionContext.getAsyncJobSnapshot()
// returns { running, recent, delivery }. Only `running` is a casualty: an
// async job is owned by this session and cannot outlive it.
const ASYNC_JOB_SOURCE = "ctx.getAsyncJobSnapshot";

function asyncJobCasualties(ctx: any) {
  const probe = ctx?.getAsyncJobSnapshot;
  if (typeof probe !== "function") {
    return {
      enumerable: false,
      source: null,
      reason: `this harness context exposes no ${ASYNC_JOB_SOURCE}, so session-owned async work cannot be counted here`,
    };
  }
  let snapshot: unknown = null;
  try {
    snapshot = probe.call(ctx);
  } catch {
    snapshot = null;
  }
  const running = (snapshot as { running?: unknown } | null)?.running;
  if (!Array.isArray(running)) {
    return {
      enumerable: false,
      source: ASYNC_JOB_SOURCE,
      reason: `${ASYNC_JOB_SOURCE} returned no running-job list for this session`,
    };
  }
  const jobs = running.map((job: any) => ({
    id: text(job?.id),
    type: text(job?.type),
    label: text(job?.label),
  }));
  return { enumerable: true, source: ASYNC_JOB_SOURCE, count: jobs.length, jobs };
}

// Hub processes (hub op:"start", persist/detached) are project-scoped and live
// outside this session. The plugin surface exposes no process listing of any
// kind, so this door refuses to classify them rather than promise their fate.
function hubProcessCasualties() {
  return {
    enumerable: false,
    reason:
      "hub processes are project-scoped and the plugin surface exposes no process listing, so this door cannot name them or their fate",
  };
}

// Turns buffered for a later GIGA flush. session.ts flushes them after the
// restart is acknowledged and before the exit; the census says what is at stake.
function gigaBufferCasualties(inspect?: () => GigaBufferCensus[] | null) {
  let buffers: GigaBufferCensus[] | null = null;
  try {
    buffers = inspect ? inspect() : null;
  } catch {
    buffers = null;
  }
  if (!Array.isArray(buffers)) {
    return {
      enumerable: false,
      reason: "no giga buffered-turn census was threaded into this door, so buffered turns cannot be counted here",
    };
  }
  const bySession = buffers.map((buffer) => ({
    session: text(buffer?.session),
    turns: Number(buffer?.turns) || 0,
  }));
  return {
    enumerable: true,
    sessions: bySession.length,
    turns: bySession.reduce((total, buffer) => total + buffer.turns, 0),
    bySession,
    fate: "flushed after the restart is acknowledged, before the exit",
  };
}

function loadedRelease(release: LoadedRelease) {
  return {
    releaseId: release?.releaseId ?? null,
    previousReleaseId: release?.previousReleaseId ?? null,
  };
}

// The Host requires a UUID commandId. Deriving it from the tool call keeps a
// replayed call on the same command, which the Host answers idempotently.
function restartCommandId(instanceId: string, toolCallId: string): string {
  const hex = createHash("sha256").update(`request_restart\0${instanceId}\0${toolCallId}`).digest("hex");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-4${hex.slice(13, 16)}-${((parseInt(hex[16], 16) & 3) | 8).toString(16)}${hex.slice(17, 20)}-${hex.slice(20, 32)}`;
}

export function registerRestartDoor(pi: any, deps: RestartDoorDeps): void {
  const z = pi.zod;
  const resolveVerifyCapability = deps.verifyCapability
    ?? ((roomDir: string) => readCapability(roomDir, VERIFY_CAPABILITY_ENV, VERIFY_CAPABILITY_FILENAME));
  const resolveRestartIntent = deps.restartIntentId
    ?? (() => text(process.env[RESTART_INTENT_ENV]));
  const resolveRestartSuccessorProof = deps.restartSuccessorProof
    ?? (() => text(process.env[RESTART_SUCCESSOR_PROOF_ENV]));
  const isEmbodied = deps.isEmbodied
    ?? ((room: string, session: string) => topLevelSession(room) === session);
  const attachmentFor = deps.attachment ?? requireAttached;
  const issueCommand: IssueCommand = deps.issueCommand ?? issueSessionCommand;
  let verifiedIntent = "";
  let verifyingIntent = "";

  // The integrated keeper gives only a relaunched OMP the intent id and the
  // successor proof. Verify on startup, after the earlier session_start hook
  // has established the embodied session.
  const verifySuccessor = async (_event: unknown, ctx: any) => {
    const intentId = text(resolveRestartIntent());
    if (!intentId || verifiedIntent === intentId || verifyingIntent === intentId) return;
    let resolved: ReturnType<typeof roomBinding> = null;
    try {
      resolved = roomBinding(ctx);
    } catch (error) {
      successorNotice(ctx, `Athanor restart successor could not verify: ${error instanceof Error ? error.message : String(error)}`, "warning");
      return;
    }
    if (!resolved) {
      successorNotice(ctx, "Athanor restart successor could not verify: this conversation holds no home.", "warning");
      return;
    }
    const { binding, context } = resolved;
    if (!isEmbodied(binding.room, binding.session)) {
      if (ctx?.mode !== "tui") {
        successorNotice(ctx, "Athanor restart successor verification skipped: this session is not the room's embodied session.", "warning");
      }
      return;
    }
    const successorProof = text(resolveRestartSuccessorProof());
    if (!successorProof) {
      successorNotice(
        ctx,
        "Athanor restart successor could not verify: the keeper supplied no successor proof.",
        "warning",
      );
      return;
    }
    const capability = text(resolveVerifyCapability(context.effectiveRoomDir));
    if (!capability) {
      successorNotice(
        ctx,
        "Athanor restart successor could not verify: this room holds no restart_verify capability.",
        "warning",
      );
      return;
    }
    verifyingIntent = intentId;
    try {
      const receipt = await deps.requestDomain(binding, "restart_verify", {
        intentId,
        successorProof,
        capability,
      }, undefined, true);
      if (receipt?.ok === false || text(receipt?.state) !== "verified") {
        const code = text(receipt?.code) || "not_verified";
        successorNotice(ctx, `Athanor restart successor verification failed (${code}).`, "warning");
        return;
      }
      verifiedIntent = intentId;
      delete process.env[RESTART_INTENT_ENV];
      delete process.env[RESTART_SUCCESSOR_PROOF_ENV];
      successorNotice(ctx, "Athanor restart successor verified.", "info");
      await continueAfterRestart(ctx, binding, intentId);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      successorNotice(ctx, `Athanor restart successor verification failed: ${message}`, "warning");
    } finally {
      verifyingIntent = "";
    }
  };

  // A restart that nobody continues is a cold boot with history (Sol,
  // 2026-09-05: "nothing triggered continuation after restart"). The successor
  // owes itself one turn: the reason the requester gave, delivered as the
  // next turn and triggered. Once per intent.
  const continueAfterRestart = async (ctx: any, binding: HostBinding, intentId: string) => {
    const workspace = text(ctx?.cwd) || process.cwd();
    let intent: Record<string, unknown> | null = null;
    try {
      const status = await deps.requestDomain(binding, "restart_status", { workspace, intentId }, undefined, true);
      intent = pendingIntent(status);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      successorNotice(ctx, `Athanor restart continuation skipped: ${message}`, "warning");
      return;
    }
    if (!intent) {
      successorNotice(ctx, "Athanor restart continuation skipped: the House returned no pending intent.", "warning");
      return;
    }
    const mode = text(intent.mode) || "resume";
    const reason = text(intent.reason);
    if (typeof pi.sendMessage !== "function") {
      successorNotice(ctx, "Athanor restart continuation skipped: OMP cannot enqueue a next turn.", "warning");
      return;
    }
    pi.sendMessage(continuationMessage(intentId, mode, reason), { deliverAs: "nextTurn", triggerTurn: true });
    if (ctx?.mode !== "tui") {
      successorNotice(ctx, "Athanor restart continuation queued for the next turn.", "info");
    }
  };

  // What the successor is handed. It is a turn, so it is displayed and
  // attributed to the House, not to the operator.
  function continuationMessage(intentId: string, mode: string, reason: string): Record<string, unknown> {
    const why = reason ? `Reason given: ${reason}` : "No reason was recorded.";
    return {
      customType: "athanor-restart-continuation",
      content: [
        "<athanor-attention>",
        `You restarted yourself (mode ${mode}, intent ${intentId}); the keeper relaunched this session and the House verified it.`,
        why,
        mode === "fresh"
          ? "Your paper boat is the letter from the session that restarted; orient from it, then continue what the reason names."
          : "Your history is intact; continue what the reason names. Nobody typed this turn.",
        "</athanor-attention>",
      ].join("\n"),
      display: true,
      attribution: "agent",
      details: { intentId, mode, reason },
    };
  }
  pi.on("session_start", verifySuccessor);
  pi.on("session_switch", verifySuccessor);

  deps.registerTool({
    name: "request_restart",
    label: "Athanor Restart Request",
    description: [
      "Ask the Athanor Host to restart this exact omp session and resume this same conversation.",
      "The Host records the intent, delivers the restart to this session, waits for this process to exit, and relaunches it:",
      "in a new Tern tab beside this pane, or through the integrated keeper. The predecessor pane is never closed or typed into.",
      "The exit never fires inside a turn: the session applies it once idle, and refuses it while async jobs would die.",
      "It refuses when this conversation has no live Host registration or no launcher that can relaunch it.",
      "On success it reports what the exit costs: async jobs, buffered GIGA turns, and every casualty class it cannot see.",
    ].join("\n"),
    parameters: z.object({
      mode: z.enum(["resume", "fresh"])
        .describe("resume relaunches and resumes this conversation; fresh relaunches a new conversation after the House confirms a paper boat."),
      reason: z.string()
        .describe("Why this session must restart, in the room's own words; it is stored on the restart intent."),
    }),
    approval: "write",
    async execute(toolCallId: string, params: any, signal: AbortSignal, _onUpdate: unknown, ctx: any) {
      const mode = text(params?.mode) || "resume";
      const reason = text(params?.reason);
      if (mode !== "resume" && mode !== "fresh") {
        return refuse("invalid_mode", "request_restart refuses: mode is resume or fresh");
      }
      if (!reason) {
        return refuse("reason_required", "request_restart refuses: the Host records a reason with every restart intent");
      }
      let attachment: Attachment;
      try {
        attachment = attachmentFor(ctx);
      } catch (error) {
        return refuse(
          "no_live_registration",
          `request_restart refuses: ${error instanceof Error ? error.message : String(error)}`,
        );
      }
      if (attachment.exiting) {
        return refuse("already_exiting", "request_restart refuses: this session already acknowledged an exit");
      }
      if (!attachment.restart?.supported) {
        return refuse(
          "restart_unsupported",
          "request_restart refuses: no launcher the Host can drive is registered for this session",
          {
            reason: attachment.restart?.reason || attachment.launcherReason || "no launcher",
            terminal: attachment.terminal ? { session: attachment.terminal.session, pane: attachment.terminal.pane } : attachment.terminalReason,
          },
        );
      }
      if (mode === "resume" && !existsSync(attachment.sessionFile)) {
        return refuse(
          "conversation_not_saved",
          "request_restart refuses: OMP has not saved this conversation to disk yet, so a resume relaunch could not find it",
          { sessionFile: attachment.sessionFile },
        );
      }
      // The frozen contract: fresh throws this conversation's context away, so
      // the letter has to be waiting on the other side before the door agrees
      // to leave. Confirming never consumes: paper_boat_wake is a pure read.
      // session.ts checks again before it acknowledges the delivered restart.
      let boat: Record<string, unknown> | undefined;
      if (mode === "fresh") {
        const confirmed = await confirmPaperBoat(ctx, signal, deps.latestBoat);
        if (!confirmed.ok) {
          return refuse(confirmed.code, `request_restart refuses: ${confirmed.error}`, {
            remedy: "write the boat first with the sleep tool, or restart with mode resume to carry this conversation's context",
          });
        }
        boat = confirmed.boat;
      }
      const commandId = restartCommandId(attachment.instanceId, toolCallId);
      let issued: Record<string, unknown>;
      try {
        issued = await issueCommand(ctx, attachment.instanceId, attachment.conversationId, "restart", { reason, mode }, commandId, signal);
      } catch (error) {
        const code = error instanceof PortableSessionError ? error.code : "restart_request_failed";
        return refuse(
          "restart_request_refused",
          `request_restart refuses: the Host would not queue this restart (${code})`,
          { upstreamCode: code, upstreamError: error instanceof Error ? error.message : String(error) },
        );
      }
      return report({
        ok: true,
        queued: true,
        tool: "request_restart",
        commandId,
        restartId: text(issued.restartId) || null,
        deadlineAt: text(issued.deadlineAt) || null,
        launcher: attachment.restart.launcher,
        mode,
        ...(boat ? { boat } : {}),
        appliesAt: "the first idle moment after this turn: the session acknowledges, flushes buffered turns, then exits",
        instanceId: attachment.instanceId,
        conversationId: attachment.conversationId,
        room: attachment.room,
        reason,
        authority: {
          requester: "this exact instance, as registered with the Host; never a tool parameter",
          intent: "recorded and armed by the Host's existing restart intent authority",
        },
        loadedRelease: loadedRelease(deps.release),
        dies: {
          asyncJobs: asyncJobCasualties(ctx),
          gigaTurnBuffers: gigaBufferCasualties(deps.gigaBuffers),
          transports: { count: 0, open: [], unenumerableFamilies: [], unenumerableReason: null },
          hubProcesses: hubProcessCasualties(),
        },
        standsDownIf: "async jobs are still running when the session goes idle (the restart is refused), the restart deadline passes first, or the predecessor never exits (the Host launches nothing)",
      });
    },
  });
}

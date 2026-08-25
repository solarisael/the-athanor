// The adapter's exit door: one tool arms the restart, one agent_end hook fires
// it. This lives beside tools.ts because the concern owns a lifecycle fence (an
// exit may only fall between agents, never inside a tool loop), a handshake exit
// code the keeper reads, and armed state that no tool grab-bag should hold.
// Every substrate call and every casualty source arrives through the deps this
// door is registered with, so the door can be proven without a live House.

import { roomContext } from "./room.ts";
import { hostSessionIdentity } from "./host.ts";
import type { RustJsonlTransport } from "../rust-transport.ts";

// omp exit code 87 means "armed exit, restart me" (keeper handshake, frozen
// wire contract v1). Any other code makes the keeper poll restart_status.
export const ARMED_EXIT_CODE = 87;

// A restart intent exists only because restart_request accepted a consentSource,
// so a pending intent is a consented intent; the door needs no second consent
// field. An intent already past claimed is spoken for and never arms again.
const ARMABLE_STATES = new Set(["requested", "claimed"]);

const MISSING_PREREQUISITE = "restart_status";

// Echoed on refusal so the caller can create the intent instead of guessing.
const CREATE_INTENT_RECIPE = [
  "Create the intent first with restart_request:",
  '{ harness: "omp", workspace, mode: "resume" | "fresh", reason,',
  ' consentSource: "operator-standing-policy" | "operator-approval",',
  " requesterRoom, requesterSpirit, requesterSession, idempotencyKey }",
  "restart_status then reports it as pending and this door can arm.",
].join("\n");

type DomainReceipt = Record<string, unknown>;

type DomainRequest = (
  method: string,
  params: Record<string, unknown>,
  signal?: AbortSignal,
  write?: boolean,
) => Promise<DomainReceipt>;

type LoadedRelease = {
  releaseId?: string | null;
  previousReleaseId?: string | null;
} | null | undefined;

type HubJob = {
  id?: unknown;
  name?: unknown;
  jobId?: unknown;
  persist?: unknown;
  detached?: unknown;
};

export type RestartDoorDeps = {
  // tools.ts's existing requestRustDomain seam.
  requestDomain: DomainRequest;
  // tools.ts's live rustRememberTransports map: each entry is a child process.
  transports: Map<string, RustJsonlTransport>;
  // The house tool registrar, so this tool wears the same feedback renderers.
  registerTool: (definition: Record<string, unknown>) => void;
  // Threaded from configureInstalledAthanor through the adapter entry.
  release?: LoadedRelease;
  hubJobs?: () => HubJob[] | null;
  exit?: (code: number) => void;
};

type ArmedExit = {
  intentId: string;
  mode: string;
  room: string;
  spirit: string;
  session: string;
  workspace: string;
  reason: string;
};

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

function intentId(intent: Record<string, unknown>): string {
  return text(intent.intentId ?? intent.id);
}

// Every map entry is a spawned substrate process holding a JSONL pipe. The exit
// kills them where a graceful shutdown would close them, so they are named.
function transportCasualties(transports: Map<string, RustJsonlTransport>) {
  const open = [...transports.entries()].map(([executable, transport]) => ({
    executable,
    usable: transport?.usable !== false,
  }));
  return { count: open.length, transports: open };
}

function hubJobName(job: HubJob, index: number): string {
  return text(job?.id) || text(job?.jobId) || text(job?.name) || `job-${index}`;
}

function survivesExit(job: HubJob): boolean {
  return job?.persist === true || job?.detached === true;
}

// DEVIATION (harness census 2026-08-25, declared unknown): the running adapter
// has no evidenced door onto hub jobs, and persist/detached semantics are
// undocumented for an omp exit. The door asks the plugin surface it was handed
// and reports the class as unenumerable when nothing answers, so the report can
// never promise a survivor it cannot see.
function probeHubJobs(pi: any): HubJob[] | null {
  const candidates = [pi?.hub?.jobs, pi?.hub?.listJobs, pi?.jobs?.list, pi?.jobs];
  for (const candidate of candidates) {
    const value = typeof candidate === "function" ? candidate.call(pi?.hub ?? pi) : candidate;
    if (Array.isArray(value)) return value as HubJob[];
  }
  return null;
}

function hubJobCasualties(pi: any, provider?: () => HubJob[] | null) {
  const jobs = provider ? provider() : probeHubJobs(pi);
  if (!Array.isArray(jobs)) {
    return {
      enumerable: false,
      dies: [],
      survives: [],
      note: "hub jobs are not enumerable from inside the adapter; every non-persist hub job dies with this exit",
    };
  }
  return {
    enumerable: true,
    dies: jobs.filter((job) => !survivesExit(job)).map(hubJobName),
    survives: jobs.filter(survivesExit).map(hubJobName),
  };
}

function loadedRelease(release: LoadedRelease) {
  return {
    releaseId: release?.releaseId ?? null,
    previousReleaseId: release?.previousReleaseId ?? null,
  };
}

export function registerRestartDoor(pi: any, deps: RestartDoorDeps): void {
  const z = pi.zod;
  const exitProcess = deps.exit ?? ((code: number) => process.exit(code));
  // Armed state is closure-local: one door, one pending exit, no process-wide
  // flag another registration could inherit.
  let armed: ArmedExit | null = null;

  deps.registerTool({
    name: "request_restart",
    label: "Athanor Restart Request",
    description: [
      "Arm this omp session's own exit so the keeper can relaunch it.",
      "The exit never fires inside a turn: it falls at agent_end, after the current agent loop finishes.",
      "This refuses unless restart_status already reports a pending consented restart intent for this workspace.",
      "On arming it reports what dies with the exit: non-persist hub jobs and open Rust substrate transports.",
    ].join("\n"),
    parameters: z.object({
      mode: z.enum(["resume", "fresh"])
        .describe("resume relaunches the same invocation and resumes this session; fresh starts a new session after the House confirms a paper boat."),
      reason: z.string()
        .describe("Why this session must restart, in the room's own words; it is stored on the transition event."),
    }),
    approval: "write",
    async execute(_toolCallId: string, params: any, signal: AbortSignal, _onUpdate: unknown, ctx: any) {
      const workspace = text(ctx?.cwd) || process.cwd();
      const { room, spirit, effectiveRoomDir } = roomContext(ctx?.cwd);
      const session = hostSessionIdentity(ctx, effectiveRoomDir);
      const mode = text(params?.mode);
      const reason = text(params?.reason);

      const status = await deps.requestDomain("restart_status", { workspace }, signal);
      if (status?.ok === false) {
        return refuse(
          "restart_status_unreachable",
          `request_restart refuses: it cannot read ${MISSING_PREREQUISITE} for this workspace`,
          { missingPrerequisite: MISSING_PREREQUISITE, workspace, upstream: status },
        );
      }

      const intent = pendingIntent(status);
      if (!intent) {
        return refuse(
          "no_pending_intent",
          "request_restart refuses: this workspace has no pending consented restart intent",
          { missingPrerequisite: MISSING_PREREQUISITE, workspace, createIntent: CREATE_INTENT_RECIPE },
        );
      }

      const state = text(intent.state);
      if (!ARMABLE_STATES.has(state)) {
        return refuse(
          "intent_not_armable",
          `request_restart refuses: the intent for this workspace is ${state || "in an unreported state"}, not pending`,
          { missingPrerequisite: MISSING_PREREQUISITE, intentId: intentId(intent), state, workspace },
        );
      }

      const intentMode = text(intent.mode);
      if (intentMode && mode && intentMode !== mode) {
        return refuse(
          "mode_mismatch",
          `request_restart refuses: the pending intent is ${intentMode}, not ${mode}`,
          { intentId: intentId(intent), intentMode, requestedMode: mode, workspace },
        );
      }

      armed = {
        intentId: intentId(intent),
        mode: intentMode || mode,
        room,
        spirit,
        session,
        workspace,
        reason,
      };

      return report({
        ok: true,
        armed: true,
        tool: "request_restart",
        firesAt: "agent_end",
        exitCode: ARMED_EXIT_CODE,
        intent: {
          intentId: armed.intentId,
          state,
          mode: armed.mode,
          sessionId: text(intent.sessionId) || null,
          deadlines: intent.deadlines ?? intent.stageDeadlines ?? null,
        },
        workspace,
        room,
        spirit,
        session,
        reason,
        loadedRelease: loadedRelease(deps.release),
        dies: {
          hubJobs: hubJobCasualties(pi, deps.hubJobs),
          rustTransports: transportCasualties(deps.transports),
        },
      });
    },
  });

  // agent_end, never turn_end: turn_end fires once per provider turn while the
  // tool loop is still running (census 2.1-2.2), so an exit there would kill
  // the session between a tool call and its result.
  pi.on("agent_end", async (_event: unknown, ctx: any) => {
    const exit = armed;
    if (!exit) return;
    // One arming fires one exit. Cleared before the transition so a refused
    // handshake cannot re-fire on the next agent_end.
    armed = null;

    // DEVIATION (token fence): the contract fences restart_transition with the
    // keeper's claimToken and this adapter never holds one, so the door names
    // the intent and carries its own session identity inside `detail` — the
    // params struct denies unknown fields, so a new identity param would be
    // refused outright. The substrate may still refuse this unfenced request.
    const receipt = await deps.requestDomain("restart_transition", {
      intentId: exit.intentId,
      to: "exiting",
      detail: `omp adapter armed exit; session=${exit.session}; room=${exit.room}; spirit=${exit.spirit}; mode=${exit.mode}; reason=${exit.reason}`,
    }, undefined, true);

    if (receipt?.ok === false) {
      // A refused transition means the keeper never saw `exiting`, so the
      // session stays alive instead of dying on a substrate hiccup. The intent
      // stays pending and the operator can arm again.
      ctx?.ui?.notify?.("Athanor restart exit stood down: the intent never reached exiting.", "warning");
      return;
    }

    // The exit waits for this handler to return, so every other agent_end tap
    // finishes its own write before the process dies.
    setTimeout(() => exitProcess(ARMED_EXIT_CODE), 0);
  });
}

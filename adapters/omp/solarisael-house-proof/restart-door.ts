// The adapter's exit door: one tool arms the restart, one agent_end hook fires
// it. This lives beside tools.ts because the concern owns a lifecycle fence (an
// exit may only fall between agents, never inside a tool loop), a handshake exit
// code the keeper reads, and armed state that no tool grab-bag should hold.
// Every substrate call and every casualty source arrives through the deps this
// door is registered with, so the door can be proven without a live House.

import path from "node:path";
import { readFileSync } from "node:fs";

import { roomContext } from "./room.ts";
import { hostSessionIdentity } from "./host.ts";
import type { RustJsonlTransport } from "../rust-transport.ts";

// omp exit code 87 means "armed exit, restart me" (keeper handshake, frozen
// wire contract v1). Any other code makes the keeper poll restart_status.
export const ARMED_EXIT_CODE = 87;

// Only a `requested` intent may reach exiting: the substrate refuses a claimed
// one with exit_not_requested (Kodo's amendment, 2026-08-25), so arming on
// claimed would buy a refusal at agent_end instead of a restart.
// These spellings answer to house_protocol::restart, which no TypeScript door
// can import; that crate stays the authority if the two ever disagree.
const ARMABLE_STATES = new Set(["requested"]);

// A pending intent is NOT evidence of consent (Kintsu's item-1 verdict,
// 2026-08-25). restart_status is capability-free and hands the intent id to
// anyone who asks, so the id proves nothing; the earlier door treated it as
// proof and called that a consent fence. The exiting arm is now fenced by the
// room's provisioned restart_exit capability plus the session identity
// restart_request recorded, which the substrate compares against the intent
// row itself. Both travel from places a tool caller cannot reach: a secret in
// the room's runtime config, and the session binding the harness handed us.
const DETAIL_LIMIT_BYTES = 2048;
const DETAIL_SOURCE = "omp-adapter";

const MISSING_PREREQUISITE = "restart_status";

// Room-local, operation-scoped restart_exit capability, resolved exactly the
// way room.ts:114-136 resolves the Docket one: the environment override wins
// for tests and one-room installs, the durable answer is the room-local
// runtime file, and it is read at call time so provisioning a room never needs
// a harness restart. The secret is spent on the wire and never enters a
// schema, a parameter, a receipt, or a log line.
const EXIT_CAPABILITY_ENV = "ATHANOR_RESTART_EXIT_CAPABILITY";
const EXIT_CAPABILITY_FILENAME = "restart-exit-capability";

// Echoed on refusal so the caller can create the intent instead of guessing.
const CREATE_INTENT_RECIPE = [
  "Create the intent first with restart_request:",
  '{ harness: "omp", workspace, mode: "resume" | "fresh", reason,',
  ' consentSource: "operator-standing-policy" | "operator-approval",',
  " requesterRoom, requesterSpirit, requesterSession, capability, idempotencyKey }",
  "The capability is the room's provisioned restart_request secret; consentSource",
  "alone is a declaration, not authority, and is refused as restart_capability.",
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

// One row per session that still holds turns waiting for a GIGA flush.
type GigaBufferCensus = { session?: unknown; cwd?: unknown; turns?: unknown };

export type RestartDoorDeps = {
  // tools.ts's existing requestRustDomain seam.
  requestDomain: DomainRequest;
  // tools.ts's live rustRememberTransports map: each entry is a child process.
  transports: Map<string, RustJsonlTransport>;
  // The house tool registrar, so this tool wears the same feedback renderers.
  registerTool: (definition: Record<string, unknown>) => void;
  // Threaded from configureInstalledAthanor through the adapter entry.
  release?: LoadedRelease;
  // giga.ts's read-only buffered-turn census.
  gigaBuffers?: () => GigaBufferCensus[] | null;
  // Resolves the room's restart_exit secret at spend time. Defaults to the
  // room's own runtime config; injected in tests so proving the fence never
  // requires provisioning a real secret.
  exitCapability?: (effectiveRoomDir: string) => string | null;
  exit?: (code: number) => void;
};

type ArmedExit = {
  intentId: string;
  mode: string;
  room: string;
  spirit: string;
  session: string;
  workspace: string;
  // Captured at arm time from the tool's trusted ctx, so the exit resolves its
  // capability against the room that armed it, not whatever ctx agent_end sees.
  roomDir: string;
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

function exitCapabilityPath(effectiveRoomDir: string): string {
  return path.join(effectiveRoomDir, ".omp", "runtime", EXIT_CAPABILITY_FILENAME);
}

function readExitCapability(effectiveRoomDir: string, environ: NodeJS.ProcessEnv = process.env): string | null {
  const configured = String(environ[EXIT_CAPABILITY_ENV] || "").trim();
  if (configured) return configured;
  try {
    return readFileSync(exitCapabilityPath(effectiveRoomDir), "utf8").trim() || null;
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

function intentId(intent: Record<string, unknown>): string {
  return text(intent.intentId ?? intent.id);
}

// Transport families this door is not handed. Named, because the one map it
// holds is not the whole process: index.ts:1542-1548 closes seven families on a
// graceful shutdown and tools.ts owns only the remember map. The door cannot
// count the rest, so it names them instead of letting one map look like all of
// them (Kintsu item 3, 2026-08-25).
const UNENUMERABLE_TRANSPORT_FAMILIES = Object.freeze([
  "recall",
  "paper-boat",
  "anamnesis",
  "giga",
  "lesson-trigger",
  "lesson-context",
  "entity-resolution",
]);

// Every map entry is a spawned substrate process holding a JSONL pipe. The exit
// kills them where a graceful shutdown would close them, so they are named.
function transportCasualties(transports: Map<string, RustJsonlTransport>) {
  const open = [...transports.entries()].map(([executable, transport]) => ({
    executable,
    usable: transport?.usable !== false,
  }));
  return {
    count: open.length,
    open,
    unenumerableFamilies: [...UNENUMERABLE_TRANSPORT_FAMILIES],
    unenumerableReason:
      "each of those families keeps its transport map module-private; only the remember map is handed to this door",
  };
}

// The harness's own async-job door: ExtensionContext.getAsyncJobSnapshot()
// returns { running, recent, delivery }, and each item is
// Pick<AsyncJob, "id" | "type" | "status" | "label" | "startTime">
// (pi-coding-agent dist/types/extensibility/extensions/types.d.ts:304,
// dist/types/async/job-manager.d.ts:4-10). Only `running` is a casualty, and
// the shape carries no persist or detached field, so nothing here is
// classified as a survivor: an async job is owned by this session and cannot
// outlive it. The earlier report filtered on persist/detached, which belong to
// project-scoped hub processes and never appear on an async job at all.
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
// kind — only the async-job snapshot above — so this door refuses to classify
// them at all rather than promise their fate.
function hubProcessCasualties() {
  return {
    enumerable: false,
    reason:
      "hub processes are project-scoped and the plugin surface exposes no process listing, so this door cannot name them or their fate",
  };
}

// Turns buffered for a later GIGA flush. closeGigaTransports() drains them on a
// graceful shutdown (index.ts:1547) and an armed process.exit never reaches
// that door, so an armed buffer is a real casualty: counted from giga.ts's
// read-only census, never guessed.
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
  };
}

function loadedRelease(release: LoadedRelease) {
  return {
    releaseId: release?.releaseId ?? null,
    previousReleaseId: release?.previousReleaseId ?? null,
  };
}

// Clamp on a character boundary without ever growing past the byte budget: a
// truncated multi-byte tail decodes to U+FFFD, so it is dropped rather than
// shipped as a mangled rune.
function clampUtf8(value: string, limitBytes: number): string {
  if (limitBytes <= 0) return "";
  const bytes = Buffer.from(value, "utf8");
  if (bytes.length <= limitBytes) return value;
  const cut = bytes.subarray(0, limitBytes).toString("utf8");
  return cut.endsWith("\uFFFD") ? cut.slice(0, -1) : cut;
}

function serialize(detail: Record<string, unknown>): string {
  return JSON.stringify(detail);
}

function detailBytes(detail: Record<string, unknown>): number {
  return Buffer.byteLength(serialize(detail), "utf8");
}

// This blob is the transition event's only account of the exit, and the
// substrate refuses an over-budget detail outright - a refused transition means
// the session never restarts. So the named ceiling is enforced on the SERIALIZED
// payload, not on the free-text field alone: the identity the substrate
// verifies is built first, then each account field is added only while it still
// fits, in a declared yielding order (the operator's reason goes first, the
// session identity last). `truncated` is seeded false so flipping it to true
// can never add a byte, which is what lets this function promise the ceiling
// instead of hoping for it. The previous version subtracted the identity from
// the budget and clamped only the reason, so a deep workspace path made the
// budget negative and spun the clamp loop forever.
function exitDetail(exit: ArmedExit): string {
  let detail: Record<string, unknown> = { source: DETAIL_SOURCE, session: exit.session, truncated: false };
  if (detailBytes(detail) > DETAIL_LIMIT_BYTES) {
    const room = DETAIL_LIMIT_BYTES - detailBytes({ source: DETAIL_SOURCE, session: "", truncated: true });
    detail = { source: DETAIL_SOURCE, session: clampUtf8(exit.session, room), truncated: true };
  }
  const account: Array<[string, unknown]> = [
    ["mode", exit.mode],
    ["exitCode", ARMED_EXIT_CODE],
    ["room", exit.room],
    ["spirit", exit.spirit],
    ["workspace", exit.workspace],
    ["reason", exit.reason],
  ];
  for (const [key, value] of account) {
    const candidate = { ...detail, [key]: value };
    if (detailBytes(candidate) <= DETAIL_LIMIT_BYTES) {
      detail = candidate;
      continue;
    }
    detail.truncated = true;
    if (typeof value !== "string") continue;
    const clipped = clampUtf8(value, DETAIL_LIMIT_BYTES - detailBytes({ ...detail, [key]: "" }));
    if (clipped) detail = { ...detail, [key]: clipped };
  }
  return serialize(detail);
}

export function registerRestartDoor(pi: any, deps: RestartDoorDeps): void {
  const z = pi.zod;
  const exitProcess = deps.exit ?? ((code: number) => process.exit(code));
  const resolveCapability = deps.exitCapability ?? ((roomDir: string) => readExitCapability(roomDir));
  // Armed state is closure-local: one door, one pending exit, no process-wide
  // flag another registration could inherit.
  let armed: ArmedExit | null = null;

  deps.registerTool({
    name: "request_restart",
    label: "Athanor Restart Request",
    description: [
      "Arm this omp session's own exit so the keeper can relaunch it.",
      "The exit never fires inside a turn: it falls at agent_end, after the current agent loop finishes.",
      "This refuses unless restart_status already reports a pending restart intent for this workspace",
      "AND this room holds the restart_exit capability that proves the exit to the substrate.",
      "On arming it reports what dies with the exit: this session's async jobs, buffered GIGA turns,",
      "open substrate transports, and every casualty class it cannot see, named as unseen.",
    ].join("\n"),
    parameters: z.object({
      mode: z.enum(["resume", "fresh"])
        .describe("resume relaunches the same invocation and resumes this session; fresh starts a new session after the House confirms a paper boat."),
      reason: z.string()
        .describe("Why this session must restart, in the room's own words; it is stored on the transition event."),
    }),
    approval: "write",
    async execute(_toolCallId: string, params: any, signal: AbortSignal, _onUpdate: unknown, ctx: any) {
      // One door, one pending exit. A second arm while one is live would race
      // two intents onto a single agent_end, so it is refused by name before
      // anything is asked of the substrate.
      if (armed) {
        return refuse(
          "already_armed",
          "request_restart refuses: an exit is already armed for this session and fires at agent_end",
          { armedIntentId: armed.intentId, armedMode: armed.mode, firesAt: "agent_end" },
        );
      }

      const workspace = text(ctx?.cwd) || process.cwd();
      const { room, spirit, effectiveRoomDir } = roomContext(ctx?.cwd);
      // The door's own session binding, from the harness context. The exiting
      // transition is authorized against this and it is never a tool param.
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
          "request_restart refuses: this workspace has no pending restart intent",
          { missingPrerequisite: MISSING_PREREQUISITE, workspace, createIntent: CREATE_INTENT_RECIPE },
        );
      }

      const state = text(intent.state);
      if (!ARMABLE_STATES.has(state)) {
        // Refused here, at the tool, rather than at agent_end: the substrate
        // would answer exit_not_requested long after the turn ended, where the
        // operator can no longer see why nothing happened.
        return refuse(
          "intent_not_armable",
          `request_restart refuses: the intent for this workspace is ${state || "in an unreported state"}, and only a requested intent may exit`,
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

      // Authority before arming, for the same reason as the state fence: an
      // exit armed without the room's capability buys restart_capability at
      // agent_end, long after the operator could see why nothing happened.
      // The secret is only proven present here; it is spent, and read again,
      // when the exit actually fires.
      if (!text(resolveCapability(effectiveRoomDir))) {
        return refuse(
          "restart_capability_unavailable",
          "request_restart refuses: this room holds no restart_exit capability, and a pending intent alone does not authorize an exit",
          {
            intentId: intentId(intent),
            workspace,
            provision: `provision the room's restart_exit capability: set ${EXIT_CAPABILITY_ENV} or write ${exitCapabilityPath(effectiveRoomDir)}`,
          },
        );
      }

      armed = {
        intentId: intentId(intent),
        mode: intentMode || mode,
        room,
        spirit,
        session,
        workspace,
        roomDir: effectiveRoomDir,
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
        // What actually proves the exit, said plainly so no reader mistakes the
        // pending row for consent again.
        authority: {
          requesterSession: session,
          capability: "the room's restart_exit capability, read again when the exit fires and never shown",
          intentIdProves: "nothing: restart_status is capability-free, so the intent id is not evidence of consent",
        },
        loadedRelease: loadedRelease(deps.release),
        dies: {
          asyncJobs: asyncJobCasualties(ctx),
          gigaTurnBuffers: gigaBufferCasualties(deps.gigaBuffers),
          transports: transportCasualties(deps.transports),
          hubProcesses: hubProcessCasualties(),
        },
        // An unclaimed intent retires 300s after restart_request, and an agent
        // loop can outlive that. The exit then stands down instead of firing,
        // so the arming report says it rather than letting the silence explain.
        standsDownIf: "the intent expires before this agent loop ends (intent_expired), the room's capability is revoked, or the substrate refuses the transition",
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

    // The secret is spent, never stored: it is read again here against the room
    // that armed the exit, so a capability revoked mid-turn stands the exit
    // down instead of firing it.
    const capability = text(resolveCapability(exit.roomDir));
    if (!capability) {
      ctx?.ui?.notify?.(
        "Athanor restart exit stood down (restart_capability_unavailable): the room's restart_exit capability is gone, so the intent never reached exiting.",
        "warning",
      );
      return;
    }

    // No claimToken: exiting stays tokenless in the keeper-lease sense, and the
    // substrate refuses this arm outright if a token is present. Authority is
    // the room's capability plus this session's own identity, which the
    // substrate compares against the requester the intent recorded.
    const receipt = await deps.requestDomain("restart_transition", {
      intentId: exit.intentId,
      to: "exiting",
      requesterSession: exit.session,
      capability,
      detail: exitDetail(exit),
    }, undefined, true);

    if (receipt?.ok === false) {
      // A refused transition means the keeper never saw `exiting`, so the
      // session stays alive instead of dying on a substrate hiccup. The intent
      // stays pending and the operator can arm again. The substrate's own code
      // travels into the notice: restart_capability, exit_not_authorized,
      // exit_not_requested, restart_storm, intent_expired, unknown_intent and
      // invalid_params each mean a different repair.
      const code = text(receipt.code) || "refused";
      ctx?.ui?.notify?.(`Athanor restart exit stood down (${code}): the intent never reached exiting.`, "warning");
      return;
    }

    // The exit waits for this handler to return, so every other agent_end tap
    // finishes its own write before the process dies.
    setTimeout(() => exitProcess(ARMED_EXIT_CODE), 0);
  });
}

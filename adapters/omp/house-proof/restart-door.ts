// The adapter's exit door: one tool arms the restart, one agent_end hook fires
// it. This lives beside tools.ts because the concern owns a lifecycle fence (an
// exit may only fall between agents, never inside a tool loop), a handshake exit
// code the keeper reads, and armed state that no tool grab-bag should hold.
// Every substrate call and every casualty source arrives through the deps this
// door is registered with, so the door can be proven without a live House.

import path from "node:path";
import { randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";

import { roomContext } from "./room.ts";
import { hostSessionIdentity } from "./host.ts";
import { topLevelSession } from "./top-level-session-fence.ts";
import { catchBoat } from "./substrate.ts";

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

// Room-local, operation-scoped restart capabilities, resolved exactly the way
// room.ts:114-136 resolves the Docket one: the environment override wins for
// tests and one-room installs, the durable answer is the room-local runtime
// file, and it is read at call time so provisioning a room never needs a
// harness restart. Two classes, two secrets, because they are two different
// rights: restart_request is the right to have the House record an intent,
// restart_exit is the right to prove one at the door. A secret is spent on the
// wire and never enters a schema, a parameter, a receipt, or a log line.
const EXIT_CAPABILITY_ENV = "ATHANOR_RESTART_EXIT_CAPABILITY";
const EXIT_CAPABILITY_FILENAME = "restart-exit-capability";
const REQUEST_CAPABILITY_ENV = "ATHANOR_RESTART_REQUEST_CAPABILITY";
const REQUEST_CAPABILITY_FILENAME = "restart-request-capability";
const VERIFY_CAPABILITY_ENV = "ATHANOR_RESTART_VERIFY_CAPABILITY";
const VERIFY_CAPABILITY_FILENAME = "restart-verify-capability";
const RESTART_INTENT_ENV = "ATHANOR_RESTART_INTENT_ID";
const RESTART_SUCCESSOR_PROOF_ENV = "ATHANOR_RESTART_SUCCESSOR_PROOF";

// The House records an intent because the room is provisioned to ask for one,
// and the operator's standing policy is what that provisioning means. The door
// never lets a caller declare a stronger consent than the room was given.
const DOOR_CONSENT_SOURCE = "operator-standing-policy";

// Echoed when this room cannot ask for its own intent, so an operator can
// provision the room instead of guessing at the wire.
const CREATE_INTENT_RECIPE = [
  "This room holds no restart_request capability, so it cannot record its own intent.",
  `Provision one (${REQUEST_CAPABILITY_ENV}, or the room's runtime ${REQUEST_CAPABILITY_FILENAME} file)`,
  "and this tool will call restart_request itself:",
  '{ harness: "omp", workspace, mode: "resume" | "fresh", reason,',
  ` consentSource: "${DOOR_CONSENT_SOURCE}",`,
  " requesterRoom, requesterSpirit, requesterSession, sessionId, capability, idempotencyKey }",
  "consentSource alone is a declaration, not authority: without the capability the",
  "substrate refuses the call as restart_capability.",
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
  // Resolve the room's two restart secrets at spend time. Both default to the
  // room's own runtime config; injected in tests so proving a fence never
  // requires provisioning a real secret.
  exitCapability?: (effectiveRoomDir: string) => string | null;
  // Reads the room's latest paper boat. Defaults to the adapter's own wake
  // seam; a read, never a consume.
  latestBoat?: (room: string, options?: { signal?: AbortSignal }) => Promise<Record<string, unknown>>;
  requestCapability?: (effectiveRoomDir: string) => string | null;
  verifyCapability?: (effectiveRoomDir: string) => string | null;
  restartIntentId?: () => string | null;
  restartSuccessorProof?: () => string | null;
  isEmbodied?: (room: string, session: string) => boolean;
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

function capabilityPath(effectiveRoomDir: string, filename: string): string {
  return path.join(effectiveRoomDir, ".omp", "runtime", filename);
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
    return readFileSync(capabilityPath(effectiveRoomDir, filename), "utf8").trim() || null;
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

// A tail cut on UTF-16 code units can split a surrogate pair. JSON.stringify
// serializes the orphan as \udXXX, so it survives the wire and lands in the
// event log as a broken rune; drop it instead.
function dropOrphanSurrogate(value: string): string {
  const last = value.charCodeAt(value.length - 1);
  return last >= 0xd800 && last <= 0xdbff ? value.slice(0, -1) : value;
}

// Shrink one field until the SERIALIZED candidate fits. Measuring the raw value
// against a budget taken from an empty field is the bug Kintsu reproduced: JSON
// escaping expands as it serializes - one quote or backslash becomes two bytes,
// one control character becomes six - so a raw-byte budget lied by up to 6x
// (4,000 quotes "clamped" to 2,048 serialized as 3,883 bytes; 4,000 control
// characters as 11,223). Only the serialized candidate is what the substrate
// refuses, so only the serialized candidate is measured here.
//
// Termination is load-bearing: each pass keeps at most candidate.length - 1
// characters, so the string strictly shrinks to "" and the loop ends even when
// no length can ever fit. The first repair of this function spun forever on a
// negative budget, and a synchronous spin here cannot be interrupted by a test
// timeout.
function fitField(
  base: Record<string, unknown>,
  key: string,
  value: string,
  limitBytes: number,
): Record<string, unknown> | null {
  let candidate = value;
  while (candidate.length > 0) {
    const trial = { ...base, [key]: candidate };
    const bytes = detailBytes(trial);
    if (bytes <= limitBytes) return trial;
    // Scale by the observed byte ratio so an escape-heavy field converges in a
    // few passes instead of one character at a time, and always drop at least
    // one character so the loop cannot stall.
    const scaled = Math.floor((candidate.length * limitBytes) / bytes);
    const keep = Math.max(0, Math.min(candidate.length - 1, scaled));
    candidate = dropOrphanSurrogate(candidate.slice(0, keep));
  }
  return null;
}

function serialize(detail: Record<string, unknown>): string {
  return JSON.stringify(detail);
}

function detailBytes(detail: Record<string, unknown>): number {
  return Buffer.byteLength(serialize(detail), "utf8");
}

// This blob is the transition event's only account of the exit, and the
// substrate refuses an over-budget detail outright - a refused transition means
// the session never restarts. The identity the substrate verifies is built
// first, then each account field is added only while the serialized whole still
// fits, in a declared yielding order: the operator's reason yields first, the
// session identity last. `truncated` is seeded false so flipping it to true can
// never add a byte.
//
// The comment that stood here claimed the ceiling was "enforced on the
// SERIALIZED payload" while the clamp underneath measured raw UTF-8 - true for
// every input the tests fed it, false for anything JSON escapes. Kintsu found
// it with 4,000 quotes. The claim now matches the code because every measure
// below runs through detailBytes on a real candidate.
export function exitDetailFor(exit: ArmedExit, limitBytes: number = DETAIL_LIMIT_BYTES): string {
  const seed: Record<string, unknown> = { source: DETAIL_SOURCE, session: exit.session, truncated: false };
  let detail: Record<string, unknown>;
  if (detailBytes(seed) <= limitBytes) {
    detail = seed;
  } else {
    // Even the identity overflows: keep the field the substrate verifies, cut
    // to fit, and say so. If nothing fits, the smallest honest blob goes out
    // and the substrate refuses it loudly instead of this door hanging.
    detail = fitField({ source: DETAIL_SOURCE, truncated: true }, "session", exit.session, limitBytes)
      ?? { source: DETAIL_SOURCE, truncated: true };
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
    if (detailBytes(candidate) <= limitBytes) {
      detail = candidate;
      continue;
    }
    detail = { ...detail, truncated: true };
    if (typeof value !== "string") continue;
    detail = fitField(detail, key, value, limitBytes) ?? detail;
  }
  return serialize(detail);
}

function exitDetail(exit: ArmedExit): string {
  return exitDetailFor(exit, DETAIL_LIMIT_BYTES);
}

export function registerRestartDoor(pi: any, deps: RestartDoorDeps): void {
  const z = pi.zod;
  const exitProcess = deps.exit ?? ((code: number) => process.exit(code));
  const resolveCapability = deps.exitCapability
    ?? ((roomDir: string) => readCapability(roomDir, EXIT_CAPABILITY_ENV, EXIT_CAPABILITY_FILENAME));
  const resolveRequestCapability = deps.requestCapability
    ?? ((roomDir: string) => readCapability(roomDir, REQUEST_CAPABILITY_ENV, REQUEST_CAPABILITY_FILENAME));
  const resolveLatestBoat = deps.latestBoat ?? catchBoat;
  const resolveVerifyCapability = deps.verifyCapability
    ?? ((roomDir: string) => readCapability(roomDir, VERIFY_CAPABILITY_ENV, VERIFY_CAPABILITY_FILENAME));
  const resolveRestartIntent = deps.restartIntentId
    ?? (() => text(process.env[RESTART_INTENT_ENV]));
  const resolveRestartSuccessorProof = deps.restartSuccessorProof
    ?? (() => text(process.env[RESTART_SUCCESSOR_PROOF_ENV]));
  const isEmbodied = deps.isEmbodied
    ?? ((room: string, session: string) => topLevelSession(room) === session);
  // Armed state is closure-local: one door, one pending exit, no process-wide
  // flag another registration could inherit.
  let armed: ArmedExit | null = null;
  let verifiedIntent = "";
  let verifyingIntent = "";

  // The keeper gives only a relaunched OMP the intent id. Verify on startup,
  // after the earlier session_start hook has established the embodied session.
  const verifySuccessor = async (_event: unknown, ctx: any) => {
    const intentId = text(resolveRestartIntent());
    if (!intentId || verifiedIntent === intentId || verifyingIntent === intentId) return;
    const { room, spirit, effectiveRoomDir } = roomContext(ctx?.cwd);
    const session = hostSessionIdentity(ctx, effectiveRoomDir);
    if (ctx?.mode !== "tui" || !isEmbodied(room, session)) return;
    const successorProof = text(resolveRestartSuccessorProof());
    if (!successorProof) {
      ctx?.ui?.notify?.(
        "Athanor restart successor could not verify: the keeper supplied no successor proof.",
        "warning",
      );
      return;
    }
    const capability = text(resolveVerifyCapability(effectiveRoomDir));
    if (!capability) {
      ctx?.ui?.notify?.(
        "Athanor restart successor could not verify: this room holds no restart_verify capability.",
        "warning",
      );
      return;
    }
    verifyingIntent = intentId;
    try {
      const receipt = await deps.requestDomain("restart_verify", {
        intentId,
        successorProof,
        successorSession: session,
        room,
        spirit,
        capability,
      }, undefined, true);
      if (receipt?.ok === false || text(receipt?.state) !== "verified") {
        const code = text(receipt?.code) || "not_verified";
        ctx?.ui?.notify?.(`Athanor restart successor verification failed (${code}).`, "warning");
        return;
      }
      verifiedIntent = intentId;
      delete process.env[RESTART_INTENT_ENV];
      delete process.env[RESTART_SUCCESSOR_PROOF_ENV];
      ctx?.ui?.notify?.("Athanor restart successor verified.", "info");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      ctx?.ui?.notify?.(`Athanor restart successor verification failed: ${message}`, "warning");
    } finally {
      verifyingIntent = "";
    }
  };
  pi.on("session_start", verifySuccessor);
  pi.on("session_switch", verifySuccessor);

  deps.registerTool({
    name: "request_restart",
    label: "Athanor Restart Request",
    description: [
      "Arm this omp session's own exit so the keeper can relaunch it.",
      "The exit never fires inside a turn: it falls at agent_end, after the current agent loop finishes.",
      "It records the intent itself when the House holds none and this room is provisioned to ask,",
      "then arms that intent in the same call. It refuses unless this room holds the restart_exit",
      "capability that proves the exit to the substrate; an unprovisioned room cannot self-restart.",
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

      // Authority before anything is recorded OR armed. An exit armed without
      // the room's capability buys restart_capability at agent_end, long after
      // the operator could see why nothing happened - and an intent recorded
      // for a room that cannot prove it would sit pending until it expired,
      // spending the storm guard on a restart that could never fire. The
      // secret is only proven present here; it is spent, and read again, when
      // the exit actually fires.
      if (!text(resolveCapability(effectiveRoomDir))) {
        return refuse(
          "restart_capability_unavailable",
          "request_restart refuses: this room holds no restart_exit capability, and a pending intent alone does not authorize an exit",
          {
            workspace,
            provision: `provision the room's restart_exit capability: set ${EXIT_CAPABILITY_ENV} or write ${capabilityPath(effectiveRoomDir, EXIT_CAPABILITY_FILENAME)}`,
          },
        );
      }

      // One state fence, whichever intent we end up arming: a pre-existing one
      // is checked here, a freshly recorded one right after it is recorded.
      // Refused at the tool rather than at agent_end, because the substrate
      // would answer exit_not_requested long after the turn ended, where the
      // operator can no longer see why nothing happened.
      const notArmable = (candidate: Record<string, unknown>) => {
        const candidateState = text(candidate.state);
        if (ARMABLE_STATES.has(candidateState)) return null;
        return refuse(
          "intent_not_armable",
          `request_restart refuses: the intent for this workspace is ${candidateState || "in an unreported state"}, and only a requested intent may exit`,
          { missingPrerequisite: MISSING_PREREQUISITE, intentId: intentId(candidate), state: candidateState, workspace },
        );
      };

      let intent = pendingIntent(status);
      let created = false;
      if (intent) {
        const refusal = notArmable(intent);
        if (refusal) return refusal;
      }

      // The mode the House already agreed to wins over the word the caller
      // typed, and a disagreement is a refusal rather than a silent override.
      const pendingMode = intent ? text(intent.mode) : "";
      if (pendingMode && mode && pendingMode !== mode) {
        return refuse(
          "mode_mismatch",
          `request_restart refuses: the pending intent is ${pendingMode}, not ${mode}`,
          { intentId: intentId(intent!), intentMode: pendingMode, requestedMode: mode, workspace },
        );
      }
      const effectiveMode = pendingMode || mode;

      // The frozen contract: fresh mode is the same launch "after the House
      // confirms a paper boat exists for the session's room". Fresh throws this
      // session's context away, so the letter has to be waiting on the other
      // side before the door agrees to leave - otherwise the restart is just
      // amnesia. Confirming never consumes: paper_boat_wake is a pure read, and
      // crates/house-substrate/tests/paper_boat_integration.rs wakes one room
      // twice and gets the boat both times.
      let boat: Record<string, unknown> | undefined;
      if (effectiveMode === "fresh") {
        const wake = await resolveLatestBoat(room, { signal });
        if (wake?.ok === false) {
          return refuse(
            "fresh_boat_unconfirmed",
            "request_restart refuses: a fresh restart needs a confirmed paper boat, and the House could not be asked for one",
            { workspace, room, upstreamError: text(wake?.error) },
          );
        }
        if (wake?.found !== true) {
          return refuse(
            "fresh_without_boat",
            "request_restart refuses: a fresh restart abandons this session's context and this room has no paper boat waiting",
            {
              workspace,
              room,
              remedy: "write the boat first with the sleep tool, or restart with mode resume to carry this session's context",
            },
          );
        }
        boat = {
          confirmed: true,
          id: text(wake.id) || null,
          title: text(wake.title) || null,
          createdAt: text(wake.createdAt) || null,
        };
      }

      if (!intent) {
        // Nothing else in the House can open one: the substrate only records an
        // intent, the keeper only claims one. So the room asking to restart asks
        // for its own, and only when the operator provisioned it to ask - which
        // is what makes "operator-standing-policy" true here rather than a
        // caller's word. This completes the sentence the quest opened with:
        // the agent requests, the House records, the adapter arms.
        const requestCapability = text(resolveRequestCapability(effectiveRoomDir));
        if (!requestCapability) {
          return refuse(
            "no_pending_intent",
            "request_restart refuses: this workspace has no pending restart intent and this room may not open one",
            { missingPrerequisite: MISSING_PREREQUISITE, workspace, createIntent: CREATE_INTENT_RECIPE },
          );
        }

        // A fresh key per genuine creation: the door only reaches here because
        // restart_status showed nothing pending, so a retry that already
        // succeeded finds the intent and arms it instead of forking a second
        // one, and the substrate's storm guard fences the racing edge.
        const receipt = await deps.requestDomain("restart_request", {
          harness: "omp",
          workspace,
          mode,
          sessionId: session,
          reason,
          consentSource: DOOR_CONSENT_SOURCE,
          requesterRoom: room,
          requesterSpirit: spirit,
          requesterSession: session,
          capability: requestCapability,
          idempotencyKey: randomUUID(),
        }, signal, true);

        if (receipt?.ok === false) {
          // The substrate's own name travels, minus the secret it was given:
          // restart_capability, restart_storm and invalid_params each mean a
          // different repair.
          const upstreamCode = text(receipt.code) || "refused";
          return refuse(
            "restart_request_refused",
            `request_restart refuses: the House would not record this intent (${upstreamCode})`,
            { workspace, upstreamCode, upstreamError: text(receipt.error) },
          );
        }

        intent = pendingIntent(receipt);
        if (!intent) {
          return refuse(
            "restart_request_unusable",
            "request_restart refuses: the House accepted the request but named no intent id, so there is nothing to arm",
            { workspace, upstreamState: text(receipt?.state) },
          );
        }
        created = true;
        const refusal = notArmable(intent);
        if (refusal) return refusal;
      }

      const state = text(intent.state);
      armed = {
        intentId: intentId(intent),
        mode: effectiveMode,
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
        // True when this call opened the intent itself, false when it armed one
        // the House already held: the operator can tell a self-request from a
        // pre-recorded one without reading the event log.
        created,
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
        // Present only for fresh, because only fresh has to prove it: this is
        // the letter the next session wakes into.
        ...(boat ? { boat } : {}),
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
      //
      // enough: an intent requested by another session is armed and only learns
      // exit_not_authorized here, at agent_end; a status-side ownSession flag is
      // the way up if operators ever hit it. Deliberate - pulling the check to
      // tool time would put a live session id on restart_status, which is
      // capability-free and callable by the whole machine.
      const code = text(receipt.code) || "refused";
      ctx?.ui?.notify?.(`Athanor restart exit stood down (${code}): the intent never reached exiting.`, "warning");
      return;
    }

    // The exit waits for this handler to return, so every other agent_end tap
    // finishes its own write before the process dies.
    setTimeout(() => exitProcess(ARMED_EXIT_CODE), 0);
  });
}

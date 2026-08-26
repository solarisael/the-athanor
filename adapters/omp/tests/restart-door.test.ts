import { afterEach, describe, expect, test } from "bun:test";

import { exitDetailFor, registerRestartDoor } from "../solarisael-house-proof/restart-door.ts";

// 87 is the keeper handshake code in the frozen wire contract, so it is pinned
// here as a literal. Asserting the module's own constant would defend nothing.
const ARMED_EXIT_CODE = 87;
// Same reason: the substrate's ceiling is the contract's number, not the
// module's variable. A test that read DETAIL_LIMIT_BYTES would follow the bug.
const DETAIL_CEILING_BYTES = 2048;

// The pi surface this door touches, built the way adapter-registration.test.ts
// builds its fake: a zod stand-in that only records shape, plus hook and tool
// capture. Nothing here reaches a substrate, a House, or a real process.
type Schema = {
  kind: string;
  values?: string[];
  shape?: Record<string, Schema>;
  describe(description: string): Schema;
};

function makeSchema(kind: string, fields: Partial<Schema> = {}): Schema {
  return {
    kind,
    ...fields,
    describe(_description: string) {
      return this;
    },
  } as Schema;
}

const zodStub = {
  string: () => makeSchema("string"),
  enum: (values: string[]) => makeSchema("enum", { values }),
  object: (shape: Record<string, Schema>) => makeSchema("object", { shape }),
};

type CapturedTool = {
  name: string;
  approval?: string;
  parameters: Schema;
  execute: (
    toolCallId: string,
    params: unknown,
    signal?: unknown,
    onUpdate?: unknown,
    ctx?: unknown,
  ) => Promise<{ isError?: boolean; details: any }>;
};

type DomainCall = { method: string; params: Record<string, unknown>; write?: boolean };

type DoorHarness = {
  tool: CapturedTool;
  hooks: Array<{ name: string; handler: (event?: unknown, ctx?: unknown) => Promise<unknown> }>;
  calls: DomainCall[];
  exits: number[];
  notices: string[];
  agentEnd: () => Promise<unknown>;
  hookNames: () => string[];
  sessionStart: (ctx?: unknown) => Promise<unknown>;
};

const FAKE_EXECUTABLE = "C:/fake/versions/0.10.1/bin/athanor-substrate.exe";
// The room's provisioned restart_exit secret. It never appears in a tool
// parameter and must never appear in a receipt.
const EXIT_CAPABILITY = "restart-exit-secret-0e6c";
const CAPABILITY_ENV = "ATHANOR_RESTART_EXIT_CAPABILITY";
const REQUEST_CAPABILITY = "restart-request-secret-4711";
const REQUEST_CAPABILITY_ENV = "ATHANOR_RESTART_REQUEST_CAPABILITY";
const VERIFY_CAPABILITY = "restart-verify-secret-159";
const SUCCESSOR_PROOF = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
const SUCCESSOR_PROOF_ENV = "ATHANOR_RESTART_SUCCESSOR_PROOF";
const SUCCESSOR_INTENT = "06f33aab-fdca-489b-8dd9-1020e2efc384";

// The real harness shape: AsyncJobSnapshotItem is Pick<AsyncJob, "id" | "type" |
// "status" | "label" | "startTime"> and AsyncJob carries no persist or detached
// field (pi-coding-agent dist/types/async/job-manager.d.ts:4-10,
// session/agent-session-types.d.ts:44-50). A fake that invents persist here
// would prove the door against a surface omp never hands it.
const RUNNING_JOB = { id: "bash_a1b2c3", type: "bash", status: "running", label: "bun test --isolate", startTime: 1 };
const FINISHED_JOB = { id: "task_d4e5f6", type: "task", status: "completed", label: "faro census", startTime: 2 };

const PENDING_INTENT = {
  ok: true,
  intent: {
    intentId: "intent-4711",
    state: "requested",
    mode: "resume",
    sessionId: "session-under-restart",
    deadlines: { exiting: 60, relaunching: 120 },
  },
};

function buildDoor(options: {
  status?: Record<string, unknown>;
  request?: Record<string, unknown>;
  transition?: Record<string, unknown>;
  release?: { releaseId?: string | null; previousReleaseId?: string | null } | null;
  verify?: Record<string, unknown>;
  transports?: Map<string, { usable: boolean }>;
  gigaBuffers?: () => Array<{ session: string; cwd: string; turns: number }> | null;
  // Omitted entirely (not set to a stub) when a test wants the module's own
  // env/file resolver to answer.
  exitCapability?: (roomDir: string) => string | null;
  requestCapability?: (roomDir: string) => string | null;
  latestBoat?: (room: string, options?: unknown) => Promise<Record<string, unknown>>;
  verifyCapability?: (roomDir: string) => string | null;
  restartIntentId?: () => string | null;
  restartSuccessorProof?: () => string | null;
  isEmbodied?: (room: string, session: string) => boolean;
} = {}): DoorHarness {
  const hooks: DoorHarness["hooks"] = [];
  const tools: CapturedTool[] = [];
  const calls: DomainCall[] = [];
  const exits: number[] = [];
  const notices: string[] = [];

  const pi = {
    zod: zodStub,
    on(name: string, handler: DoorHarness["hooks"][number]["handler"]) {
      hooks.push({ name, handler });
    },
  };

  registerRestartDoor(pi, {
    async requestDomain(method, params, _signal, write) {
      calls.push({ method, params, write });
      if (method === "restart_status") return options.status ?? PENDING_INTENT;
      if (method === "restart_request") {
        return options.request ?? { ok: true, intentId: "intent-fresh-91", state: "requested", expiresAt: "2026-08-25T18:00:00Z" };
      }
      if (method === "restart_transition") return options.transition ?? { ok: true, state: "exiting" };
      if (method === "restart_verify") return options.verify ?? { ok: true, state: "verified" };
      return { ok: false, error: `unexpected method ${method}` };
    },
    transports: (options.transports ?? new Map([[FAKE_EXECUTABLE, { usable: true }]])) as any,
    registerTool(definition) {
      tools.push(definition as unknown as CapturedTool);
    },
    release: "release" in options ? options.release : { releaseId: "0.9.3-abc", previousReleaseId: "0.9.2-def" },
    ...("gigaBuffers" in options ? { gigaBuffers: options.gigaBuffers } : {}),
    ...("exitCapability" in options ? { exitCapability: options.exitCapability } : {}),
    ...("requestCapability" in options ? { requestCapability: options.requestCapability } : {}),
    ...("latestBoat" in options ? { latestBoat: options.latestBoat } : {}),
    ...("verifyCapability" in options ? { verifyCapability: options.verifyCapability } : {}),
    ...("restartIntentId" in options ? { restartIntentId: options.restartIntentId } : {}),
    ...("restartSuccessorProof" in options ? { restartSuccessorProof: options.restartSuccessorProof } : {}),
    ...("isEmbodied" in options ? { isEmbodied: options.isEmbodied } : {}),
    exit(code) {
      exits.push(code);
    },
  });

  const agentEnd = hooks.find((hook) => hook.name === "agent_end");
  if (!agentEnd) throw new Error("the door registered no agent_end hook");
  const sessionStart = hooks.find((hook) => hook.name === "session_start");
  if (!sessionStart) throw new Error("the door registered no session_start hook");
  if (tools.length !== 1) throw new Error(`the door registered ${tools.length} tools`);

  return {
    tool: tools[0]!,
    hooks,
    calls,
    exits,
    notices,
    hookNames: () => hooks.map((hook) => hook.name),
    agentEnd: () => agentEnd.handler({ messages: [] }, { ui: { notify: (text: string) => notices.push(text) } }),
    sessionStart: (ctx: any = toolContext()) => sessionStart.handler(
      {},
      { ...ctx, ui: { notify: (text: string) => notices.push(text) } },
    ),
  };
}

// The tool's own ctx: the harness hands the session binding and the async-job
// door here, and this is the only place either may come from.
function toolContext(options: { cwd?: string; jobs?: unknown; withJobDoor?: boolean } = {}) {
  const ctx: Record<string, unknown> = {
    cwd: options.cwd ?? process.cwd(),
    sessionId: "session-under-restart",
    mode: "tui",
  };
  if (options.withJobDoor !== false) {
    ctx.getAsyncJobSnapshot = () => options.jobs ?? null;
  }
  return ctx;
}

function callTool(
  door: DoorHarness,
  params: unknown = { mode: "resume", reason: "the loaded release is stale" },
  ctx: unknown = toolContext(),
) {
  return door.tool.execute("call-1", params, undefined, undefined, ctx);
}

// The exit is scheduled for after the agent_end handler settles, so a test that
// wants to see it must let one timer turn pass.
function settle() {
  return new Promise((resolve) => setTimeout(resolve, 5));
}

function withCapability(options: Parameters<typeof buildDoor>[0] = {}) {
  return buildDoor({ exitCapability: () => EXIT_CAPABILITY, ...options });
}

afterEach(() => {
  delete process.env[CAPABILITY_ENV];
  delete process.env[SUCCESSOR_PROOF_ENV];
  delete process.env[REQUEST_CAPABILITY_ENV];
});

describe("adapter exit door", () => {
  test("registers one write tool and arms on agent_end only", () => {
    const door = withCapability();

    expect(door.tool.name).toBe("request_restart");
    expect(door.tool.approval).toBe("write");
    // mode and reason are the ONLY caller-visible parameters: neither the
    // session identity nor the capability may be nameable by a caller.
    expect(Object.keys(door.tool.parameters.shape ?? {})).toEqual(["mode", "reason"]);
    expect(door.tool.parameters.shape?.mode?.values).toEqual(["resume", "fresh"]);
    // The fence itself: this door owns no turn_end tap, because turn_end fires
    // mid-tool-loop and an exit there would kill a turn in progress.
    expect(door.hookNames()).toEqual(["session_start", "session_switch", "agent_end"]);
  });

  test("a keeper-launched embodied successor verifies its exact intent and proof on session start", async () => {
    const door = buildDoor({
      verifyCapability: () => VERIFY_CAPABILITY,
      restartIntentId: () => SUCCESSOR_INTENT,
      restartSuccessorProof: () => SUCCESSOR_PROOF,
      isEmbodied: () => true,
    });
    await door.sessionStart();
    await door.sessionStart();
    const verifies = door.calls.filter((call) => call.method === "restart_verify");
    expect(verifies).toHaveLength(1);
    expect(verifies[0]).toMatchObject({
      write: true,
      params: {
        intentId: SUCCESSOR_INTENT,
        successorSession: "session-under-restart",
        successorProof: SUCCESSOR_PROOF,
        capability: VERIFY_CAPABILITY,
      },
    });
    expect(door.notices.join(" ")).toContain("successor verified");
  });
  test("refuses loudly when the keeper gives an intent without its successor proof", async () => {
    const door = buildDoor({
      verifyCapability: () => VERIFY_CAPABILITY,
      restartIntentId: () => SUCCESSOR_INTENT,
      restartSuccessorProof: () => null,
      isEmbodied: () => true,
    });
    await door.sessionStart();
    expect(door.calls.filter((call) => call.method === "restart_verify")).toHaveLength(0);
    expect(door.notices.join(" ")).toContain("successor proof");
  });

  test("a non-TUI worker session cannot verify the keeper intent", async () => {
    const door = buildDoor({
      verifyCapability: () => VERIFY_CAPABILITY,
      restartIntentId: () => SUCCESSOR_INTENT,
      isEmbodied: () => true,
    });
    await door.sessionStart({ ...toolContext(), mode: "print" });
    expect(door.calls).toEqual([]);
  });

  // An unprovisioned room simply cannot self-restart: it can neither record an
  // intent nor prove one, so this refusal is now honestly reachable instead of
  // echoing a recipe no surface ever called.
  test("refuses by name when there is no pending intent and no request capability", async () => {
    const door = withCapability({ status: { ok: true }, requestCapability: () => null });

    const result = await callTool(door);

    expect(result.isError).toBe(true);
    expect(result.details.ok).toBe(false);
    expect(result.details.code).toBe("no_pending_intent");
    expect(result.details.error).toContain("request_restart refuses");
    expect(result.details.missingPrerequisite).toBe("restart_status");
    expect(result.details.createIntent).toContain("restart_request");
    expect(result.details.createIntent).toContain("consentSource");
    // The recipe has to name the capability too, or it teaches a call the
    // substrate now refuses with restart_capability.
    expect(result.details.createIntent).toContain("capability");
    // Nothing was recorded: no intent may exist that this room cannot exit.
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);
  });

  // ---------------------------------------------------------------------------
  // Completing the circuit (Kodo's ruling, 2026-08-25): "agent requests, House
  // records, adapter arms". Nothing else in the House can create an intent, so
  // the tool creates its own when the room is provisioned to ask.
  // ---------------------------------------------------------------------------

  // This one asks for fresh, so it now has to show a boat too: the fence below
  // caught this very test creating a fresh intent with no letter waiting.
  test("creates the intent through restart_request when the room may ask, then arms it", async () => {
    const door = withCapability({
      status: { ok: true },
      requestCapability: () => REQUEST_CAPABILITY,
      latestBoat: async (room: string) => ({ ok: true, found: true, room, id: "boat-12" }),
    });

    const result = await callTool(door, { mode: "fresh", reason: "the loaded release is stale" });

    expect(door.calls.map((call) => call.method)).toEqual(["restart_status", "restart_request"]);
    const request = door.calls[1]!;
    expect(request.write).toBe(true);
    expect(request.params).toMatchObject({
      harness: "omp",
      mode: "fresh",
      reason: "the loaded release is stale",
      consentSource: "operator-standing-policy",
      requesterRoom: expect.any(String),
      requesterSpirit: expect.any(String),
      requesterSession: "session-under-restart",
      sessionId: "session-under-restart",
      capability: REQUEST_CAPABILITY,
    });
    expect(String(request.params.workspace ?? "").length).toBeGreaterThan(0);
    // A retry must be able to land on the same intent instead of a second one.
    expect(String(request.params.idempotencyKey ?? "").length).toBeGreaterThan(0);

    // The fresh intent is armed in the same call: request, record, arm.
    expect(result.isError).toBeUndefined();
    expect(result.details.armed).toBe(true);
    expect(result.details.created).toBe(true);
    expect(result.details.intent.intentId).toBe("intent-fresh-91");
    expect(result.details.intent.mode).toBe("fresh");

    await door.agentEnd();
    const transition = door.calls[2]!;
    expect(transition.method).toBe("restart_transition");
    expect(transition.params.intentId).toBe("intent-fresh-91");
  });

  test("the created intent carries the door's own session id, never a caller's", async () => {
    const door = withCapability({ status: { ok: true }, requestCapability: () => REQUEST_CAPABILITY });

    await callTool(door, {
      mode: "resume",
      reason: "a forged requester",
      requesterSession: "forged-session",
      sessionId: "forged-session",
      consentSource: "operator-approval",
    });

    const request = door.calls[1]!;
    expect(request.params.requesterSession).toBe("session-under-restart");
    expect(request.params.sessionId).toBe("session-under-restart");
    // Consent is the room's standing policy, not a caller's declaration.
    expect(request.params.consentSource).toBe("operator-standing-policy");
  });

  // The sharp edge: recording an intent this room cannot prove would leave a
  // pending row that expires unused and counts against the storm guard.
  test("never records an intent the room could not exit", async () => {
    const door = buildDoor({
      status: { ok: true },
      exitCapability: () => null,
      requestCapability: () => REQUEST_CAPABILITY,
    });

    const result = await callTool(door);

    expect(result.details.code).toBe("restart_capability_unavailable");
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);
  });

  test("never creates a second intent when one is already pending", async () => {
    const door = withCapability({ requestCapability: () => REQUEST_CAPABILITY });

    const result = await callTool(door);

    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);
    expect(result.details.created).toBe(false);
    expect(result.details.intent.intentId).toBe("intent-4711");
  });

  test("carries a refused restart_request into a named refusal without arming", async () => {
    const door = withCapability({
      status: { ok: true },
      requestCapability: () => REQUEST_CAPABILITY,
      request: { ok: false, code: "restart_storm", error: "more than 3 intents this hour" },
    });

    const result = await callTool(door);
    await door.agentEnd();
    await settle();

    expect(result.isError).toBe(true);
    expect(result.details.code).toBe("restart_request_refused");
    expect(result.details.upstreamCode).toBe("restart_storm");
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status", "restart_request"]);
    expect(door.exits).toEqual([]);
  });

  test("refuses when restart_request answers without a usable intent", async () => {
    const door = withCapability({
      status: { ok: true },
      requestCapability: () => REQUEST_CAPABILITY,
      request: { ok: true, state: "requested" },
    });

    const result = await callTool(door);

    expect(result.isError).toBe(true);
    expect(result.details.code).toBe("restart_request_unusable");
    expect(door.exits).toEqual([]);
  });

  test("refuses an intent that is no longer pending", async () => {
    const door = withCapability({
      status: { ok: true, intent: { intentId: "intent-9", state: "exiting", mode: "resume" } },
    });

    const result = await callTool(door);

    expect(result.details.code).toBe("intent_not_armable");
    expect(result.details.state).toBe("exiting");
  });

  // The keeper's claim is not an invitation to exit: the substrate answers
  // exit_not_requested for a claimed intent, and it answers at agent_end, long
  // after the operator could see why the session simply carried on.
  test("refuses a claimed intent at the tool instead of arming a doomed exit", async () => {
    const door = withCapability({
      status: { ok: true, intent: { intentId: "intent-9", state: "claimed", mode: "resume" } },
    });

    const result = await callTool(door);
    await door.agentEnd();
    await settle();

    expect(result.details.code).toBe("intent_not_armable");
    expect(result.details.error).toContain("only a requested intent may exit");
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);
    expect(door.exits).toEqual([]);
  });

  test("refuses a mode the pending intent does not carry", async () => {
    const door = withCapability();

    const result = await callTool(door, { mode: "fresh", reason: "wrong mode" });

    expect(result.details.code).toBe("mode_mismatch");
    expect(result.details.intentMode).toBe("resume");
  });

  test("a refusal never arms the exit", async () => {
    const door = withCapability({ status: { ok: true } });

    await callTool(door);
    await door.agentEnd();
    await settle();

    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);
    expect(door.exits).toEqual([]);
  });

  // ---------------------------------------------------------------------------
  // Kintsu item 1 (not_met at 8deb66a): a pending row is not evidence of
  // authorized consent. The intent id authorizes nothing now; the exiting arm
  // carries the room's provisioned restart_exit capability and the session
  // identity the door itself is bound to.
  // ---------------------------------------------------------------------------

  test("proves the exit with the room capability and its own session binding, never with caller input", async () => {
    const door = withCapability();

    // A caller trying to name the authority: both fields are forged here and
    // both must be ignored, because the tool schema cannot even carry them.
    await callTool(door, {
      mode: "resume",
      reason: "the loaded release is stale",
      requesterSession: "forged-session",
      capability: "forged-capability",
    });
    await door.agentEnd();

    const transition = door.calls[1]!;
    expect(transition.method).toBe("restart_transition");
    expect(transition.write).toBe(true);
    expect(transition.params.to).toBe("exiting");
    expect(transition.params.requesterSession).toBe("session-under-restart");
    expect(transition.params.capability).toBe(EXIT_CAPABILITY);
    // Tokenless in the keeper-lease sense: capability-fenced, never token-fenced.
    expect(transition.params.claimToken).toBeUndefined();
  });

  test("refuses to arm when the room holds no restart_exit capability", async () => {
    const door = buildDoor({ exitCapability: () => null });

    const result = await callTool(door);
    await door.agentEnd();
    await settle();

    expect(result.isError).toBe(true);
    expect(result.details.code).toBe("restart_capability_unavailable");
    expect(result.details.error).toContain("request_restart refuses");
    // Named so the operator can provision it, and never the secret itself.
    expect(result.details.provision).toContain(CAPABILITY_ENV);
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);
    expect(door.exits).toEqual([]);
  });

  test("reads the capability from the room's own runtime config, not from a stub", async () => {
    process.env[CAPABILITY_ENV] = "env-provisioned-secret";
    const door = buildDoor();

    await callTool(door);
    await door.agentEnd();

    expect(door.calls[1]!.params.capability).toBe("env-provisioned-secret");
  });

  // The secret is spent, never stored: it is resolved again at agent_end, so a
  // capability revoked mid-turn stands the exit down instead of firing it.
  test("stands the exit down when the capability is revoked between arming and agent_end", async () => {
    let available = true;
    const door = buildDoor({ exitCapability: () => (available ? EXIT_CAPABILITY : null) });

    const result = await callTool(door);
    expect(result.details.armed).toBe(true);
    available = false;
    await door.agentEnd();
    await settle();

    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);
    expect(door.exits).toEqual([]);
    expect(door.notices.join(" ")).toContain("stood down");
  });

  test("never echoes the capability secret into the arm report", async () => {
    const door = withCapability();

    const result = await callTool(door);

    expect(JSON.stringify(result.details)).not.toContain(EXIT_CAPABILITY);
  });

  test("refuses a second arm by name while one exit is already armed", async () => {
    const door = withCapability();

    const first = await callTool(door);
    const second = await callTool(door, { mode: "resume", reason: "a second thought" });

    expect(first.details.armed).toBe(true);
    expect(second.isError).toBe(true);
    expect(second.details.code).toBe("already_armed");
    expect(second.details.armedIntentId).toBe("intent-4711");
    // The second call is refused before the substrate is asked anything.
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);

    await door.agentEnd();
    await settle();
    expect(door.calls.filter((call) => call.method === "restart_transition")).toHaveLength(1);
    expect(door.exits).toEqual([ARMED_EXIT_CODE]);
  });

  // ---------------------------------------------------------------------------
  // Kintsu's overflow risk: the ceiling has to hold on the SERIALIZED payload,
  // not on the free-text field alone.
  // ---------------------------------------------------------------------------

  test("keeps detail inside the substrate byte ceiling without losing the identity", async () => {
    const door = withCapability();

    await callTool(door, { mode: "resume", reason: "\u00e9".repeat(4000) });
    await door.agentEnd();

    const detail = String(door.calls[1]!.params.detail);
    // Multi-byte on purpose: a length-based clamp would pass this and still
    // hand the substrate an over-budget payload.
    expect(Buffer.byteLength(detail)).toBeLessThanOrEqual(DETAIL_CEILING_BYTES);
    const parsed = JSON.parse(detail);
    expect(parsed.session).toBe("session-under-restart");
    expect(parsed.reason.length).toBeGreaterThan(0);
  });

  // The identity is not free: a deep worktree path is a real workspace value
  // and it can exhaust the whole ceiling by itself. Clamping only the reason
  // leaves the payload over budget (and, with a subtractive budget, spins
  // forever), so the ceiling is asserted against the serialized blob.
  test("holds the ceiling when the identity alone overflows it", async () => {
    const door = withCapability();
    const deepWorkspace = `D:/athanor-wt/${"restart-door-".repeat(240)}`;

    await callTool(door, { mode: "resume", reason: "the loaded release is stale" }, toolContext({ cwd: deepWorkspace }));
    await door.agentEnd();

    const detail = String(door.calls[1]!.params.detail);
    expect(Buffer.byteLength(detail)).toBeLessThanOrEqual(DETAIL_CEILING_BYTES);
    const parsed = JSON.parse(detail);
    // The session identity is what the substrate verifies, so it is the one
    // field that survives; the report says it was cut rather than pretending.
    expect(parsed.session).toBe("session-under-restart");
    expect(parsed.truncated).toBe(true);
  }, 5000);

  // Kintsu's second review, reproduced live: 4,000 quote characters clamped
  // against a 2,048-byte "budget" serialized to 3,892 bytes, because the clamp
  // measured the RAW value and never re-measured after JSON escaping. One quote
  // costs two bytes once serialized, one control character costs six, so a
  // budget taken from an empty field can lie by up to 6x. The ceiling now holds
  // on the serialized candidate itself, which is the only thing the substrate
  // ever sees.
  const escapingFloods: Array<[string, string]> = [
    ["quote flood", '"'.repeat(4000)],
    ["backslash flood", "\\".repeat(4000)],
    ["control characters", "\u0001".repeat(4000)],
    ["multibyte and escaping combined", '\u00e9"\u0007\u2026\\'.repeat(1200)],
    ["astral plane and quotes", '\u{1F409}"'.repeat(1200)],
  ];

  for (const [name, reason] of escapingFloods) {
    test(`holds the serialized ceiling against a ${name}`, async () => {
      const door = withCapability();

      await callTool(door, { mode: "resume", reason });
      await door.agentEnd();

      const detail = String(door.calls[1]!.params.detail);
      expect(Buffer.byteLength(detail, "utf8")).toBeLessThanOrEqual(DETAIL_CEILING_BYTES);
      // Valid JSON, not a blob cut mid-escape: the substrate parses this.
      const parsed = JSON.parse(detail);
      expect(parsed.session).toBe("session-under-restart");
      expect(parsed.truncated).toBe(true);
      // A lone surrogate would survive JSON.parse but is still a broken rune.
      if (typeof parsed.reason === "string") {
        expect(parsed.reason).not.toMatch(/[\uD800-\uDBFF](?![\uDC00-\uDFFF])/);
      }
    }, 5000);
  }

  // The loop that shrinks the candidate has to terminate even when no field can
  // ever fit, because the fixed overhead alone already exceeds the ceiling. The
  // infinite-loop scar from the first repair is why this is proven, not argued:
  // a synchronous spin here cannot be interrupted by a test timeout.
  test("terminates and stays parseable when the ceiling is smaller than the fixed overhead", () => {
    const armed = {
      intentId: "intent-4711",
      mode: "resume",
      room: "kodo",
      spirit: "Kodo",
      session: "session-under-restart",
      workspace: "D:/athanor-wt/restart-door",
      roomDir: "C:/Solarisael/Obsidian/obsidian/kodo",
      reason: "\u00e9".repeat(500),
    };

    for (const limit of [0, 1, 8, 46, 64, 200]) {
      const detail = exitDetailFor(armed, limit);
      expect(() => JSON.parse(detail)).not.toThrow();
      expect(JSON.parse(detail).source).toBe("omp-adapter");
    }

    // And with room to work it still spends the whole ceiling on the account.
    const generous = exitDetailFor(armed, DETAIL_CEILING_BYTES);
    expect(Buffer.byteLength(generous, "utf8")).toBeLessThanOrEqual(DETAIL_CEILING_BYTES);
    expect(JSON.parse(generous).session).toBe("session-under-restart");
  }, 5000);

  // ---------------------------------------------------------------------------
  // The frozen contract: fresh mode is "the same launch after the House
  // confirms a paper boat exists for the session's room". Nothing checked it,
  // so a fresh restart could throw away a session with no letter waiting on the
  // other side. The door reads the boat and never consumes it: paper_boat_wake
  // is a pure read (crates/house-substrate/tests/paper_boat_integration.rs
  // wakes the same room twice and gets the boat both times).
  // ---------------------------------------------------------------------------

  test("refuses a fresh restart when the House holds no paper boat for this room", async () => {
    const boatReads: string[] = [];
    const door = withCapability({
      status: { ok: true },
      requestCapability: () => REQUEST_CAPABILITY,
      latestBoat: async (room: string) => {
        boatReads.push(room);
        return { ok: true, found: false, room };
      },
    });

    const result = await callTool(door, { mode: "fresh", reason: "start clean" });
    await door.agentEnd();
    await settle();

    expect(result.isError).toBe(true);
    expect(result.details.code).toBe("fresh_without_boat");
    expect(result.details.remedy).toContain("sleep");
    expect(boatReads.length).toBe(1);
    // Nothing recorded, nothing armed: the refusal lands before the intent.
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);
    expect(door.exits).toEqual([]);
  });

  test("creates and arms a fresh restart once the boat is confirmed", async () => {
    const door = withCapability({
      status: { ok: true },
      requestCapability: () => REQUEST_CAPABILITY,
      latestBoat: async (room: string) => ({ ok: true, found: true, room, id: "boat-77", title: "paper boat" }),
    });

    const result = await callTool(door, { mode: "fresh", reason: "start clean" });

    expect(result.details.armed).toBe(true);
    expect(result.details.created).toBe(true);
    expect(result.details.boat).toMatchObject({ confirmed: true, id: "boat-77" });
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status", "restart_request"]);
    expect(door.calls[1]!.params.mode).toBe("fresh");
  });

  // An unreadable boat is not an absent boat, and it is not a confirmation
  // either. The contract says the House confirms; silence is not a confirmation.
  test("refuses a fresh restart when the boat cannot be read at all", async () => {
    const door = withCapability({
      status: { ok: true },
      requestCapability: () => REQUEST_CAPABILITY,
      latestBoat: async () => ({ ok: false, error: "Rust substrate executable is unavailable" }),
    });

    const result = await callTool(door, { mode: "fresh", reason: "start clean" });

    expect(result.isError).toBe(true);
    expect(result.details.code).toBe("fresh_boat_unconfirmed");
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);
  });

  // The fence follows the mode that will actually be honored, not the word the
  // caller typed: a pending fresh intent needs the boat just as much.
  test("refuses arming a pending fresh intent with no boat", async () => {
    const door = withCapability({
      status: { ok: true, intent: { intentId: "intent-fresh-7", state: "requested", mode: "fresh" } },
      latestBoat: async (room: string) => ({ ok: true, found: false, room }),
    });

    const result = await callTool(door, { mode: "fresh", reason: "start clean" });
    await door.agentEnd();
    await settle();

    expect(result.details.code).toBe("fresh_without_boat");
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);
    expect(door.exits).toEqual([]);
  });

  test("a resume restart never reads a paper boat", async () => {
    let reads = 0;
    const door = withCapability({
      latestBoat: async (room: string) => {
        reads += 1;
        return { ok: true, found: false, room };
      },
    });

    const result = await callTool(door, { mode: "resume", reason: "the loaded release is stale" });

    expect(result.details.armed).toBe(true);
    expect(reads).toBe(0);
    expect(result.details.boat).toBeUndefined();
  });

  // ---------------------------------------------------------------------------
  // Kintsu item 3 (not_met at 8deb66a): the report enumerated only the one
  // transport map it was handed, and asserted a fate for hub jobs it could not
  // see. Every class is now either really enumerated or named as unseen.
  // ---------------------------------------------------------------------------

  test("enumerates the session's real async jobs through the harness snapshot", async () => {
    const door = withCapability();

    const result = await callTool(door, undefined, toolContext({
      jobs: { running: [RUNNING_JOB], recent: [FINISHED_JOB], delivery: {} },
    }));

    const jobs = result.details.dies.asyncJobs;
    expect(jobs.enumerable).toBe(true);
    expect(jobs.source).toBe("ctx.getAsyncJobSnapshot");
    expect(jobs.count).toBe(1);
    // Reported by what the real shape carries: id, type, label. Nothing claims
    // a survivor, because a session-owned async job cannot outlive the session.
    expect(jobs.jobs).toEqual([{ id: "bash_a1b2c3", type: "bash", label: "bun test --isolate" }]);
    // A completed job is not a casualty.
    expect(JSON.stringify(jobs)).not.toContain("task_d4e5f6");
  });

  test("enumerates armed GIGA turn buffers, which the exit destroys unflushed", async () => {
    const door = withCapability({
      gigaBuffers: () => [
        { session: "session-under-restart", cwd: "C:/Solarisael/Obsidian/obsidian/kodo", turns: 7 },
        { session: "session-elsewhere", cwd: "C:/Solarisael/Obsidian/obsidian/kintsu", turns: 2 },
      ],
    });

    const result = await callTool(door);

    const buffers = result.details.dies.gigaTurnBuffers;
    expect(buffers.enumerable).toBe(true);
    expect(buffers.sessions).toBe(2);
    expect(buffers.turns).toBe(9);
  });

  test("names every open transport by executable", async () => {
    const second = "C:/fake/versions/0.10.1/bin/athanor-substrate-2.exe";
    const door = withCapability({
      transports: new Map([[FAKE_EXECUTABLE, { usable: true }], [second, { usable: false }]]),
    });

    const result = await callTool(door);

    const transports = result.details.dies.transports;
    expect(transports.count).toBe(2);
    expect(transports.open).toEqual([
      { executable: FAKE_EXECUTABLE, usable: true },
      { executable: second, usable: false },
    ]);
  });

  // The honesty fence. The previous suite let a class report enumerable:false
  // with empty arrays AND a note that still promised which jobs die; it also
  // filtered on persist/detached, fields the harness surface never carries.
  test("declares each class it cannot see and asserts no fate for any of them", async () => {
    const door = withCapability();

    // A harness with no async-job door at all: the class is unseen, not empty.
    const result = await callTool(door, undefined, toolContext({ withJobDoor: false }));

    const dies = result.details.dies;
    expect(dies.asyncJobs.enumerable).toBe(false);
    expect(dies.asyncJobs.jobs).toBeUndefined();
    expect(dies.asyncJobs.reason).toContain("getAsyncJobSnapshot");

    // Hub processes (persist/detached) are project-scoped and simply absent
    // from the plugin surface, so the door may not classify them at all.
    expect(dies.hubProcesses.enumerable).toBe(false);
    expect(dies.hubProcesses.reason.length).toBeGreaterThan(0);

    // No GIGA inspector threaded: unseen, not zero.
    expect(dies.gigaTurnBuffers.enumerable).toBe(false);
    expect(dies.gigaTurnBuffers.turns).toBeUndefined();

    // The one handed map is not the whole process: the unseen families are
    // named rather than silently implied away.
    expect(dies.transports.unenumerableFamilies).toContain("recall");
    expect(dies.transports.unenumerableFamilies).toContain("giga");
    expect(dies.transports.unenumerableFamilies).toContain("anamnesis");

    // No class that cannot be seen may use the vocabulary of a verdict.
    for (const [name, casualty] of Object.entries(dies) as Array<[string, any]>) {
      if (casualty?.enumerable !== false) continue;
      expect(JSON.stringify(casualty), `${name} asserts a fate it cannot see`)
        .not.toMatch(/\bdies\b|\bsurvives\b|\bevery\b/i);
    }
  });

  test("the arm report still names where the exit falls and what release is loaded", async () => {
    const door = withCapability();

    const result = await callTool(door);

    expect(result.isError).toBeUndefined();
    expect(result.details.armed).toBe(true);
    expect(result.details.firesAt).toBe("agent_end");
    expect(result.details.exitCode).toBe(ARMED_EXIT_CODE);
    expect(result.details.intent).toMatchObject({ intentId: "intent-4711", state: "requested", mode: "resume" });
    expect(result.details.loadedRelease).toEqual({ releaseId: "0.9.3-abc", previousReleaseId: "0.9.2-def" });
  });

  test("the armed exit fires only after agent_end, transitioning the intent to exiting first", async () => {
    const door = withCapability();

    await callTool(door);
    // Arming alone kills nothing: the tool returns into a live turn.
    await settle();
    expect(door.exits).toEqual([]);
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);

    await door.agentEnd();
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status", "restart_transition"]);
    const transition = door.calls[1]!;
    expect(transition.params.intentId).toBe("intent-4711");
    expect(transition.params.to).toBe("exiting");
    const detail = JSON.parse(String(transition.params.detail));
    expect(detail).toMatchObject({
      source: "omp-adapter",
      session: "session-under-restart",
      mode: "resume",
      exitCode: ARMED_EXIT_CODE,
    });

    await settle();
    expect(door.exits).toEqual([ARMED_EXIT_CODE]);
  });

  test("a refused transition stands the exit down instead of dying", async () => {
    const door = withCapability({ transition: { ok: false, code: "stale_lease", error: "refused" } });

    await callTool(door);
    await door.agentEnd();
    await settle();

    expect(door.exits).toEqual([]);
    expect(door.notices.join(" ")).toContain("stood down");
  });

  // The substrate's new authority refusals travel into the operator's notice:
  // each one means a different repair, and silence would mean none of them.
  test("carries the substrate's authority refusals into the stand-down notice", async () => {
    for (const code of ["restart_capability", "exit_not_authorized", "restart_storm"]) {
      const door = withCapability({ transition: { ok: false, code, error: "refused" } });

      await callTool(door);
      await door.agentEnd();
      await settle();

      expect(door.exits).toEqual([]);
      expect(door.notices.join(" ")).toContain(code);
    }
  });

  test("one arming fires one exit", async () => {
    const door = withCapability();

    await callTool(door);
    await door.agentEnd();
    await door.agentEnd();
    await settle();

    expect(door.exits).toEqual([ARMED_EXIT_CODE]);
    expect(door.calls.filter((call) => call.method === "restart_transition")).toHaveLength(1);
  });

  test("an unarmed agent_end asks the substrate nothing", async () => {
    const door = withCapability();

    await door.agentEnd();
    await settle();

    expect(door.calls).toEqual([]);
    expect(door.exits).toEqual([]);
  });

  test("reports a null loaded release when the loader threaded none", async () => {
    const door = withCapability({ release: null });

    const result = await callTool(door);

    expect(result.details.loadedRelease).toEqual({ releaseId: null, previousReleaseId: null });
  });
});

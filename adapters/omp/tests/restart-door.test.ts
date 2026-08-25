import { describe, expect, test } from "bun:test";

import { registerRestartDoor } from "../solarisael-house-proof/restart-door.ts";

// 87 is the keeper handshake code in the frozen wire contract, so it is pinned
// here as a literal. Asserting the module's own constant would defend nothing.
const ARMED_EXIT_CODE = 87;

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
};

const FAKE_EXECUTABLE = "C:/fake/versions/0.10.1/bin/athanor-substrate.exe";

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
  transition?: Record<string, unknown>;
  hubJobs?: () => any[] | null;
  release?: { releaseId?: string | null; previousReleaseId?: string | null } | null;
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
      if (method === "restart_transition") return options.transition ?? { ok: true, state: "exiting" };
      return { ok: false, error: `unexpected method ${method}` };
    },
    transports: new Map([[FAKE_EXECUTABLE, { usable: true }]]) as any,
    registerTool(definition) {
      tools.push(definition as unknown as CapturedTool);
    },
    hubJobs: options.hubJobs ?? (() => null),
    release: "release" in options ? options.release : { releaseId: "0.9.3-abc", previousReleaseId: "0.9.2-def" },
    exit(code) {
      exits.push(code);
    },
  });

  const agentEnd = hooks.find((hook) => hook.name === "agent_end");
  if (!agentEnd) throw new Error("the door registered no agent_end hook");
  if (tools.length !== 1) throw new Error(`the door registered ${tools.length} tools`);

  return {
    tool: tools[0]!,
    hooks,
    calls,
    exits,
    notices,
    hookNames: () => hooks.map((hook) => hook.name),
    agentEnd: () => agentEnd.handler({ messages: [] }, { ui: { notify: (text: string) => notices.push(text) } }),
  };
}

function callTool(door: DoorHarness, params: unknown = { mode: "resume", reason: "the loaded release is stale" }) {
  return door.tool.execute("call-1", params, undefined, undefined, {
    cwd: process.cwd(),
    sessionId: "session-under-restart",
  });
}

// The exit is scheduled for after the agent_end handler settles, so a test that
// wants to see it must let one timer turn pass.
function settle() {
  return new Promise((resolve) => setTimeout(resolve, 5));
}

describe("adapter exit door", () => {
  test("registers one write tool and arms on agent_end only", () => {
    const door = buildDoor();

    expect(door.tool.name).toBe("request_restart");
    expect(door.tool.approval).toBe("write");
    expect(Object.keys(door.tool.parameters.shape ?? {})).toEqual(["mode", "reason"]);
    expect(door.tool.parameters.shape?.mode?.values).toEqual(["resume", "fresh"]);
    // The fence itself: this door owns no turn_end tap, because turn_end fires
    // mid-tool-loop and an exit there would kill a turn in progress.
    expect(door.hookNames()).toEqual(["agent_end"]);
  });

  test("refuses by name when no pending intent exists, naming restart_status", async () => {
    const door = buildDoor({ status: { ok: true } });

    const result = await callTool(door);

    expect(result.isError).toBe(true);
    expect(result.details.ok).toBe(false);
    expect(result.details.code).toBe("no_pending_intent");
    expect(result.details.error).toContain("request_restart refuses");
    expect(result.details.missingPrerequisite).toBe("restart_status");
    expect(result.details.createIntent).toContain("restart_request");
    expect(result.details.createIntent).toContain("consentSource");
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);
  });

  test("refuses an intent that is no longer pending", async () => {
    const door = buildDoor({
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
    const door = buildDoor({
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

  test("keeps detail inside the substrate byte ceiling without losing the identity", async () => {
    const door = buildDoor();

    await callTool(door, { mode: "resume", reason: "\u00e9".repeat(4000) });
    await door.agentEnd();

    const detail = String(door.calls[1]!.params.detail);
    // Multi-byte on purpose: a length-based clamp would pass this and still
    // hand the substrate an over-budget payload.
    expect(Buffer.byteLength(detail)).toBeLessThanOrEqual(2048);
    const parsed = JSON.parse(detail);
    expect(parsed.session).toBe("session-under-restart");
    expect(parsed.reason.length).toBeGreaterThan(0);
  });

  test("refuses a mode the pending intent does not carry", async () => {
    const door = buildDoor();

    const result = await callTool(door, { mode: "fresh", reason: "wrong mode" });

    expect(result.details.code).toBe("mode_mismatch");
    expect(result.details.intentMode).toBe("resume");
  });

  test("a refusal never arms the exit", async () => {
    const door = buildDoor({ status: { ok: true } });

    await callTool(door);
    await door.agentEnd();
    await settle();

    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);
    expect(door.exits).toEqual([]);
  });

  test("the arm report names hub-job and transport casualties", async () => {
    const door = buildDoor({
      hubJobs: () => [
        { id: "bash_a1b2c3", persist: false },
        { id: "web", persist: true },
        { id: "watcher_detached", detached: true },
        { name: "unnamed_lane" },
      ],
    });

    const result = await callTool(door);

    expect(result.isError).toBeUndefined();
    expect(result.details.armed).toBe(true);
    expect(result.details.firesAt).toBe("agent_end");
    expect(result.details.exitCode).toBe(ARMED_EXIT_CODE);
    expect(result.details.intent).toMatchObject({ intentId: "intent-4711", state: "requested", mode: "resume" });
    expect(result.details.loadedRelease).toEqual({ releaseId: "0.9.3-abc", previousReleaseId: "0.9.2-def" });

    expect(result.details.dies.hubJobs.enumerable).toBe(true);
    expect(result.details.dies.hubJobs.dies).toEqual(["bash_a1b2c3", "unnamed_lane"]);
    expect(result.details.dies.hubJobs.survives).toEqual(["web", "watcher_detached"]);
    expect(result.details.dies.rustTransports).toEqual({
      count: 1,
      transports: [{ executable: FAKE_EXECUTABLE, usable: true }],
    });
  });

  test("declares hub jobs unenumerable instead of promising survivors", async () => {
    const door = buildDoor();

    const result = await callTool(door);

    expect(result.details.dies.hubJobs.enumerable).toBe(false);
    expect(result.details.dies.hubJobs.note).toContain("not enumerable");
    expect(result.details.dies.rustTransports.count).toBe(1);
  });

  test("the armed exit fires only after agent_end, transitioning the intent to exiting first", async () => {
    const door = buildDoor();

    await callTool(door);
    // Arming alone kills nothing: the tool returns into a live turn.
    await settle();
    expect(door.exits).toEqual([]);
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status"]);

    await door.agentEnd();
    expect(door.calls.map((call) => call.method)).toEqual(["restart_status", "restart_transition"]);
    const transition = door.calls[1]!;
    expect(transition.write).toBe(true);
    expect(transition.params.intentId).toBe("intent-4711");
    expect(transition.params.to).toBe("exiting");
    // The identity rides as JSON in detail, and no claimToken may be present:
    // the substrate refuses a tokenful requested -> exiting outright.
    expect(transition.params.claimToken).toBeUndefined();
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
    const door = buildDoor({ transition: { ok: false, code: "stale_lease", error: "refused" } });

    await callTool(door);
    await door.agentEnd();
    await settle();

    expect(door.exits).toEqual([]);
    expect(door.notices.join(" ")).toContain("stood down");
  });

  test("one arming fires one exit", async () => {
    const door = buildDoor();

    await callTool(door);
    await door.agentEnd();
    await door.agentEnd();
    await settle();

    expect(door.exits).toEqual([ARMED_EXIT_CODE]);
    expect(door.calls.filter((call) => call.method === "restart_transition")).toHaveLength(1);
  });

  test("an unarmed agent_end asks the substrate nothing", async () => {
    const door = buildDoor();

    await door.agentEnd();
    await settle();

    expect(door.calls).toEqual([]);
    expect(door.exits).toEqual([]);
  });

  test("reports a null loaded release when the loader threaded none", async () => {
    const door = buildDoor({ release: null });

    const result = await callTool(door);

    expect(result.details.loadedRelease).toEqual({ releaseId: null, previousReleaseId: null });
  });
});

// Exercise adapter registration, the Host wire, and the real buffer census. Native authorization needs separate Rust proof.

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import path from "node:path";
import { tmpdir } from "node:os";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";

import solarisaelHouseProof from "../index.ts";
import { roomContext } from "../house-proof/room.ts";
import { registerTopLevelSession, retireTopLevelSession } from "../house-proof/top-level-session-fence.ts";
import { __gigaTest, ingestGigaLoggedTurnsDetached } from "../giga.ts";

type CapturedTool = {
  name: string;
  execute: (
    toolCallId: string,
    params: unknown,
    signal?: unknown,
    onUpdate?: unknown,
    ctx?: unknown,
  ) => Promise<{ isError?: boolean; details: any }>;
};

// The adapter asks zod for shapes at registration time and never validates with
// it here, so a shape recorder is the whole surface this test needs.
function makeSchema(kind: string, fields: Record<string, unknown> = {}): any {
  return {
    kind,
    ...fields,
    describe() { return this; },
    regex() { return this; },
    optional() { return this; },
    strict() { return this; },
    default() { return this; },
  };
}

const zodStub = {
  string: () => makeSchema("string"),
  boolean: () => makeSchema("boolean"),
  number: () => makeSchema("number"),
  enum: (values: string[]) => makeSchema("enum", { values }),
  literal: (value: unknown) => makeSchema("literal", { values: [String(value)] }),
  discriminatedUnion: (_key: string, variants: unknown[]) => makeSchema("discriminatedUnion", { variants }),
  object: (shape: Record<string, unknown>) => makeSchema("object", { shape }),
  array: (element: unknown) => makeSchema("array", { element }),
  record: (key: unknown, value: unknown) => makeSchema("record", { key, value }),
  unknown: () => makeSchema("unknown"),
};

const EXIT_CAPABILITY_ENV = "ATHANOR_RESTART_EXIT_CAPABILITY";
const GIGA_ENV = "ATHANOR_GIGA_ENABLED";
const KEEPER_CONFIG_ENV = "ATHANOR_OMP_KEEPER_CONFIG";

// A provisioned claimant, laid down the way provision-local.ps1 lays it down:
// omp-keeper.json naming the restart_claim secret file beside it. The door
// reads the pair and never a running process, so a temporary directory is the
// whole seam. `KEEPERLESS_CONFIG` is a path that is never written.
const KEEPER_ROOT = path.join(tmpdir(), "athanor-production-seam-keeper");
const KEEPER_CONFIG = path.join(KEEPER_ROOT, "omp-keeper.json");
const KEEPER_CAPABILITY = path.join(KEEPER_ROOT, "restart-capability");
const KEEPERLESS_CONFIG = path.join(KEEPER_ROOT, "no-keeper-here", "omp-keeper.json");

function provisionKeeper(capabilityPath: string): void {
  mkdirSync(KEEPER_ROOT, { recursive: true });
  writeFileSync(KEEPER_CAPABILITY, "seam-keeper-capability\n", "utf8");
  writeFileSync(
    KEEPER_CONFIG,
    JSON.stringify({
      ompLaunch: [process.execPath],
      workspace: KEEPER_ROOT,
      programRoot: KEEPER_ROOT,
      stateRoot: KEEPER_ROOT,
      capabilityPath,
      claimant: "omp-keeper",
      watchIntervalSecs: 30,
    }),
    "utf8",
  );
}

// A whole room on disk, so the door's durable answer — the room's own
// `.omp/runtime` — is exercised and not only the environment override that the
// tests above use. `active_spirit.md` is what makes room.ts recognize it.
const SEAM_ROOM = path.join(tmpdir(), "athanor-production-seam-room", "seam-room");
const SEAM_ROOM_RUNTIME = path.join(SEAM_ROOM, ".omp", "runtime");

function provisionSeamRoom(withCapability: boolean): void {
  mkdirSync(SEAM_ROOM_RUNTIME, { recursive: true });
  writeFileSync(path.join(SEAM_ROOM, "active_spirit.md"), "# Active Spirit: Seam\n", "utf8");
  const capability = path.join(SEAM_ROOM_RUNTIME, "restart-capability");
  if (withCapability) writeFileSync(capability, "seam-room-keeper-capability\n", "utf8");
  else rmSync(capability, { force: true });
  writeFileSync(
    path.join(SEAM_ROOM_RUNTIME, "omp-keeper.json"),
    JSON.stringify({
      ompLaunch: [process.execPath],
      workspace: SEAM_ROOM,
      programRoot: SEAM_ROOM,
      stateRoot: SEAM_ROOM,
      capabilityPath: capability,
      claimant: "omp-keeper",
      watchIntervalSecs: 30,
    }),
    "utf8",
  );
}

const PENDING_INTENT = {
  ok: true,
  intent: {
    intentId: "intent-production-seam",
    state: "requested",
    mode: "resume",
    sessionId: "session-under-restart",
    deadlines: { exiting: 60, relaunching: 120 },
  },
};

const hostKeys = ["ATHANOR_HOST_URL", "ATHANOR_HOST_TOKEN", "ATHANOR_HOST_HOUSE_ID"] as const;
const originalHost = Object.fromEntries(hostKeys.map((key) => [key, process.env[key]]));
let server: ReturnType<typeof Bun.serve>;
const originalGiga = process.env[GIGA_ENV];
const originalKeeperConfig = process.env[KEEPER_CONFIG_ENV];

let observed: Array<{ method: string; params: any }> = [];

// The one session-file shape giga.ts accepts as a top-level session: a real
// path whose sibling `<dir>.jsonl` does not exist (giga.ts isSubagentSessionContext).
const SESSION_FILE = path.join(tmpdir(), "athanor-production-seam-session", "session.json");

function registerRealAdapter(cwd = SEAM_ROOM): CapturedTool {
  const tools: CapturedTool[] = [];
  const pi = {
    zod: zodStub,
    setLabel() {},
    on() {},
    events: { on: () => () => {} },
    registerMessageRenderer() {},
    registerCommand() {},
    registerTool(tool: CapturedTool) {
      tools.push(tool);
    },
  };

  solarisaelHouseProof(pi as any);
  registerTopLevelSession(roomContext(cwd).room, "session-under-restart");

  const door = tools.find((tool) => tool.name === "request_restart");
  if (!door) throw new Error("the production registration path registered no request_restart tool");
  return door;
}

beforeEach(() => {
  observed = [];
  process.env[EXIT_CAPABILITY_ENV] = "production-seam-secret";
  process.env[GIGA_ENV] = "1";
  provisionKeeper(KEEPER_CAPABILITY);
  provisionSeamRoom(true);
  process.env[KEEPER_CONFIG_ENV] = KEEPER_CONFIG;
  __gigaTest.resetState();
  server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch(request, server) {
      if (server.upgrade(request, { data: {} })) return;
      return new Response("WebSocket required", { status: 400 });
    },
    websocket: {
      message(socket, data) {
        const envelope = JSON.parse(String(data));
        const { operation: method, params } = envelope.organ_request;
        observed.push({ method, params });
        const result = method === "restart_status" ? PENDING_INTENT
          : method === "restart_transition" ? { ok: true, state: "exiting" }
          : { ok: false, error: `unexpected method ${method}` };
        socket.send(JSON.stringify({
          command_or_event_type: "athanor.organ.result",
          correlation_id: envelope.message_id,
          result,
        }));
      },
    },
  });
  process.env.ATHANOR_HOST_URL = `ws://127.0.0.1:${server.port}`;
  process.env.ATHANOR_HOST_TOKEN = "isolated-adapter-wire";
  process.env.ATHANOR_HOST_HOUSE_ID = "isolated-house";
});

afterEach(async () => {
  await server.stop(true);
  for (const key of hostKeys) {
    if (originalHost[key] === undefined) delete process.env[key];
    else process.env[key] = originalHost[key];
  }
  retireTopLevelSession(roomContext(SEAM_ROOM).room, "session-under-restart");
  __gigaTest.resetState();
  delete process.env[EXIT_CAPABILITY_ENV];
  if (originalGiga === undefined) delete process.env[GIGA_ENV];
  else process.env[GIGA_ENV] = originalGiga;
  if (originalKeeperConfig === undefined) delete process.env[KEEPER_CONFIG_ENV];
  else process.env[KEEPER_CONFIG_ENV] = originalKeeperConfig;
  rmSync(KEEPER_ROOT, { recursive: true, force: true });
  rmSync(SEAM_ROOM, { recursive: true, force: true });
});

describe("exit door through the production registration seam", () => {
  test("reports the GIGA census that tools.ts actually threads, not an injected lookalike", async () => {
    const tool = registerRealAdapter();
    const cwd = SEAM_ROOM;
    const bufferCtx = {
      cwd,
      sessionManager: { getSessionFile: () => SESSION_FILE },
    };

    // Real buffering through giga.ts's own door: three turns for this cwd that
    // an armed process.exit would destroy unflushed.
    ingestGigaLoggedTurnsDetached(bufferCtx, [
      { role: "user", text: "one", sourceID: "s1", contentHash: "h1", sessionID: "seam-session", sourceTimestamp: "t1", hasStableID: true },
      { role: "assistant", text: "two", sourceID: "s2", contentHash: "h2", sessionID: "seam-session", sourceTimestamp: "t2", hasStableID: true },
      { role: "user", text: "three", sourceID: "s3", contentHash: "h3", sessionID: "seam-session", sourceTimestamp: "t3", hasStableID: true },
    ] as any);

    const result = await tool.execute("call-1", { mode: "resume", reason: "prove the production seam" }, undefined, undefined, {
      cwd,
      sessionId: "session-under-restart",
    });

    expect(result.isError).toBeUndefined();
    expect(result.details.armed).toBe(true);
    // The census reached the report through the production wiring only.
    const buffers = result.details.dies.gigaTurnBuffers;
    expect(buffers.enumerable).toBe(true);
    expect(buffers.turns).toBe(3);
    expect(buffers.bySession).toEqual([{ session: "seam-session", turns: 3 }]);
    expect(observed.map((call) => call.method)).toEqual(["restart_status"]);
  });

  test("reports no armed buffers through the same seam when nothing is buffered", async () => {
    const tool = registerRealAdapter();

    const result = await tool.execute("call-1", { mode: "resume", reason: "prove the empty case" }, undefined, undefined, {
      cwd: SEAM_ROOM,
      sessionId: "session-under-restart",
    });

    const buffers = result.details.dies.gigaTurnBuffers;
    // Enumerable and empty is a different claim from unseen, and the seam is
    // what makes the difference honest.
    expect(buffers.enumerable).toBe(true);
    expect(buffers.turns).toBe(0);
    expect(buffers.sessions).toBe(0);
  });

  // The named seam of 2026-09-05: Kodo's runtime held all three room secrets
  // and no keeper, so request_restart armed an exit nobody could claim and
  // stranded a live `exiting` intent that refused every later restart.
  test("refuses a room with no provisioned keeper, and records nothing", async () => {
    const tool = registerRealAdapter();
    process.env[KEEPER_CONFIG_ENV] = KEEPERLESS_CONFIG;

    const result = await tool.execute("call-1", { mode: "resume", reason: "no keeper is watching" }, undefined, undefined, {
      cwd: SEAM_ROOM,
      sessionId: "session-under-restart",
    });

    expect(result.isError).toBe(true);
    expect(result.details.code).toBe("no_restart_owner");
    expect(result.details.armed).toBeUndefined();
    expect(result.details.error).toContain("athanor.exe keeper");
    // Nothing recorded and nothing armed: the refusal lands before
    // restart_request, so no intent exists to strand.
    expect(observed.map((call) => call.method)).not.toContain("restart_request");
    expect(observed.map((call) => call.method)).not.toContain("restart_transition");
  });

  test("refuses a half-provisioned keeper whose capability file is absent", async () => {
    const tool = registerRealAdapter();
    // The config is there and the restart_claim secret it names is not, so the
    // keeper could start and could never claim.
    provisionKeeper(path.join(KEEPER_ROOT, "restart-capability-that-was-never-written"));

    const result = await tool.execute("call-1", { mode: "resume", reason: "half a keeper" }, undefined, undefined, {
      cwd: SEAM_ROOM,
      sessionId: "session-under-restart",
    });

    expect(result.isError).toBe(true);
    expect(result.details.code).toBe("no_restart_owner");
    expect(observed.map((call) => call.method)).not.toContain("restart_request");
  });

  // The durable answer, not the override: the door finds the keeper in the
  // room's own `.omp/runtime`, which is where provision-local.ps1 writes it.
  test("finds the keeper in the room runtime, and refuses the same room without it", async () => {
    delete process.env[KEEPER_CONFIG_ENV];
    provisionSeamRoom(true);

    const armed = await registerRealAdapter(SEAM_ROOM).execute("call-1", { mode: "resume", reason: "the room runtime holds the pair" }, undefined, undefined, {
      cwd: SEAM_ROOM,
      sessionId: "session-under-restart",
    });

    expect(armed.isError).toBeUndefined();
    expect(armed.details.armed).toBe(true);

    // The same room, one file short. A fresh registration, because an armed
    // door refuses a second arm by name.
    provisionSeamRoom(false);
    const refused = await registerRealAdapter(SEAM_ROOM).execute("call-1", { mode: "resume", reason: "the capability is gone" }, undefined, undefined, {
      cwd: SEAM_ROOM,
      sessionId: "session-under-restart",
    });

    expect(refused.isError).toBe(true);
    expect(refused.details.code).toBe("no_restart_owner");
    expect(refused.details.keeperConfig).toBe(path.join(SEAM_ROOM_RUNTIME, "omp-keeper.json"));
  });
});

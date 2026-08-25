// Kintsu's proof gap on the second review: every casualty test injected its
// deps BELOW the tools.ts registration seam, so the census the report consumed
// was a lookalike the tests built, never the one production threads. This file
// registers the adapter exactly the way index.ts does - the real entry, the real
// tools.ts wiring, the real giga.ts census - and drives the real request_restart
// tool. If the seam ever stops handing the door its census, this goes red and
// the injected tests stay green, which is precisely the gap being closed.

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import path from "node:path";
import { tmpdir } from "node:os";

import solarisaelHouseProof from "../index.ts";
import { RustJsonlTransport } from "../rust-transport.ts";
import { __gigaTest, ingestGigaLoggedTurnsDetached } from "../giga.ts";
import { closeRustRememberTransports } from "../solarisael-house-proof/tools.ts";

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
};

const SUBSTRATE_EXE_ENV = "ATHANOR_SUBSTRATE_EXE";
const EXIT_CAPABILITY_ENV = "ATHANOR_RESTART_EXIT_CAPABILITY";
const GIGA_ENV = "SOLARISAEL_GIGA_ENABLED";

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

const originalRequest = RustJsonlTransport.prototype.request;
const originalExe = process.env[SUBSTRATE_EXE_ENV];
const originalGiga = process.env[GIGA_ENV];

let observed: Array<{ method: string; params: any }> = [];

// The one session-file shape giga.ts accepts as a top-level session: a real
// path whose sibling `<dir>.jsonl` does not exist (giga.ts isSubagentSessionContext).
const SESSION_FILE = path.join(tmpdir(), "athanor-production-seam-session", "session.json");

function registerRealAdapter(): CapturedTool {
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

  const door = tools.find((tool) => tool.name === "request_restart");
  if (!door) throw new Error("the production registration path registered no request_restart tool");
  return door;
}

beforeEach(() => {
  observed = [];
  process.env[SUBSTRATE_EXE_ENV] = process.execPath;
  process.env[EXIT_CAPABILITY_ENV] = "production-seam-secret";
  process.env[GIGA_ENV] = "1";
  __gigaTest.resetState();
  RustJsonlTransport.prototype.request = async function (method: string, params: any) {
    observed.push({ method, params });
    if (method === "restart_status") return PENDING_INTENT;
    if (method === "restart_transition") return { ok: true, state: "exiting" };
    return { ok: false, error: `unexpected method ${method}` };
  } as typeof RustJsonlTransport.prototype.request;
});

afterEach(() => {
  RustJsonlTransport.prototype.request = originalRequest;
  closeRustRememberTransports();
  __gigaTest.resetState();
  delete process.env[EXIT_CAPABILITY_ENV];
  if (originalExe === undefined) delete process.env[SUBSTRATE_EXE_ENV];
  else process.env[SUBSTRATE_EXE_ENV] = originalExe;
  if (originalGiga === undefined) delete process.env[GIGA_ENV];
  else process.env[GIGA_ENV] = originalGiga;
});

describe("exit door through the production registration seam", () => {
  test("reports the GIGA census that tools.ts actually threads, not an injected lookalike", async () => {
    const tool = registerRealAdapter();
    const cwd = process.cwd();
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
      cwd: process.cwd(),
      sessionId: "session-under-restart",
    });

    const buffers = result.details.dies.gigaTurnBuffers;
    // Enumerable and empty is a different claim from unseen, and the seam is
    // what makes the difference honest.
    expect(buffers.enumerable).toBe(true);
    expect(buffers.turns).toBe(0);
    expect(buffers.sessions).toBe(0);
  });
});

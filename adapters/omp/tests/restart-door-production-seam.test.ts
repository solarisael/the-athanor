// request_restart refusal guards through the production registration path.
// These refusals are decided before any Host request, so no fake Host stands
// in for the restart authority here: the queued-restart path, its intent
// arming and the Tern relaunch are proven only against a real Host at the
// integration gate.
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import path from "node:path";
import { tmpdir } from "node:os";
import { z } from "zod";
import solarisaelHouseProof from "../index.ts";
import { __sessionTest, type Attachment } from "../house-proof/session.ts";
import { exitGuard } from "../house-proof/exit-guards.ts";

type CapturedTool = {
  name: string;
  execute: (toolCallId: string, params: unknown, signal?: unknown, onUpdate?: unknown, ctx?: unknown) => Promise<{ isError?: boolean; details: any }>;
};

const ROOT = path.join(tmpdir(), "athanor-production-seam");
const PROJECT = path.join(ROOT, "project");
const SESSION_FILE = path.join(ROOT, "sessions", "conversation.jsonl");

function harnessCtx() {
  return {
    cwd: PROJECT,
    agent: { kind: "main", id: "Main", name: "main", depth: 0 },
    sessionManager: { getSessionId: () => "conversation-seam", getSessionFile: () => SESSION_FILE, getBranch: () => [] },
    ui: { notify: () => {} },
  };
}

function attach(fields: Partial<Attachment> = {}): void {
  const key = __sessionTest.conversationKey(harnessCtx())!;
  __sessionTest.attachments.set(key, {
    key, room: "kintsu", source: "selected", status: "attached", error: null,
    instanceId: "5b0c2a52-8c55-4b55-9a39-0a3a8f6f5e10", conversationId: "conversation-seam", sessionFile: SESSION_FILE, workspace: PROJECT,
    spirit: "Kintsu", operator: "Sol", homeDir: path.join(ROOT, "homes", "kintsu"),
    context: { revision: "sha256:00", text: "You are Kintsu." }, controls: ["deliver", "interrupt", "compact", "close", "restart"],
    chatDelivery: false, restart: { supported: true, launcher: "tern" }, heartbeatSecs: 10,
    terminal: null, terminalReason: null, launcher: { kind: "tern", program: process.execPath, args: [], environment: {} }, launcherReason: null,
    exiting: false, connected: true,
    ...fields,
  });
}

function registerRealAdapter(): CapturedTool {
  const tools: CapturedTool[] = [];
  const pi = {
    zod: z,
    setLabel() {},
    on() {},
    events: { on: () => () => {} },
    registerMessageRenderer() {},
    registerCommand() {},
    registerTool(tool: CapturedTool) { tools.push(tool); },
  };
  solarisaelHouseProof(pi as any);
  const door = tools.find((tool) => tool.name === "request_restart");
  if (!door) throw new Error("the production registration path registered no request_restart tool");
  return door;
}

beforeEach(() => __sessionTest.reset());
afterEach(() => __sessionTest.reset());

describe("request_restart refusals through the production registration seam", () => {
  test("an ordinary session with no live Host registration cannot ask for a restart", async () => {
    const result = await registerRealAdapter().execute("call-1", { mode: "resume", reason: "no home" }, undefined, undefined, harnessCtx());
    expect(result.isError).toBe(true);
    expect(result.details.code).toBe("no_live_registration");
  });

  test("a session no launcher can relaunch refuses visibly", async () => {
    attach({ restart: { supported: false, launcher: null, reason: "not running in a Tern pane" }, launcher: null });
    const result = await registerRealAdapter().execute("call-1", { mode: "resume", reason: "no launcher" }, undefined, undefined, harnessCtx());
    expect(result.isError).toBe(true);
    expect(result.details.code).toBe("restart_unsupported");
  });

  test("a session that already acknowledged an exit cannot ask again", async () => {
    attach({ exiting: true });
    const result = await registerRealAdapter().execute("call-1", { mode: "resume", reason: "twice" }, undefined, undefined, harnessCtx());
    expect(result.isError).toBe(true);
  });
});

describe("Host-issued exit guards", () => {
  test("a busy session holds the exit; running async jobs refuse it", () => {
    expect(exitGuard({ isIdle: () => false, hasPendingMessages: () => false })).toHaveProperty("hold");
    expect(exitGuard({ isIdle: () => true, hasPendingMessages: () => true })).toHaveProperty("hold");
    expect(exitGuard({
      isIdle: () => true,
      hasPendingMessages: () => false,
      getAsyncJobSnapshot: () => ({ running: [{ id: "job-1", label: "build" }] }),
    })).toHaveProperty("refuse");
    expect(exitGuard({ isIdle: () => true, hasPendingMessages: () => false, getAsyncJobSnapshot: () => ({ running: [] }) })).toBeNull();
  });
});

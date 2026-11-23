import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { z } from "zod";
import { registerSolarisaelTools } from "../house-proof/tools.ts";
import { __sessionTest } from "../house-proof/session.ts";

// #57: a Host `command_failed` kept only `reason` and surfaced as
// transport_unavailable. #58: a nested `{error: {code, message}}` refusal
// surfaced as tool_failure / "Athanor tool failed".

const KEYS = ["ATHANOR_HOST_URL", "ATHANOR_HOST_TOKEN", "ATHANOR_HOST_HOUSE_ID", "ATHANOR_CLIENT_ROOM_DIR"] as const;
const HOST_FAILURE = { code: "native_request_failed", message: "the lane table is unreadable", retryable: false };
const CONFLICT = { code: "revision_conflict", message: "the document changed since it was read", currentRevision: "sha256:feed" };

let directory = "";
let saved: Record<string, string | undefined> = {};
let server: ReturnType<typeof Bun.serve>;

beforeEach(() => {
  __sessionTest.reset();
  directory = mkdtempSync(path.join(tmpdir(), "host-refusal-shapes-"));
  writeFileSync(path.join(directory, ".athanor-room.json"), JSON.stringify({ room: "probe", trueName: "Probe" }));
  saved = Object.fromEntries(KEYS.map((key) => [key, process.env[key]]));
  server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    async fetch(request, server) {
      if (new URL(request.url).pathname === "/room/probe/session") {
        return Response.json({ ok: false, error: CONFLICT }, { status: 409 });
      }
      return server.upgrade(request) ? undefined : new Response("not found", { status: 404 });
    },
    websocket: {
      message(socket, input) {
        const request = JSON.parse(String(input));
        socket.send(JSON.stringify({
          correlation_id: request.message_id,
          command_or_event_type: `athanor.${request.projection_id}.command_failed`,
          reason: HOST_FAILURE.message,
          error: HOST_FAILURE,
        }));
      },
    },
  });
  process.env.ATHANOR_HOST_URL = `ws://127.0.0.1:${server.port}`;
  process.env.ATHANOR_HOST_TOKEN = "test-only";
  process.env.ATHANOR_HOST_HOUSE_ID = "test-house";
  process.env.ATHANOR_CLIENT_ROOM_DIR = directory;
});

afterEach(() => {
  server.stop(true);
  __sessionTest.reset();
  for (const key of KEYS) {
    if (saved[key] === undefined) delete process.env[key]; else process.env[key] = saved[key];
  }
  rmSync(directory, { recursive: true, force: true });
});

function ctx() {
  return { cwd: directory, sessionManager: { getSessionId: () => "probe-session" } };
}

async function runTool(name: string, params: Record<string, unknown>) {
  const tools = new Map<string, { execute: (...args: unknown[]) => Promise<any> }>();
  registerSolarisaelTools({
    zod: z,
    registerTool(definition: { name: string; execute: (...args: unknown[]) => Promise<any> }) {
      tools.set(definition.name, definition);
    },
    on() {},
    registerCommand() {},
    registerHook() {},
  }, { version: "test" });
  return await tools.get(name)!.execute("call-1", params, undefined, undefined, ctx());
}

test("a Host command_failed reaches an organ tool with the Host's code", async () => {
  const result = await runTool("canon_read", { name: "Perseus" });

  expect(result.isError).toBe(true);
  expect(result.details.code).toBe("native_request_failed");
  expect(result.details.message).toBe(HOST_FAILURE.message);
  expect(result.details.retryable).toBe(false);
});

test("a Host command_failed reaches a direct sendHostCommand tool with the Host's code", async () => {
  const result = await runTool("house_lane_status", {});

  expect(result.isError).toBe(true);
  expect(result.details.code).toBe("native_request_failed");
  expect(result.details.message).toBe(HOST_FAILURE.message);
});

test("home_resource_write refused with revision_conflict keeps its code and currentRevision", async () => {
  const key = __sessionTest.conversationKey(ctx())!;
  __sessionTest.attachments.set(key, {
    key, room: "probe", source: "selected", status: "attached", error: null,
    instanceId: "5b0c2a52-8c55-4b55-9a39-0a3a8f6f5e10", conversationId: "probe-session", sessionFile: "", workspace: directory,
    spirit: "Probe", operator: "Sol", homeDir: directory,
    context: { revision: "sha256:00", text: "You are Probe." }, controls: [],
    chatDelivery: false, restart: { supported: false, launcher: null }, heartbeatSecs: 10,
    terminal: null, terminalReason: null, launcher: null, launcherReason: null,
    exiting: false, connected: true,
  } as any);

  const result = await runTool("home_resource_write", { path: "notes.md", text: "new", expectedRevision: "sha256:old" });

  expect(result.isError).toBe(true);
  expect(result.details.code).toBe("revision_conflict");
  expect(result.details.message).toBe(CONFLICT.message);
  expect(result.details.currentRevision).toBe("sha256:feed");
});

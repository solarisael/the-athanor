import { afterEach, beforeEach, expect, spyOn, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { z } from "zod";
import { noteChatMessageStart, noteChatToolEnd, noteChatToolStart, startChatDoorman, stopChatDoorman } from "../house-proof/chat.ts";
import { registerTopLevelSession, retireTopLevelSession } from "../house-proof/top-level-session-fence.ts";
import { registerSolarisaelTools } from "../house-proof/tools.ts";
import { __sessionTest } from "../house-proof/session.ts";

// A Host failure reaches the tool result with its class, code, message and
// hops, on the organ path and on a direct sendHostCommand path (#57, #56 A1).
// A nested `{error: {code, message}}` session refusal keeps its code (#58).
// recall_policy and the chat doorman tell a refusal apart from an absent Host (#60).

const KEYS = ["ATHANOR_HOST_URL", "ATHANOR_HOST_TOKEN", "ATHANOR_HOST_HOUSE_ID", "ATHANOR_CLIENT_ROOM_DIR"] as const;
const HOP = { layer: "host", file: "crates/host/src/server/native_commands.rs", line: 8, op: "routing_status" };
const HOST_FAILURE = { class: "input_refused", code: "invalid_request", message: "the lane table is unreadable", hops: [HOP] };
const ORGAN_FAILURE = { class: "conflict", code: "knock_state_conflict", message: "the knock was already claimed", hops: [HOP] };
const CONFLICT = { code: "revision_conflict", message: "the document changed since it was read", currentRevision: "sha256:feed" };
const CHAT = { room: "probe", spirit: "Probe", session: "probe-session" };
// Tick refusals from the Host's command checks (crates/host/src/server.rs, validate_command).
const FOREIGN_BINDING = { class: "not_permitted", code: "foreign_binding", message: "command binding is foreign", hops: [HOP] };
const ROOM_STATE_UNREADABLE = { class: "configuration", code: "room_state_unavailable", message: "cannot read room state", hops: [HOP] };
const COMMAND_EXPIRED = { class: "deadline", code: "command_expired", message: "command has expired", hops: [HOP] };

let directory = "";
let saved: Record<string, string | undefined> = {};
let server: ReturnType<typeof Bun.serve>;
let commands: Array<Record<string, any>> = [];
// The failure every command except an organ call gets.
let hostFailure: Record<string, unknown> = HOST_FAILURE;
// The say the Host hands to chatNext. With none, chatNext gets hostFailure.
let offeredSay: Record<string, unknown> | null = null;
let warnings: ReturnType<typeof spyOn>;

beforeEach(() => {
  __sessionTest.reset();
  commands = [];
  hostFailure = HOST_FAILURE;
  offeredSay = null;
  warnings = spyOn(console, "warn").mockImplementation(() => {});
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
        commands.push(request);
        if (offeredSay && request.lifecycle_request?.action === "chatNext") {
          socket.send(JSON.stringify({
            correlation_id: request.message_id,
            command_or_event_type: "athanor.lifecycle.result",
            result: { next: offeredSay },
          }));
          return;
        }
        const organ = request.command_or_event_type === "athanor.organ.call";
        socket.send(JSON.stringify({
          correlation_id: request.message_id,
          command_or_event_type: organ ? "athanor.organ.result" : `athanor.${request.projection_id}.command_failed`,
          failure: organ ? ORGAN_FAILURE : hostFailure,
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
  stopChatDoorman(CHAT);
  retireTopLevelSession(CHAT.room, CHAT.session);
  warnings.mockRestore();
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

function sent(type: string) {
  return commands.filter((command) => command.command_or_event_type === type);
}

function chatWarnings(): string[] {
  return warnings.mock.calls.map((args) => String(args[0])).filter((line) => line.includes("Chat doorman"));
}

// The doorman's OMP side: the poll it registers and the says it sends.
function chatDoorman() {
  const says: any[] = [];
  let poll!: () => Promise<void>;
  const context = {
    ...ctx(),
    isIdle: () => true,
    setInterval: (callback: () => Promise<void>) => { poll = callback; return 1; },
    clearInterval() {},
  };
  startChatDoorman({ sendMessage: (message: any) => says.push({ ...message, role: "custom" }) }, context, CHAT);
  return { says, poll: () => poll() };
}

async function until(condition: () => boolean): Promise<void> {
  for (let waited = 0; !condition() && waited < 2_000; waited += 10) await Bun.sleep(10);
}

test("an organ failure reaches the tool with its class, code and hops", async () => {
  const result = await runTool("canon_read", { name: "Perseus" });

  expect(result.isError).toBe(true);
  expect(result.details.code).toBe("knock_state_conflict");
  expect(result.details.message).toBe(ORGAN_FAILURE.message);
  expect(result.details.retryable).toBe(false);
  expect(result.details.details.class).toBe("conflict");
  expect(result.details.details.hops).toEqual([HOP]);
});

test("a Host command_failed reaches a direct sendHostCommand tool with its class and code", async () => {
  const result = await runTool("house_lane_status", {});

  expect(result.isError).toBe(true);
  expect(result.details.code).toBe("invalid_request");
  expect(result.details.message).toBe(HOST_FAILURE.message);
  expect(result.details.details.class).toBe("input_refused");
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

test("recall_policy refused keeps the Host's class and code, and is not degraded", async () => {
  const result = await runTool("recall_policy", {});

  expect(result.isError).toBe(true);
  expect(result.details.code).toBe("invalid_request");
  expect(result.details.message).toBe(HOST_FAILURE.message);
  expect(result.details.details.class).toBe("input_refused");
  expect(result.details.degraded).toBeUndefined();
});

test("recall_policy with no Host to answer is degraded and keeps its code", async () => {
  server.stop(true);

  const result = await runTool("recall_policy", {});

  expect(result.isError).toBe(true);
  expect(result.details.degraded).toBe(true);
  expect(result.details.code).toBe("recall_policy_host_unavailable");
});

test("a doorman tick refused as a wrong call or binding stops it after one warning; a new doorman asks again", async () => {
  registerTopLevelSession(CHAT.room, CHAT.session);
  const wrongCall = chatDoorman();

  await wrongCall.poll();
  await wrongCall.poll();

  expect(sent("athanor.lifecycle.plan")).toHaveLength(1);
  expect(chatWarnings()).toEqual([expect.stringContaining("stopped until the next turn: the Host refused (invalid_request)")]);

  hostFailure = FOREIGN_BINDING;
  const wrongBinding = chatDoorman();
  await wrongBinding.poll();
  await wrongBinding.poll();

  expect(sent("athanor.lifecycle.plan")).toHaveLength(2);
  expect(chatWarnings()).toHaveLength(2);
  expect(chatWarnings()[1]).toContain("stopped until the next turn: the Host refused (foreign_binding)");
});

test("a doorman tick refusal that can clear keeps it asking, warns once per code, and the next answer delivers the say", async () => {
  registerTopLevelSession(CHAT.room, CHAT.session);
  const doorman = chatDoorman();

  hostFailure = ROOM_STATE_UNREADABLE;
  await doorman.poll();
  await doorman.poll();
  hostFailure = COMMAND_EXPIRED;
  await doorman.poll();
  offeredSay = { author: "operator", authorName: "Sol", turnId: "say-1", sequence: 1, text: "are you there?" };
  await doorman.poll();

  expect(sent("athanor.lifecycle.plan")).toHaveLength(4);
  expect(doorman.says.map((say) => say.details.sayId)).toEqual(["say-1"]);
  expect(chatWarnings()).toEqual([
    expect.stringContaining("asks again at the next poll: the Host refused (room_state_unavailable)"),
    expect.stringContaining("asks again at the next poll: the Host refused (command_expired)"),
  ]);
});

test("a refused chat draft warns once, and its say sends no more drafts", async () => {
  registerTopLevelSession(CHAT.room, CHAT.session);
  offeredSay = { author: "operator", authorName: "Sol", turnId: "say-1", sequence: 1, text: "how is the dragon doing?" };
  const doorman = chatDoorman();
  await doorman.poll();
  noteChatMessageStart(doorman.says[0]);

  noteChatToolStart({ toolCallId: "tool-1", toolName: "read" });
  await until(() => chatWarnings().length > 0);
  noteChatToolEnd({ toolCallId: "tool-1" });
  // A tool step reports at once, so a second draft would reach the Host within this wait.
  await Bun.sleep(200);

  expect(sent("athanor.chat.draft")).toHaveLength(1);
  expect(chatWarnings()).toEqual([expect.stringContaining("sends no more drafts for say say-1: the Host refused (invalid_request)")]);
});

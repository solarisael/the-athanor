// 2026-09-28: Sol wants the handoff in Pulse. A Pulse say reaches OMP as a
// custom message and never as `input`, so the boat door never saw `/handoff`
// from Pulse, and a boat turn ends on `sleep` with no text for the say. The
// doorman now hands such says to the door and answers them after the handoff.

import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

import { boatCast, installBoatDoor, resetBoatDoor } from "../house-proof/boat-door.ts";
import { noteChatMessageStart, noteChatToolStart, noteChatTurnEnd, startChatDoorman, stopChatDoorman } from "../house-proof/chat.ts";
import { registerTopLevelSession, retireTopLevelSession } from "../house-proof/top-level-session-fence.ts";

const ROOM = "pulse-door-room";
const SESSION = "0199b000-0000-7000-8000-000000000001";
const binding = { room: ROOM, spirit: "Kodo", session: SESSION };

type Handler = (event: any, ctx: any) => unknown;

const ENV = ["ATHANOR_HOST_URL", "ATHANOR_HOST_TOKEN", "ATHANOR_HOST_HOUSE_ID"];
const savedEnv: Record<string, string | undefined> = {};

let host: ReturnType<typeof fakeHost>;

// Native policy is scripted here; these cases exercise the OMP handoff sequence.
function fakeHost() {
  const lines: Array<Record<string, any>> = [];
  const commands: Array<Record<string, any>> = [];
  let next: Record<string, unknown> | null = null;
  const answers: Array<{ text: string; thinking: string[]; outcome: string } | null> = [];
  const server = Bun.serve({
    port: 0,
    hostname: "127.0.0.1",
    fetch(request, server) {
      if (server.upgrade(request)) return;
      return new Response("not a websocket", { status: 400 });
    },
    websocket: {
      message(socket, data) {
        const command = JSON.parse(String(data));
        commands.push(command);
        const type = String(command.command_or_event_type ?? "");
        const reply = (kind: string, extra: Record<string, unknown> = {}) =>
          socket.send(JSON.stringify({ correlation_id: command.message_id, command_or_event_type: kind, ...extra }));
        if (type === "athanor.chat.subscribe") return reply("athanor.chat.snapshot", { messages: lines });
        if (type === "athanor.lifecycle.plan") {
          const request = command.lifecycle_request;
          if (request.action === "chatNext") {
            return reply("athanor.lifecycle.result", { result: { next } });
          }
          if (request.action === "chatOutcome") {
            if (answers.length === 0) return reply(`${type}.command_refused`, { reason: "unscripted outcome" });
            return reply("athanor.lifecycle.result", { result: { answer: answers.shift() } });
          }
        }
        if (type === "athanor.chat.turn") {
          lines.push({ author: "spirit", ...command.chat_turn });
          if (next?.turnId === command.chat_turn.turnId) next = null;
          return reply("athanor.chat.command_accepted");
        }
        if (type === "athanor.chat.draft") return reply("athanor.chat.command_accepted");
        reply(`${type}.command_refused`, { reason: "not answered by this test" });
      },
    },
  });
  process.env.ATHANOR_HOST_URL = `ws://127.0.0.1:${server.port}`;
  return {
    server,
    say: (turnId: string, text: string) => {
      next = { author: "operator", authorName: "Sol", turnId, sequence: lines.length + 1, text };
      lines.push(next);
    },
    answer: (text: string | null) => answers.push(text === null ? null : { text, thinking: [], outcome: "complete" }),
    turns: () => commands.filter((c) => c.command_or_event_type === "athanor.chat.turn").map((c) => c.chat_turn),
  };
}

beforeEach(() => {
  for (const key of ENV) savedEnv[key] = process.env[key];
  process.env.ATHANOR_HOST_TOKEN = "test-token";
  process.env.ATHANOR_HOST_HOUSE_ID = "solarisael";
  registerTopLevelSession(ROOM, SESSION);
  host = fakeHost();
});

afterEach(() => {
  stopChatDoorman(binding);
  resetBoatDoor();
  host.server.stop(true);
  retireTopLevelSession(ROOM, SESSION);
  for (const [key, value] of Object.entries(savedEnv)) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
});

// One fake OMP: the door and the doorman share its sendMessage, as in the adapter.
function omp(options: { over?: number; handoff?: () => Promise<unknown> } = {}) {
  const handlers = new Map<string, Handler[]>();
  const sent: any[] = [];
  const handoffs: Array<string | undefined> = [];
  const pi = {
    on(name: string, handler: Handler) { handlers.set(name, [...(handlers.get(name) ?? []), handler]); },
    sendMessage(message: any) { sent.push({ ...message, role: "custom" }); },
    sendUserMessage() {},
  };
  const session = {
    isStreaming: false,
    isCompacting: false,
    agent: { abort() {} },
    waitForIdle: async () => {},
    handoff: options.handoff ?? (async (focus?: string) => { handoffs.push(focus); return { document: "doc" }; }),
  };
  const ctx = {
    sessionManager: { getSessionId: () => SESSION },
    ui: { notify() {} },
    isIdle: () => true,
    setInterval: (callback: () => Promise<void>) => { poll = callback; return 1; },
    clearInterval() {},
  };
  let poll!: () => Promise<void>;
  installBoatDoor(pi, {
    session: () => session,
    isTopLevel: () => true,
    tokensOverBoatLine: () => options.over ?? -1,
    room: () => ({ dir: mkdtempSync(path.join(tmpdir(), "pulse-door-")), spirit: "Kodo" }),
  });
  startChatDoorman(pi, ctx, binding);

  // agent_end reaches the door and the doorman, as the adapter wires both.
  const agentEnd = async (messages: any[]) => {
    for (const handler of handlers.get("agent_end") ?? []) await handler({ type: "agent_end", messages }, ctx);
    await noteChatTurnEnd(binding, { messages });
  };
  return { ctx, sent, handoffs, poll: () => poll(), agentEnd };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 20));
const reply = (text: string) => ({ role: "assistant", stopReason: "stop", content: [{ type: "text", text }] });
const sleepCall = { role: "assistant", stopReason: "toolUse", content: [{ type: "toolCall", id: "t-sleep", name: "write" }] };

test("/handoff from Pulse casts the boat, hands off, and the turn after answers the say", async () => {
  const { ctx, sent, handoffs, poll, agentEnd } = omp();
  host.say("say-1", "/handoff keep the Pulse thread");
  await poll();

  expect(sent.map((m) => m.customType)).toEqual(["athanor-boat-before-handoff"]);
  expect(sent[0].details).toEqual({ sayId: "say-1" });
  await poll();
  expect(sent).toHaveLength(1);

  // The boat turn: its tool shows on the say's draft, and it answers nothing.
  noteChatMessageStart(sent[0]);
  noteChatToolStart({ toolCallId: "t-sleep", toolName: "write", intent: "Casting the paper boat" });
  boatCast(ctx);
  host.answer(null);
  await agentEnd([sent[0], sleepCall]);
  await settle();
  expect(handoffs).toEqual(["keep the Pulse thread"]);
  expect(host.turns()).toEqual([]);

  expect(sent[1].customType).toBe("athanor-after-handoff");
  expect(sent[1].details).toEqual({ sayId: "say-1" });
  host.answer("hi solzinho, I'm back");
  await agentEnd([sent[1], reply("hi solzinho, I'm back")]);
  expect(host.turns()).toEqual([expect.objectContaining({
    turnId: "say-1",
    text: "hi solzinho, I'm back",
    outcome: "complete",
    steps: [expect.objectContaining({ tool: "write", summary: "Casting the paper boat" })],
  })]);
});

test("a Pulse say over the boat line waits for the boat and the handoff, then is answered itself", async () => {
  const { ctx, sent, handoffs, poll, agentEnd } = omp({ over: 500 });
  host.say("say-2", "how is the dragon doing?");
  await poll();
  expect(sent.map((m) => m.customType)).toEqual(["athanor-boat-before-handoff"]);

  boatCast(ctx);
  host.answer(null);
  await agentEnd([sent[0], sleepCall]);
  await settle();
  expect(handoffs).toEqual([undefined]);
  expect(sent.map((m) => m.customType)).toEqual(["athanor-boat-before-handoff", "athanor-chat-say"]);
  expect(sent[1].content).toContain("how is the dragon doing?");

  host.answer("doing great");
  await agentEnd([sent[1], reply("doing great")]);
  expect(host.turns().map((turn: any) => [turn.turnId, turn.text])).toEqual([["say-2", "doing great"]]);
});

test("a Pulse /handoff that ends without a boat or a handoff is Cancelled, not left pending", async () => {
  const noBoat = omp();
  host.say("say-3", "/handoff");
  await noBoat.poll();
  host.answer(null);
  await noBoat.agentEnd([noBoat.sent[0], reply("oops, I talked instead")]);
  await settle();
  expect(host.turns()).toEqual([expect.objectContaining({ turnId: "say-3", text: "", outcome: "aborted" })]);

  stopChatDoorman(binding);
  resetBoatDoor();
  const failed = omp({ handoff: async () => { throw new Error("provider down"); } });
  host.say("say-4", "/handoff");
  await failed.poll();
  boatCast(failed.ctx);
  host.answer(null);
  await failed.agentEnd([failed.sent[0], sleepCall]);
  await settle();
  expect(host.turns().at(-1)).toEqual(expect.objectContaining({ turnId: "say-4", text: "", outcome: "aborted" }));
});

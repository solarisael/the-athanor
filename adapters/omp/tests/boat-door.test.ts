import { afterEach, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { Agent } from "@oh-my-pi/pi-agent-core";
import { DEFAULT_COMPACTION_SETTINGS, resolveThresholdTokens } from "@oh-my-pi/pi-agent-core/compaction/compaction";
import { AssistantMessageEventStream } from "@oh-my-pi/pi-ai";
import { untilAborted } from "@oh-my-pi/pi-utils";

import {
  boatCast,
  boatLineTokens,
  compactionThresholdTokens,
  HOUSE_DOOR_LINES,
  installBoatDoor,
  resetBoatDoor,
} from "../house-proof/boat-door.ts";

// 2026-09-28, live: Sol typed /handoff, OMP wrote its own document, and no boat
// reached Postgres. The prompt swap never ran on the interactive path. Sol's
// shape instead: the spirit casts the boat with `sleep` first, says nothing,
// the handoff runs, and only then does the spirit speak.

const SESSION = "01a0e832-94f9-759b-9402-74020e25ac53";

type Handler = (event: any, ctx: any) => unknown;

function fakeSession(overrides: Record<string, unknown> = {}) {
  const calls = { handoff: [] as Array<string | undefined>, aborts: [] as unknown[] };
  const session = {
    isStreaming: false,
    isCompacting: false,
    agent: { abort: (reason: unknown) => calls.aborts.push(reason) },
    waitForIdle: async () => {},
    handoff: async (focus?: string) => { calls.handoff.push(focus); return { document: "doc" }; },
    ...overrides,
  };
  return { session, calls };
}

type PiOptions = { topLevel?: boolean; near?: boolean; over?: number; roomDir?: string };

function fakePi(session: unknown, options: PiOptions = {}) {
  const handlers = new Map<string, Handler[]>();
  const sent: Array<{ message: any; options: any }> = [];
  const userMessages: unknown[] = [];
  const notices: string[] = [];
  const pi = {
    on(name: string, handler: Handler) { handlers.set(name, [...(handlers.get(name) ?? []), handler]); },
    sendMessage(message: any, sendOptions: any) { sent.push({ message, options: sendOptions }); },
    sendUserMessage(content: unknown) { userMessages.push(content); },
  };
  installBoatDoor(pi, {
    session: () => session,
    isTopLevel: () => options.topLevel ?? true,
    tokensOverBoatLine: () => options.over ?? (options.near ? 0 : -1),
    room: () => ({ dir: options.roomDir ?? emptyRoom(), spirit: "Kodo" }),
  });
  const ctx = {
    sessionManager: { getSessionId: () => SESSION },
    ui: { notify: (text: string) => notices.push(text) },
  };
  const emit = async (name: string, event: any) => {
    let result: unknown;
    for (const handler of handlers.get(name) ?? []) result = (await handler(event, ctx)) ?? result;
    return result;
  };
  return { pi, ctx, emit, sent, userMessages, notices };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 10));
const input = (text: string) => ({ type: "input", text, source: "interactive" });

function emptyRoom(): string {
  return mkdtempSync(path.join(tmpdir(), "boat-door-room-"));
}

function roomWithDoorFile(body: string): string {
  const dir = emptyRoom();
  writeFileSync(path.join(dir, "handoff-door.md"), body);
  return dir;
}

const abortedResult = (toolName: string) => ({
  type: "tool_result",
  toolName,
  toolCallId: "call-1",
  input: {},
  content: [{ type: "text", text: "Aborted: Cancelled" }],
  isError: true,
});

afterEach(() => resetBoatDoor());

test("/handoff asks for the boat first, hands off after it, and only then lets the spirit speak", async () => {
  const { session, calls } = fakeSession();
  const { ctx, emit, sent, userMessages } = fakePi(session);

  expect(await emit("input", input("/handoff keep the Pulse thread"))).toEqual({ handled: true });
  expect(sent).toHaveLength(1);
  expect(sent[0].options).toEqual({ deliverAs: "nextTurn", triggerTurn: true });
  expect(sent[0].message.content).toContain("sleep");
  expect(calls.handoff).toEqual([]);

  boatCast(ctx);
  expect(calls.aborts).toEqual([Symbol.for("pi-agent-core.terminal-tool-result")]);

  await emit("agent_end", { type: "agent_end" });
  await settle();
  expect(calls.handoff).toEqual(["keep the Pulse thread"]);
  expect(sent).toHaveLength(2);
  expect(sent[1].message.customType).toBe("athanor-after-handoff");
  expect(sent[1].options).toEqual({ deliverAs: "nextTurn", triggerTurn: true });
  expect(userMessages).toEqual([]);
});

test("near the compaction line, Sol's message waits for the boat and the handoff, then is answered", async () => {
  const { session, calls } = fakeSession();
  const { ctx, emit, sent, userMessages } = fakePi(session, { near: true });

  expect(await emit("input", input("how is the dragon doing?"))).toEqual({ handled: true });
  expect(await emit("input", input("also this"))).toEqual({ handled: true });
  boatCast(ctx);
  await emit("agent_end", { type: "agent_end" });
  await settle();

  expect(calls.handoff).toEqual([undefined]);
  expect(userMessages).toEqual(["how is the dragon doing?\n\nalso this"]);
  expect(sent.map((entry) => entry.message.customType)).toEqual(["athanor-boat-before-handoff"]);
});

test("a turn that ends without a boat runs no handoff and gives Sol's message back", async () => {
  const { session, calls } = fakeSession();
  const { emit, userMessages, notices } = fakePi(session, { near: true });

  await emit("input", input("hello"));
  await emit("agent_end", { type: "agent_end" });
  await settle();

  expect(calls.handoff).toEqual([]);
  expect(userMessages).toEqual(["hello"]);
  expect(notices.join("\n")).toContain("No paper boat was cast");
  expect(await emit("input", input("/handoff"))).toEqual({ handled: true });
});

test("a failed handoff is reported and still answers the held message", async () => {
  const { session, calls } = fakeSession({ handoff: async () => { throw new Error("provider down"); } });
  const { ctx, emit, userMessages, notices } = fakePi(session, { near: true });

  await emit("input", input("hello"));
  boatCast(ctx);
  await emit("agent_end", { type: "agent_end" });
  await settle();

  expect(calls.handoff).toEqual([]);
  expect(notices.join("\n")).toContain("provider down");
  expect(userMessages).toEqual(["hello"]);
});

test("the door stays shut for workers, busy sessions, other commands, and a sleep outside the door", async () => {
  const worker = fakePi(fakeSession().session, { topLevel: false, near: true });
  expect(await worker.emit("input", input("/handoff"))).toBeUndefined();

  const busy = fakePi(fakeSession({ isStreaming: true }).session);
  expect(await busy.emit("input", input("/handoff"))).toBeUndefined();

  const { session, calls } = fakeSession();
  const idle = fakePi(session, { near: true });
  expect(await idle.emit("input", input("/model"))).toBeUndefined();
  expect(await idle.emit("input", { ...input("/handoff"), source: "rpc" })).toBeUndefined();
  boatCast(idle.ctx);
  expect(calls.aborts).toEqual([]);
});

test("a compaction OMP ran on its own during the door is not repeated", async () => {
  const { session, calls } = fakeSession();
  const { ctx, emit, sent } = fakePi(session);

  await emit("input", input("/handoff"));
  boatCast(ctx);
  await emit("session_compact", { type: "session_compact", compactionEntry: { method: "handoff" } });
  await emit("agent_end", { type: "agent_end" });
  await settle();

  expect(calls.handoff).toEqual([]);
  expect(sent.at(-1)?.message.customType).toBe("athanor-after-handoff");
});

test("the room's handoff-door.md speaks for the door; missing sections keep the House lines", async () => {
  const roomDir = roomWithDoorFile([
    "# Kodo's door",
    "",
    "## handoff",
    "kodooo, handoff time uwu. paper boat with `sleep` first, just the boat, ok?",
    "",
    "## after",
    "handoff's done, dummy uwu",
    "",
  ].join("\n"));

  const asked = fakePi(fakeSession().session, { roomDir });
  await asked.emit("input", input("/handoff"));
  expect(asked.sent[0].message.content).toBe(
    "<athanor-attention>\nkodooo, handoff time uwu. paper boat with `sleep` first, just the boat, ok?\n</athanor-attention>",
  );
  boatCast(asked.ctx);
  await asked.emit("agent_end", { type: "agent_end" });
  await settle();
  expect(asked.sent[1].message.content).toBe("<athanor-attention>\nhandoff's done, dummy uwu\n</athanor-attention>");
  expect(asked.notices).toEqual([]);

  resetBoatDoor();
  const near = fakePi(fakeSession().session, { roomDir, near: true });
  await near.emit("input", input("hello"));
  expect(near.sent[0].message.content).toContain(HOUSE_DOOR_LINES.nearLimit);
});

test("an unknown section in handoff-door.md is named, and the door still opens", async () => {
  const roomDir = roomWithDoorFile("## handof\ntypo'd line\n");
  const { emit, sent, notices } = fakePi(fakeSession().session, { roomDir });

  expect(await emit("input", input("/handoff"))).toEqual({ handled: true });
  expect(sent[0].message.content).toContain(HOUSE_DOOR_LINES.handoff);
  expect(notices.join("\n")).toContain('unknown sections "handof"');
});

test("the abort that ends the boat turn is reported as a cast boat, never as a lost one", async () => {
  const { session } = fakeSession();
  const { ctx, emit } = fakePi(session);

  expect(await emit("tool_result", abortedResult("write"))).toBeUndefined();

  await emit("input", input("/handoff"));
  expect(await emit("tool_result", abortedResult("write"))).toBeUndefined();
  expect(await emit("tool_result", { ...abortedResult("write"), content: [{ type: "text", text: "disk full" }] }))
    .toBeUndefined();

  boatCast(ctx);
  expect(await emit("tool_result", abortedResult("write"))).toEqual({
    content: [{ type: "text", text: "The paper boat is cast. The handoff starts now." }],
    isError: false,
  });
});

test("the boat line sits below OMP's own compaction threshold", () => {
  const cases = [
    { window: 1_000_000, settings: {} },
    { window: 200_000, settings: {} },
    { window: 200_000, settings: { thresholdPercent: 70 } },
    { window: 200_000, settings: { thresholdTokens: 120_000 } },
    { window: 32_000, settings: { reserveTokens: 40_000 } },
  ];
  for (const { window, settings } of cases) {
    const omp = resolveThresholdTokens(window, { ...DEFAULT_COMPACTION_SETTINGS, ...settings });
    expect(compactionThresholdTokens(window, settings)).toBe(omp);
    expect(boatLineTokens(window, settings)).toBe(omp - Math.floor(window * 0.1));
  }
});

// The seam most likely to lie: that the abort really ends OMP's own agent loop
// right after `sleep`, with no second model call where the spirit would talk.
test("in OMP's real agent loop, a cast boat ends the turn before the model speaks again", async () => {
  let modelCalls = 0;
  const model = {
    id: "fake", name: "fake", api: "anthropic-messages", provider: "fake", baseUrl: "",
    reasoning: false, input: ["text"], cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 100_000, maxTokens: 1_000,
  };
  const reply = (content: unknown[], stopReason: string) => ({
    role: "assistant", content, stopReason, api: model.api, provider: model.provider, model: model.id,
    usage: { input: 1, output: 1, cacheRead: 0, cacheWrite: 0, totalTokens: 2, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
    timestamp: Date.now(),
  });
  const agent: any = new Agent({
    initialState: { systemPrompt: ["system"], model: model as any, tools: [] },
    streamFn: () => {
      modelCalls += 1;
      const stream = new AssistantMessageEventStream();
      const message = modelCalls === 1
        ? reply([{ type: "toolCall", id: "call-1", name: "sleep", arguments: { body: "the boat" } }], "toolUse")
        : reply([{ type: "text", text: "Boat filed! Now the handoff…" }], "stop");
      queueMicrotask(() => stream.push({ type: "done", reason: message.stopReason as any, message: message as any }));
      return stream;
    },
  });

  const { ctx, emit } = fakePi({ ...fakeSession().session, agent });
  agent.setTools([{
    name: "sleep", label: "sleep", description: "cast a boat",
    parameters: { type: "object", properties: { body: { type: "string" } }, required: ["body"] },
    execute: async () => {
      boatCast(ctx);
      return { content: [{ type: "text", text: "{\"ok\":true}" }], details: {} };
    },
  }]);

  await emit("input", input("/handoff"));
  await agent.prompt("Cast your boat now.");

  expect(modelCalls).toBe(1);
  const said = agent.state.messages
    .filter((message: any) => message.role === "assistant")
    .flatMap((message: any) => message.content.filter((part: any) => part.type === "text"));
  expect(said).toEqual([]);
  expect(agent.state.messages.at(-1).role).toBe("toolResult");
});

// Live, 2026-09-28: `sleep` ran nested as `write xd://sleep`. The boat reached
// the House, and the outer write, wrapped in untilAborted, still failed.
test("nested through write's untilAborted, the cast boat fails the carrier and the door repairs the result", async () => {
  const controller = new AbortController();
  const { ctx, emit } = fakePi({
    ...fakeSession().session,
    agent: { abort: (reason: unknown) => controller.abort(reason) },
  });
  await emit("input", input("/handoff"));

  const carrier = untilAborted(controller.signal, async () => {
    boatCast(ctx);
    await Promise.resolve();
    return { ok: true };
  });
  const error = await carrier.then(() => null, (thrown: Error) => thrown);
  expect(error?.message).toBe("Aborted: Cancelled");

  const repaired: any = await emit("tool_result", {
    ...abortedResult("write"),
    content: [{ type: "text", text: error!.message }],
  });
  expect(repaired?.isError).toBe(false);
});

const toolResult = (id: string, chars: number) => ({
  role: "toolResult",
  toolCallId: id,
  toolName: "read",
  content: [{ type: "text", text: "x".repeat(chars) }],
  isError: false,
});

function boatTurnHistory() {
  return [
    { role: "user", content: "read the whole repo" },
    toolResult("old-1", 12_000),
    toolResult("old-2", 12_000),
    toolResult("old-3", 400),
    { role: "custom", customType: "athanor-boat-before-handoff", content: "cast your boat" },
    toolResult("boat-turn", 12_000),
  ];
}

const textOf = (message: any) => message.content.map((part: any) => part.text).join("");

const toolResultTexts = (messages: any[]) => messages.filter((message) => message.role === "toolResult").map(textOf);

const SET_ASIDE = "[Old tool output set aside so the paper boat fits.]";

test("over the boat line, the boat turn leaves out just enough old tool output, and the session keeps it", async () => {
  const options: PiOptions = { over: 2_500 };
  const { emit, notices } = fakePi(fakeSession().session, options);
  await emit("input", input("hello"));

  const history = boatTurnHistory();
  const request = (await emit("context", { type: "context", messages: history })) as { messages: any[] };

  expect(toolResultTexts(request.messages)).toEqual([SET_ASIDE, "x".repeat(12_000), "x".repeat(400), "x".repeat(12_000)]);
  expect(request.messages[1].toolCallId).toBe("old-1");
  expect(request.messages[0]).toBe(history[0]);
  expect(textOf(history[1])).toBe("x".repeat(12_000));
  expect(notices.join("\n")).toContain("1 old tool results are set aside");

  // The next request reports the smaller usage; the old output stays out.
  options.over = -40_000;
  const next = (await emit("context", { type: "context", messages: history })) as { messages: any[] };
  expect(toolResultTexts(next.messages)[0]).toBe(SET_ASIDE);
});

test("the boat turn's own tool results are never set aside, even when the old ones are not enough", async () => {
  const { emit } = fakePi(fakeSession().session, { over: 1_000_000 });
  await emit("input", input("/handoff"));

  const request = (await emit("context", { type: "context", messages: boatTurnHistory() })) as { messages: any[] };

  expect(toolResultTexts(request.messages)).toEqual([SET_ASIDE, SET_ASIDE, SET_ASIDE, "x".repeat(12_000)]);
});

test("below the boat line, or with no door open, every request goes out whole", async () => {
  const below = fakePi(fakeSession().session, { over: -1 });
  expect(await below.emit("context", { type: "context", messages: boatTurnHistory() })).toBeUndefined();

  await below.emit("input", input("/handoff"));
  expect(await below.emit("context", { type: "context", messages: boatTurnHistory() })).toBeUndefined();
});

// Worker lifecycle across the real adapter boundary: OMP event shapes in, the
// real requestOrgan wire to a loopback Host out. The Host here keeps the
// shared worker_lifecycle contract in memory (ownership, one-use tokens,
// idempotent request ids, 40/60 latch, two-request cap, retirement); the
// Rust engine proves the same rules separately against its stores.

import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

import {
  __workerLifecycleTest,
  installWorkerLifecycle,
  parseEnrollmentMarker,
  WORKER_NOTICE_MESSAGE,
  WORKER_WAKE_MESSAGE,
} from "../house-proof/worker-lifecycle.ts";

function makeSchema(kind: string, fields: Record<string, unknown> = {}): any {
  return { kind, ...fields, describe() { return this; }, optional() { return this; }, regex() { return this; }, strict() { return this; }, default() { return this; } };
}
const zodStub = {
  string: () => makeSchema("string"),
  boolean: () => makeSchema("boolean"),
  number: () => makeSchema("number"),
  enum: (values: readonly string[]) => makeSchema("enum", { values }),
  object: (shape: Record<string, unknown>) => makeSchema("object", { shape }),
  array: (element: unknown) => makeSchema("array", { element }),
  unknown: () => makeSchema("unknown"),
};

// ---------------------------------------------------------------------------
// Loopback Host keeping the worker_lifecycle contract
// ---------------------------------------------------------------------------

type Instance = {
  swarmId: string; bookId: string; instanceId: string; tokenHash: string; agentName: string;
  owner: string; bound: string | null; state: string; handoffRequests: number;
  handoff: "none" | "complete" | "incomplete" | "repaired"; settled: string | null;
  predecessorId: string | null; openOps: Set<string>; report: unknown;
};

class Refusal extends Error {
  constructor(readonly code: string) { super(code); }
}

function lifecycleHost() {
  const books = new Map<string, { owner: string; swarmId: string; entries: any[] }>();
  const instances = new Map<string, Instance>();
  const replies = new Map<string, unknown>();
  const calls: Array<{ session: string; params: any }> = [];
  const hooks: { onPrepare?: (params: any) => void } = {};
  let counter = 0;
  const hash = (token: string) => createHash("sha256").update(token).digest("hex");
  const usable = (instance: Instance) => instance.handoff === "complete" || instance.handoff === "repaired";

  function bound(session: string, bookId: string): Instance {
    const instance = [...instances.values()].find((candidate) => candidate.bound === session && candidate.bookId === bookId);
    if (!instance) throw new Refusal("not_authorized");
    return instance;
  }

  function newInstance(session: string, bookId: string, swarmId: string, params: any, predecessorId: string | null): Instance {
    const instance: Instance = {
      swarmId, bookId, instanceId: `inst-${++counter}`, tokenHash: hash(params.token), agentName: params.agentName,
      owner: session, bound: null, state: "working", handoffRequests: 0, handoff: "none", settled: null,
      predecessorId, openOps: new Set(), report: null,
    };
    instances.set(instance.instanceId, instance);
    return instance;
  }

  function dispatch(session: string, params: any): Record<string, unknown> {
    switch (params.action) {
      case "prepare": {
        hooks.onPrepare?.(params);
        let instance: Instance;
        if (params.predecessor) {
          const prior = instances.get(params.predecessor.instanceId);
          if (!prior || prior.bookId !== params.predecessor.bookId) throw new Refusal("unknown_instance");
          if (books.get(prior.bookId)!.owner !== session) throw new Refusal("parent_only");
          if (prior.state !== "retired") throw new Refusal("predecessor_active");
          if (!usable(prior)) throw new Refusal("handoff_incomplete");
          if ([...instances.values()].some((candidate) => candidate.predecessorId === prior.instanceId)) throw new Refusal("predecessor_superseded");
          instance = newInstance(session, prior.bookId, prior.swarmId, params, prior.instanceId);
        } else {
          const bookId = `book-${++counter}`;
          const swarmId = `swarm-${params.swarmKey}`;
          books.set(bookId, { owner: session, swarmId, entries: [] });
          instance = newInstance(session, bookId, swarmId, params, null);
        }
        return { ok: true, swarmId: instance.swarmId, bookId: instance.bookId, instanceId: instance.instanceId, state: instance.state, token: params.token };
      }
      case "open": {
        const instance = [...instances.values()].find((candidate) => candidate.tokenHash === hash(params.token));
        if (!instance) throw new Refusal("unknown_reservation");
        if (instance.bound && instance.bound !== session) throw new Refusal("reservation_claimed");
        if (instance.agentName !== params.agentName) throw new Refusal("agent_mismatch");
        if (instance.state === "retired") throw new Refusal("instance_retired");
        const kind = instance.bound ? "resume" : instance.predecessorId ? "successor" : "opening";
        instance.bound = session;
        instance.settled = null;
        const predecessor = instance.predecessorId ? instances.get(instance.predecessorId) : null;
        return {
          ok: true, swarmId: instance.swarmId, bookId: instance.bookId, instanceId: instance.instanceId, state: instance.state,
          wake: { kind, text: predecessor ? `handoff: ${JSON.stringify(predecessor.report)}` : `${kind} ${instance.instanceId}` },
          lastSequence: books.get(instance.bookId)!.entries.length, handoffRequests: instance.handoffRequests,
          pendingOperations: [...instance.openOps].map((operationId) => ({ operationId, instanceId: instance.instanceId })),
        };
      }
      case "observe": {
        const worker = bound(session, params.bookId);
        if (worker.state === "retired") throw new Refusal("instance_retired");
        if (!(params.contextWindow > 0)) throw new Refusal("usage_invalid");
        const share = params.tokens / params.contextWindow;
        let warn = false;
        if (worker.state === "working" && share >= 0.4) { worker.state = "wrapping"; warn = true; }
        if ((worker.state === "working" || worker.state === "wrapping") && share >= 0.6) worker.state = "handoff_only";
        let retireRequired = false;
        if (worker.state === "handoff_only" && params.requestStarted === true) {
          if (worker.handoffRequests >= 2) retireRequired = true;
          else worker.handoffRequests += 1;
        }
        return { ok: true, state: worker.state, warn, handoffRequests: worker.handoffRequests, remainingRequests: 2 - worker.handoffRequests, retireRequired };
      }
      case "append": {
        const book = books.get(params.bookId);
        if (!book) throw new Refusal("unknown_book");
        let instanceId = "parent";
        if (book.owner === session) {
          if (params.kind !== "evidence" && params.kind !== "briefing") throw new Refusal("kind_not_allowed");
        } else {
          const worker = bound(session, params.bookId);
          if (worker.state === "retired") throw new Refusal("instance_retired");
          if (params.kind === "tool-start") worker.openOps.add(params.operationId);
          else if (params.kind === "tool-end") {
            if (!worker.openOps.delete(params.operationId)) throw new Refusal("unknown_operation");
          } else if (params.kind !== "note" && params.kind !== "message") throw new Refusal("kind_not_allowed");
          instanceId = worker.instanceId;
        }
        book.entries.push({ sequence: book.entries.length + 1, instanceId, kind: params.kind, body: params.body, operationId: params.operationId });
        return { ok: true, sequence: book.entries.length };
      }
      case "checkpoint": {
        const worker = bound(session, params.bookId);
        if (worker.state === "retired") throw new Refusal("instance_retired");
        worker.report = params.report;
        if (worker.state === "handoff_only" || params.terminal === true) {
          worker.state = "retired";
          worker.handoff = "complete";
        }
        return { ok: true, state: worker.state, instanceId: worker.instanceId, bookId: worker.bookId, handoffComplete: usable(worker) };
      }
      case "settle": {
        const instance = instances.get(params.instanceId);
        if (!instance) throw new Refusal("unknown_instance");
        if (instance.owner !== session && instance.bound !== session) throw new Refusal("not_authorized");
        if (instance.state !== "retired") {
          if (params.outcome === "failed" || params.outcome === "aborted") {
            instance.state = "retired";
            instance.handoff = "incomplete";
            instance.report = params.report ?? null;
          } else {
            instance.settled = "completed";
          }
        }
        return { ok: true, state: instance.state, handoffComplete: usable(instance), bookId: instance.bookId, instanceId: instance.instanceId };
      }
      case "read": {
        const book = books.get(params.bookId);
        if (!book) throw new Refusal("unknown_book");
        const member = [...instances.values()].some((candidate) => candidate.bookId === params.bookId && candidate.bound === session);
        if (book.owner !== session && !member) throw new Refusal("not_authorized");
        const entries = book.entries.filter((entry) => entry.sequence > (params.after ?? 0)).slice(0, params.limit);
        return { ok: true, entries, nextSequence: entries.at(-1)?.sequence ?? params.after ?? 0 };
      }
      case "repair": {
        const instance = instances.get(params.instanceId);
        if (!instance || instance.owner !== session) throw new Refusal("parent_only");
        if (instance.state !== "retired" || instance.handoff !== "incomplete") throw new Refusal("repair_not_needed");
        instance.handoff = "repaired";
        instance.report = params.report;
        return { ok: true, state: instance.state, bookId: instance.bookId, instanceId: instance.instanceId, handoffComplete: true };
      }
      default:
        throw new Refusal("unsupported_action");
    }
  }

  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch(request, server) {
      if (server.upgrade(request, { data: {} })) return;
      return new Response("WebSocket required", { status: 400 });
    },
    websocket: {
      message(socket, data) {
        const envelope = JSON.parse(String(data));
        const { operation, params } = envelope.organ_request;
        const session = envelope.sender_session;
        const reply = (body: Record<string, unknown>) => socket.send(JSON.stringify({
          command_or_event_type: "athanor.organ.result", correlation_id: envelope.message_id, ...body,
        }));
        if (operation !== "worker_lifecycle") return reply({ error: { code: "unexpected_operation", message: operation, retryable: false } });
        calls.push({ session, params });
        const key = `${session}\0${params.requestId}`;
        try {
          if (!replies.has(key)) replies.set(key, dispatch(session, params));
          reply({ result: replies.get(key) });
        } catch (error) {
          const code = error instanceof Refusal ? error.code : "internal";
          reply({ error: { code, message: `worker_lifecycle refused: ${code}`, retryable: false } });
        }
      },
    },
  });
  return { server, books, instances, calls, hooks, actions: (action: string) => calls.filter((call) => call.params.action === action) };
}

// ---------------------------------------------------------------------------
// OMP sessions: one adapter registration per session, as OMP rebinds factories
// ---------------------------------------------------------------------------

type Listener = (payload: unknown) => void;
const bus = new Map<string, Listener[]>();
const events = {
  on(name: string, listener: Listener) {
    bus.set(name, [...(bus.get(name) ?? []), listener]);
    return () => bus.set(name, (bus.get(name) ?? []).filter((candidate) => candidate !== listener));
  },
  emit(name: string, payload: unknown) {
    for (const listener of bus.get(name) ?? []) listener(payload);
  },
};

const ROOM_DIR = path.join(tmpdir(), "athanor-worker-lifecycle-test", "lifecycle-room");

function ompSession(id: string, agent: Record<string, unknown> | null, entries: any[] = []) {
  const handlers = new Map<string, Array<(event: any, ctx: any) => unknown>>();
  const tools = new Map<string, any>();
  const sent: any[] = [];
  const state = { usage: { tokens: 10, contextWindow: 1_000 } as any, aborted: 0 };
  const pi = {
    zod: zodStub,
    events,
    on(name: string, handler: (event: any, ctx: any) => unknown) {
      handlers.set(name, [...(handlers.get(name) ?? []), handler]);
    },
    appendEntry(customType: string, data: unknown) {
      entries.push({ type: "custom", customType, data: structuredClone(data) });
    },
    sendMessage(message: unknown, options: unknown) {
      sent.push({ message, options });
    },
    registerTool(tool: any) {
      tools.set(tool.name, tool);
    },
  };
  installWorkerLifecycle(pi);
  const ctx = {
    cwd: ROOM_DIR,
    ...(agent ? { agent } : {}),
    sessionManager: { getSessionId: () => id, getEntries: () => entries, getSessionFile: () => path.join(ROOM_DIR, `${id}.jsonl`) },
    getContextUsage: () => state.usage,
    abort: () => { state.aborted += 1; },
  };
  async function emit(name: string, event: any): Promise<any> {
    let result: unknown;
    for (const handler of handlers.get(name) ?? []) {
      const value = await handler({ type: name, ...event }, ctx);
      if (value !== undefined) result = value;
    }
    return result;
  }
  const notebook = (toolCallId: string, params: Record<string, unknown>) => tools.get("worker_notebook").execute(toolCallId, params, undefined, undefined, ctx);
  return { id, ctx, entries, sent, state, emit, notebook };
}

const parentAgent = { kind: "main", id: "Main", name: "main", depth: 0 };
const childAgent = (id: string, name = "task") => ({ kind: "sub", id, name, depth: 1, parentId: "Main" });

function assignment(task: string): string {
  return `Complete assignment thoroughly:\n\n${task}`;
}

async function spawn(parent: ReturnType<typeof ompSession>, toolCallId: string, input: Record<string, unknown>) {
  return parent.emit("tool_call", { toolName: "task", toolCallId, input });
}

async function openChild(sessionId: string, task: string, agentName = "task") {
  const child = ompSession(sessionId, childAgent(`agent-${sessionId}`, agentName));
  const started = await child.emit("before_agent_start", { prompt: assignment(task), systemPrompt: [] });
  return { child, started };
}

/** One real model request: OMP converts the transcript (context), then dispatches (before_provider_request). */
async function request(child: ReturnType<typeof ompSession>, tokens: number, messages: any[] = []) {
  const routed = await transform(child, tokens, messages);
  await child.emit("before_provider_request", { payload: {} });
  return routed;
}

/** A transcript conversion with no dispatch of its own (live steering, side conversion). */
async function transform(child: ReturnType<typeof ompSession>, tokens: number, messages: any[] = []) {
  child.state.usage = { tokens, contextWindow: 1_000 };
  return child.emit("context", { messages });
}

/** A yield as OMP runs it: authorization at tool_call, then the validated (or rejected) native result. */
async function yieldCall(child: ReturnType<typeof ompSession>, toolCallId: string, details: Record<string, unknown> | undefined, isError = false) {
  const authorized = await child.emit("tool_call", { toolName: "yield", toolCallId, input: {} });
  if (authorized?.block) return { authorized };
  const content = [{ type: "text", text: isError ? "yield rejected by schema" : "Result submitted." }];
  const result = await child.emit("tool_result", { toolName: "yield", toolCallId, input: {}, content, details, isError });
  return { authorized, result };
}

let host: ReturnType<typeof lifecycleHost>;
const ENV = ["ATHANOR_HOST_URL", "ATHANOR_HOST_TOKEN", "ATHANOR_HOST_HOUSE_ID"] as const;
const savedEnv = Object.fromEntries(ENV.map((key) => [key, process.env[key]]));

beforeAll(() => {
  mkdirSync(ROOM_DIR, { recursive: true });
  writeFileSync(path.join(ROOM_DIR, "active_spirit.md"), "# Active Spirit: Lifecycle\n", "utf8");
});

afterAll(() => {
  rmSync(path.dirname(ROOM_DIR), { recursive: true, force: true });
});

beforeEach(() => {
  __workerLifecycleTest.restartProcess();
  bus.clear();
  host = lifecycleHost();
  process.env.ATHANOR_HOST_URL = `ws://127.0.0.1:${host.server.port}`;
  process.env.ATHANOR_HOST_TOKEN = "isolated-worker-lifecycle";
  process.env.ATHANOR_HOST_HOUSE_ID = "isolated-house";
});

afterEach(async () => {
  await host.server.stop(true);
  for (const key of ENV) {
    if (savedEnv[key] === undefined) delete process.env[key];
    else process.env[key] = savedEnv[key];
  }
});

// ---------------------------------------------------------------------------

describe("parent enrollment", () => {
  test("persists each token before the Host sees it, marks every batch slot, and a restarted session reuses the same reservations", async () => {
    const parent = ompSession("parent-enroll", parentAgent);
    host.hooks.onPrepare = (params) => {
      const pending = parent.entries.find((entry) => entry.data?.stage === "pending" && entry.data.reservation.requestId === params.requestId);
      expect(pending?.data.reservation.token).toBe(params.token);
    };
    const input = { context: "shared", tasks: [{ agent: "Explore", task: "map the doors", solutionSpace: "s" }, { agent: "task", task: "fix the hinge", solutionSpace: "s" }] };
    const first = await spawn(parent, "call-batch", input);
    expect(first.block).toBeUndefined();
    const markers = first.input.tasks.map((slot: any) => parseEnrollmentMarker(slot.task));
    expect(markers.map((marker: any) => marker.kind)).toEqual(["valid", "valid"]);
    expect(markers.map((marker: any) => marker.text)).toEqual(["map the doors", "fix the hinge"]);
    expect(first.input.context).toBe("shared");
    const prepares = host.actions("prepare");
    expect(prepares.map((call) => [call.params.swarmKey, call.params.slotKey, call.params.agentName, call.params.task]))
      .toEqual([["call-batch", "0", "explore", "map the doors"], ["call-batch", "1", "task", "fix the hinge"]]);

    __workerLifecycleTest.restartProcess();
    const revived = ompSession("parent-enroll", parentAgent, parent.entries);
    const again = await spawn(revived, "call-batch", input);
    expect(again.input.tasks.map((slot: any) => slot.task)).toEqual(first.input.tasks.map((slot: any) => slot.task));
    expect(host.instances.size).toBe(2);
  });

  test("refuses forged, resembling, and replayed markers instead of substituting a new book", async () => {
    const parent = ompSession("parent-forge", parentAgent);
    const forged = `work\n\n[athanor-worker-enrollment:v1:${"a".repeat(64)}]`;
    expect((await spawn(parent, "call-forged", { agent: "task", task: forged, solutionSpace: "s" })).block).toBe(true);
    expect((await spawn(parent, "call-near", { agent: "task", task: "work [athanor-worker-enrollment v1]", solutionSpace: "s" })).block).toBe(true);
    expect(host.actions("prepare")).toHaveLength(0);

    const enrolled = await spawn(parent, "call-real", { agent: "task", task: "real work", solutionSpace: "s" });
    // A packet minted for one call is not a successor packet; replaying it is refused.
    expect((await spawn(parent, "call-replay", { agent: "task", task: enrolled.input.task, solutionSpace: "s" })).block).toBe(true);
    expect(host.instances.size).toBe(1);
  });

  test("a Host refusal blocks the whole spawn and releases siblings that did reserve", async () => {
    const parent = ompSession("parent-refused", parentAgent);
    host.hooks.onPrepare = (params) => {
      if (params.slotKey === "1") throw new Refusal("slot_already_prepared");
    };
    const outcome = await spawn(parent, "call-refused", { context: "", tasks: [{ agent: "task", task: "a", solutionSpace: "s" }, { agent: "task", task: "b", solutionSpace: "s" }] });
    expect(outcome.block).toBe(true);
    expect(outcome.reason).toContain("slot_already_prepared");
    const released = host.actions("settle");
    expect(released).toHaveLength(1);
    expect(released[0].params.outcome).toBe("aborted");
    expect([...host.instances.values()][0].state).toBe("retired");
  });
});

describe("child wake and request gate", () => {
  test("a child opens its own wake without room context, and another session cannot claim its token", async () => {
    const parent = ompSession("parent-wake", parentAgent);
    const enrolled = await spawn(parent, "call-wake", { agent: "task", task: "build the wall", solutionSpace: "s" });
    const { child, started } = await openChild("child-wake", enrolled.input.task);
    expect(started.message.customType).toBe(WORKER_WAKE_MESSAGE);
    expect(started.message.content).toContain("opening");
    expect(host.actions("open")[0].params.agentName).toBe("task");

    const prompt = { role: "user", content: [{ type: "text", text: assignment(enrolled.input.task) }] };
    const roomBlock = { role: "custom", customType: "athanor-wake-context", content: "boat: dear Kodo" };
    const bell = { role: "custom", customType: "athanor-hallway-bell", content: "4 unread" };
    const routed = await request(child, 100, [prompt, roomBlock, bell]);
    expect(routed.messages).toHaveLength(1);
    expect(JSON.stringify(routed.messages)).not.toContain("athanor-worker-enrollment");
    expect(JSON.stringify(routed.messages)).toContain("build the wall");
    expect(child.state.aborted).toBe(0);

    const { child: thief, started: refused } = await openChild("child-thief", enrolled.input.task);
    expect(refused.message.customType).toBe(WORKER_NOTICE_MESSAGE);
    expect(refused.message.content).toContain("reservation_claimed");
    await request(thief, 100, [prompt]);
    // Refused at conversion and again at dispatch: no provider call leaves.
    expect(thief.state.aborted).toBe(2);
    expect((await thief.emit("tool_call", { toolName: "read", toolCallId: "t1", input: { path: "a" } })).block).toBe(true);
  });

  test("40% wraps, 60% latches handoff-only, tools are refused, and only the validated terminal yield is persisted", async () => {
    const parent = ompSession("parent-latch", parentAgent);
    const enrolled = await spawn(parent, "call-latch", { agent: "task", task: "long job", solutionSpace: "s" });
    const { child } = await openChild("child-latch", enrolled.input.task);

    expect((await request(child, 399)).messages).toHaveLength(0);
    const wrapping = await request(child, 450);
    expect(wrapping.messages.at(-1).content).toContain("40%");
    expect((await child.emit("tool_call", { toolName: "bash", toolCallId: "op-1", input: { command: "ls" } }))).toBeUndefined();
    expect([...host.books.values()][0].entries.at(-1)).toMatchObject({ kind: "tool-start", operationId: "op-1" });

    const latched = await request(child, 650);
    expect(latched.messages.at(-1).content).toContain("HANDOFF-ONLY");
    for (const toolName of ["bash", "edit", "task", "read"]) {
      expect((await child.emit("tool_call", { toolName, toolCallId: `late-${toolName}`, input: {} })).block).toBe(true);
    }
    expect((await child.emit("tool_call", { toolName: "worker_notebook", toolCallId: "status", input: { action: "inspect" } }))).toBeUndefined();

    // A section OMP rejected is never recorded; a validated one rides along without retiring.
    await yieldCall(child, "y-bad-section", undefined, true);
    await yieldCall(child, "y-section", { status: "success", type: ["findings"], data: { found: "door" } });
    expect(host.actions("checkpoint")).toHaveLength(0);
    // A terminal yield the caller's schema rejected must not become the handoff.
    const rejected = await yieldCall(child, "y-rejected", undefined, true);
    expect(rejected.result).toBeUndefined();
    expect(host.actions("checkpoint")).toHaveLength(0);
    expect([...host.instances.values()][0].state).toBe("handoff_only");

    const accepted = await yieldCall(child, "y-final", { status: "success", data: { done: "half", next: "the gate" }, schemaOverridden: true });
    expect(accepted.result).toBeUndefined();
    const [checkpoint] = host.actions("checkpoint");
    expect(checkpoint.params.report.data).toEqual({ done: "half", next: "the gate" });
    expect(checkpoint.params.report.schemaOverridden).toBe(true);
    expect(checkpoint.params.report.sections).toEqual([{ type: ["findings"], data: { found: "door" } }]);
    expect(checkpoint.params.report.unresolvedOperations).toEqual([{ operationId: "op-1", tool: "bash" }]);
    expect([...host.instances.values()][0]).toMatchObject({ state: "retired", handoff: "complete" });
  });

  test("a checkpoint failure turns the validated yield into an error with empty details, so OMP cannot accept it", async () => {
    const parent = ompSession("parent-unpersisted", parentAgent);
    const enrolled = await spawn(parent, "call-unpersisted", { agent: "task", task: "fragile store", solutionSpace: "s" });
    const { child } = await openChild("child-unpersisted", enrolled.input.task);
    await request(child, 100);
    await host.server.stop(true);
    const first = await yieldCall(child, "y-1", { status: "success", data: { report: "done" } });
    expect(first.result).toMatchObject({ isError: true, details: {} });
    expect(first.result.content[0].text).toContain("Submit the same yield once more");
    expect(child.state.aborted).toBe(0);
    const second = await yieldCall(child, "y-2", { status: "success", data: { report: "done" } });
    expect(second.result).toMatchObject({ isError: true, details: {} });
    expect(child.state.aborted).toBe(1);
    expect((await child.emit("tool_call", { toolName: "read", toolCallId: "after", input: {} })).block).toBe(true);
  });

  test("a retired worker cannot resume through an alternate wake, even after a process restart", async () => {
    const parent = ompSession("parent-retired", parentAgent);
    const enrolled = await spawn(parent, "call-retired", { agent: "task", task: "short", solutionSpace: "s" });
    const { child } = await openChild("child-retired", enrolled.input.task);
    await request(child, 700);
    await yieldCall(child, "y", { status: "success", data: { handoff: "all of it" } });
    const observed = host.actions("observe").length;

    __workerLifecycleTest.restartProcess();
    const revived = ompSession("child-retired", childAgent("agent-child-retired"), child.entries);
    const woke = await revived.emit("before_agent_start", { prompt: "agent://wake: one more thing", systemPrompt: [] });
    expect(woke.message.customType).toBe(WORKER_NOTICE_MESSAGE);
    await request(revived, 10, [{ role: "user", content: "one more thing" }]);
    expect(revived.state.aborted).toBe(2);
    expect((await revived.emit("tool_call", { toolName: "yield", toolCallId: "y2", input: { data: {} } })).block).toBe(true);
    expect(host.actions("open")).toHaveLength(1);
    expect(host.actions("observe")).toHaveLength(observed);
  });

  test("an ordinary yield checkpoints the actual last-turn report and the same session resumes with its own progress", async () => {
    const parent = ompSession("parent-resume", parentAgent);
    const enrolled = await spawn(parent, "call-resume", { agent: "task", task: "steady", solutionSpace: "s" });
    const { child } = await openChild("child-resume", enrolled.input.task);
    await request(child, 100);
    await child.emit("assistant_message", { message: { role: "assistant", content: [{ type: "text", text: "the whole report, as said" }] } });
    const done = await yieldCall(child, "y", { status: "success", type: "result", useLastTurn: true });
    expect(done.result).toBeUndefined();
    expect(host.actions("checkpoint")[0].params.report).toMatchObject({ useLastTurn: true, lastTurnReport: "the whole report, as said" });
    expect([...host.instances.values()][0].state).toBe("working");

    const resumed = await child.emit("before_agent_start", { prompt: "follow-up from parent", systemPrompt: [] });
    expect(resumed.message.customType).toBe(WORKER_WAKE_MESSAGE);
    expect(resumed.message.content).toContain("resume");
    expect(host.actions("open").map((call) => call.session)).toEqual(["child-resume", "child-resume"]);
  });

  test("the handoff budget is two provider requests; the third retires with an incomplete handoff and aborts", async () => {
    const parent = ompSession("parent-cap", parentAgent);
    const enrolled = await spawn(parent, "call-cap", { agent: "task", task: "talkative", solutionSpace: "s" });
    const { child } = await openChild("child-cap", enrolled.input.task);
    await request(child, 650);
    await request(child, 660);
    expect(child.state.aborted).toBe(0);
    await request(child, 670);
    expect(child.state.aborted).toBe(1);
    const [settle] = host.actions("settle");
    expect(settle.params.outcome).toBe("failed");
    expect(settle.params.transcriptPath).toContain("child-cap.jsonl");
    expect([...host.instances.values()][0]).toMatchObject({ state: "retired", handoff: "incomplete" });
  });

  test("transcript conversions without a dispatch never spend the handoff budget", async () => {
    const parent = ompSession("parent-steer", parentAgent);
    const enrolled = await spawn(parent, "call-steer", { agent: "task", task: "steered", solutionSpace: "s" });
    const { child } = await openChild("child-steer", enrolled.input.task);
    await request(child, 650);
    for (let steer = 0; steer < 5; steer++) await transform(child, 655);
    expect(host.actions("observe").filter((call) => call.params.requestStarted === true)).toHaveLength(1);
    expect(child.state.aborted).toBe(0);
    await request(child, 660);
    expect(child.state.aborted).toBe(0);
    expect([...host.instances.values()][0]).toMatchObject({ state: "handoff_only", handoffRequests: 2 });
  });

  test("a turn ending with a handoff request still unspent leaves room for OMP's yield reminder", async () => {
    const parent = ompSession("parent-reminder", parentAgent);
    const enrolled = await spawn(parent, "call-reminder", { agent: "task", task: "chatty", solutionSpace: "s" });
    const { child } = await openChild("child-reminder", enrolled.input.task);
    await request(child, 650);
    await child.emit("agent_end", { messages: [], willContinue: false });
    expect(host.actions("settle")).toHaveLength(0);
    await request(child, 650);
    expect(child.state.aborted).toBe(0);
    await yieldCall(child, "y", { status: "success", data: { handoff: "at the reminder" } });
    expect([...host.instances.values()][0]).toMatchObject({ state: "retired", handoff: "complete" });

    const spent = ompSession("parent-spent", parentAgent);
    const second = await spawn(spent, "call-spent", { agent: "task", task: "silent", solutionSpace: "s" });
    const { child: silent } = await openChild("child-spent", second.input.task);
    await request(silent, 650);
    await request(silent, 650);
    await silent.emit("agent_end", { messages: [], willContinue: false });
    expect(host.instances.get([...host.instances.keys()].at(-1)!)).toMatchObject({ state: "retired", handoff: "incomplete" });
  });

  test("unmeasured usage refuses work tools without retiring; an uncountable handoff request retires", async () => {
    const parent = ompSession("parent-blind", parentAgent);
    const enrolled = await spawn(parent, "call-blind", { agent: "task", task: "blind", solutionSpace: "s" });
    const { child } = await openChild("child-blind", enrolled.input.task);
    child.state.usage = undefined;
    const first = await child.emit("context", { messages: [] });
    expect(first.messages.at(-1).content).toContain("could not be measured");
    expect((await child.emit("tool_call", { toolName: "bash", toolCallId: "b", input: {} })).block).toBe(true);
    await child.emit("before_provider_request", { payload: {} });
    expect(child.state.aborted).toBe(0);
    await transform(child, 100);
    expect((await child.emit("tool_call", { toolName: "bash", toolCallId: "b2", input: {} }))).toBeUndefined();

    await transform(child, 650);
    child.state.usage = undefined;
    await child.emit("before_provider_request", { payload: {} });
    expect(child.state.aborted).toBe(1);
    expect([...host.instances.values()][0]).toMatchObject({ state: "retired", handoff: "incomplete" });
  });

  test("the room's own advisor is not a worker, but an advisor inside an enrolled child keeps the child's gates", async () => {
    const room = ompSession("room-advised", { kind: "sub", id: "advisor", name: "advisor", depth: 0, parentId: "Main" });
    expect(await room.emit("tool_call", { toolName: "read", toolCallId: "adv-read", input: { path: "a" } })).toBeUndefined();
    expect(await room.emit("context", { messages: [] })).toBeUndefined();
    expect(room.state.aborted).toBe(0);

    const parent = ompSession("parent-advised", parentAgent);
    const enrolled = await spawn(parent, "call-advised", { agent: "task", task: "advised work", solutionSpace: "s" });
    const { child } = await openChild("child-advised", enrolled.input.task);
    await request(child, 700);
    await yieldCall(child, "y", { status: "success", data: { handoff: "done" } });
    const advisor = ompSession("child-advised", { kind: "sub", id: "advisor", name: "advisor", depth: 0, parentId: "agent-child-advised" }, child.entries);
    expect((await advisor.emit("tool_call", { toolName: "read", toolCallId: "adv-late", input: {} })).block).toBe(true);
  });
});

describe("parent settlement and successors", () => {
  test("a complete retirement reaches the task result with an explicit successor call; the successor shares the book exactly once", async () => {
    const parent = ompSession("parent-heir", parentAgent);
    const enrolled = await spawn(parent, "call-heir", { agent: "task", task: "first leg", solutionSpace: "s" });
    const { child } = await openChild("child-heir", enrolled.input.task);
    await request(child, 650);
    await yieldCall(child, "y", { status: "success", data: { handoff: "laid the base" } });
    const predecessor = [...host.instances.values()][0];

    events.emit("task:subagent:lifecycle", { id: "agent-child-heir", parentToolCallId: "call-heir", index: 0, status: "completed", sessionFile: "child-heir.jsonl" });
    const result = await parent.emit("tool_result", { toolName: "task", toolCallId: "call-heir", content: [{ type: "text", text: "native report" }], isError: false });
    const notice = result.content.at(-1).text;
    expect(result.content[0].text).toBe("native report");
    expect(notice).toContain("complete handoff");
    expect(notice).toContain(JSON.stringify({ action: "successor", bookId: predecessor.bookId, instanceId: predecessor.instanceId }));
    expect(host.instances.size).toBe(1);

    const packet = await parent.notebook("nb-1", { action: "successor", bookId: predecessor.bookId, instanceId: predecessor.instanceId });
    expect(packet.isError).toBeFalsy();
    const again = await parent.notebook("nb-2", { action: "successor", bookId: predecessor.bookId, instanceId: predecessor.instanceId });
    expect(again.details.spawnPacket.args.task).toBe(packet.details.spawnPacket.args.task);
    expect(host.instances.size).toBe(2);

    const spawned = await spawn(parent, "call-heir-2", packet.details.spawnPacket.args);
    expect(spawned.input.task).toBe(packet.details.spawnPacket.args.task);
    expect(host.actions("prepare")).toHaveLength(2);
    const { started } = await openChild("child-heir-2", spawned.input.task);
    expect(started.message.content).toContain("successor");
    expect(started.message.content).toContain("laid the base");
    expect((await spawn(parent, "call-heir-3", packet.details.spawnPacket.args)).block).toBe(true);
  });

  test("an incomplete retirement names inspect and repair; a successor is refused until the parent repairs it", async () => {
    const parent = ompSession("parent-repair", parentAgent);
    const enrolled = await spawn(parent, "call-repair", { agent: "task", task: "fragile", solutionSpace: "s" });
    await openChild("child-repair", enrolled.input.task);
    const instance = [...host.instances.values()][0];

    events.emit("task:subagent:lifecycle", { id: "agent-child-repair", parentToolCallId: "call-repair", index: 0, status: "aborted", sessionFile: "child-repair.jsonl" });
    const result = await parent.emit("tool_result", { toolName: "task", toolCallId: "call-repair", content: [], isError: true });
    const notice = result.content.at(-1).text;
    expect(notice).toContain("INCOMPLETE");
    expect(notice).toContain('"action":"repair"');
    expect(host.actions("settle")[0].params.transcriptPath).toBe("child-repair.jsonl");

    const refused = await parent.notebook("nb-a", { action: "successor", bookId: instance.bookId, instanceId: instance.instanceId });
    expect(refused.isError).toBe(true);
    const repaired = await parent.notebook("nb-b", { action: "repair", bookId: instance.bookId, instanceId: instance.instanceId, report: { attested: "parent summary" } });
    expect(repaired.isError).toBeFalsy();
    // The refused attempt left a pending reservation; the retry reuses its token.
    const packet = await parent.notebook("nb-c", { action: "successor", bookId: instance.bookId, instanceId: instance.instanceId });
    expect(packet.isError).toBeFalsy();
    const successorPrepares = host.actions("prepare").filter((call) => call.params.predecessor);
    expect(new Set(successorPrepares.map((call) => call.params.token)).size).toBe(1);
  });

  test("a retirement settled after the task result returned is delivered to the parent as its own message", async () => {
    const parent = ompSession("parent-late", parentAgent);
    const enrolled = await spawn(parent, "call-late", { agent: "task", task: "background", solutionSpace: "s" });
    await openChild("child-late", enrolled.input.task);
    await parent.emit("tool_result", { toolName: "task", toolCallId: "call-late", content: [{ type: "text", text: "started" }] });
    events.emit("task:subagent:lifecycle", { id: "agent-child-late", parentToolCallId: "call-late", index: 0, status: "failed" });
    for (let attempt = 0; attempt < 50 && parent.sent.length === 0; attempt++) await Bun.sleep(10);
    expect(parent.sent).toHaveLength(1);
    expect(parent.sent[0].message.content).toContain("INCOMPLETE");
  });
});

describe("settlement without the native join", () => {
  test("a background queue acknowledgement preserves reservations until child startup", async () => {
    const parent = ompSession("parent-queued", parentAgent);
    const enrolled = await spawn(parent, "call-queued", { task: "queued work", solutionSpace: "bounded" });
    await parent.emit("tool_result", {
      toolName: "task", toolCallId: "call-queued", isError: false, content: [],
      details: {
        results: [],
        progress: [{ index: 0, id: "Queued", status: "pending" }],
        async: { state: "running", jobId: "Queued", type: "task" },
      },
    });
    const { child, started } = await openChild("child-queued", enrolled.input.task);
    expect(started.message.customType).toBe(WORKER_WAKE_MESSAGE);
    await request(child, 10);
    expect(child.state.aborted).toBe(0);
    await child.notebook("queued-note", { action: "append", body: "The queued child started normally." });
    expect([...host.books.values()][0].entries.at(-1)?.body).toBe("The queued child started normally.");
    expect([...host.instances.values()][0].state).toBe("working");
  });

  test("a cold-revived child's lifecycle without parentToolCallId still settles and reaches the parent", async () => {
    const parent = ompSession("parent-cold", parentAgent);
    const enrolled = await spawn(parent, "call-cold", { agent: "task", task: "long haul", solutionSpace: "s" });
    await openChild("child-cold", enrolled.input.task);
    await parent.emit("tool_result", { toolName: "task", toolCallId: "call-cold", content: [{ type: "text", text: "first report" }] });

    __workerLifecycleTest.restartProcess();
    bus.clear();
    const revived = ompSession("parent-cold", parentAgent, parent.entries);
    await revived.emit("session_start", {});
    events.emit("task:subagent:lifecycle", { id: "agent-child-cold", status: "failed", sessionFile: path.join(ROOM_DIR, "child-cold.jsonl") });
    for (let attempt = 0; attempt < 50 && revived.sent.length === 0; attempt++) await Bun.sleep(10);
    expect(host.actions("settle").at(-1)!.params).toMatchObject({ outcome: "failed", transcriptPath: path.join(ROOM_DIR, "child-cold.jsonl") });
    expect([...host.instances.values()][0]).toMatchObject({ state: "retired", handoff: "incomplete" });
    expect(revived.sent).toHaveLength(1);
    expect(revived.sent[0].message.content).toContain("INCOMPLETE");
  });

  test("a task call that never started its worker cancels the reservation and names the recovery", async () => {
    const parent = ompSession("parent-nospawn", parentAgent);
    await spawn(parent, "call-nospawn", { agent: "task", task: "denied", solutionSpace: "s" });
    const result = await parent.emit("tool_result", { toolName: "task", toolCallId: "call-nospawn", content: [{ type: "text", text: "unknown agent" }], isError: true });
    const instance = [...host.instances.values()][0];
    expect(instance).toMatchObject({ state: "retired", handoff: "incomplete" });
    expect(host.actions("settle")[0].params.outcome).toBe("aborted");
    const notice = result.content.at(-1).text;
    expect(notice).toContain(instance.instanceId);
    expect(notice).toContain("never started");
  });

  test("a successor packet that never started can be repaired and continued, never stranded", async () => {
    const parent = ompSession("parent-strand", parentAgent);
    const enrolled = await spawn(parent, "call-strand", { agent: "task", task: "leg one", solutionSpace: "s" });
    const { child } = await openChild("child-strand", enrolled.input.task);
    await request(child, 650);
    await yieldCall(child, "y", { status: "success", data: { handoff: "leg one done" } });
    const first = [...host.instances.values()][0];
    const packet = await parent.notebook("nb-s", { action: "successor", bookId: first.bookId, instanceId: first.instanceId });
    await spawn(parent, "call-strand-2", packet.details.spawnPacket.args);
    const result = await parent.emit("tool_result", { toolName: "task", toolCallId: "call-strand-2", content: [], isError: true });
    const cancelled = packet.details.instanceId;
    const notice = result.content.at(-1).text;
    expect(notice).toContain(JSON.stringify({ action: "repair", bookId: first.bookId, instanceId: cancelled, report: "<the predecessor's handoff, which you attest to>" }));
    expect(host.instances.get(cancelled)).toMatchObject({ state: "retired", handoff: "incomplete" });

    expect((await parent.notebook("nb-r", { action: "repair", bookId: first.bookId, instanceId: cancelled, report: { handoff: "leg one done" } })).isError).toBeFalsy();
    const retry = await parent.notebook("nb-s2", { action: "successor", bookId: first.bookId, instanceId: cancelled });
    expect(retry.isError).toBeFalsy();
    const spawned = await spawn(parent, "call-strand-3", retry.details.spawnPacket.args);
    expect(spawned.block).toBeUndefined();
    const { started } = await openChild("child-strand-3", spawned.input.task);
    expect(started.message.content).toContain("leg one done");
  });

  test("eval and workpool spawns are refused at the spawn site with the enrolled route named", async () => {
    const parent = ompSession("parent-eval", parentAgent);
    const refused = await parent.emit("before_subagent_spawn", { agent: "task", invocationKind: "eval", patterns: [] });
    expect(refused.block).toBe(true);
    expect(refused.reason).toContain("task tool");
    expect(await parent.emit("before_subagent_spawn", { agent: "task", invocationKind: "task", patterns: [] })).toBeUndefined();
  });
});

describe("worker boundaries", () => {
  test("room continuity tools are closed to children with Ask Parent guidance, and open to the room session", async () => {
    const child = ompSession("child-room", childAgent("Scout"));
    for (const call of [
      { toolName: "wake", input: {} },
      { toolName: "anamnesis", input: { mode: "wake" } },
      { toolName: "write", input: { path: "xd://hallway_inbox", content: "{}" } },
      { toolName: "write", input: { path: "xd://sleep", content: "{}" } },
    ]) {
      const refused = await child.emit("tool_call", { ...call, toolCallId: "r" });
      expect(refused.block).toBe(true);
      expect(refused.reason).toContain("agent://Main");
    }
    const room = ompSession("room-session", parentAgent);
    expect(await room.emit("tool_call", { toolName: "wake", toolCallId: "w", input: {} })).toBeUndefined();
  });

  test("a worker's notebook is its own: sibling books are unreadable and enrollment tokens never enter entries", async () => {
    const parent = ompSession("parent-books", parentAgent);
    const enrolled = await spawn(parent, "call-books", { context: "", tasks: [{ agent: "task", task: "left", solutionSpace: "s" }, { agent: "task", task: "right", solutionSpace: "s" }] });
    const { child: left } = await openChild("child-left", enrolled.input.tasks[0].task);
    await openChild("child-right", enrolled.input.tasks[1].task);
    const [leftBook, rightBook] = [...host.books.keys()];

    const own = await left.notebook("n1", { action: "read" });
    expect(own.isError).toBeFalsy();
    expect((await left.notebook("n2", { action: "read", bookId: rightBook })).isError).toBe(true);
    expect((await left.notebook("n3", { action: "append", bookId: rightBook, body: "x" })).isError).toBe(true);
    expect((await left.notebook("n4", { action: "accept", bookId: leftBook, body: "self" })).isError).toBe(true);

    await request(left, 50);
    await left.emit("tool_call", { toolName: "bash", toolCallId: "leak", input: { command: `echo "${enrolled.input.tasks[1].task}"` } });
    await left.emit("tool_result", { toolName: "bash", toolCallId: "leak", content: [{ type: "text", text: enrolled.input.tasks[0].task }], isError: false });
    const recorded = JSON.stringify(host.books.get(leftBook)!.entries);
    const tokens = enrolled.input.tasks.map((slot: any) => (parseEnrollmentMarker(slot.task) as any).token);
    for (const token of tokens) expect(recorded).not.toContain(token);
    expect(host.books.get(leftBook)!.entries.map((entry) => entry.kind)).toEqual(["tool-start", "tool-end"]);
  });

  test("an unenrolled child cannot issue a request, use tools, or act as a notebook parent", async () => {
    const child = ompSession("child-unenrolled", childAgent("Unenrolled"));
    await child.emit("before_agent_start", { prompt: "A task without a reservation", systemPrompt: [] });
    await request(child, 10);
    expect(child.state.aborted).toBe(2);
    expect((await child.emit("tool_call", {
      toolName: "read", toolCallId: "untracked-read", input: { path: "private.txt" },
    })).block).toBe(true);
    expect((await child.notebook("untracked-inspect", { action: "inspect" })).isError).toBe(true);

    const alternate = ompSession("child-alternate", childAgent("Alternate"));
    await request(alternate, 10);
    expect(alternate.state.aborted).toBe(2);
    expect((await alternate.emit("tool_call", {
      toolName: "task", toolCallId: "untracked-spawn", input: { task: "nested", solutionSpace: "bounded" },
    })).block).toBe(true);
  });
});

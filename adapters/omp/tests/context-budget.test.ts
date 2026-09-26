// 2026-09-25, live (kodo): on Recall-firing turns the substrate's embed stalled
// for its whole 3 s limit, Recall ran until the adapter cut it at 4.5 s, and
// recall-turns logged `rust_transport_failure` for a budget the adapter itself
// spent. The lesson sieve queued behind Recall, found no budget left, and
// never ran. Separately, every tool step of every turn replayed the turn memo
// and Insula logged it as a degraded, cancelled context assembly: 17,055 rows
// in three days. This file drives the registered context hook through the
// real budget, the real Rust JSONL transport, and a loopback Host.

import { afterEach, beforeAll, beforeEach, expect, test } from "bun:test";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

import solarisaelHouseProof from "../index.ts";
import { closeRustRecallTransports } from "../house-proof/recall.ts";
import { closeInsulaWriter, INSULA_EVENTS_PATH } from "../house-proof/insula.ts";
import { registerTopLevelSession, retireTopLevelSession } from "../house-proof/top-level-session-fence.ts";

const ROOM_KEY = "budget-room";
const ROOMS = path.join(tmpdir(), "athanor-context-budget");
let ROOM = "";
const PRESENCE = "athanor-presence-context";
const PROMPT = "why does the lesson sieve never run on a recall turn";
const FAKE_SUBSTRATE = path.join(import.meta.dir, "fixtures", "fake-substrate.ts").replaceAll("\\", "/");

type Handler = (event: unknown, ctx: unknown) => Promise<unknown> | unknown;

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

const POLICY_STATE = {
  requestedMode: "auto",
  resolvedMode: "work",
  activeProject: null,
  resolutionReason: "tool-evidence",
  lastRefreshAt: null,
  lastRefreshReason: null,
  workingSetEntries: 0,
  recoveryPending: false,
  recoveryTerms: [],
  degraded: null,
  updatedAt: null,
};

// Answers the commands a Recall-firing work turn sends and refuses the rest by
// name, so conversation capture, the Bell, and the wake take their fail-open paths.
function loopbackHost() {
  const commands: Array<Record<string, any>> = [];
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
        const reply = (kind: string, extra: Record<string, unknown>) =>
          socket.send(JSON.stringify({ correlation_id: command.message_id, command_or_event_type: kind, ...extra }));
        const snapshot = (extra: Record<string, unknown> = {}) =>
          reply("athanor.recall_policy.snapshot", { state: POLICY_STATE, version: 1, sequence: 1, state_hash: "h", ...extra });
        if (type === "athanor.context.analyze") {
          reply("athanor.context.analyzed", {
            analysis: {
              route: {
                intent: "technical_project",
                terms: ["lesson", "sieve", "recall"],
                requiredTerms: [],
                recognizedEntities: [],
                entityResolutionSuggested: false,
                shouldAutoRecall: true,
                lanes: { lexical: true },
                reasons: [],
              },
              keywordDirectives: [],
              keywordReminder: null,
              nudge: null,
              roomReminder: "",
              routingReminder: null,
            },
          });
        } else if (type === "athanor.recall_policy.subscribe" || type === "athanor.recall_policy.fail_refresh") {
          snapshot();
        } else if (type === "athanor.recall_policy.evaluate") {
          snapshot({
            decision: {
              shouldRecall: true,
              clearWorkingSet: false,
              query: "lesson sieve recall",
              queryTerms: ["lesson", "sieve", "recall"],
              refreshReason: "topic-shift",
              intent: "technical_project",
              resolvedMode: "work",
            },
          });
        } else if (type === "athanor.presence.open") {
          reply("athanor.presence.opened", {
            result: { operation: "open", value: { frameId: "frame-1", rendered: "FRAME", version: 1 } },
          });
        } else if (type === "athanor.presence.compile") {
          reply("athanor.presence.compiled", {
            result: { operation: "compile", value: { contractId: "contract-1", rendered: "CONTRACT", guards: [] } },
          });
        } else {
          reply(`${type}.command_refused`, { reason: "not answered by this test" });
        }
      },
    },
  });
  process.env.ATHANOR_HOST_URL = `ws://127.0.0.1:${server.port}`;
  return {
    server,
    ofType: (type: string) => commands.filter((command) => command.command_or_event_type === type),
  };
}

// Jev answers every card with one score; Insula batches are kept for inspection.
function outboundFetch() {
  const jevCalls: string[] = [];
  const insula: Array<Record<string, any>> = [];
  const fetcher = (async (url: unknown, init?: RequestInit) => {
    const body = JSON.parse(String(init?.body ?? "{}"));
    if (String(url).endsWith(INSULA_EVENTS_PATH)) {
      insula.push(...body.events);
      return new Response("{}", { status: 200 });
    }
    jevCalls.push(String(body.purpose ?? body.grant?.purpose ?? "jev"));
    const answers = Object.fromEntries((body.state?.cards ?? []).map((card: { token: string }) =>
      [card.token, { type: "noul", noul: 0.9 }]));
    return new Response(JSON.stringify({ model: "jev-latest", answers }), { status: 200 });
  }) as typeof fetch;
  return { fetcher, jevCalls, insula };
}

const ENV = [
  "ATHANOR_HOST_URL",
  "ATHANOR_HOST_TOKEN",
  "ATHANOR_HOST_HOUSE_ID",
  "ATHANOR_DISABLE_AUTO_RECALL",
  "ATHANOR_DISABLE_INSULA",
  "ATHANOR_RECALL_TELEMETRY",
  "ATHANOR_SUBSTRATE_EXE",
  "ATHANOR_FAKE_RECALL",
  "BUN_OPTIONS",
];
const savedEnv: Record<string, string | undefined> = {};
const originalFetch = globalThis.fetch;
let host: ReturnType<typeof loopbackHost>;
let outbound: ReturnType<typeof outboundFetch>;
let handlers = new Map<string, Handler[]>();
let session = "";
let sessionCounter = 0;

function room(jevRecall: boolean): void {
  const marker: Record<string, unknown> = {
    room: ROOM_KEY,
    jevLessons: {
      mode: "shadow", provider: "typesafe",
      grant: { purpose: "lesson-sieve", allowPrivateLessonPackets: true, policyRevision: "sol-test-v1" },
    },
  };
  if (jevRecall) {
    marker.jevRecall = {
      mode: "active", provider: "typesafe",
      grant: { purpose: "recall-rerank", allowPrivateRecallPackets: true, policyRevision: "sol-test-v1" },
    };
  }
  writeFileSync(path.join(ROOM, ".athanor-room.json"), JSON.stringify(marker));
}

beforeEach(() => {
  for (const key of ENV) savedEnv[key] = process.env[key];
  process.env.ATHANOR_HOST_TOKEN = "test-token";
  process.env.ATHANOR_HOST_HOUSE_ID = "solarisael";
  process.env.ATHANOR_RECALL_TELEMETRY = "1";
  delete process.env.ATHANOR_DISABLE_AUTO_RECALL;
  // Insula stays on so its rows can be read; outboundFetch keeps them local.
  delete process.env.ATHANOR_DISABLE_INSULA;
  // The transport spawns the substrate with no arguments; Bun reads the script
  // to run from BUN_OPTIONS, so the real transport speaks to the fake child.
  process.env.ATHANOR_SUBSTRATE_EXE = process.execPath;
  process.env.BUN_OPTIONS = `"${FAKE_SUBSTRATE}"`;
  // The lesson transport keeps its child in the room directory for the life of
  // the process, so each test gets its own room and the tree is cleared once.
  sessionCounter += 1;
  ROOM = path.join(ROOMS, `room-${sessionCounter}`);
  mkdirSync(ROOM, { recursive: true });
  writeFileSync(path.join(ROOM, "active_spirit.md"), "# Active Spirit: Budget\n", "utf8");
  session = `0199b000-0000-7000-8000-${String(sessionCounter).padStart(12, "0")}`;
  registerTopLevelSession(ROOM_KEY, session);
  host = loopbackHost();
  outbound = outboundFetch();
  globalThis.fetch = outbound.fetcher;
  handlers = new Map();
  const pi = {
    zod: zodStub,
    setLabel() {},
    on(name: string, handler: Handler) {
      handlers.set(name, [...(handlers.get(name) ?? []), handler]);
    },
    events: { on: () => () => {} },
    registerMessageRenderer() {},
    registerCommand() {},
    registerTool() {},
  };
  solarisaelHouseProof(pi as any);
});

afterEach(async () => {
  await closeInsulaWriter();
  closeRustRecallTransports();
  globalThis.fetch = originalFetch;
  host.server.stop(true);
  retireTopLevelSession(ROOM_KEY, session);
  for (const [key, value] of Object.entries(savedEnv)) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
});

beforeAll(() => rmSync(ROOMS, { recursive: true, force: true }));

function ctx() {
  return {
    cwd: ROOM,
    mode: "tui",
    sessionManager: { getSessionId: () => session },
    modelRegistry: { authStorage: { getApiKey: async () => "test-key" } },
    ui: { notify() {} },
  };
}

async function runContext(messages: unknown[]): Promise<any[] | undefined> {
  const [handler] = handlers.get("context") ?? [];
  const result = await handler!({ type: "context", messages }, ctx()) as { messages?: any[] } | undefined;
  return result?.messages;
}

function user(id: string, text: string) {
  return { role: "user", id, content: [{ type: "text", text }], attribution: "user", timestamp: 1 };
}

function recallTurns(): Array<Record<string, any>> {
  return readFileSync(path.join(ROOM, ".omp", "runtime", "recall-turns.jsonl"), "utf8")
    .trim().split("\n").map((line) => JSON.parse(line));
}

async function insulaRows(): Promise<Array<Record<string, any>>> {
  await closeInsulaWriter();
  return outbound.insula;
}

test.each([
  ["without Jev recall rerank", false],
  ["with Jev recall rerank active", true],
])("a Recall that spends its whole share leaves the sieve and Presence their turn (%s)", async (_label, jevRecall) => {
  room(jevRecall);
  process.env.ATHANOR_FAKE_RECALL = "hang";

  const out = await runContext([user("u1", PROMPT)]);

  const presence = out?.find((message) => message.customType === PRESENCE);
  expect(presence?.content).toBe("FRAME\n\nCONTRACT");
  expect(presence.details.lessonSieve).toMatchObject({ status: "shadow" });
  expect(out?.some((message) => message.customType === "athanor-recall-context")).toBe(false);

  const [row] = recallTurns();
  expect(row.status).toBe("budget_exhausted");
  expect(row.viewport_diagnostics).toMatchObject({ code: "AUTOMATIC_RECALL_BUDGET_EXHAUSTED", category: "budget" });
  expect(host.ofType("athanor.recall_policy.fail_refresh").map((command) => command.failure_reason))
    .toEqual([expect.stringContaining("budget share")]);

  const rows = await insulaRows();
  const recall = rows.filter((row) => row.operation === "automatic_recall");
  expect(recall).toMatchObject([{ phase: "point", outcomeClass: "timeout", errorClass: "recall_budget_exhausted" }]);
  const assembly = rows.find((row) => row.operation === "context_assembly" && row.phase === "end");
  expect(assembly).toMatchObject({ outcomeClass: "degraded", errorClass: "partial_context" });
  expect(recall[0].parentSpanId).toBe(assembly!.spanId);
}, 20_000);

test("a substrate that dies mid-Recall keeps the transport's own name, not the budget's", async () => {
  room(false);
  process.env.ATHANOR_FAKE_RECALL = "exit";

  const out = await runContext([user("u1", PROMPT)]);

  expect(out?.find((message) => message.customType === PRESENCE)?.details.lessonSieve).toMatchObject({ status: "shadow" });
  const [row] = recallTurns();
  expect(row.status).toBe("error");
  expect(row.viewport_diagnostics.code).toBe("RUST_TRANSPORT_CHILD_EXITED");
  const recall = (await insulaRows()).filter((row) => row.operation === "automatic_recall");
  expect(recall).toMatchObject([{ outcomeClass: "error", errorClass: "rust_transport_child_exited" }]);
}, 20_000);

test("tool steps that replay the turn memo open no context_assembly span", async () => {
  room(false);
  process.env.ATHANOR_DISABLE_AUTO_RECALL = "1";
  const first = [user("u1", PROMPT)];
  const toolStep = [
    ...first,
    { role: "assistant", content: [{ type: "toolCall", id: "call-1", name: "read", arguments: {} }], timestamp: 2 },
    { role: "toolResult", toolCallId: "call-1", toolName: "read", content: [{ type: "text", text: "body" }], timestamp: 2 },
  ];

  const firstOut = await runContext(first);
  const replayOut = await runContext(toolStep);
  await runContext([...toolStep]);

  expect(replayOut?.find((message) => message.customType === PRESENCE))
    .toEqual(firstOut?.find((message) => message.customType === PRESENCE));
  const assembly = (await insulaRows()).filter((row) => row.operation === "context_assembly");
  expect(assembly.map((row) => row.phase)).toEqual(["start", "end"]);
  expect(assembly.some((row) => row.errorClass === "automatic_context_cancelled")).toBe(false);
});

import { afterEach, beforeEach, expect, spyOn, test } from "bun:test";
import { createHash, randomUUID } from "node:crypto";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

import {
  __gigaTest,
  flushGigaTurns,
  flushGigaTurnsDetached,
  gigaBufferedTurnCensus,
  ingestGigaLoggedTurnsDetached,
} from "./giga.ts";
import type { LoggedTurn } from "./house-proof/conversation-log.ts";

const ENV_KEYS = [
  "ATHANOR_HOST_URL", "ATHANOR_HOST_TOKEN", "ATHANOR_HOST_HOUSE_ID",
  "ATHANOR_CLIENT_ROOM_DIR", "ATHANOR_GIGA_ENABLED", "ATHANOR_HIPPOCAMPUS_ENABLED",
  "ATHANOR_REPLAY_MODE", "ATHANOR_GIGA_PROJECT_KEY",
] as const;
const savedEnv = new Map<string, string | undefined>();
const REFUSED = {
  error: { code: "fixture_ingest_refused", message: "fixture admission refused", retryable: false },
};

type WireTurn = {
  role: string;
  source_id: string;
  content_hash: string;
  session_id: string;
  timestamp: string;
  has_stable_id: boolean;
};
type IngestCommand = {
  message_id: string;
  sender_session: string;
  sender_room: string;
  command_or_event_type: string;
  organ_request: { operation: string; params: { turns: WireTurn[] } };
};
type Delivery = {
  command: IngestCommand;
  reply: (receipt?: Record<string, unknown>) => void;
  disconnect: () => void;
};

let root: string;
let room: string;
let deliveries: Delivery[];
let receive: (delivery: Delivery) => void;
let server: ReturnType<typeof Bun.serve>;
let sockets: Set<{ close(): void }>;
let warnings: unknown[][];
let warn: ReturnType<typeof spyOn>;

async function until(condition: () => boolean): Promise<void> {
  const deadline = performance.now() + 2_000;

  while (!condition()) {
    if (performance.now() >= deadline) throw new Error("GIGA fixture did not reach the observed boundary");

    await Bun.sleep(5);
  }
}

beforeEach(() => {
  for (const key of ENV_KEYS) savedEnv.set(key, process.env[key]);

  __gigaTest.resetState();
  root = mkdtempSync(path.join(tmpdir(), "athanor-giga-hardening-"));
  room = path.join(root, "fixture-room");
  mkdirSync(room);
  mkdirSync(path.join(root, "sessions"));
  writeFileSync(path.join(room, ".athanor-room.json"), JSON.stringify({
    room: "fixture-room", trueName: "Fixture", operator: "Fixture operator",
  }));

  deliveries = [];
  sockets = new Set();
  warnings = [];
  warn = spyOn(console, "warn").mockImplementation((...args) => { warnings.push(args); });
  receive = (delivery) => delivery.reply();
  server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch(request, server) {
      if (server.upgrade(request)) return;

      return new Response("WebSocket required", { status: 400 });
    },
    websocket: {
      open(socket) { sockets.add(socket); },
      close(socket) { sockets.delete(socket); },
      message(socket, message) {
        const command = JSON.parse(String(message)) as IngestCommand;

        if (command.command_or_event_type !== "athanor.organ.call"
          || command.organ_request?.operation !== "giga_conversation_ingest") {
          socket.send(JSON.stringify({
            correlation_id: command.message_id,
            command_or_event_type: "athanor.command_refused",
            reason: "unscripted operation",
          }));
          return;
        }

        let replied = false;
        const delivery: Delivery = {
          command,
          reply(receipt = { result: { event_id: "a".repeat(64), accepted: true, duplicate: false } }) {
            if (replied) return;

            replied = true;
            socket.send(JSON.stringify({
              correlation_id: command.message_id,
              command_or_event_type: "athanor.organ.result",
              ...receipt,
            }));
          },
          disconnect() {
            replied = true;
            socket.close();
          },
        };
        deliveries.push(delivery);
        receive(delivery);
      },
    },
  });

  process.env.ATHANOR_HOST_URL = `ws://127.0.0.1:${server.port}`;
  process.env.ATHANOR_HOST_TOKEN = "giga-hardening-test-token";
  process.env.ATHANOR_HOST_HOUSE_ID = "giga-hardening-test-house";
  process.env.ATHANOR_GIGA_ENABLED = "1";
  process.env.ATHANOR_HIPPOCAMPUS_ENABLED = "0";
  process.env.ATHANOR_REPLAY_MODE = "0";
  delete process.env.ATHANOR_CLIENT_ROOM_DIR;
  delete process.env.ATHANOR_GIGA_PROJECT_KEY;
});

afterEach(async () => {
  try {
    receive = (delivery) => delivery.reply();
    for (const delivery of deliveries) delivery.reply();

    await flushGigaTurns();
    await until(() => sockets.size === 0);
  } finally {
    server.stop(true);
    __gigaTest.resetState();
    warn.mockRestore();
    for (const [key, value] of savedEnv) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }

    rmSync(root, { recursive: true, force: true });
  }
});

function context() {
  const conversation = randomUUID();

  return {
    cwd: room,
    sessionManager: {
      getSessionId: () => conversation,
      getSessionFile: () => path.join(root, "sessions", `${conversation}.jsonl`),
    },
  };
}

function turns(sessionID: string, count: number, start = 0): LoggedTurn[] {
  return Array.from({ length: count }, (_, offset) => {
    const index = start + offset;
    const text = `private fixture turn ${sessionID} ${index}`;

    return {
      role: index % 2 === 0 ? "user" : "assistant",
      text,
      sourceID: `${sessionID}:turn:${index}`,
      contentHash: createHash("sha256").update(text).digest("hex"),
      sessionID,
      sourceTimestamp: new Date(Date.UTC(2026, 0, 1, 0, 0, index)).toISOString(),
      hasStableID: true,
    };
  });
}

function pending(session: string): number {
  return gigaBufferedTurnCensus()
    .filter((entry) => entry.session === session)
    .reduce((sum, entry) => sum + entry.turns, 0);
}

function sent(conversation: string): WireTurn[][] {
  return deliveries
    .filter(({ command }) => command.sender_session === conversation)
    .map(({ command }) => command.organ_request.params.turns);
}

test("hardening: shutdown sends one-to-eight-source windows in ledger order without loss", async () => {
  const ctx = context();
  const logged = turns("bounded-source", 17);
  ingestGigaLoggedTurnsDetached(ctx, logged);

  await flushGigaTurns();

  const windows = sent(ctx.sessionManager.getSessionId());
  expect(windows.length).toBeGreaterThan(0);
  for (const window of windows) {
    expect(window.length).toBeGreaterThanOrEqual(1);
    expect(window.length).toBeLessThanOrEqual(8);
  }

  const emitted = windows.flat();
  expect(emitted.map((turn) => turn.source_id)).toEqual(logged.map((turn) => turn.sourceID));
  for (let index = 0; index < logged.length; index += 1) {
    expect(emitted[index]).toEqual({
      role: logged[index].role,
      source_id: logged[index].sourceID,
      content_hash: logged[index].contentHash,
      session_id: logged[index].sessionID,
      timestamp: logged[index].sourceTimestamp,
      has_stable_id: true,
    });
  }
  expect(gigaBufferedTurnCensus()).toEqual([]);

  const acknowledged = deliveries.length;
  await flushGigaTurns();
  expect(deliveries).toHaveLength(acknowledged);
});

test("hardening: detached flush owns its conversation and keeps source sessions separate", async () => {
  const first = context();
  const second = context();
  const firstTurns = turns("shared-source", 2);
  const otherSession = turns("other-source", 1);
  const secondTurns = turns("shared-source", 1, 10);
  ingestGigaLoggedTurnsDetached(first, [firstTurns[0], otherSession[0], firstTurns[1]]);
  ingestGigaLoggedTurnsDetached(second, secondTurns);

  flushGigaTurnsDetached(first);
  await until(() => deliveries.length >= 2 && sockets.size === 0);

  expect(sent(first.sessionManager.getSessionId())).toHaveLength(2);
  expect(sent(second.sessionManager.getSessionId())).toEqual([]);
  expect(gigaBufferedTurnCensus()).toEqual([{ session: "shared-source", cwd: room, turns: 1 }]);
  const windows = sent(first.sessionManager.getSessionId());
  expect(windows.find((window) => window[0].session_id === "shared-source")?.map((turn) => turn.source_id))
    .toEqual(firstTurns.map((turn) => turn.sourceID));
  expect(windows.find((window) => window[0].session_id === "other-source")?.map((turn) => turn.source_id))
    .toEqual(otherSession.map((turn) => turn.sourceID));

  await flushGigaTurns();

  expect(sent(second.sessionManager.getSessionId()).flat().map((turn) => turn.source_id))
    .toEqual(secondTurns.map((turn) => turn.sourceID));
  expect(gigaBufferedTurnCensus()).toEqual([]);
});

for (const [name, receipt] of [
  ["refused", REFUSED],
  ["unacknowledged", { result: { event_id: "b".repeat(64), accepted: false, duplicate: false } }],
] as const) {
  test(`hardening: ${name} receipt keeps pending turns while a successful conversation drains`, async () => {
    const failed = context();
    const successful = context();
    receive = (delivery) => delivery.reply(
      delivery.command.sender_session === failed.sessionManager.getSessionId() ? receipt : undefined,
    );
    ingestGigaLoggedTurnsDetached(failed, turns("pending-source", 2));
    ingestGigaLoggedTurnsDetached(successful, turns("accepted-source", 1));

    await flushGigaTurns();

    expect(sent(failed.sessionManager.getSessionId())).toHaveLength(1);
    expect(sent(successful.sessionManager.getSessionId())).toHaveLength(1);
    expect(pending("accepted-source")).toBe(0);
    expect(gigaBufferedTurnCensus()).toEqual([{ session: "pending-source", cwd: room, turns: 2 }]);

    await Bun.sleep(25);
    expect(deliveries).toHaveLength(2);
  });
}

test("hardening: refusal reports the ingest operation and error without leaking source text or credentials", async () => {
  const ctx = context();
  const logged = turns("diagnostic-source", 1);
  receive = (delivery) => delivery.reply({
    error: {
      ...REFUSED.error,
      message: `${logged[0].text} ${process.env.ATHANOR_HOST_TOKEN}`,
      details: { privateSource: logged[0].text },
    },
  });
  ingestGigaLoggedTurnsDetached(ctx, logged);

  await flushGigaTurns();

  expect(sent(ctx.sessionManager.getSessionId())).toHaveLength(1);
  const diagnostic = JSON.stringify(warnings);
  expect(diagnostic).toContain("giga_conversation_ingest");
  expect(diagnostic).toContain(REFUSED.error.code);
  expect(diagnostic).not.toContain(logged[0].text);
  expect(diagnostic).not.toContain(process.env.ATHANOR_HOST_TOKEN!);
});

for (const duplicate of [false, true]) {
  test(`hardening: ${duplicate ? "duplicate" : "accepted"} acknowledgment retires an in-flight window exactly once`, async () => {
    const ctx = context();
    const logged = turns("inflight-source", 2);
    receive = () => {};
    ingestGigaLoggedTurnsDetached(ctx, logged);
    flushGigaTurnsDetached(ctx);
    await until(() => deliveries.length === 1);
    const beforeAck = pending("inflight-source");

    deliveries[0].reply({ result: { event_id: "c".repeat(64), accepted: !duplicate, duplicate } });
    await until(() => sockets.size === 0);
    await flushGigaTurns();

    expect(beforeAck).toBe(2);
    expect(gigaBufferedTurnCensus()).toEqual([]);
    expect(sent(ctx.sessionManager.getSessionId()).flat().map((turn) => turn.source_id))
      .toEqual(logged.map((turn) => turn.sourceID));
    expect(deliveries).toHaveLength(1);
  });
}

test("hardening: shutdown joins detached work before sending appended turns or completing", async () => {
  const ctx = context();
  const first = turns("serialized-source", 2);
  const appended = turns("serialized-source", 1, 2);
  receive = () => {};
  ingestGigaLoggedTurnsDetached(ctx, first);
  flushGigaTurnsDetached(ctx);
  await until(() => deliveries.length === 1);

  ingestGigaLoggedTurnsDetached(ctx, appended);
  flushGigaTurnsDetached(ctx);
  let shutdownComplete = false;
  const shutdown = Promise.all([flushGigaTurns(), flushGigaTurns()]).then(() => { shutdownComplete = true; });
  await Bun.sleep(25);
  const beforeAck = { requests: deliveries.length, shutdownComplete };

  receive = (delivery) => delivery.reply();
  for (const delivery of deliveries) delivery.reply();
  await shutdown;
  await until(() => sockets.size === 0);

  expect(beforeAck.requests).toBe(1);
  expect(beforeAck.shutdownComplete).toBe(false);
  expect(sent(ctx.sessionManager.getSessionId()).flat().map((turn) => turn.source_id))
    .toEqual([...first, ...appended].map((turn) => turn.sourceID));
  expect(gigaBufferedTurnCensus()).toEqual([]);
});

test("hardening: explicit retry preserves a lost-ack window before draining appended turns", async () => {
  const ctx = context();
  const first = turns("retry-source", 2);
  const appended = turns("retry-source", 2, 2);
  process.env.ATHANOR_GIGA_PROJECT_KEY = "original-project";
  receive = (delivery) => delivery.disconnect();
  ingestGigaLoggedTurnsDetached(ctx, first);
  flushGigaTurnsDetached(ctx);
  await until(() => deliveries.length === 1 && sockets.size === 0);

  const original = structuredClone(deliveries[0].command);
  const nextRoom = path.join(root, "next-room");
  mkdirSync(nextRoom);
  writeFileSync(path.join(nextRoom, ".athanor-room.json"), JSON.stringify({
    room: "next-room", trueName: "Next", operator: "Fixture operator",
  }));
  ctx.cwd = nextRoom;
  process.env.ATHANOR_GIGA_PROJECT_KEY = "next-project";
  ingestGigaLoggedTurnsDetached(ctx, appended);
  await Bun.sleep(25);
  expect(deliveries).toHaveLength(1);
  receive = (delivery) => delivery.reply(deliveries.length === 2
    ? { result: { event_id: "d".repeat(64), accepted: false, duplicate: true } }
    : undefined);
  flushGigaTurnsDetached(ctx);
  await until(() => deliveries.length >= 2 && sockets.size === 0);
  await flushGigaTurns();

  const retried = deliveries[1].command;
  expect(retried.sender_room).toBe(original.sender_room);
  expect(retried.sender_session).toBe(original.sender_session);
  expect(retried.organ_request.params).toEqual(original.organ_request.params);
  expect(deliveries[2].command.sender_room).toBe("next-room");
  expect(sent(ctx.sessionManager.getSessionId())).toHaveLength(3);
  expect(sent(ctx.sessionManager.getSessionId())[2].map((turn) => turn.source_id))
    .toEqual(appended.map((turn) => turn.sourceID));
  expect(gigaBufferedTurnCensus()).toEqual([]);
});

for (const fence of ["disabled", "replay", "subagent"] as const) {
  test(`hardening: ${fence} capture stays fenced while a top-level opt-in capture still drains`, async () => {
    const ctx = context();
    const blocked = turns("fenced-source", 1);

    if (fence === "disabled") process.env.ATHANOR_GIGA_ENABLED = "0";
    if (fence === "replay") process.env.ATHANOR_REPLAY_MODE = "1";
    if (fence === "subagent") {
      const parentFile = ctx.sessionManager.getSessionFile();
      const workerFile = path.join(parentFile.slice(0, -".jsonl".length), "worker.jsonl");
      writeFileSync(parentFile, "");
      ctx.sessionManager.getSessionFile = () => workerFile;
    }

    ingestGigaLoggedTurnsDetached(ctx, blocked);
    process.env.ATHANOR_GIGA_ENABLED = "1";
    process.env.ATHANOR_REPLAY_MODE = "0";
    const allowed = context();
    const admitted = turns("admitted-source", 1);
    ingestGigaLoggedTurnsDetached(allowed, admitted);
    await flushGigaTurns();

    expect(sent(ctx.sessionManager.getSessionId())).toEqual([]);
    expect(sent(allowed.sessionManager.getSessionId()).flat().map((turn) => turn.source_id))
      .toEqual(admitted.map((turn) => turn.sourceID));
    expect(gigaBufferedTurnCensus()).toEqual([]);
  });
}

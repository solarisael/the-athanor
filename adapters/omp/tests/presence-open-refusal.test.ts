import { afterEach, beforeEach, expect, test } from "bun:test";

import { HostRefused, HostUnavailable } from "../house-proof/host.ts";
import { compilePresenceContext } from "../house-proof/presence.ts";
import { registerTopLevelSession, retireTopLevelSession } from "../house-proof/top-level-session-fence.ts";

// 2026-09-05, live: after `sleep` closed the frame and the keeper resumed the
// session, the Host refused the reopen as a replay body conflict. The adapter
// treated the refusal as absence and retried it four times, warning each
// time, on every turn. Asking again never changes a refusal.

const SESSION = "01a0730b-1e40-7383-8209-4af4316a65e6";
const savedEnv: Record<string, string | undefined> = {};

beforeEach(() => {
  for (const key of ["ATHANOR_HOST_URL", "ATHANOR_HOST_TOKEN", "ATHANOR_HOST_HOUSE_ID"]) savedEnv[key] = process.env[key];
  process.env.ATHANOR_HOST_TOKEN = "test-token";
  process.env.ATHANOR_HOST_HOUSE_ID = "solarisael";
  registerTopLevelSession("kodo", SESSION);
});

afterEach(() => {
  retireTopLevelSession("kodo", SESSION);
  for (const [key, value] of Object.entries(savedEnv)) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
});

function refusingHost(reason: string) {
  const opens: string[] = [];
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
        opens.push(String(command.idempotency_key ?? ""));
        socket.send(JSON.stringify({
          correlation_id: command.message_id,
          command_or_event_type: "athanor.presence.command_refused",
          reason,
        }));
      },
    },
  });
  process.env.ATHANOR_HOST_URL = `ws://127.0.0.1:${server.port}`;
  return { server, opens };
}

function input() {
  return {
    binding: { room: "kodo", spirit: "Kodo", session: SESSION },
    operator: "Sol",
    prompt: "shalom dummy",
    turnId: "turn:1",
    openRetry: { delaysMs: [1, 1, 1, 1], deadlineMs: 5_000 },
  };
}

test("a refused Presence open throws once and is never retried", async () => {
  const reason = "Presence idempotency key presence-open:01a0730b-1e40-7383-8209-4af4316a65e6 already answered a different open body";
  const { server, opens } = refusingHost(reason);
  const warnings: string[] = [];
  const warn = console.warn;
  console.warn = (...args: unknown[]) => { warnings.push(args.map(String).join(" ")); };
  try {
    const failure = await compilePresenceContext(input()).then(() => null, (error) => error);
    expect(failure).toBeInstanceOf(HostRefused);
    expect(failure).toBeInstanceOf(HostUnavailable);
    expect(failure.message).toBe(reason);
  } finally {
    console.warn = warn;
    server.stop(true);
  }
  expect(opens).toHaveLength(1);
  expect(warnings).toEqual([]);
});

test("an unreachable Host is still retried on the bounded schedule", async () => {
  // A port nobody listens on.
  const probe = Bun.serve({ port: 0, hostname: "127.0.0.1", fetch: () => new Response("") });
  const port = probe.port;
  probe.stop(true);
  process.env.ATHANOR_HOST_URL = `ws://127.0.0.1:${port}`;
  const warnings: string[] = [];
  const warn = console.warn;
  console.warn = (...args: unknown[]) => { warnings.push(args.map(String).join(" ")); };
  try {
    const failure = await compilePresenceContext(input()).then(() => null, (error) => error);
    expect(failure).toBeInstanceOf(HostUnavailable);
    expect(failure).not.toBeInstanceOf(HostRefused);
    expect(failure.message).toBe(`Athanor Host is unavailable at ws://127.0.0.1:${port}/room/kodo/athanor/v1/ws`);
  } finally {
    console.warn = warn;
  }
  expect(warnings).toHaveLength(4);
  expect(warnings.every((line) => line.startsWith("[athanor] Presence open retrying in 1ms") && line.endsWith(`is unavailable at ws://127.0.0.1:${port}/room/kodo/athanor/v1/ws`))).toBe(true);
});

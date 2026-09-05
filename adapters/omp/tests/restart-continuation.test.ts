import { afterEach, beforeEach, expect, test } from "bun:test";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

import { registerRestartDoor } from "../house-proof/restart-door.ts";

// 2026-09-05, live: the keeper relaunched `omp --resume <session>`, the House
// verified the successor, and then nothing happened. The spirit sat idle with
// its history until the operator typed. A restart that nobody continues is a
// cold boot with history.

const ROOM = path.join(tmpdir(), "athanor-restart-continuation", "kodo");
const SESSION = "01a0730b-1e40-7383-8209-4af4316a65e6";
const INTENT = "7f892471-8bfb-4e98-9646-008470c42ee9";

const savedEnv: Record<string, string | undefined> = {};
const ENV = [
  "ATHANOR_RESTART_INTENT_ID",
  "ATHANOR_RESTART_SUCCESSOR_PROOF",
  "ATHANOR_RESTART_VERIFY_CAPABILITY",
];

type Sent = { message: Record<string, unknown>; options: Record<string, unknown> };

function fakePi() {
  const handlers = new Map<string, Array<(event: unknown, ctx: unknown) => Promise<void> | void>>();
  const sent: Sent[] = [];
  const pi = {
    zod: { object: (s: unknown) => s, enum: () => ({ describe: () => ({}) }), string: () => ({ describe: () => ({}) }) },
    on(name: string, handler: (event: unknown, ctx: unknown) => Promise<void> | void) {
      handlers.set(name, [...(handlers.get(name) ?? []), handler]);
    },
    sendMessage(message: Record<string, unknown>, options: Record<string, unknown>) {
      sent.push({ message, options });
    },
  };
  return { pi, handlers, sent };
}

function ctxFor(notices: string[]) {
  return {
    cwd: ROOM,
    mode: "tui",
    sessionManager: { getSessionId: () => SESSION },
    ui: { notify: (text: string) => { notices.push(text); } },
  };
}

beforeEach(() => {
  for (const key of ENV) savedEnv[key] = process.env[key];
  mkdirSync(ROOM, { recursive: true });
  writeFileSync(path.join(ROOM, "active_spirit.md"), "# Active Spirit: Kodo\n", "utf8");
});

afterEach(() => {
  for (const [key, value] of Object.entries(savedEnv)) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
  rmSync(path.dirname(ROOM), { recursive: true, force: true });
});

test("a verified successor is handed one continuation turn carrying the reason", async () => {
  process.env.ATHANOR_RESTART_INTENT_ID = INTENT;
  process.env.ATHANOR_RESTART_SUCCESSOR_PROOF = "a".repeat(64);
  process.env.ATHANOR_RESTART_VERIFY_CAPABILITY = "verify-secret";
  const { pi, handlers, sent } = fakePi();
  const domain: Array<{ op: string; params: Record<string, unknown> }> = [];
  registerRestartDoor(pi, {
    requestDomain: async (op: string, params: Record<string, unknown>) => {
      domain.push({ op, params });
      if (op === "restart_verify") return { ok: true, state: "verified" };
      if (op === "restart_status") {
        return {
          ok: true,
          workspace: params.workspace,
          intent: {
            intentId: INTENT,
            state: "verified",
            mode: "resume",
            sessionId: SESSION,
            reason: "prove the Presence reopen fix on the wake turn",
            deadlines: { expiresAt: "2026-09-05T22:00:00Z" },
          },
        };
      }
      throw new Error(`unexpected domain call ${op}`);
    },
    transports: new Map(),
    registerTool: () => {},
    isEmbodied: () => true,
  });
  const notices: string[] = [];
  for (const handler of handlers.get("session_start") ?? []) await handler({}, ctxFor(notices));

  expect(domain.map((call) => call.op)).toEqual(["restart_verify", "restart_status"]);
  expect(domain[1]!.params).toEqual({ workspace: ROOM, intentId: INTENT });
  expect(notices).toEqual(["Athanor restart successor verified."]);
  expect(sent).toHaveLength(1);
  expect(sent[0]!.options).toEqual({ deliverAs: "nextTurn", triggerTurn: true });
  expect(sent[0]!.message.customType).toBe("athanor-restart-continuation");
  expect(sent[0]!.message.display).toBe(true);
  expect(String(sent[0]!.message.content)).toContain("Reason given: prove the Presence reopen fix on the wake turn");
  expect(String(sent[0]!.message.content)).toContain("mode resume");
  expect(sent[0]!.message.details).toEqual({
    intentId: INTENT,
    mode: "resume",
    reason: "prove the Presence reopen fix on the wake turn",
  });

  // A second start in the same process (session_switch) continues nothing:
  // the intent is already verified and its environment is gone.
  for (const handler of handlers.get("session_switch") ?? []) await handler({}, ctxFor(notices));
  expect(sent).toHaveLength(1);
  expect(process.env.ATHANOR_RESTART_INTENT_ID).toBeUndefined();
});

test("a bare start with no intent continues nothing", async () => {
  for (const key of ENV) delete process.env[key];
  const { pi, handlers, sent } = fakePi();
  const domain: string[] = [];
  registerRestartDoor(pi, {
    requestDomain: async (op: string) => { domain.push(op); return { ok: true }; },
    transports: new Map(),
    registerTool: () => {},
    isEmbodied: () => true,
  });
  const notices: string[] = [];
  for (const handler of handlers.get("session_start") ?? []) await handler({}, ctxFor(notices));
  expect(domain).toEqual([]);
  expect(sent).toEqual([]);
  expect(notices).toEqual([]);
});

test("a status the substrate cannot answer skips the continuation with a notice, after verification", async () => {
  process.env.ATHANOR_RESTART_INTENT_ID = INTENT;
  process.env.ATHANOR_RESTART_SUCCESSOR_PROOF = "a".repeat(64);
  process.env.ATHANOR_RESTART_VERIFY_CAPABILITY = "verify-secret";
  const { pi, handlers, sent } = fakePi();
  registerRestartDoor(pi, {
    requestDomain: async (op: string) => {
      if (op === "restart_verify") return { ok: true, state: "verified" };
      throw new Error("substrate went away");
    },
    transports: new Map(),
    registerTool: () => {},
    isEmbodied: () => true,
  });
  const notices: string[] = [];
  for (const handler of handlers.get("session_start") ?? []) await handler({}, ctxFor(notices));
  expect(notices).toEqual([
    "Athanor restart successor verified.",
    "Athanor restart continuation skipped: substrate went away",
  ]);
  expect(sent).toEqual([]);
});

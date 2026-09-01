import { afterAll, afterEach, describe, expect, mock, test } from "bun:test";
import {
  activeProjectFromEvidence,
  RecallPolicyHostClient,
  RecallPolicyHostUnavailable,
  hasToolEvidence,
  isMutateTool,
} from "../house-proof/recall-policy.ts";
import { hostCommand, hostSessionIdentity } from "../house-proof/host.ts";
import { roomContext } from "../house-proof/room.ts";
import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";

// The adapter's own tool registration needs OMP's zod surface, which a test pi
// does not have. Everything else in the graph stays real: the evidence must
// travel the product's own tap. Bun keeps this mock for the whole process, so
// the fakes answer only while this file runs; later files get the genuine
// tools surface, whose GIGA census the production-seam test reads.
const realTools = await import("../house-proof/tools.ts?real");
let toolsFixtureLive = true;
afterAll(() => {
  toolsFixtureLive = false;
});
mock.module("../house-proof/tools.ts", () => ({
  ...realTools,
  closeRustRememberTransports: () =>
    toolsFixtureLive ? undefined : realTools.closeRustRememberTransports(),
  registerSolarisaelTools: ((...args: any[]) =>
    toolsFixtureLive ? undefined : (realTools.registerSolarisaelTools as any)(...args)) as any,
  writeRustMemory: (async (...args: any[]) =>
    toolsFixtureLive ? undefined : (realTools.writeRustMemory as any)(...args)) as any,
}));

const originalWebSocket = globalThis.WebSocket;
const originalToken = process.env.ATHANOR_HOST_TOKEN;
const originalHouseId = process.env.ATHANOR_HOST_HOUSE_ID;
const originalHostUrl = process.env.ATHANOR_HOST_URL;

afterEach(() => {
  (globalThis as any).WebSocket = originalWebSocket;
  if (originalToken === undefined) delete process.env.ATHANOR_HOST_TOKEN;
  else process.env.ATHANOR_HOST_TOKEN = originalToken;
  if (originalHouseId === undefined) delete process.env.ATHANOR_HOST_HOUSE_ID;
  else process.env.ATHANOR_HOST_HOUSE_ID = originalHouseId;
  if (originalHostUrl === undefined) delete process.env.ATHANOR_HOST_URL;
  else process.env.ATHANOR_HOST_URL = originalHostUrl;
});

function hostState() {
  return {
    requestedMode: "auto",
    resolvedMode: "work",
    activeProject: "the-athanor",
    resolutionReason: "technical-project",
    lastRefreshReason: null,
    lastRefreshAt: null,
    workingSetEntries: 0,
    recoveryPending: false,
    recoveryTerms: [],
    degraded: null,
    updatedAt: "2026-08-10T00:00:00.000Z",
  };
}

describe("Recall Policy Host adapter", () => {
  test("namespaces caller idempotency by Host binding while preserving exact retries", () => {
    process.env.ATHANOR_HOST_HOUSE_ID = "solarisael";
    const parent = { room: "kodo", spirit: "Kodo", session: "shared-parent" };
    const child = { room: "kodo", spirit: "Kodo", session: "child-session" };
    const commandType = "athanor.recall_policy.evaluate";
    const requestedKey = "id:shared-user-message:evaluate";

    const first = hostCommand(parent, commandType, "recall_policy", { facts: { working: false } }, requestedKey);
    const retry = hostCommand(parent, commandType, "recall_policy", { facts: { working: false } }, requestedKey);
    const changedBody = hostCommand(parent, commandType, "recall_policy", { facts: { working: true } }, requestedKey);
    const subagent = hostCommand(child, commandType, "recall_policy", { facts: { working: true } }, requestedKey);

    expect(retry.idempotency_key).toBe(first.idempotency_key);
    expect(changedBody.idempotency_key).toBe(first.idempotency_key);
    expect(subagent.idempotency_key).not.toBe(first.idempotency_key);
  });

  test("uses the harness session manager before the shared working directory fallback", () => {
    expect(hostSessionIdentity({
      sessionManager: { getSessionId: () => "subagent-session-7" },
      cwd: "C:\\Solarisael\\Obsidian\\obsidian\\kodo",
    }, "fallback")).toBe("subagent-session-7");
    expect(hostSessionIdentity({
      sessionManager: { getSessionId: () => "" },
      cwd: "C:\\Solarisael\\Obsidian\\obsidian\\kodo",
    }, "fallback")).toBe("C:\\Solarisael\\Obsidian\\obsidian\\kodo");
  });


  test("sends facts to Host and consumes its decision without evaluating policy locally", async () => {
    process.env.ATHANOR_HOST_TOKEN = "test-token";
    process.env.ATHANOR_HOST_HOUSE_ID = "solarisael";
    process.env.ATHANOR_HOST_URL = "ws://127.0.0.1:8787";
    let command: Record<string, any> | null = null;

    class FakeWebSocket {
      listeners = new Map<string, Array<(event: any) => void>>();

      constructor(url: string, options: any) {
        expect(url).toBe("ws://127.0.0.1:8787/room/kintsu/athanor/v1/ws");
        expect(options.headers.Authorization).toBe("Bearer test-token");
        queueMicrotask(() => this.emit("open", {}));
      }

      addEventListener(kind: string, listener: (event: any) => void) {
        const listeners = this.listeners.get(kind) || [];
        listeners.push(listener);
        this.listeners.set(kind, listeners);
      }

      send(payload: string) {
        command = JSON.parse(payload);
        queueMicrotask(() => this.emit("message", {
          data: JSON.stringify({
            schema_version: 1,
            event_id: "event-1",
            command_or_event_type: "athanor.recall_policy.command_accepted",
            correlation_id: command!.message_id,
            sequence: 12,
            state_hash: "hash-12",
            version: 9,
            state: hostState(),
            decision: {
              shouldRecall: true,
              clearWorkingSet: false,
              query: "athanor recall policy",
              queryTerms: ["athanor", "recall", "policy"],
              refreshReason: "empty-working-set",
              intent: "technical_project",
              resolvedMode: "work",
            },
          }),
        }));
      }

      close() {}

      emit(kind: string, event: any) {
        for (const listener of this.listeners.get(kind) || []) listener(event);
      }
    }

    (globalThis as any).WebSocket = FakeWebSocket;
    const result = await new RecallPolicyHostClient({
      room: "kintsu",
      spirit: "Kintsu",
      session: "session-1",
    }).evaluate({
      queryRoute: {
        intent: "technical_project",
        terms: ["athanor", "recall", "policy"],
        requiredTerms: ["athanor"],
        recognizedEntities: [],
      },
      activeProject: "the-athanor",
      conversationTokens: 200,
      workingSetPresent: false,
    });

    expect(command).toMatchObject({
      command_or_event_type: "athanor.recall_policy.evaluate",
      sender_room: "kintsu",
      sender_session: "session-1",
      facts: {
        active_project: "the-athanor",
        conversation_tokens: 200,
        working_set_present: false,
      },
    });
    expect(result.decision).toMatchObject({
      shouldRecall: true,
      resolvedMode: "work",
      refreshReason: "empty-working-set",
    });
    expect(result.snapshot).toMatchObject({ version: 9, sequence: 12, stateHash: "hash-12" });
  });


  test("reports degraded Host instead of creating a fallback policy owner", async () => {
    delete process.env.ATHANOR_HOST_TOKEN;
    process.env.ATHANOR_HOST_HOUSE_ID = "solarisael";
    const client = new RecallPolicyHostClient({
      room: "kintsu",
      spirit: "Kintsu",
      session: "session-1",
    });
    await expect(client.inspect()).rejects.toBeInstanceOf(RecallPolicyHostUnavailable);
  });

  test("scopes a room on the shared Host instead of routing another room", async () => {
    process.env.ATHANOR_HOST_TOKEN = "test-token";
    process.env.ATHANOR_HOST_HOUSE_ID = "solarisael";
    process.env.ATHANOR_HOST_URL = "ws://127.0.0.1:8787";
    class RefusingWebSocket {
      constructor(url: string) {
        expect(url).toBe("ws://127.0.0.1:8787/room/kintsu/athanor/v1/ws");
        throw new Error("offline");
      }
    }
    (globalThis as any).WebSocket = RefusingWebSocket;
    const client = new RecallPolicyHostClient({
      room: "kintsu",
      spirit: "Kintsu",
      session: "session-1",
    });
    await expect(client.inspect()).rejects.toBeInstanceOf(RecallPolicyHostUnavailable);
  });

});

describe("hands-on-files evidence", () => {
  const casualRoute = {
    intent: "casual_contact",
    terms: ["hello"],
    requiredTerms: [],
    recognizedEntities: [],
  };

  function installFakeHost(): Array<Record<string, any>> {
    process.env.ATHANOR_HOST_TOKEN = "test-token";
    process.env.ATHANOR_HOST_HOUSE_ID = "solarisael";
    process.env.ATHANOR_HOST_URL = "ws://127.0.0.1:8787";
    const commands: Array<Record<string, any>> = [];

    class FakeWebSocket {
      listeners = new Map<string, Array<(event: any) => void>>();

      constructor() {
        queueMicrotask(() => this.emit("open", {}));
      }

      addEventListener(kind: string, listener: (event: any) => void) {
        const listeners = this.listeners.get(kind) || [];
        listeners.push(listener);
        this.listeners.set(kind, listeners);
      }

      send(payload: string) {
        const command = JSON.parse(payload);
        commands.push(command);
        queueMicrotask(() => this.emit("message", {
          data: JSON.stringify({
            schema_version: 1,
            event_id: `event-${commands.length}`,
            command_or_event_type: "athanor.recall_policy.command_accepted",
            correlation_id: command.message_id,
            sequence: commands.length,
            state_hash: `hash-${commands.length}`,
            version: commands.length,
            state: hostState(),
            decision: {
              shouldRecall: false,
              clearWorkingSet: false,
              query: "",
              queryTerms: [],
              refreshReason: null,
              intent: "casual_contact",
              resolvedMode: "work",
            },
          }),
        }));
      }

      close() {}

      emit(kind: string, event: any) {
        for (const listener of this.listeners.get(kind) || []) listener(event);
      }
    }

    (globalThis as any).WebSocket = FakeWebSocket;
    return commands;
  }

  // The taps are read from the real adapter registration: the evidence must
  // reach the shared stash through index.ts wiring, not through a test-local
  // call the product never makes.
  async function toolCallTaps(): Promise<Array<(event: any, ctx: any) => Promise<unknown>>> {
    const { default: registerAdapter } = await import("../index.ts?recall-policy-evidence");
    const taps: Array<(event: any, ctx: any) => Promise<unknown>> = [];
    registerAdapter({
      setLabel() {},
      events: { on: () => () => undefined },
      on(name: string, handler: any) {
        if (name === "tool_call") taps.push(handler);
      },
      sendMessage() {},
    });
    if (!taps.length) throw new Error("tool_call hook was not registered");
    return taps;
  }

  async function runToolCall(event: any, ctx: any): Promise<void> {
    for (const tap of await toolCallTaps()) await tap(event, ctx);
  }

  function bindingFor(session: string) {
    const { room, spirit } = roomContext(process.cwd());
    return { room, spirit, session };
  }

  // Kills: the mark dropped at the tool tap, or marked and then never carried
  // on the evaluate wire. Either break leaves auto mode blind to active work.
  // red-proof: delete markToolEvidence from the tool_call tap, or drop
  // tool_evidence from the evaluate facts.
  test("an edit tool_call marks the session and the next evaluate carries the flag", async () => {
    const binding = bindingFor("evidence-marked-session");
    expect(hasToolEvidence(binding)).toBe(false);
    await runToolCall(
      {
        toolName: "edit",
        toolCallId: "evidence-call-1",
        input: { input: "[src/a.ts#1A2B]\nPUT 1.=1:\n+const a = 1;\n" },
      },
      { cwd: process.cwd(), sessionID: binding.session },
    );
    expect(hasToolEvidence(binding)).toBe(true);

    const commands = installFakeHost();
    await new RecallPolicyHostClient(binding).evaluate({
      queryRoute: casualRoute,
      workingSetPresent: false,
      toolEvidence: hasToolEvidence(binding),
    });
    expect(commands.at(-1)).toMatchObject({
      command_or_event_type: "athanor.recall_policy.evaluate",
      facts: { tool_evidence: true },
    });
  });

  // Kills: evidence claimed by any tool at all (a read counts), and a clean
  // session shipping `tool_evidence: false` — a Host built before this field
  // refuses unknown fact keys outright, so the absent case must stay absent.
  // red-proof: replace the isMutateTool guard with `true`, or send
  // `tool_evidence: Boolean(input.toolEvidence)` unconditionally.
  test("a session with no mutate call claims nothing and sends no flag", async () => {
    expect(isMutateTool("edit")).toBe(true);
    expect(isMutateTool("write")).toBe(true);
    for (const toolName of ["read", "grep", "bash", "task", "", null]) {
      expect(isMutateTool(toolName)).toBe(false);
    }

    const binding = bindingFor("evidence-clean-session");
    const commands = installFakeHost();
    await new RecallPolicyHostClient(binding).evaluate({
      queryRoute: casualRoute,
      workingSetPresent: false,
      toolEvidence: hasToolEvidence(binding),
    });
    expect("tool_evidence" in commands.at(-1)!.facts).toBe(false);
    expect(commands.at(-1)!.facts).toMatchObject({ working_set_present: false });
  });

  // Kills: fail-closed regression. An unreadable room must cost the work hint,
  // never the tool call.
  // red-proof: remove the try/catch around markToolEvidence in index.ts.
  test("an unreadable room costs the hint, never the tool call", async () => {
    const hostile = {
      get cwd(): string {
        throw new Error("room directory is unreadable");
      },
      sessionID: "evidence-hostile-session",
    };
    await runToolCall(
      { toolName: "write", toolCallId: "evidence-call-2", input: { path: "a.ts", content: "const a = 1;\n" } },
      hostile,
    );
    expect(hasToolEvidence(bindingFor("evidence-hostile-session"))).toBe(false);
  });

  // Kills: losing the repo walk or the edit-path parser. Either break drops the
  // active project before the Host can apply project-aware Recall policy.
  // red-proof: return null from activeProjectFromEvidence.
  test("an absolute edit inside a repo names the active project", async () => {
    const repo = mkdtempSync(join(tmpdir(), "athanor-evidence-repo-"));
    try {
      mkdirSync(join(repo, ".git"));
      const binding = bindingFor("evidence-project-session");
      const target = join(repo, "src", "main.rs");
      await runToolCall(
        {
          toolName: "edit",
          toolCallId: "evidence-project-1",
          input: { input: `[${target}#1A2B]\nPUT 1.=1:\n+fn main() {}\n` },
        },
        { cwd: process.cwd(), sessionID: binding.session },
      );
      expect(activeProjectFromEvidence(binding)).toBe(basename(repo).toLowerCase());
    } finally {
      rmSync(repo, { recursive: true, force: true });
    }
  });
});

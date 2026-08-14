import { afterEach, describe, expect, test } from "bun:test";
import {
  RecallPolicyHostClient,
  RecallPolicyHostUnavailable,
} from "../solarisael-house-proof/recall-policy.ts";
import { hostCommand, hostSessionIdentity } from "../solarisael-house-proof/host.ts";

const originalWebSocket = globalThis.WebSocket;
const originalToken = process.env.ATHANOR_HOST_TOKEN;
const originalHouseId = process.env.ATHANOR_HOST_HOUSE_ID;
const originalEndpoints = process.env.ATHANOR_HOST_ENDPOINTS;
const originalHostUrl = process.env.ATHANOR_HOST_WS_URL;

afterEach(() => {
  (globalThis as any).WebSocket = originalWebSocket;
  if (originalToken === undefined) delete process.env.ATHANOR_HOST_TOKEN;
  else process.env.ATHANOR_HOST_TOKEN = originalToken;
  if (originalHouseId === undefined) delete process.env.ATHANOR_HOST_HOUSE_ID;
  else process.env.ATHANOR_HOST_HOUSE_ID = originalHouseId;
  if (originalEndpoints === undefined) delete process.env.ATHANOR_HOST_ENDPOINTS;
  else process.env.ATHANOR_HOST_ENDPOINTS = originalEndpoints;
  if (originalHostUrl === undefined) delete process.env.ATHANOR_HOST_WS_URL;
  else process.env.ATHANOR_HOST_WS_URL = originalHostUrl;
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
    delete process.env.ATHANOR_HOST_WS_URL;
    process.env.ATHANOR_HOST_ENDPOINTS = JSON.stringify({
      kintsu: { url: "ws://127.0.0.1:8787/athanor/v1/ws", spirit: "Kintsu" },
      kodo: { url: "ws://127.0.0.1:8788/athanor/v1/ws", spirit: "Kodo" },
    });
    let command: Record<string, any> | null = null;

    class FakeWebSocket {
      listeners = new Map<string, Array<(event: any) => void>>();

      constructor(url: string, options: any) {
        expect(url).toBe("ws://127.0.0.1:8787/athanor/v1/ws");
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

  test("refuses a missing room endpoint instead of routing to the wrong Host", async () => {
    process.env.ATHANOR_HOST_TOKEN = "test-token";
    process.env.ATHANOR_HOST_HOUSE_ID = "solarisael";
    delete process.env.ATHANOR_HOST_WS_URL;
    process.env.ATHANOR_HOST_ENDPOINTS = JSON.stringify({
      kodo: { url: "ws://127.0.0.1:8788/athanor/v1/ws", spirit: "Kodo" },
    });
    const client = new RecallPolicyHostClient({
      room: "kintsu",
      spirit: "Kintsu",
      session: "session-1",
    });
    await expect(client.inspect()).rejects.toThrow("no installed Athanor Host endpoint exists for room kintsu");
  });

});

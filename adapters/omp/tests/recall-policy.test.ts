import { afterEach, describe, expect, test } from "bun:test";
import {
  compactAutomaticRecallPayload,
  normalizePersistedRecallPolicy,
  RecallPolicyHostClient,
  RecallPolicyHostUnavailable,
} from "../solarisael-house-proof/recall-policy.ts";

const originalWebSocket = globalThis.WebSocket;
const originalToken = process.env.ATHANOR_HOST_TOKEN;
const originalHouseId = process.env.ATHANOR_HOST_HOUSE_ID;

afterEach(() => {
  (globalThis as any).WebSocket = originalWebSocket;
  if (originalToken === undefined) delete process.env.ATHANOR_HOST_TOKEN;
  else process.env.ATHANOR_HOST_TOKEN = originalToken;
  if (originalHouseId === undefined) delete process.env.ATHANOR_HOST_HOUSE_ID;
  else process.env.ATHANOR_HOST_HOUSE_ID = originalHouseId;
});

function hostState() {
  return {
    requested_mode: "auto",
    resolved_mode: "work",
    active_project: "the-athanor",
    resolution_reason: "technical-project",
    last_refresh_reason: null,
    last_refresh_at: null,
    working_set_entries: 0,
    recovery_pending: false,
    recovery_terms: [],
    degraded: null,
    updated_at: "2026-08-10T00:00:00.000Z",
  };
}

describe("Recall Policy Host adapter", () => {
  test("normalizes the authenticated Host projection for existing GUI/tool contracts", () => {
    expect(normalizePersistedRecallPolicy(hostState())).toEqual({
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
    });
  });

  test("sends facts to Host and consumes its decision without evaluating policy locally", async () => {
    process.env.ATHANOR_HOST_TOKEN = "test-token";
    process.env.ATHANOR_HOST_HOUSE_ID = "solarisael";
    let command: Record<string, any> | null = null;

    class FakeWebSocket {
      listeners = new Map<string, Array<(event: any) => void>>();

      constructor(_url: string, options: any) {
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
              should_recall: true,
              clear_working_set: false,
              query: "athanor recall policy",
              query_terms: ["athanor", "recall", "policy"],
              refresh_reason: "empty-working-set",
              intent: "technical_project",
              resolved_mode: "work",
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

  test("automatic context remains compact presentation data", () => {
    const compact = compactAutomaticRecallPayload({
      warnings: ["w".repeat(400), "second", "third"],
      retrievalCandidates: [
        { title: "first", excerpt: "x".repeat(800), matched_terms: [1, 2, 3, 4, 5], reasons: [1, 2, 3, 4] },
        { title: "second", excerpt: "ok" },
        { title: "third", excerpt: "drop" },
      ],
    });
    expect(compact.retrievalCandidates).toHaveLength(2);
    expect(compact.retrievalCandidates[0].excerpt).toHaveLength(480);
    expect(compact.warnings).toHaveLength(2);
    expect(compact.warnings[0]).toHaveLength(240);
  });
});

import { afterEach, describe, expect, test } from "bun:test";

import {
  closePresence,
  compilePresence,
  compilePresenceContext,
  openPresence,
  settlePresence,
} from "../solarisael-house-proof/presence.ts";
import { anamnesisMaterial, paperBoatMaterial } from "../solarisael-house-proof/presence-materials.ts";
import {
  registerTopLevelSession,
  retireTopLevelSession,
  topLevelSession,
} from "../solarisael-house-proof/top-level-session-fence.ts";

const originalWebSocket = globalThis.WebSocket;
const originalToken = process.env.ATHANOR_HOST_TOKEN;
const originalHouseId = process.env.ATHANOR_HOST_HOUSE_ID;
const originalHostUrl = process.env.ATHANOR_HOST_WS_URL;
let activeRoom = "";

function installFakeHost(options: { frameVersion?: number; contractVersion?: number } = {}) {
  process.env.ATHANOR_HOST_TOKEN = "test-token";
  process.env.ATHANOR_HOST_HOUSE_ID = "solarisael";
  process.env.ATHANOR_HOST_WS_URL = "ws://127.0.0.1:8787/athanor/v1/ws";
  const commands: Array<Record<string, any>> = [];
  const frameVersion = options.frameVersion ?? 7;
  const contractVersion = options.contractVersion ?? 2;

  class FakeWebSocket {
    listeners = new Map<string, Array<(event: any) => void>>();

    constructor(url: string, request: { headers: { Authorization: string } }) {
      expect(url).toBe("ws://127.0.0.1:8787/athanor/v1/ws");
      expect(request.headers.Authorization).toBe("Bearer test-token");
      queueMicrotask(() => this.emit("open", {}));
    }

    addEventListener(kind: string, listener: (event: any) => void) {
      const listeners = this.listeners.get(kind) || [];
      listeners.push(listener);
      this.listeners.set(kind, listeners);
    }

    send(payload: string) {
      const command = JSON.parse(payload) as Record<string, any>;
      commands.push(command);
      const event = command.command_or_event_type === "athanor.presence.open"
        ? {
          command_or_event_type: "athanor.presence.opened",
          result: {
            operation: "open",
            value: { frameId: "frame-top", version: frameVersion, rendered: "frame" },
          },
        }
        : command.command_or_event_type === "athanor.presence.compile"
          ? {
            command_or_event_type: "athanor.presence.compiled",
            result: {
              operation: "compile",
              value: {
                contractId: "contract-top",
                frameId: "frame-top",
                turnId: "turn-1",
                version: contractVersion,
                rendered: "contract",
                guards: [],
              },
            },
          }
          : command.command_or_event_type === "athanor.presence.settle"
            ? {
              command_or_event_type: "athanor.presence.settled",
              result: { operation: "settle", value: { contractId: "contract-top" } },
            }
            : {
              command_or_event_type: "athanor.presence.closed",
              result: {
                operation: "close",
                value: { frameId: "frame-top", body: "boat", provenanceDigest: "digest" },
              },
            };
      queueMicrotask(() => this.emit("message", {
        data: JSON.stringify({
          schema_version: 1,
          event_id: `presence-event-${commands.length}`,
          correlation_id: command.message_id,
          ...event,
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

function binding(room: string, session: string) {
  return { room, spirit: "Kintsu", session };
}

function openRequest(session: string) {
  return {
    binding: { room: "presence-door-room", spirit: "Kintsu", operator: "Sol", session },
    identity: [],
  };
}

function compileRequest() {
  return {
    frameId: "frame-top",
    turnId: "turn-1",
    userText: "hello",
    frameVersion: 7,
  };
}

afterEach(() => {
  if (activeRoom) retireTopLevelSession(activeRoom, "top-level-session");
  activeRoom = "";
  (globalThis as any).WebSocket = originalWebSocket;
  if (originalToken === undefined) delete process.env.ATHANOR_HOST_TOKEN;
  else process.env.ATHANOR_HOST_TOKEN = originalToken;
  if (originalHouseId === undefined) delete process.env.ATHANOR_HOST_HOUSE_ID;
  else process.env.ATHANOR_HOST_HOUSE_ID = originalHouseId;
  if (originalHostUrl === undefined) delete process.env.ATHANOR_HOST_WS_URL;
  else process.env.ATHANOR_HOST_WS_URL = originalHostUrl;
});

describe("Presence client session fence", () => {
  test("accepts the registered top-level session and refuses every worker door", async () => {
    const room = "presence-door-room";
    const top = "top-level-session";
    const worker = "worker-session";
    activeRoom = room;
    registerTopLevelSession(room, top);
    const commands = installFakeHost();
    const topBinding = binding(room, top);
    const workerBinding = binding(room, worker);

    await expect(openPresence(topBinding, openRequest(top), "open-top")).resolves.toMatchObject({
      frameId: "frame-top",
    });
    await expect(compilePresence(topBinding, compileRequest(), "compile-top")).resolves.toMatchObject({
      contractId: "contract-top",
    });
    await expect(settlePresence(topBinding, { contractId: "contract-top" }, "settle-top"))
      .resolves.toMatchObject({ contractId: "contract-top" });
    await expect(closePresence(topBinding, { frameId: "frame-top", body: "boat" }, "close-top"))
      .resolves.toMatchObject({ frameId: "frame-top" });
    expect(topLevelSession(room)).toBe(top);

    const refusal = "Presence requires the authenticated top-level OMP session";
    await expect(openPresence(workerBinding, openRequest(worker), "open-worker")).rejects.toThrow(refusal);
    await expect(compilePresence(workerBinding, compileRequest(), "compile-worker")).rejects.toThrow(refusal);
    await expect(settlePresence(workerBinding, { contractId: "contract-top" }, "settle-worker"))
      .rejects.toThrow(refusal);
    await expect(closePresence(workerBinding, { frameId: "frame-top", body: "boat" }, "close-worker"))
      .rejects.toThrow(refusal);
    expect(commands).toHaveLength(4);
    expect(commands.map((command) => command.sender_session)).toEqual([top, top, top, top]);
  });

  test("reports the opened frame version instead of the compiled contract version", async () => {
    const room = "presence-version-room";
    const top = "presence-version-top";
    activeRoom = room;
    registerTopLevelSession(room, top);
    const commands = installFakeHost({ frameVersion: 17, contractVersion: 3 });
    const previousBoat = paperBoatMaterial({ memoryId: "42", letter: "p".repeat(5000) });
    const anamnesis = anamnesisMaterial("a".repeat(5000));
    const compiled = await compilePresenceContext({
      binding: binding(room, top),
      operator: "Sol",
      prompt: "hello",
      turnId: "turn-1",
      previousBoat,
      anamnesis,
    });

    expect(compiled).toMatchObject({
      frameId: "frame-top",
      frameVersion: 17,
      contractId: "contract-top",
    });
    const openCommand = commands.find((command) => command.command_or_event_type === "athanor.presence.open");
    expect(openCommand?.presence_open.anamnesis[0].body).toBe("a".repeat(4096));
    expect(openCommand?.presence_open.previousBoat.body).toBe("p".repeat(4096));
    expect(openCommand?.presence_open.anamnesis[0].authority).toEqual({
      kind: "anamnesis",
      source: "anamnesis:wake",
    });
    expect(openCommand?.presence_open.previousBoat.authority).toEqual({
      kind: "paper_boat",
      memory_id: 42,
    });
    const compileCommand = commands.find((command) => command.command_or_event_type === "athanor.presence.compile");
    expect(compileCommand?.presence_compile.frameVersion).toBe(17);
    expect(compileCommand?.presence_compile.sessionLedger).toBeUndefined();
  });
});

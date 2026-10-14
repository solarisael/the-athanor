import { afterEach, describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { roomContext, SelectedHomeUnavailable } from "../house-proof/room.ts";
import { hostSessionIdentity } from "../house-proof/host.ts";
import {
  __sessionTest,
  installPortableSession,
  persistedSelection,
  SELECTION_ENTRY,
  type Attachment,
} from "../house-proof/session.ts";

// A harness context the way OMP hands one to an extension: a cwd, plus a
// session manager naming the conversation and its file.
function harness(cwd: string, sessionId = "conversation-a", sessionFile = "", branch: unknown[] = [], kind: "main" | "sub" = "main") {
  return {
    cwd,
    agent: { kind, id: kind === "main" ? "Main" : "0-Task", name: kind, depth: kind === "main" ? 0 : 1 },
    sessionManager: {
      getSessionId: () => sessionId,
      getSessionFile: () => sessionFile || undefined,
      getBranch: () => branch,
    },
  };
}

function attachment(fields: Partial<Attachment>): Attachment {
  return {
    key: "", room: "kintsu", source: "selected", status: "attached", error: null,
    instanceId: "11111111-1111-4111-8111-111111111111", conversationId: "conversation-a", sessionFile: "",
    workspace: "", spirit: "Kintsu", operator: "Sol", homeDir: path.resolve("/homes/kintsu"),
    context: { revision: "sha256:abc", text: "You are Kintsu." }, controls: ["deliver"], chatDelivery: false,
    restart: { supported: false, launcher: null }, heartbeatSecs: 10, terminal: null, terminalReason: null,
    launcher: null, launcherReason: null, exiting: false, connected: true,
    ...fields,
  };
}

function keyOf(sessionFile: string): string {
  return `file:${process.platform === "win32" ? path.resolve(sessionFile).toLowerCase() : path.resolve(sessionFile)}`;
}

describe("OMP adapter contract", () => {
  afterEach(() => __sessionTest.reset());

  test("an unselected session in an arbitrary project is an ordinary harness session", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-contract-"));
    try {
      expect(roomContext(harness(path.join(root, "some-project")))).toBeNull();
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("rejects the reserved house room key from adapter room resolution", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-reserved-room-"));
    try {
      const cwd = path.join(root, "example");
      await mkdir(cwd);
      await writeFile(
        path.join(cwd, ".athanor-room.json"),
        `${JSON.stringify({ version: 1, room: "house", trueName: "Example Room", operator: "Example Operator" })}\n`,
        "utf8",
      );
      expect(roomContext(harness(cwd))).toMatchObject({ room: "example", operator: "Example Operator", source: "home" });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("uses a neutral operator when a marker omits one", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-operator-"));
    try {
      const cwd = path.join(root, "example");
      await mkdir(cwd);
      await writeFile(
        path.join(cwd, ".athanor-room.json"),
        `${JSON.stringify({ version: 1, room: "example", trueName: "Example Room" })}\n`,
        "utf8",
      );
      expect(roomContext(harness(cwd))).toMatchObject({
        room: "example",
        spirit: "Example Room",
        operator: "Operator",
      });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("retains explicit persisted room markers for the existing home launch", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-marker-"));
    try {
      const cwd = path.join(root, "example");
      await mkdir(cwd);
      await writeFile(
        path.join(cwd, ".athanor-room.json"),
        `${JSON.stringify({ version: 1, room: "custom-room", trueName: "Example Room", operator: "Example Operator" })}\n`,
        "utf8",
      );
      expect(roomContext(harness(cwd))).toMatchObject({ room: "custom-room", spirit: "Example Room", operator: "Example Operator" });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("Host control identity never falls back to the workspace", () => {
    expect(() => hostSessionIdentity({ cwd: "C:/work" } as any)).toThrow();
    expect(hostSessionIdentity(harness("C:/work", "conversation-b"))).toBe("conversation-b");
  });

  test("a selected home resolves per conversation, and its workers inherit it", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-selected-"));
    try {
      const project = path.join(root, "project");
      const fileA = path.join(root, "sessions", "a.jsonl");
      const fileB = path.join(root, "sessions", "b.jsonl");
      const workerFile = path.join(root, "sessions", "a", "0-Task.jsonl");
      await mkdir(path.dirname(workerFile), { recursive: true });
      await writeFile(fileA, "", "utf8");
      await writeFile(fileB, "", "utf8");
      await writeFile(workerFile, "", "utf8");
      __sessionTest.attachments.set(keyOf(fileA), attachment({ key: keyOf(fileA), sessionFile: fileA, workspace: project }));

      expect(roomContext(harness(project, "conversation-a", fileA))).toMatchObject({ room: "kintsu", spirit: "Kintsu", source: "attached" });
      // A second conversation in the same workspace keeps its own (absent) selection.
      expect(roomContext(harness(project, "conversation-b", fileB))).toBeNull();
      // A worker of conversation A acts for A's home, whatever its own cwd.
      expect(roomContext(harness(path.join(root, "elsewhere"), "worker-1", workerFile, [], "sub"))).toMatchObject({ room: "kintsu" });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("an unreachable selected home refuses instead of impersonating", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-failed-"));
    try {
      const file = path.join(root, "a.jsonl");
      __sessionTest.attachments.set(keyOf(file), attachment({ key: keyOf(file), status: "failed", error: "room_unknown" }));
      expect(() => roomContext(harness(root, "conversation-a", file))).toThrow(SelectedHomeUnavailable);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("the selection is read from the conversation's own custom entries", () => {
    const entry = (room: unknown) => ({ type: "custom", customType: SELECTION_ENTRY, data: { room } });
    expect(persistedSelection(harness("/", "c", "", []))).toBeUndefined();
    expect(persistedSelection(harness("/", "c", "", [entry("kintsu")]))).toBe("kintsu");
    expect(persistedSelection(harness("/", "c", "", [entry("kintsu"), { type: "compaction" }, entry(null)]))).toBeNull();
    expect(persistedSelection(harness("/", "c", "", [entry(null), { type: "compaction" }, entry("kodo")]))).toBe("kodo");
  });

  test("an unavailable selection refuses input while leaving the repair command reachable", async () => {
    const handlers = new Map<string, Array<(event: any, ctx: any) => any>>();
    const pi = {
      on: (name: string, handler: any) => handlers.set(name, [...(handlers.get(name) ?? []), handler]),
      registerCommand: () => {},
      appendEntry: () => {},
    };
    installPortableSession(pi);
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-context-"));
    try {
      const file = path.join(root, "a.jsonl");
      const ctx = { ...harness(root, "conversation-a", file), ui: { notify: () => {} }, abort: () => {} };
      __sessionTest.attachments.set(keyOf(file), attachment({ key: keyOf(file), status: "failed", error: "unauthorized" }));
      const input = handlers.get("input")![0];
      expect(await input({ type: "input", text: "hello" }, ctx)).toEqual({ handled: true });
      expect(await input({ type: "input", text: "/spirit off" }, ctx)).toBeUndefined();
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });


});

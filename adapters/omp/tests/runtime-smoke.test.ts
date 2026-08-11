import { afterEach, describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import solarisaelHouseProof from "../index.ts";
import { roomContext, statePathForRoom } from "../solarisael-house-proof/room.ts";
import { recallTelemetryPath } from "../solarisael-house-proof/recall-telemetry.ts";
import { closeRustRecallTransports } from "../solarisael-house-proof/recall.ts";
import { closeRustRememberTransports } from "../solarisael-house-proof/tools.ts";
import { closePaperBoatTransports } from "../solarisael-house-proof/substrate.ts";
import { RustJsonlTransport } from "../rust-transport.ts";


const originalWebSocket = globalThis.WebSocket;
const originalHostToken = process.env.ATHANOR_HOST_TOKEN;
const originalHostHouseId = process.env.ATHANOR_HOST_HOUSE_ID;
type CapturedTool = {
  name: string;
  description?: string;
  execute?: (
    toolCallId: string,
    params: Record<string, unknown>,
    signal: AbortSignal | null,
    onUpdate: unknown,
    ctx: Record<string, unknown>,
  ) => Promise<ToolResult>;
};

type ToolResult = {
  isError?: boolean;
  content?: Array<{ type: string; text: string }>;
  details?: Record<string, unknown>;
};

type CapturedHook = {
  name: string;
  handler: (event: Record<string, unknown>, ctx: Record<string, unknown>) => Promise<{ messages: unknown[] } | undefined>;
};

type Schema = {
  kind: "string" | "boolean" | "number" | "enum" | "object" | "array";
  describe(description: string): Schema;
  regex(pattern: RegExp): Schema;
  optional(): Schema;
  default(value: unknown): Schema;
};

const zodStub = {
  string() {
    return makeSchema("string");
  },
  boolean() {
    return makeSchema("boolean");
  },
  number() {
    return makeSchema("number");
  },
  enum(_values: string[]) {
    return makeSchema("enum");
  },
  object(_shape: Record<string, Schema>) {
    return makeSchema("object");
  },
  array(_element: Schema) {
    return makeSchema("array");
  },
};

const tempRoots: string[] = [];
const ENV_KEYS = [
  "ATHANOR_SUBSTRATE_EXE",
  "ATHANOR_SUBSTRATE_ROOT",
  "ATHANOR_HOST_TOKEN",
  "ATHANOR_HOST_HOUSE_ID",
  "SOLARISAEL_TEST_NATIVE_PYTHON",
];
function snapshotEnv() {
  return Object.fromEntries(ENV_KEYS.map((key) => [key, process.env[key]]));
}

function restoreEnv(snapshot: Record<string, string | undefined>) {
  for (const key of ENV_KEYS) {
    if (snapshot[key] === undefined) {
      delete process.env[key];
    } else {
      process.env[key] = snapshot[key];
    }
  }
}

function installRecallPolicyHostFake() {
  process.env.ATHANOR_HOST_TOKEN = "runtime-smoke-token";
  process.env.ATHANOR_HOST_HOUSE_ID = "solarisael";
  let requestedMode = "auto";
  let version = 1;
  let projection: Record<string, any> = {
    requested_mode: "auto",
    resolved_mode: "conversation",
    active_project: null,
    resolution_reason: "default",
    last_refresh_reason: null,
    last_refresh_at: null,
    working_set_entries: 0,
    recovery_pending: false,
    recovery_terms: [],
    degraded: null,
    updated_at: null,
  };
  const sessions = new Map<string, Record<string, any>>();

  class FakePolicyWebSocket {
    listeners = new Map<string, Array<(event: any) => void>>();
    constructor() { queueMicrotask(() => this.emit("open", {})); }
    addEventListener(kind: string, listener: (event: any) => void) {
      const listeners = this.listeners.get(kind) || [];
      listeners.push(listener);
      this.listeners.set(kind, listeners);
    }
    send(payload: string) {
      const command = JSON.parse(payload);
      const sessionKey = command.sender_session;
      const state = { ...(sessions.get(sessionKey) || projection) };
      let decision: Record<string, any> | undefined;
      switch (command.command_or_event_type) {
        case "athanor.recall_policy.set_requested_mode": {
          requestedMode = command.mutations[0].value;
          projection = {
            ...projection,
            requested_mode: requestedMode,
            resolved_mode: requestedMode === "auto" ? projection.resolved_mode : requestedMode,
            resolution_reason: requestedMode === "auto" ? "awaiting-auto-resolution" : "explicit-override",
          };
          break;
        }
        case "athanor.recall_policy.invalidate_after_compaction": {
          const recoveryTerms = String(command.compaction_summary || "")
            .toLowerCase()
            .match(/[a-z0-9_.:/+#-]{3,}/g) || [];
          Object.assign(state, {
            requested_mode: requestedMode,
            recovery_pending: true,
            recovery_terms: [...new Set(recoveryTerms)].slice(0, 12),
            working_set_entries: 0,
            last_refresh_reason: "compaction-invalidated",
          });
          sessions.set(sessionKey, state);
          projection = state;
          break;
        }
        case "athanor.recall_policy.evaluate": {
          const intent = command.facts.query_route.intent || "general";
          const activeProject = command.facts.active_project || null;
          const resolvedMode = requestedMode === "quiet"
            ? "quiet"
            : requestedMode === "work" || requestedMode === "conversation"
              ? requestedMode
              : intent === "technical_project"
                ? "work"
                : "conversation";
          const terms = [
            ...command.facts.query_route.required_terms,
            ...(state.recovery_terms || []),
            ...command.facts.query_route.terms,
          ].filter(Boolean);
          const uniqueTerms = [...new Set(terms.map((term: unknown) => String(term).toLowerCase()))];
          const eligible = resolvedMode !== "quiet"
            && (intent === "technical_project"
              || ["memory_lookup", "entity_lookup", "date_lookup"].includes(intent)
              || state.recovery_pending);
          const refreshReason = eligible
            ? state.recovery_pending
              ? "post-compaction-recovery"
              : command.facts.working_set_present
                ? null
                : "empty-working-set"
            : null;
          Object.assign(state, {
            requested_mode: requestedMode,
            resolved_mode: resolvedMode,
            active_project: activeProject,
            resolution_reason: requestedMode === "auto"
              ? intent === "technical_project" ? "technical-project" : "general"
              : "explicit-override",
          });
          decision = {
            should_recall: Boolean(refreshReason && uniqueTerms.length),
            clear_working_set: resolvedMode === "quiet",
            query: uniqueTerms.join(" "),
            query_terms: uniqueTerms,
            refresh_reason: refreshReason,
            intent,
            resolved_mode: resolvedMode,
          };
          sessions.set(sessionKey, state);
          projection = state;
          break;
        }
        case "athanor.recall_policy.complete_refresh": {
          Object.assign(state, {
            recovery_pending: false,
            recovery_terms: [],
            working_set_entries: command.refresh.entries,
            last_refresh_reason: command.refresh.refresh_reason,
            degraded: command.refresh.warning,
          });
          sessions.set(sessionKey, state);
          projection = state;
          break;
        }
        case "athanor.recall_policy.fail_refresh": {
          Object.assign(state, {
            last_refresh_reason: "failed",
            degraded: command.failure_reason,
          });
          sessions.set(sessionKey, state);
          projection = state;
          break;
        }
      }
      const snapshot = command.command_or_event_type === "athanor.recall_policy.subscribe";
      if (!snapshot) version += 1;
      queueMicrotask(() => this.emit("message", { data: JSON.stringify({
        command_or_event_type: snapshot
          ? "athanor.recall_policy.snapshot"
          : "athanor.recall_policy.command_accepted",
        correlation_id: command.message_id,
        version,
        sequence: version,
        state_hash: `runtime-hash-${version}`,
        state: projection,
        decision,
      }) }));
    }
    close() {}
    emit(kind: string, event: any) {
      for (const listener of this.listeners.get(kind) || []) listener(event);
    }
  }
  (globalThis as any).WebSocket = FakePolicyWebSocket;
}



async function removeTempRoot(root: string) {
  let lastError: unknown;
  for (let attempt = 0; attempt < 10; attempt += 1) {
    try {
      await rm(root, { recursive: true, force: true });
      return;
    } catch (error: any) {
      lastError = error;
      if (!["EBUSY", "EPERM", "ENOTEMPTY"].includes(error?.code)) throw error;
      await new Promise((resolve) => setTimeout(resolve, 50 * (attempt + 1)));
    }
  }
  throw lastError;
}
afterEach(async () => {
  (globalThis as any).WebSocket = originalWebSocket;
  if (originalHostToken === undefined) delete process.env.ATHANOR_HOST_TOKEN;
  else process.env.ATHANOR_HOST_TOKEN = originalHostToken;
  if (originalHostHouseId === undefined) delete process.env.ATHANOR_HOST_HOUSE_ID;
  else process.env.ATHANOR_HOST_HOUSE_ID = originalHostHouseId;
  await Promise.all(tempRoots.splice(0).map(removeTempRoot));
});

function makeSchema(kind: Schema["kind"]): Schema {
  return {
    kind,
    describe(_description: string) {
      return this;
    },
    regex(_pattern: RegExp) {
      return this;
    },
    optional() {
      return this;
    },
    default(_value: unknown) {
      return this;
    },
  };
}

async function makeTempSmokeCwd() {
  const root = await mkdtemp(path.join(os.tmpdir(), "omp-runtime-smoke-"));
  tempRoots.push(root);
  const cwd = path.join(root, "example");
  await mkdir(cwd, { recursive: true });
  await writeJson(path.join(cwd, ".solarisael-room.json"), {
    version: 1,
    room: "example",
    trueName: "Smoke Room",
    operator: "Test Operator",
  });
  return { root, cwd };
}

async function makeTempRoom(folder: string) {
  const root = await mkdtemp(path.join(os.tmpdir(), "omp-runtime-room-"));
  tempRoots.push(root);
  const cwd = path.join(root, folder);
  await mkdir(cwd, { recursive: true });
  return { root, cwd };
}

async function makeTempMarkedRoom() {
  const room = await makeTempRoom("example");
  await writeJson(path.join(room.cwd, ".solarisael-room.json"), {
    version: 1,
    room: "example",
    trueName: "Moonlit Example Room",
    operator: "Ada Lovelace",
  });
  return room;
}

function registerAdapter() {
  installRecallPolicyHostFake();
  const hooks: CapturedHook[] = [];
  const tools: CapturedTool[] = [];
  const appliedModels: string[] = [];

  const pi = {
    zod: zodStub,
    setLabel(_label: string) {},
    on(name: string, handler: CapturedHook["handler"]) {
      hooks.push({ name, handler });
    },
    registerTool(tool: CapturedTool) {
      tools.push(tool);
    },
    async setModel(model: string) {
      appliedModels.push(model);
    },
  };

  solarisaelHouseProof(pi);

  return { hooks, tools: Object.fromEntries(tools.map((tool) => [tool.name, tool])), appliedModels };
}

function tool(tools: Record<string, CapturedTool>, name: string) {
  const registered = tools[name];
  if (!registered?.execute) throw new Error(`Missing executable tool: ${name}`);
  return registered.execute;
}

async function executeTool(
  tools: Record<string, CapturedTool>,
  name: string,
  params: Record<string, unknown>,
  ctx: Record<string, unknown>,
) {
  return await tool(tools, name)(`test-${name}`, params, null, null, ctx);
}

function parseToolJson(result: ToolResult) {
  const text = result.content?.[0]?.text;
  if (!text) throw new Error("Tool result did not include text content");
  return JSON.parse(text);
}

async function seedHouseState(cwd: string, state: Record<string, unknown>) {
  const target = statePathForRoom(cwd);
  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, `${JSON.stringify(state, null, 2)}\n`, "utf8");
}

async function writeJson(target: string, value: unknown) {
  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}






describe("room onboarding contracts", () => {
  test("resolves a generic marker-backed room key, true name, and operator", async () => {
    const { cwd } = await makeTempMarkedRoom();
    const { tools } = registerAdapter();

    const result = await executeTool(tools, "room_state", {}, { cwd });
    expect(result.isError).toBeUndefined();
    expect(result.details).toMatchObject({ state: { room: "example" } });
    expect(parseToolJson(result).state).toMatchObject({
      room: "example",
      agentName: "Moonlit Example Room",
      embodiedSpirit: "Moonlit Example Room",
      operator: "Ada Lovelace",
    });
    expect(roomContext(cwd)).toMatchObject({
      room: "example",
      spirit: "Moonlit Example Room",
      operator: "Ada Lovelace",
      effectiveRoomDir: cwd,
    });
  });

});

describe("OMP context hook runtime smoke", () => {
  test("injects hidden room and routing context without re-running existing substrate custom messages", async () => {
    const { cwd } = await makeTempSmokeCwd();
    await seedHouseState(cwd, {
      version: 1,
      room: "example",
      operator: "Test Operator",
      embodiedSpirit: "Smoke Room",
      agentName: "Smoke Room",
      routingMode: { enabled: true, updatedAt: "2026-07-04T00:00:00.000Z" },
      modelDefault: { enabled: false, model: null, updatedAt: null },
    });
    const { hooks } = registerAdapter();
    const contextHook = hooks.find((hook) => hook.name === "context")?.handler;
    if (!contextHook) throw new Error("Context hook was not registered");

    const existingSubstrateMessages = [
      { role: "custom", customType: "solarisael-recall-context", content: "existing recall", display: false },
      { role: "custom", customType: "solarisael-process-lessons", content: "existing lessons", display: false },
      { role: "custom", customType: "solarisael-wake-context", content: "existing wake", display: false },
    ];
    const messages = [
      ...existingSubstrateMessages,
      { role: "user", id: "neutral-context-prompt", content: "Hello there." },
    ];

    const result = await contextHook({ messages }, { cwd, sessionID: "runtime-smoke-context" });
    expect(result?.messages).toHaveLength(messages.length + 2);

    const additions = result?.messages.slice(messages.length) as Array<Record<string, unknown>>;
    expect(additions.map((message) => message.customType)).toEqual([
      "solarisael-room-context",
      "solarisael-routing-mode",
    ]);
    expect(additions.every((message) => message.role === "custom" && message.display === false)).toBe(true);
    expect(additions[0].content).toContain("Room: example");
    expect(additions[0].content).toContain("Active spirit: Smoke Room");
    expect(additions[0].content).toContain("Operator: Test Operator");
    expect(additions[0].content).toContain("A memory must stand alone.");
    expect(additions[0].content).toContain("PostgreSQL is authoritative for canon, durable memories, and lessons.");
    expect(additions[0].content).toContain("Do not claim canon or memory was written without the corresponding successful PostgreSQL receipt.");
    expect(additions[0].content).toContain("Athanor organs:");
    expect(additions[0].content).toContain("A candidate is a proposal, never authority or evidence, until it is promoted.");
    expect(additions[0].content).toContain("Authority order: PostgreSQL is authoritative");
    expect(additions[1].content).toContain("The Athanor worker-routing mode is enabled.");
    expect(additions[1].details).toEqual({ enabled: true });
    expect(additions.map((message) => message.customType)).not.toContain("solarisael-recall-context");
    expect(additions.map((message) => message.customType)).not.toContain("solarisael-process-lessons");
    expect(additions.map((message) => message.customType)).not.toContain("solarisael-wake-context");

    const duplicateResult = await contextHook(
      { messages: [...messages, ...additions] },
      { cwd, sessionID: "runtime-smoke-context-duplicate" },
    );
    expect(duplicateResult).toBeUndefined();
  });

  test("captures opt-in turn telemetry through the production context hook", async () => {
    const { cwd } = await makeTempRoom("telemetry");
    await writeJson(path.join(cwd, ".solarisael-room.json"), {
      version: 1,
      room: "telemetry",
      trueName: "Telemetry",
      operator: "Test Operator",
      recallTelemetry: true,
    });
    const { hooks } = registerAdapter();
    const contextHook = hooks.find((hook) => hook.name === "context")?.handler;
    if (!contextHook) throw new Error("Context hook was not registered");
    const prompt = "Hi.";
    await contextHook(
      { messages: [{ role: "user", id: "telemetry-prompt", content: prompt }] },
      { cwd, sessionID: "runtime-smoke-telemetry" },
    );
    const source = await readFile(recallTelemetryPath(cwd), "utf8");
    expect(source).not.toContain(prompt);
    expect(JSON.parse(source.trim())).toMatchObject({
      schema_version: 1,
      session_id: "runtime-smoke-telemetry",
      room: "telemetry",
      status: "skipped",
      prompt_chars: prompt.length,
    });
  });

  test("casual prompt without existing recall context skips auto recall while preserving room context", async () => {
    const { cwd } = await makeTempSmokeCwd();
    await seedHouseState(cwd, {
      version: 1,
      room: "example",
      operator: "Test Operator",
      embodiedSpirit: "Smoke Room",
      agentName: "Smoke Room",
      routingMode: { enabled: true, updatedAt: "2026-07-04T00:00:00.000Z" },
      modelDefault: { enabled: false, model: null, updatedAt: null },
    });

      const { hooks } = registerAdapter();
      const contextHook = hooks.find((hook) => hook.name === "context")?.handler;
      if (!contextHook) throw new Error("Context hook was not registered");

      const messages = [
        { role: "user", id: "casual-no-recall", content: "hello love" },
      ];

      const result = await contextHook({ messages }, { cwd, sessionID: "runtime-smoke-casual-no-recall" });
      const additions = result?.messages.slice(messages.length) as Array<Record<string, unknown>>;
      const customTypes = additions.map((message) => message.customType);

      expect(customTypes).toContain("solarisael-room-context");
      expect(customTypes).toContain("solarisael-routing-mode");
      expect(customTypes).not.toContain("solarisael-recall-context");
      expect(additions.find((message) => message.customType === "solarisael-room-context")?.content)
        .toContain("Room: example");
      expect(additions.find((message) => message.customType === "solarisael-routing-mode")?.details)
        .toEqual({ enabled: true });
  });

  test("keeps one static context and one cached Recall tray across original-message turns", async () => {
    const { cwd } = await makeTempSmokeCwd();
    await seedHouseState(cwd, {
      version: 1,
      room: "example",
      operator: "Test Operator",
      embodiedSpirit: "Smoke Room",
      agentName: "Smoke Room",
      routingMode: { enabled: true, updatedAt: null },
      modelDefault: { enabled: false, model: null, updatedAt: null },
    });
    const snapshot = snapshotEnv();
    const originalRequest = RustJsonlTransport.prototype.request;
    let recallRequests = 0;
    try {
      process.env.ATHANOR_SUBSTRATE_EXE = process.execPath;
      closeRustRecallTransports();
      RustJsonlTransport.prototype.request = async function (method, params: any) {
        if (method !== "recall") return { ok: true, matches: [] };
        recallRequests += 1;
        return {
          ok: true,
          query: params.query,
          found: true,
          source: "rust-postgres",
          warnings: [],
          retrievalCandidates: [{
            source_path: "memory/recall-policy.md",
            title: "Recall Policy architecture",
            heading_path: "__preamble__",
            sources: ["memory/recall-policy.md"],
            score: 1,
            term_coverage: 1,
            memory_id: 1,
            matched_terms: ["recall", "policy"],
            missing_terms: [],
            reasons: ["project evidence"],
            excerpt: "Host-owned Recall Policy evidence.",
          }],
          canonMatches: [],
          semanticChunks: [],
          contentChunks: [],
          dateMatches: [],
          queryDates: [],
          taxonomy: { memoryTypes: [], threadKeys: [], namedEntities: [] },
        };
      };

      const { hooks } = registerAdapter();
      const contextHook = hooks.find((hook) => hook.name === "context")?.handler;
      const compactHook = hooks.find((hook) => hook.name === "session_compact")?.handler;
      if (!contextHook || !compactHook) throw new Error("Required hooks were not registered");
      const session = { cwd, sessionID: "runtime-smoke-working-set" };
      const firstMessages = [{
        role: "user",
        id: "work-one",
        content: "Inspect the Recall Policy adapter and database architecture.",
      }];
      const first = await contextHook({ messages: firstMessages }, session);
      expect(first?.messages.filter((message: any) => message.customType === "solarisael-recall-context")).toHaveLength(1);

      const secondMessages = [
        ...firstMessages,
        { role: "assistant", content: "The Recall Policy is Host-owned." },
        { role: "user", id: "work-two", content: "Check the same Recall Policy adapter tests." },
      ];
      const second = await contextHook({ messages: secondMessages }, session);
      const customTypes = second?.messages
        .filter((message: any) => message.role === "custom")
        .map((message: any) => message.customType) || [];
      expect(customTypes.filter((type) => type === "solarisael-room-context")).toHaveLength(1);
      expect(customTypes.filter((type) => type === "solarisael-routing-mode")).toHaveLength(1);
      expect(customTypes.filter((type) => type === "solarisael-recall-context")).toHaveLength(1);
      expect(recallRequests).toBe(1);

      await compactHook({
        compactionEntry: { summary: "Recall Policy adapter and database architecture remain active." },
      }, session);
      const recovered = await contextHook({
        messages: [{ role: "user", id: "work-after-compaction", content: "hello again" }],
      }, session);
      expect(recallRequests).toBe(2);
      expect(recovered?.messages.filter((message: any) => message.customType === "solarisael-recall-context")).toHaveLength(1);
    } finally {
      RustJsonlTransport.prototype.request = originalRequest;
      closeRustRecallTransports();
      restoreEnv(snapshot);
    }
  });

  test("Quiet override suppresses proactive Recall and compaction rebuilds one bounded tray", async () => {
    const { cwd } = await makeTempMarkedRoom();
    const snapshot = snapshotEnv();
    const originalRequest = RustJsonlTransport.prototype.request;
    let recallRequests = 0;
    try {
      process.env.ATHANOR_SUBSTRATE_EXE = process.execPath;
      closeRustRecallTransports();
      RustJsonlTransport.prototype.request = async function (method, params: any) {
        if (method !== "recall") return { ok: true, matches: [] };
        recallRequests += 1;
        return {
          ok: true,
          query: params.query,
          found: true,
          source: "rust-postgres",
          warnings: [],
          retrievalCandidates: [{
            source_path: "memory/compaction-recovery.md",
            title: "Recall Policy compaction recovery",
            heading_path: "__preamble__",
            sources: ["memory/compaction-recovery.md"],
            score: 1,
            term_coverage: 1,
            memory_id: 2,
            matched_terms: ["compaction", "recovery"],
            missing_terms: [],
            reasons: ["exact recovery"],
            excerpt: "Bounded post-compaction continuity.",
          }],
          canonMatches: [],
          semanticChunks: [],
          contentChunks: [],
          dateMatches: [],
          queryDates: [],
          taxonomy: { memoryTypes: [], threadKeys: [], namedEntities: [] },
        };
      };
      const { hooks, tools } = registerAdapter();
      const contextHook = hooks.find((hook) => hook.name === "context")?.handler;
      const compactHook = hooks.find((hook) => hook.name === "session_compact")?.handler;
      if (!contextHook || !compactHook) throw new Error("Required hooks were not registered");
      const session = { cwd, sessionID: "runtime-smoke-quiet" };

      await executeTool(tools, "recall_policy", { requestedMode: "quiet" }, session);
      const quiet = await contextHook({
        messages: [{ role: "user", id: "quiet-turn", content: "Do you remember the Recall Policy decision?" }],
      }, session);
      expect(quiet?.messages.some((message: any) => message.customType === "solarisael-recall-context")).toBe(false);
      expect(recallRequests).toBe(0);

      await executeTool(tools, "recall_policy", { requestedMode: "auto" }, session);
      await compactHook({
        compactionEntry: { summary: "Recall Policy compaction recovery PostgreSQL continuity" },
      }, session);
      await contextHook({
        messages: [{ role: "user", id: "after-compaction", content: "hello" }],
      }, session);
      expect(recallRequests).toBe(1);
      const state = parseToolJson(await executeTool(tools, "recall_policy", {}, session)).recallPolicy;
      expect(state).toMatchObject({
        requestedMode: "auto",
        recoveryPending: false,
        lastRefreshReason: "post-compaction-recovery",
      });
    } finally {
      RustJsonlTransport.prototype.request = originalRequest;
      closeRustRecallTransports();
      restoreEnv(snapshot);
    }
  });

  test("manual recall tool disables temporal decay", async () => {
    const { cwd } = await makeTempMarkedRoom();
    const snapshot = snapshotEnv();
    const originalRequest = RustJsonlTransport.prototype.request;
    try {
      process.env.ATHANOR_SUBSTRATE_EXE = process.execPath;
      closeRustRecallTransports();
      RustJsonlTransport.prototype.request = async function (method, params: any) {
        expect(method).toBe("recall");
        expect(params).not.toHaveProperty("temporal_decay");
        return {
          ok: true,
          query: params.query,
          found: false,
          source: "rust-postgres",
          retrievalCandidates: [],
          canonMatches: [],
          semanticChunks: [],
          contentChunks: [],
          dateMatches: [],
          queryDates: [],
          taxonomy: { memoryTypes: [], threadKeys: [], namedEntities: [] },
        };
      };

      const { tools } = registerAdapter();
      const response = await executeTool(tools, "recall", { query: "manual recall" }, { cwd });
      expect(response.isError).not.toBe(true);
    } finally {
      RustJsonlTransport.prototype.request = originalRequest;
      closeRustRecallTransports();
      restoreEnv(snapshot);
    }
  });

  test("retains semantic-lane warnings in automatic recall context and diagnostics", async () => {
    const { cwd } = await makeTempRoom("automatic-context-warning");
    await writeJson(path.join(cwd, ".solarisael-room.json"), {
      version: 1,
      room: "automatic-context-warning",
    });
    const snapshot = snapshotEnv();
    const originalRequest = RustJsonlTransport.prototype.request;
    const warning = "semantic retrieval disabled: embedding model unavailable";
    try {
      process.env.ATHANOR_SUBSTRATE_EXE = process.execPath;
      closeRustRecallTransports();
      RustJsonlTransport.prototype.request = async function (method, params: any) {
        if (method !== "recall") return { ok: true, matches: [] };
        expect(params.temporal_decay).toBe(true);
        return {
          ok: true,
          query: params.query,
          found: true,
          source: "rust-postgres",
          warnings: [warning],
          retrievalCandidates: [],
          canonMatches: [],
          semanticChunks: [{ source_path: "memory/raw-semantic.md", body: "raw semantic context" }],
          contentChunks: [{ source_path: "memory/raw-content.md", body: "raw content context" }],
          dateMatches: [],
          queryDates: [],
          taxonomy: { memoryTypes: [], threadKeys: [], namedEntities: [] },
        };
      };

      const { hooks } = registerAdapter();
      const contextHook = hooks.find((hook) => hook.name === "context")?.handler;
      if (!contextHook) throw new Error("Context hook was not registered");
      const messages = [{
        role: "user",
        id: "automatic-context-warning",
        content: "Recall the embedding-disabled warning.",
      }];
      const result = await contextHook({ messages }, { cwd, sessionID: "automatic-context-warning" });
      const additions = (result?.messages.slice(messages.length) || []) as Array<Record<string, any>>;
      const recallContext = additions.find((message) => message.customType === "solarisael-recall-context");

      expect(recallContext?.content).toContain(`"warnings": [\n    "${warning}"\n  ]`);
      expect(recallContext?.content).not.toContain("semanticChunks");
      expect(recallContext?.content).not.toContain("contentChunks");
      expect(recallContext?.details.warnings).toEqual([warning]);
      expect(recallContext?.details.found).toBe(false);
    } finally {
      RustJsonlTransport.prototype.request = originalRequest;
      closeRustRecallTransports();
      restoreEnv(snapshot);
    }
  });

  test("fails open while retaining redacted automatic recall diagnostics", async () => {
    const { cwd } = await makeTempRoom("automatic-context-diagnostic");
    await writeJson(path.join(cwd, ".solarisael-room.json"), {
      version: 1,
      room: "automatic-context-diagnostic",
      recallTelemetry: true,
    });
    const snapshot = snapshotEnv();
    const prompt = "Recall the automatic diagnostic sentinel with token=private-value.";
    try {
      process.env.ATHANOR_SUBSTRATE_EXE = process.execPath;
      const { hooks } = registerAdapter();
      const contextHook = hooks.find((hook) => hook.name === "context")?.handler;
      if (!contextHook) throw new Error("Context hook was not registered");

      const messages = [{ role: "user", id: "automatic-context-diagnostic", content: prompt }];
      const result = await contextHook({ messages }, { cwd, sessionID: "automatic-context-diagnostic" });
      const additions = (result?.messages.slice(messages.length) || []) as Array<Record<string, unknown>>;
      expect(additions.every((message) => message.display === false)).toBe(true);
      expect(additions.map((message) => message.customType)).not.toContain("solarisael-recall-context");

      const telemetry = JSON.parse(await readFile(recallTelemetryPath(cwd), "utf8").then((source) => source.trim()));
      expect(telemetry).toMatchObject({
        status: "error",
        viewport_diagnostics: {
          operation: "automatic_recall",
          owner: { component: "omp-adapter", path: "index.ts" },
          execution: { request_dispatched: true, write_outcome: "not_started" },
        },
      });
      expect(JSON.stringify(telemetry)).not.toContain(prompt);
      expect(JSON.stringify(telemetry)).not.toContain("private-value");
      expect(telemetry.viewport_diagnostics.evidence.some((entry: Record<string, unknown>) => entry.kind === "automatic_context_failure")).toBe(true);
    } finally {
      restoreEnv(snapshot);
    }
  });
});

describe("OMP safe tool execute runtime smoke", () => {
  test("room state tools persist explicit room, spirit, and routing updates in the temp room", async () => {
    const { cwd } = await makeTempSmokeCwd();
    const { tools } = registerAdapter();
    const ctx = { cwd };

    const setState = await executeTool(tools, "set_room_state", { operator: "Smoke Tester", embodiedSpirit: "Updated Spirit" }, ctx);
    expect(setState.isError).toBeUndefined();
    expect(parseToolJson(setState).state).toMatchObject({
      room: "example",
      operator: "Smoke Tester",
      embodiedSpirit: "Updated Spirit",
      agentName: "Updated Spirit",
    });

    const activeSpirit = await readFile(path.join(cwd, "active_spirit.md"), "utf8");
    expect(activeSpirit).toContain("# Active Spirit: Updated Spirit");
    expect(activeSpirit).toContain("Operator: Smoke Tester");

    const routingUpdate = await executeTool(tools, "house_routing_mode", { enabled: true }, ctx);
    expect(routingUpdate.isError).toBeUndefined();
    expect(routingUpdate.details?.routingMode).toMatchObject({ enabled: true });
    expect(parseToolJson(routingUpdate).routingMode).toMatchObject({ enabled: true });

    const roomState = await executeTool(tools, "room_state", {}, ctx);
    expect(roomState.isError).toBeUndefined();
    expect(parseToolJson(roomState).state).toMatchObject({
      room: "example",
      operator: "Smoke Tester",
      embodiedSpirit: "Updated Spirit",
      routingMode: { enabled: true },
    });
  });
  test("anamnesis read validates consult queries and anamnesis_write enforces operation fields", async () => {
    const { cwd } = await makeTempSmokeCwd();
    const { tools } = registerAdapter();
    const ctx = { cwd };

    const missingQuery = await executeTool(tools, "anamnesis", { mode: "consult" }, ctx);
    expect(missingQuery.isError).toBe(true);
    expect(parseToolJson(missingQuery)).toMatchObject({ ok: false, error: "consult requires a non-empty query" });

    const missingAddFields = await executeTool(
      tools,
      "anamnesis_write",
      { operation: "add", title: "Incomplete drawer" },
      ctx,
    );
    expect(missingAddFields.isError).toBe(true);
    expect(parseToolJson(missingAddFields).error).toContain("add requires kind, fidelity, activation, and ramp");

    const missingRepFields = await executeTool(
      tools,
      "anamnesis_write",
      { operation: "append-rep", title: "Drawer", sourcePaths: [] },
      ctx,
    );
    expect(missingRepFields.isError).toBe(true);
    expect(parseToolJson(missingRepFields).error).toContain("append-rep requires integer repNumber");
  });


  test("accepts a generic embodied spirit string and refreshes the marker-backed room snapshot", async () => {
    const { cwd } = await makeTempMarkedRoom();
    const { tools } = registerAdapter();

    const result = await executeTool(tools, "set_room_state", { embodiedSpirit: "Aurora" }, { cwd });
    expect(result.isError).toBeUndefined();
    expect(parseToolJson(result).state).toMatchObject({
      room: "example",
      operator: "Ada Lovelace",
      embodiedSpirit: "Aurora",
      agentName: "Aurora",
    });

    const activeSpirit = await readFile(path.join(cwd, "active_spirit.md"), "utf8");
    expect(activeSpirit).toContain("# Active Spirit: Aurora");
    expect(activeSpirit).toContain("Agent: Aurora | Operator: Ada Lovelace");
    expect(activeSpirit).toContain("Embodied: Aurora | Conjured: none | Summoned: none");
  });

  test("preserves the active-spirit body when refreshing its header", async () => {
    const { cwd } = await makeTempMarkedRoom();
    const body = [
      "# SPIRIT: Before Refresh",
      "",
      "This room-authored body must survive a state update.",
      "It carries onboarding instructions and is not generated header data.",
    ].join("\n");
    await writeFile(
      path.join(cwd, "active_spirit.md"),
      [
        "# Active Spirit: Before Refresh",
        "Agent: Before Refresh | Operator: Before Operator",
        "Embodied: Before Refresh | Conjured: none | Summoned: none",
        "",
        body,
      ].join("\n"),
      "utf8",
    );
    const { tools } = registerAdapter();

    const result = await executeTool(
      tools,
      "set_room_state",
      { operator: "New Operator", embodiedSpirit: "After Refresh" },
      { cwd },
    );
    expect(result.isError).toBeUndefined();

    const activeSpirit = await readFile(path.join(cwd, "active_spirit.md"), "utf8");
    expect(activeSpirit).toContain("# Active Spirit: After Refresh");
    expect(activeSpirit).toContain("Agent: After Refresh | Operator: New Operator");
    expect(activeSpirit).toContain("Embodied: After Refresh | Conjured: none | Summoned: none");
    expect(activeSpirit).toContain(body);
    expect(activeSpirit).not.toContain("# Active Spirit: Before Refresh");
    expect(activeSpirit).not.toContain("Agent: Before Refresh | Operator: Before Operator");
  });


  test("remember rejects invalid supersession IDs and lesson-store supersession", async () => {
    const { cwd } = await makeTempSmokeCwd();
    const { tools } = registerAdapter();

    const invalidIds = await executeTool(
      tools,
      "remember",
      {
        title: "Invalid supersession",
        body: "This write must be refused.",
        supersedes: ["0", "not-an-id"],
      },
      { cwd },
    );
    expect(invalidIds.isError).toBe(true);
    expect(parseToolJson(invalidIds).error).toContain("positive numeric memory IDs");

    const lessonSupersession = await executeTool(
      tools,
      "remember",
      {
        kind: "coding-lesson",
        title: "Wrong store",
        body: "Lesson stores must not supersede memory rows.",
        supersedes: ["41"],
      },
      { cwd },
    );
    expect(lessonSupersession.isError).toBe(true);
    expect(parseToolJson(lessonSupersession).error).toContain("supersedes is memory-only");
  });

  test("update_lesson rejects fields from the wrong typed store", async () => {
    const { cwd } = await makeTempSmokeCwd();
    const { tools } = registerAdapter();

    const writingScope = await executeTool(
      tools,
      "update_lesson",
      {
        kind: "writing-lesson",
        id: "41",
        expectedTitle: "Writing rule",
        scope: "house",
      },
      { cwd },
    );
    expect(writingScope.isError).toBe(true);
    expect(parseToolJson(writingScope).error).toContain("field not allowed for writing-lesson: scope");

    const codingRegister = await executeTool(
      tools,
      "update_lesson",
      {
        kind: "coding-lesson",
        id: "42",
        expectedTitle: "Coding rule",
        register: ["prose"],
      },
      { cwd },
    );
    expect(codingRegister.isError).toBe(true);
    expect(parseToolJson(codingRegister).error).toContain("field not allowed for coding-lesson: register");
  });

  test("design_doc_write routes structured provenance through the Rust catalogue operation", async () => {
    const { cwd } = await makeTempSmokeCwd();
    const { tools } = registerAdapter();
    const snapshot = snapshotEnv();
    const originalRequest = RustJsonlTransport.prototype.request;
    let observed: { method: string; params: any } | undefined;
    try {
      process.env.ATHANOR_SUBSTRATE_EXE = process.execPath;
      closeRustRememberTransports();
      RustJsonlTransport.prototype.request = async function (method, params: any) {
        observed = { method, params };
        return { ok: true, id: 17, system: params.system, doc_type: params.docType, name: params.name, superseded: [] };
      };
      const routed = await executeTool(tools, "design_doc_write", {
        system: "solarisael",
        docType: "token",
        name: "color.accent",
        values: { hex: "#d4af37" },
        provenance: { repo: "solarisael-house-site" },
      }, { cwd });
      expect(routed.isError).toBeUndefined();
      expect(parseToolJson(routed).ok).toBe(true);
      expect(observed).toEqual({
        method: "design_document_write",
        params: {
          system: "solarisael", docType: "token", name: "color.accent", group: undefined,
          values: { hex: "#d4af37" }, body: "", provenance: { repo: "solarisael-house-site" },
          tags: [], supersedes: undefined, allowIdentityChange: false,
        },
      });
    } finally {
      RustJsonlTransport.prototype.request = originalRequest;
      closeRustRememberTransports();
      restoreEnv(snapshot);
    }
    const missingSystem = await executeTool(tools, "design_doc_write", {
      docType: "token", name: "color.accent", body: "The current accent token.",
    }, { cwd });
    expect(missingSystem.isError).toBe(true);
    expect(parseToolJson(missingSystem).error).toBe("system is required");
  });

  test("remember routes design lessons with the design defaults and refuses audio-only fields", async () => {
    const { cwd } = await makeTempSmokeCwd();
    const { tools } = registerAdapter();
    const snapshot = snapshotEnv();
    const originalRequest = RustJsonlTransport.prototype.request;
    let observed: { method: string; params: any } | undefined;
    try {
      process.env.ATHANOR_SUBSTRATE_EXE = process.execPath;
      closeRustRememberTransports();
      RustJsonlTransport.prototype.request = async function (method, params: any) {
        observed = { method, params };
        return {
          lesson_id: 51,
          kind: params.kind,
          durable: true,
          authority: "postgres",
          warnings: [],
        };
      };

      const designWrite = await executeTool(
        tools,
        "remember",
        {
          kind: "design-lesson",
          title: "Keep interaction floors explicit",
          body: "Every component contract names its keyboard and contrast floor.",
          voice: "system-craft",
          shape: "component-contract",
          proofPattern: "Exercise keyboard interaction and contrast checks.",
          triggerContext: "Before adding a component variant.",
          exampleText: "A menu opens with Enter and closes with Escape.",
          sourceMemoryPath: "memory/design-contract.md",
          tags: ["a11y", "components"],
        },
        { cwd },
      );

      expect(designWrite.isError).toBeUndefined();
      expect(parseToolJson(designWrite)).toMatchObject({ ok: true, id: 51, kind: "design-lesson" });
      expect(observed).toEqual({
        method: "remember",
        params: {
          room: "example",
          kind: "design-lesson",
          title: "Keep interaction floors explicit",
          body: "Every component contract names its keyboard and contrast floor.",
          shape: "component-contract",
          voice: "system-craft",
          register: ["general"],
          scope: null,
          project: null,
          proofPattern: "Exercise keyboard interaction and contrast checks.",
          triggerContext: "Before adding a component variant.",
          exampleText: "A menu opens with Enter and closes with Escape.",
          sourceMemoryPath: "memory/design-contract.md",
          languageKeys: [],
          technologyKeys: [],
          tags: ["a11y", "components"],
          threadKeys: [],
          backup: false,
        },
      });
    } finally {
      RustJsonlTransport.prototype.request = originalRequest;
      closeRustRememberTransports();
      restoreEnv(snapshot);
    }

    const designStage = await executeTool(
      tools,
      "update_lesson",
      {
        kind: "design-lesson",
        id: "51",
        expectedTitle: "Keep interaction floors explicit",
        stage: "mix",
      },
      { cwd },
    );
    expect(designStage.isError).toBe(true);
    expect(parseToolJson(designStage).error).toContain("field not allowed for design-lesson: stage");
  });

  test("remember validates continuation contracts before dispatch", async () => {
    const { cwd } = await makeTempSmokeCwd();
    const { tools } = registerAdapter();

    const missingThread = await executeTool(
      tools,
      "remember",
      {
        title: "Invalid continuation membership",
        body: "This write must be refused.",
        threads: ["work / page"],
        continues: [{ thread: "work / other", previousMemoryId: "41" }],
      },
      { cwd },
    );

    expect(missingThread.isError).toBe(true);
    expect(parseToolJson(missingThread).error).toContain("must also be present in threads");

    const invalidId = await executeTool(
      tools,
      "remember",
      {
        title: "Invalid continuation ID",
        body: "This write must be refused.",
        threads: ["work / page"],
        continues: [{ thread: "work / page", previousMemoryId: "0" }],
      },
      { cwd },
    );

    expect(invalidId.isError).toBe(true);
    expect(parseToolJson(invalidId).error).toContain("positive PostgreSQL BIGINT");

    const lessonContinuation = await executeTool(
      tools,
      "remember",
      {
        kind: "coding-lesson",
        title: "Wrong continuation store",
        body: "Lesson stores must not link memory threads.",
        continues: [{ thread: "work / page", previousMemoryId: "41" }],
      },
      { cwd },
    );

    expect(lessonContinuation.isError).toBe(true);
    expect(parseToolJson(lessonContinuation).error).toContain("continues is memory-only");
  });
  test("remember routes house-targeted memory writes to the house room and refuses room on lesson stores", async () => {
    const { cwd } = await makeTempSmokeCwd();
    const { tools } = registerAdapter();
    const originalRequest = RustJsonlTransport.prototype.request;
    const seen: Array<{ method: string; params: any }> = [];
    try {
      RustJsonlTransport.prototype.request = async function (method, params: any) {
        seen.push({ method, params });
        return {
          memory_id: 41,
          room: params.room,
          source_path: params.source_path,
          durable: true,
          authority: "postgres",
          warnings: [],
        };
      };

      const houseWrite = await executeTool(
        tools,
        "remember",
        { title: "House note", body: "Durable work any room can use.", room: "house" },
        { cwd },
      );
      expect(Boolean(houseWrite.isError)).toBe(false);
      expect(seen).toHaveLength(1);
      expect(seen[0].method).toBe("remember");
      expect(seen[0].params.room).toBe("house");
      expect(parseToolJson(houseWrite)).toMatchObject({ ok: true, id: 41, room: "house" });

      const ownWrite = await executeTool(
        tools,
        "remember",
        { title: "Room note", body: "Stays in this room." },
        { cwd },
      );
      expect(Boolean(ownWrite.isError)).toBe(false);
      expect(seen).toHaveLength(2);
      expect(seen[1].params.room).toBe(roomContext(cwd).room);
      expect(seen[1].params.room).not.toBe("house");
    } finally {
      RustJsonlTransport.prototype.request = originalRequest;
      closeRustRememberTransports();
    }

    const lessonRoom = await executeTool(
      tools,
      "remember",
      { kind: "coding-lesson", title: "Wrong room", body: "Lesson stores route by scope.", room: "house" },
      { cwd },
    );
    expect(lessonRoom.isError).toBe(true);
    expect(parseToolJson(lessonRoom).error).toContain("room is memory-only");
  });
  test("canon tools dispatch distinct typed write and exact history methods", async () => {
    const { cwd } = await makeTempSmokeCwd();
    const { tools } = registerAdapter();
    const originalRequest = RustJsonlTransport.prototype.request;
    const seen: Array<{ method: string; params: any }> = [];
    try {
      RustJsonlTransport.prototype.request = async function (method, params: any) {
        seen.push({ method, params });
        if (method === "canon_write") {
          return {
            ok: true,
            durable: true,
            authority: "postgres",
            entityAuthority: "active",
            entityId: "42",
            room: params.room,
            name: params.name,
            supersededEntityIds: params.supersedes,
            attributedBy: params.attribution.actor,
            attributionOrigin: params.attribution.origin,
          };
        }
        return {
          ok: true,
          entities: [{ entityId: params.id, authority: "superseded" }],
        };
      };

      const written = await executeTool(
        tools,
        "canon_write",
        {
          name: "The Athanor",
          kind: "project",
          summary: "Current authority",
          pointerFiles: [{ file: "canon.md", lines: [2, 7] }],
          supersedes: ["41"],
        },
        { cwd },
      );
      expect(Boolean(written.isError)).toBe(false);
      expect(seen[0]).toMatchObject({
        method: "canon_write",
        params: {
          room: roomContext(cwd).room,
          name: "The Athanor",
          kind: "project",
          supersedes: ["41"],
          attribution: { actor: roomContext(cwd).spirit },
        },
      });
      expect(seen[0].params.attribution.origin).toContain("canon_write");
      expect(parseToolJson(written)).toMatchObject({
        authority: "postgres",
        entityAuthority: "active",
        entityId: "42",
      });

      const history = await executeTool(
        tools,
        "canon_read",
        { id: "41", includeHistory: true },
        { cwd },
      );
      expect(Boolean(history.isError)).toBe(false);
      expect(seen[1]).toEqual({
        method: "canon_read",
        params: {
          room: roomContext(cwd).room,
          id: "41",
          name: undefined,
          includeHistory: true,
        },
      });
    } finally {
      RustJsonlTransport.prototype.request = originalRequest;
      closeRustRememberTransports();
    }
  });

  test("wake and sleep tools use only typed Rust Paper Boat methods", async () => {
    const { cwd } = await makeTempSmokeCwd();
    const { tools } = registerAdapter();
    const room = roomContext(cwd).room;
    const originalRequest = RustJsonlTransport.prototype.request;
    const environment = snapshotEnv();
    const seen: Array<{ method: string; params: any }> = [];
    process.env.ATHANOR_SUBSTRATE_EXE = process.execPath;
    try {
      RustJsonlTransport.prototype.request = async function (method, params: any) {
        seen.push({ method, params });
        if (method === "paper_boat_sleep") {
          return {
            ok: true,
            memory_id: "71",
            room,
            source_path: "db-only/paper-boats/sha256-proof.md",
            outbox_event_id: "event-71",
            inserted: true,
            durable: true,
            authority: "postgres",
            backup_status: "completed",
            warnings: [],
          };
        }
        return {
          ok: true,
          found: true,
          room,
          id: "71",
          title: "paper boat — 2026-08-10",
          body: "letter for tomorrow",
          date: "2026-08-10",
          source_path: "db-only/paper-boats/sha256-proof.md",
          created_at: "2026-08-10T00:00:00Z",
          unboated: [],
          unboated_truncated: false,
          warnings: [],
        };
      };

      const slept = await executeTool(tools, "sleep", { body: "letter for tomorrow" }, { cwd });
      const woke = await executeTool(tools, "wake", {}, { cwd });
      expect(Boolean(slept.isError)).toBe(false);
      expect(Boolean(woke.isError)).toBe(false);
      expect(seen.filter(({ method }) => method.startsWith("paper_boat_"))).toEqual([
        { method: "paper_boat_sleep", params: { room, body: "letter for tomorrow", backup: true } },
        { method: "paper_boat_wake", params: { room } },
      ]);
      expect(parseToolJson(slept)).toMatchObject({ durable: true, backup_status: "completed" });
      expect(parseToolJson(woke)).toMatchObject({ found: true, body: "letter for tomorrow" });
    } finally {
      RustJsonlTransport.prototype.request = originalRequest;
      closePaperBoatTransports();
      restoreEnv(environment);
    }
  });



  test("routing tools expose core lane status and return dispatch receipts without spawning workers", async () => {
    const { tools } = registerAdapter();

    const status = await executeTool(tools, "house_lane_status", {}, {});
    const statusJson = parseToolJson(status);
    expect(status.isError).toBeUndefined();
    expect(statusJson.ok).toBe(true);
    expect(statusJson.lanes.map((lane: { name: string }) => lane.name)).toEqual([
      "smol-scout",
      "smol-executor",
      "tester",
      "verifier",
    ]);
    expect(statusJson.advisor.name).toBe("advisor");

    const readyDispatch = await executeTool(
      tools,
      "house_dispatch",
      {
        lane: "tester",
        task: "Add a focused smoke test.",
        target: "tests/runtime-smoke.test.ts",
        context: [{ mode: "exact", source: "tests/runtime-smoke.test.ts", reason: "target file" }],
        acceptance: ["Targeted test passes."],
        risk: "low",
      },
      {},
    );
    const readyJson = parseToolJson(readyDispatch);
    expect(readyDispatch.isError).toBeUndefined();
    expect(readyJson).toMatchObject({
      ok: true,
      status: "ready",
      selector: { kind: "lane", value: "tester" },
      lane: "tester",
      dispatcher: { executed: false },
      spawnPacket: {
        tool: "task",
        args: { tasks: [{ name: "Gauge" }] },
      },
    });
    expect(readyJson.spawnPacket.args.tasks[0].task).toContain("Add a focused smoke test.");
    expect(readyJson.spawnPacket.args.tasks[0].task).toContain("- Targeted test passes.");
    expect(readyJson.spawnPacket.args.tasks[0]).not.toHaveProperty("agent");

    const rejectedDispatch = await executeTool(
      tools,
      "house_dispatch",
      { lane: "advisor", task: "Review this.", acceptance: ["Receipt rejects advisor."] },
      {},
    );
    const rejectedJson = parseToolJson(rejectedDispatch);
    expect(rejectedDispatch.isError).toBe(true);
    expect(rejectedJson).toMatchObject({
      ok: false,
      status: "error",
      lane: null,
      spawnPacket: null,
      details: {
        operation: "house_dispatch",
        observed: { errors: ["Unknown worker lane: advisor"] },
      },
    });
    expect(rejectedJson.errors).toEqual(["Unknown worker lane: advisor"]);
  });

  test("familiar tools load room aliases and package the bound scout lane", async () => {
    const { cwd } = await makeTempSmokeCwd();
    const { tools } = registerAdapter();
    const familiarDir = path.join(cwd, "familiars");
    await mkdir(familiarDir);
    await writeJson(path.join(familiarDir, "spellbook.json"), {
      version: 1,
      collective: "familiars",
      collectiveAliases: ["kittens"],
      spellbookAliases: ["litters.json"],
      familiars: [{
        id: "cisma",
        name: "Cisma",
        aliases: ["scout-kitten"],
        lane: "smol-scout",
        description: "A bounded scout.",
      }],
    });

    const status = await executeTool(tools, "familiar_status", {}, { cwd });
    const statusJson = parseToolJson(status);
    expect(status.isError).toBeUndefined();
    expect(statusJson).toMatchObject({
      ok: true,
      sourceAlias: false,
      spellbook: {
        collective: "familiars",
        collectiveAliases: ["kittens"],
        spellbookAliases: ["litters.json"],
      },
    });

    const dispatch = await executeTool(
      tools,
      "house_dispatch",
      {
        familiar: "scout-kitten",
        task: "Map the exact target.",
        context: [{ mode: "exact", source: "src/example.ts" }],
        acceptance: ["Report exact symbols."],
      },
      { cwd },
    );
    const dispatchJson = parseToolJson(dispatch);
    expect(dispatch.isError).toBeUndefined();
    expect(dispatchJson).toMatchObject({
      ok: true,
      status: "ready",
      selector: { kind: "familiar", value: "scout-kitten" },
      familiar: { id: "cisma", lane: "smol-scout" },
      modelRole: "pi/smol",
      ompAgent: "scout",
      spawnPacket: {
        tool: "task",
        args: { tasks: [{ name: "Cisma", agent: "scout" }] },
      },
    });
  });

  test("model default tool resolves before saving, applies resolved selectors, clears them, and reports validation errors", async () => {
    const { cwd } = await makeTempSmokeCwd();
    const { tools, appliedModels } = registerAdapter();
    const resolved: string[] = [];
    const ctx = {
      cwd,
      models: {
        resolve(selector: string) {
          resolved.push(selector);
          return selector === "pi/default" ? { id: "resolved-default" } : null;
        },
      },
    };

    const missingSelector = await executeTool(
      tools,
      "house_model_default",
      { model: "missing-model", enabled: true, applyNow: true },
      ctx,
    );
    expect(missingSelector.isError).toBe(true);
    expect(parseToolJson(missingSelector)).toMatchObject({
      ok: false,
      status: "error",
      error: "Could not resolve model selector for this session: [redacted]",
      message: "Could not resolve model selector for this session: [redacted]",
    });
    expect(appliedModels).toEqual([]);

    const enableWithoutModel = await executeTool(tools, "house_model_default", { enabled: true }, ctx);
    expect(enableWithoutModel.isError).toBe(true);
    expect(parseToolJson(enableWithoutModel)).toMatchObject({
      ok: false,
      status: "error",
      error: "Cannot enable room model default without a model selector.",
      message: "Cannot enable room model default without a model selector.",
    });
    expect(appliedModels).toEqual([]);

    const applied = await executeTool(
      tools,
      "house_model_default",
      { model: "pi/default", enabled: true, applyNow: true },
      ctx,
    );
    expect(applied.isError).toBeUndefined();
    expect(applied.details).toMatchObject({ applied: true, modelDefault: { enabled: true, model: "pi/default" } });
    expect(parseToolJson(applied)).toMatchObject({
      modelDefault: { enabled: true, model: "pi/default" },
      applied: true,
    });
    expect(appliedModels).toEqual(["pi/default"]);
    expect(resolved).toContain("missing-model");
    expect(resolved).toContain("pi/default");

    const cleared = await executeTool(tools, "house_model_default", { clear: true }, ctx);
    expect(cleared.isError).toBeUndefined();
    expect(cleared.details).toMatchObject({ applied: false, modelDefault: { enabled: false, model: null } });
    expect(parseToolJson(cleared)).toMatchObject({
      modelDefault: { enabled: false, model: null },
      applied: false,
    });
    expect(appliedModels).toEqual(["pi/default"]);
  });
});

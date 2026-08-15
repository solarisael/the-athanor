import { mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";

const evaluations: Array<{ session: string; workingSetPresent: boolean }> = [];
const evaluationCount = new Map<string, number>();
let effectiveRoomDir = "";
let omitViewportWarnings = false;
const completedRefreshWarnings: Array<string | undefined> = [];

const recallPolicy = {
  requestedMode: "auto",
  resolvedMode: "work",
  activeProject: null,
  resolutionReason: "technical-project",
  lastRefreshReason: null,
  lastRefreshAt: null,
  workingSetEntries: 1,
  recoveryPending: false,
  recoveryTerms: [],
  degraded: null,
  updatedAt: "2026-08-14T00:00:00.000Z",
};

mock.module("../solarisael-house-proof/room.ts", () => ({
  applyPromptDirectives: async () => ({ state: { operator: "Sol", embodiedSpirit: "Kodo" } }),
  roomContext: () => ({
    room: "kodo",
    spirit: "Kodo",
    operator: "Sol",
    effectiveRoomDir,
  }),
  writeActiveSpiritSnapshot: async () => undefined,
}));

mock.module("../solarisael-house-proof/conversation-log.ts", () => ({
  logConversationWindow: async () => ({ fresh: false, loggedTurns: [] }),
}));

mock.module("../giga.ts", () => ({
  closeGigaTransports: async () => undefined,
  ingestGigaLoggedTurnsDetached: () => undefined,
}));

mock.module("../solarisael-house-proof/recall.ts", () => ({
  closeRustRecallTransports: () => undefined,
  recallWithRouting: async (_roomDir: string, _room: string, query: string) => ({
    ok: true,
    result: { query },
  }),
}));

mock.module("../solarisael-house-proof/tools.ts", () => ({
  closeRustRememberTransports: () => undefined,
  registerSolarisaelTools: () => undefined,
  writeRustMemory: async () => undefined,
}));

mock.module("../solarisael-house-proof/anamnesis.ts", () => ({
  closeRustAnamnesisTransports: () => undefined,
  formatAnamnesisContext: () => "",
  queryAnamnesis: async () => ({ ok: true }),
}));

mock.module("../solarisael-house-proof/substrate.ts", () => ({
  catchBoat: async () => ({ ok: true, found: false }),
  closePaperBoatTransports: () => undefined,
}));

mock.module("../solarisael-house-proof/entity-resolution.ts", () => ({
  resolveEntities: async () => ({ ok: true, matches: [] }),
}));

mock.module("../solarisael-house-proof/recall-telemetry.ts", () => ({
  recordRecallTelemetry: async () => true,
}));

mock.module("../solarisael-house-proof/triggers.ts", () => ({
  processLessonsReminder: async () => null,
}));

mock.module("../solarisael-house-proof/context.ts", () => ({
  analyzeContext: async () => ({
    route: {
      entityResolutionSuggested: false,
      intent: "technical_project",
      terms: ["cache"],
      requiredTerms: [],
      recognizedEntities: [],
    },
  }),
  applyRecallViewport: async (_binding: unknown, recalled: { query: string }) => ({
    presentation: {
      found: [{ id: recalled.query }],
      ...(omitViewportWarnings ? {} : { warnings: [] }),
      retrievalCandidates: [{ id: recalled.query }],
      canonMatches: [],
      dateMatches: [],
    },
    diagnostics: {},
  }),
}));

mock.module("../solarisael-house-proof/recall-policy.ts", () => ({
  RecallPolicyHostClient: class {
    session: string;

    constructor(binding: { session: string }) {
      this.session = binding.session;
    }

    async inspect() {
      return { recallPolicy };
    }

    async evaluate(input: { workingSetPresent: boolean }) {
      evaluations.push({ session: this.session, workingSetPresent: input.workingSetPresent });
      const count = (evaluationCount.get(this.session) ?? 0) + 1;
      evaluationCount.set(this.session, count);
      return {
        decision: {
          shouldRecall: true,
          clearWorkingSet: count > 1,
          query: `${this.session}:working-set:${count}`,
          queryTerms: ["cache"],
          refreshReason: count === 1 ? "empty-working-set" : "host-refresh",
          intent: "technical_project",
          resolvedMode: "work",
        },
        snapshot: { recallPolicy },
      };
    }

    async completeRefresh(input: { warning?: string }) {
      completedRefreshWarnings.push(input.warning);
      return { recallPolicy };
    }

    async failRefresh() {
      return { recallPolicy };
    }

    async invalidateAfterCompaction() {
      return undefined;
    }
  },
}));

const { default: registerAdapter } = await import("../index.ts");

type ContextHandler = (
  event: { messages: Array<Record<string, unknown>> },
  context: { cwd: string; sessionID: string },
) => Promise<{ messages: Array<Record<string, any>> } | undefined>;

type CompactHandler = (
  event: { compactionEntry?: { id?: string; summary?: string }; summary?: string },
  context: { cwd: string; sessionID: string },
) => Promise<void>;

function adapterHandlers(adapter: typeof registerAdapter = registerAdapter): {
  context: ContextHandler;
  compact: CompactHandler;
} {
  let contextHook: ContextHandler | undefined;
  let compactHook: CompactHandler | undefined;
  adapter({
    setLabel() {},
    events: { on: () => () => undefined },
    on(name: string, candidate: ContextHandler | CompactHandler) {
      if (name === "context") contextHook = candidate as ContextHandler;
      if (name === "session_compact") compactHook = candidate as CompactHandler;
    },
  });
  if (!contextHook) throw new Error("context hook was not registered");
  if (!compactHook) throw new Error("session_compact hook was not registered");
  return { context: contextHook, compact: compactHook };
}

function contextHandler(): ContextHandler {
  return adapterHandlers().context;
}

function user(id: string, content: string) {
  return { id, role: "user", content };
}

function context(sessionID: string) {
  return { cwd: "C:/test/kodo", sessionID };
}

function recallBlocks(messages: Array<Record<string, any>>) {
  return messages.filter((message) => message.customType === "solarisael-recall-context");
}

beforeEach(async () => {
  effectiveRoomDir = await mkdtemp(path.join(os.tmpdir(), "athanor-turn-additions-"));
  omitViewportWarnings = false;
  completedRefreshWarnings.length = 0;
});

afterEach(async () => {
  await rm(effectiveRoomDir, { recursive: true, force: true });
  effectiveRoomDir = "";
});

describe("OMP prompt-cache history", () => {
  test("keeps an earlier working set byte-stable and appends a Host-authorized refresh at the current turn", async () => {
    const handler = contextHandler();
    const sessionID = "cache-history";
    const firstTurn = user("turn-1", "first recall request");
    const first = await handler({ messages: [firstTurn] }, context(sessionID));
    if (!first) throw new Error("first context event returned no additions");
    const firstBlock = recallBlocks(first.messages)[0];
    const firstBytes = JSON.stringify(firstBlock);

    const secondTurn = user("turn-2", "refresh recall now");
    const second = await handler({
      messages: [firstTurn, { role: "assistant", content: "first answer" }, secondTurn],
    }, context(sessionID));
    if (!second) throw new Error("second context event returned no additions");
    const blocks = recallBlocks(second.messages);

    expect(blocks).toHaveLength(2);
    expect(JSON.stringify(blocks[0])).toBe(firstBytes);
    expect(second.messages[1]).toEqual(firstBlock);
    expect(second.messages.at(-1)).toEqual(blocks[1]);
    expect(blocks[1].content).toContain(
      "This working set supersedes every earlier Athanor Recall working set in this conversation; use this copy as current.",
    );
    expect(evaluations.filter((entry) => entry.session === sessionID)).toEqual([
      { session: sessionID, workingSetPresent: false },
      { session: sessionID, workingSetPresent: true },
    ]);
  });

  test("treats an omitted warning list as empty for a found working set", async () => {
    omitViewportWarnings = true;
    const result = await contextHandler()({
      messages: [user("warningless-turn", "warningless recall request")],
    }, context("warningless-recall"));
    if (!result) throw new Error("warningless context event returned no additions");

    const blocks = recallBlocks(result.messages);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].details.warnings).toEqual([]);
    expect(completedRefreshWarnings).toEqual([undefined]);
  });

  test("keeps the living main-session memo through forty sibling sessions", async () => {
    const handler = contextHandler();
    const sessionID = "living-main";
    const mainTurn = user("main-turn", "main recall request");
    const first = await handler({ messages: [mainTurn] }, context(sessionID));
    if (!first) throw new Error("main context event returned no additions");
    const firstBytes = JSON.stringify(first.messages);

    for (let index = 0; index < 40; index += 1) {
      const siblingID = `sibling-${index}`;
      await handler({
        messages: [user(`${siblingID}-turn`, `recall request ${index}`)],
      }, context(siblingID));
    }

    const replay = await handler({ messages: [mainTurn] }, context(sessionID));
    expect(JSON.stringify(replay?.messages)).toBe(firstBytes);
    expect(evaluationCount.get(sessionID)).toBe(1);
  });

  test("hydrates anchored additions after restart and persists compaction clearing", async () => {
    const sessionID = "durable-memo";
    const firstTurn = user("durable-turn", "persist this recall request");
    const processA = adapterHandlers((await import("../index.ts?memo-process-a")).default);
    const first = await processA.context({ messages: [firstTurn] }, context(sessionID));
    if (!first) throw new Error("process A returned no additions");
    const firstBytes = JSON.stringify(first.messages);

    const memoFile = path.join(
      effectiveRoomDir,
      ".omp",
      "runtime",
      "turn-additions",
      `${Bun.hash(`kodo:${sessionID}`).toString(36)}.json`,
    );
    const storedByA = JSON.parse(await Bun.file(memoFile).text()) as {
      version: number;
      turns: Record<string, Array<Record<string, any>>>;
    };
    expect(storedByA.version).toBe(1);
    expect(recallBlocks(Object.values(storedByA.turns).flat())).toHaveLength(1);

    const processB = adapterHandlers((await import("../index.ts?memo-process-b")).default);
    const replay = await processB.context({ messages: [firstTurn] }, context(sessionID));
    expect(JSON.stringify(replay?.messages)).toBe(firstBytes);
    expect(evaluationCount.get(sessionID)).toBe(1);

    await processB.compact(
      { compactionEntry: { id: "compact-1", summary: "durable compact" } },
      context(sessionID),
    );
    const storedAfterCompact = JSON.parse(await Bun.file(memoFile).text()) as {
      version: number;
      turns: Record<string, Array<Record<string, any>>>;
    };
    expect(recallBlocks(Object.values(storedAfterCompact.turns).flat())).toHaveLength(0);
  });
});

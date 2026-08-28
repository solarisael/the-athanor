import { mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";

let effectiveRoomDir = "";
let topLevelSessionId = "";
const settlements: Array<Record<string, any>> = [];
let settleFailure: string | null = null;
let compiledGuardId: string | null = "presence:nonempty-response";

const recallPolicy = {
  requestedMode: "auto",
  resolvedMode: "conversation",
  activeProject: null,
  resolutionReason: "contact",
  lastRefreshReason: null,
  lastRefreshAt: null,
  workingSetEntries: 0,
  recoveryPending: false,
  recoveryTerms: [],
  degraded: null,
  updatedAt: "2026-08-27T00:00:00.000Z",
};

mock.module("../house-proof/room.ts", () => ({
  applyPromptDirectives: async () => ({ state: { operator: "Sol", embodiedSpirit: "Kintsu" } }),
  roomContext: () => ({ room: "kintsu", spirit: "Kintsu", operator: "Sol", effectiveRoomDir }),
  writeActiveSpiritSnapshot: async () => undefined,
}));

mock.module("../house-proof/top-level-session-fence.ts", () => ({
  adoptTopLevelSession: (_room: string, session: string) => { topLevelSessionId ||= session; },
  registerTopLevelSession: (_room: string, session: string) => { topLevelSessionId = session; },
  retireTopLevelSession: (_room: string, session: string) => {
    if (topLevelSessionId === session) topLevelSessionId = "";
  },
  topLevelSession: () => topLevelSessionId || null,
}));

mock.module("../house-proof/conversation-log.ts", () => ({
  logConversationWindow: async () => ({ fresh: false, loggedTurns: [] }),
}));

mock.module("../giga.ts", () => ({
  closeGigaTransports: async () => undefined,
  ingestGigaLoggedTurnsDetached: () => undefined,
}));

mock.module("../house-proof/recall.ts", () => ({
  closeRustRecallTransports: () => undefined,
  recallWithRouting: async () => ({ ok: true, result: { query: "" } }),
}));

mock.module("../house-proof/tools.ts", () => ({
  closeRustRememberTransports: () => undefined,
  registerSolarisaelTools: () => undefined,
  writeRustMemory: async () => undefined,
}));

mock.module("../house-proof/presence.ts", () => ({
  compilePresenceContext: async (request: any) => ({
    frameId: "frame-1",
    frameVersion: 1,
    frameRendered: "Presence frame",
    contractId: "contract-1",
    turnId: request.turnId,
    directiveIds: ["presence:active-spirit", "presence:nonempty-response"],
    nonemptyGuardId: compiledGuardId,
    rendered: "Presence frame\n\nPresence contract",
  }),
  settlePresence: async (_binding: unknown, request: Record<string, any>) => {
    settlements.push(request);
    if (settleFailure) throw new Error(settleFailure);
    return { contractId: "contract-1" };
  },
  responseDigest: (text: string) => `digest:${text}`,
}));

mock.module("../house-proof/hallway.ts", () => ({
  projectHallwayInbox: async () => ({ changed: false, inbox: { ok: true, hallways: [] } }),
}));

mock.module("../house-proof/anamnesis.ts", () => ({
  closeRustAnamnesisTransports: () => undefined,
  formatAnamnesisContext: () => "",
  queryAnamnesis: async () => ({ ok: true }),
}));

mock.module("../house-proof/substrate.ts", () => ({
  catchBoat: async () => ({ ok: true, found: false }),
  closePaperBoatTransports: () => undefined,
  readQuestBoard: async () => ({ ok: true, quests: [] }),
  formatQuestBoardSection: () => "",
}));

mock.module("../house-proof/entity-resolution.ts", () => ({
  resolveEntities: async () => ({ ok: true, matches: [] }),
}));

mock.module("../house-proof/recall-telemetry.ts", () => ({
  recordRecallTelemetry: async () => true,
}));

mock.module("../house-proof/context.ts", () => ({
  analyzeContext: async () => ({
    route: {
      entityResolutionSuggested: false,
      intent: "contact",
      terms: [],
      requiredTerms: [],
      recognizedEntities: [],
    },
  }),
  applyRecallViewport: async () => ({
    presentation: { found: [], warnings: [], retrievalCandidates: [], canonMatches: [], dateMatches: [] },
    diagnostics: {},
  }),
}));

mock.module("../house-proof/recall-policy.ts", () => ({
  RecallPolicyHostClient: class {
    async inspect() {
      return { recallPolicy };
    }

    async evaluate() {
      return {
        decision: {
          shouldRecall: false,
          clearWorkingSet: false,
          query: "",
          queryTerms: [],
          refreshReason: null,
          intent: "contact",
          resolvedMode: "conversation",
        },
        snapshot: { recallPolicy },
      };
    }

    async completeRefresh() {
      return { recallPolicy };
    }

    async failRefresh() {
      return { recallPolicy };
    }

    async invalidateAfterCompaction() {
      return undefined;
    }
  },
  isMutateTool: () => false,
  markToolEvidence: () => undefined,
  hasToolEvidence: () => false,
  activeProjectFromEvidence: () => null,
  mutateToolPaths: () => [],
}));

const { default: registerAdapter } = await import("../index.ts");

type Hook = (event: any, context: any) => Promise<any>;

function adapterHooks(): { context: Hook; turnEnd: Hook } {
  let contextHook: Hook | undefined;
  let turnEndHook: Hook | undefined;
  registerAdapter({
    setLabel() {},
    events: { on: () => () => undefined },
    on(name: string, candidate: Hook) {
      if (name === "context") contextHook = candidate;
      if (name === "turn_end") turnEndHook = candidate;
    },
  } as any);
  if (!contextHook) throw new Error("context hook was not registered");
  if (!turnEndHook) throw new Error("turn_end hook was not registered");
  return { context: contextHook, turnEnd: turnEndHook };
}

function session(sessionID: string) {
  topLevelSessionId = sessionID;
  return { cwd: "C:/test/kintsu", sessionID };
}

async function openPendingContract(hooks: { context: Hook }, sessionID: string) {
  const result = await hooks.context(
    { messages: [{ id: "turn-1", role: "user", content: "stay with me" }] },
    session(sessionID),
  );
  const compiled = result?.messages?.find((message: any) =>
    message.customType === "athanor-presence-context"
  );
  if (!compiled) throw new Error("the context hook compiled no Presence contract");
}

beforeEach(async () => {
  effectiveRoomDir = await mkdtemp(path.join(os.tmpdir(), "athanor-presence-settlement-"));
  topLevelSessionId = "";
  settlements.length = 0;
  settleFailure = null;
  compiledGuardId = "presence:nonempty-response";
});

afterEach(async () => {
  await rm(effectiveRoomDir, { recursive: true, force: true });
  effectiveRoomDir = "";
});

describe("Presence settlement at turn end", () => {
  test("settles an explicit Refuse carrying the nonempty hard-guard violation", async () => {
    const hooks = adapterHooks();
    const sessionID = "empty-response-session";
    await openPendingContract(hooks, sessionID);

    await hooks.turnEnd({ message: { role: "assistant", content: "" } }, session(sessionID));

    expect(settlements).toHaveLength(1);
    expect(settlements[0]).toEqual({
      contractId: "contract-1",
      attempt: 1,
      evaluatedDirectives: ["presence:active-spirit", "presence:nonempty-response"],
      violations: [{
        directiveId: "presence:nonempty-response",
        reason: "The assistant turn emitted no text.",
      }],
      decision: "refuse",
      responseDigest: null,
    });
  });

  test("a whitespace-only turn is still an empty turn", async () => {
    const hooks = adapterHooks();
    const sessionID = "whitespace-session";
    await openPendingContract(hooks, sessionID);

    await hooks.turnEnd({ message: { role: "assistant", content: "   \n\t " } }, session(sessionID));

    expect(settlements).toHaveLength(1);
    expect(settlements[0].decision).toBe("refuse");
  });

  test("an emitted response still settles as Accept over every hard directive", async () => {
    const hooks = adapterHooks();
    const sessionID = "accept-session";
    await openPendingContract(hooks, sessionID);

    await hooks.turnEnd({ message: { role: "assistant", content: "here I am" } }, session(sessionID));

    expect(settlements).toHaveLength(1);
    expect(settlements[0]).toMatchObject({
      decision: "accept",
      violations: [],
      evaluatedDirectives: ["presence:active-spirit", "presence:nonempty-response"],
      responseDigest: "digest:here I am",
    });
  });

  test("a settled contract is forgotten, so a repeated turn end does not settle twice", async () => {
    const hooks = adapterHooks();
    const sessionID = "settled-once-session";
    await openPendingContract(hooks, sessionID);

    await hooks.turnEnd({ message: { role: "assistant", content: "" } }, session(sessionID));
    await hooks.turnEnd({ message: { role: "assistant", content: "" } }, session(sessionID));

    expect(settlements).toHaveLength(1);
  });

  test("a failed settlement keeps the contract pending so the next turn end retries it", async () => {
    const hooks = adapterHooks();
    const sessionID = "failed-settlement-session";
    await openPendingContract(hooks, sessionID);

    settleFailure = "Host refused the settlement";
    await hooks.turnEnd({ message: { role: "assistant", content: "" } }, session(sessionID));
    expect(settlements).toHaveLength(1);

    settleFailure = null;
    await hooks.turnEnd({ message: { role: "assistant", content: "" } }, session(sessionID));
    expect(settlements).toHaveLength(2);
    expect(settlements[1].decision).toBe("refuse");

    // Now it is settled, so it is forgotten.
    await hooks.turnEnd({ message: { role: "assistant", content: "" } }, session(sessionID));
    expect(settlements).toHaveLength(2);
  });

  test("a contract without the guard still settles Refuse rather than staying pending", async () => {
    compiledGuardId = null;
    const hooks = adapterHooks();
    const sessionID = "guardless-session";
    await openPendingContract(hooks, sessionID);

    await hooks.turnEnd({ message: { role: "assistant", content: "" } }, session(sessionID));

    expect(settlements).toHaveLength(1);
    expect(settlements[0]).toMatchObject({ decision: "refuse", violations: [] });
  });
});

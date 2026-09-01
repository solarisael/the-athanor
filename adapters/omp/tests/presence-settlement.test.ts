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

// Bun keeps a module mock for the whole process, so every later test file in
// the run sees these stand-ins. Each mock therefore spreads the real module
// and the fakes answer only while this file's own fixture is live; the
// fixture flag is `effectiveRoomDir`, set by beforeEach and cleared by
// afterEach. Downstream files get the genuine surface.
const realRoom = await import("../house-proof/room.ts?real");
const realFence = await import("../house-proof/top-level-session-fence.ts?real");
const realConversationLog = await import("../house-proof/conversation-log.ts?real");
const realGiga = await import("../giga.ts?real");
const realRecall = await import("../house-proof/recall.ts?real");
const realTools = await import("../house-proof/tools.ts?real");
const realPresence = await import("../house-proof/presence.ts?real");
const realHallway = await import("../house-proof/hallway.ts?real");
const realAnamnesis = await import("../house-proof/anamnesis.ts?real");
const realSubstrate = await import("../house-proof/substrate.ts?real");
const realEntities = await import("../house-proof/entity-resolution.ts?real");
const realTelemetry = await import("../house-proof/recall-telemetry.ts?real");
const realContext = await import("../house-proof/context.ts?real");
const realPolicy = await import("../house-proof/recall-policy.ts?real");

const live = () => effectiveRoomDir !== "";
function gated<F extends (...args: any[]) => any>(fake: F, genuine: F): F {
  return ((...args: any[]) => (live() ? fake(...args) : genuine(...args))) as F;
}

mock.module("../house-proof/room.ts", () => ({
  ...realRoom,
  applyPromptDirectives: gated(
    async () => ({ state: { operator: "Sol", embodiedSpirit: "Kintsu" } }),
    realRoom.applyPromptDirectives,
  ),
  roomContext: gated(
    () => ({ room: "kintsu", spirit: "Kintsu", operator: "Sol", effectiveRoomDir }),
    realRoom.roomContext,
  ),
  writeActiveSpiritSnapshot: gated(async () => undefined, realRoom.writeActiveSpiritSnapshot),
}));

mock.module("../house-proof/top-level-session-fence.ts", () => ({
  ...realFence,
  adoptTopLevelSession: gated(
    (_room: string, session: string) => { topLevelSessionId ||= session; },
    realFence.adoptTopLevelSession,
  ),
  registerTopLevelSession: gated(
    (_room: string, session: string) => { topLevelSessionId = session; },
    realFence.registerTopLevelSession,
  ),
  retireTopLevelSession: gated(
    (_room: string, session: string) => {
      if (topLevelSessionId === session) topLevelSessionId = "";
    },
    realFence.retireTopLevelSession,
  ),
  topLevelSession: gated(() => topLevelSessionId || null, realFence.topLevelSession),
}));

mock.module("../house-proof/conversation-log.ts", () => ({
  ...realConversationLog,
  logConversationWindow: gated(
    async () => ({ fresh: false, loggedTurns: [] }),
    realConversationLog.logConversationWindow,
  ),
}));

mock.module("../giga.ts", () => ({
  ...realGiga,
  closeGigaTransports: gated(async () => undefined, realGiga.closeGigaTransports),
  ingestGigaLoggedTurnsDetached: gated(() => undefined, realGiga.ingestGigaLoggedTurnsDetached),
}));

mock.module("../house-proof/recall.ts", () => ({
  ...realRecall,
  closeRustRecallTransports: gated(() => undefined, realRecall.closeRustRecallTransports),
  recallWithRouting: gated(
    async () => ({ ok: true, result: { query: "" } }),
    realRecall.recallWithRouting,
  ),
}));

mock.module("../house-proof/tools.ts", () => ({
  ...realTools,
  closeRustRememberTransports: gated(() => undefined, realTools.closeRustRememberTransports),
  registerSolarisaelTools: gated(() => undefined, realTools.registerSolarisaelTools),
  writeRustMemory: gated(async () => undefined, realTools.writeRustMemory),
}));

mock.module("../house-proof/presence.ts", () => ({
  ...realPresence,
  compilePresenceContext: gated(
    async (request: any) => ({
      frameId: "frame-1",
      frameVersion: 1,
      frameRendered: "Presence frame",
      contractId: "contract-1",
      turnId: request.turnId,
      directiveIds: ["presence:active-spirit", "presence:nonempty-response"],
      nonemptyGuardId: compiledGuardId,
      rendered: "Presence frame\n\nPresence contract",
    }),
    realPresence.compilePresenceContext,
  ),
  settlePresence: gated(
    (async (_binding: unknown, request: Record<string, any>) => {
      settlements.push(request);
      if (settleFailure) throw new Error(settleFailure);
      return { contractId: "contract-1" };
    }) as typeof realPresence.settlePresence,
    realPresence.settlePresence,
  ),
  responseDigest: gated((text: string) => `digest:${text}`, realPresence.responseDigest),
}));

mock.module("../house-proof/hallway.ts", () => ({
  ...realHallway,
  projectHallwayInbox: gated(
    async () => ({ changed: false, inbox: { ok: true, hallways: [] } }),
    realHallway.projectHallwayInbox,
  ),
}));

mock.module("../house-proof/anamnesis.ts", () => ({
  ...realAnamnesis,
  closeRustAnamnesisTransports: gated(() => undefined, realAnamnesis.closeRustAnamnesisTransports),
  formatAnamnesisContext: gated(() => "", realAnamnesis.formatAnamnesisContext),
  queryAnamnesis: gated(async () => ({ ok: true }), realAnamnesis.queryAnamnesis),
}));

mock.module("../house-proof/substrate.ts", () => ({
  ...realSubstrate,
  catchBoat: gated(async () => ({ ok: true, found: false }), realSubstrate.catchBoat),
  closePaperBoatTransports: gated(() => undefined, realSubstrate.closePaperBoatTransports),
  readQuestBoard: gated(async () => ({ ok: true, quests: [] }), realSubstrate.readQuestBoard),
  formatQuestBoardSection: gated(() => "", realSubstrate.formatQuestBoardSection),
}));

mock.module("../house-proof/entity-resolution.ts", () => ({
  ...realEntities,
  resolveEntities: gated(async () => ({ ok: true, matches: [] }), realEntities.resolveEntities),
}));

mock.module("../house-proof/recall-telemetry.ts", () => ({
  ...realTelemetry,
  recordRecallTelemetry: gated(async () => true, realTelemetry.recordRecallTelemetry),
}));

mock.module("../house-proof/context.ts", () => ({
  ...realContext,
  analyzeContext: gated(
    async () => ({
      route: {
        entityResolutionSuggested: false,
        intent: "contact",
        terms: [],
        requiredTerms: [],
        recognizedEntities: [],
      },
    }),
    realContext.analyzeContext,
  ),
  applyRecallViewport: gated(
    async () => ({
      presentation: { found: [], warnings: [], retrievalCandidates: [], canonMatches: [], dateMatches: [] },
      diagnostics: {},
    }),
    realContext.applyRecallViewport,
  ),
}));

mock.module("../house-proof/recall-policy.ts", () => ({
  ...realPolicy,
  RecallPolicyHostClient: class {
    constructor(binding: any) {
      if (!live()) {
        return new (realPolicy.RecallPolicyHostClient as any)(binding);
      }
    }

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
  isMutateTool: gated(() => false, realPolicy.isMutateTool),
  markToolEvidence: gated(() => undefined, realPolicy.markToolEvidence),
  hasToolEvidence: gated(() => false, realPolicy.hasToolEvidence),
  activeProjectFromEvidence: gated(() => null, realPolicy.activeProjectFromEvidence),
  mutateToolPaths: gated(() => [], realPolicy.mutateToolPaths),
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

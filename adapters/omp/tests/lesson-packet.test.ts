import { describe, expect, mock, test } from "bun:test";

let mode = "work";
let packet: Record<string, unknown> = { ok: true, lessons: [{ type: "coding", id: 9, title: "Keep doors narrow", lesson: "A narrow door owns one boundary.", proofPattern: "one boundary" }] };
const warnings: string[][] = [];

mock.module("../solarisael-house-proof/room.ts", () => ({
  applyPromptDirectives: async () => ({ state: { operator: "Sol", embodiedSpirit: "Kodo" } }),
  roomContext: () => ({ room: "kodo", spirit: "Kodo", operator: "Sol", effectiveRoomDir: process.cwd() }),
  writeActiveSpiritSnapshot: async () => undefined,
}));
mock.module("../solarisael-house-proof/lesson-context.ts", () => ({ runLessonQuery: async () => packet }));
mock.module("../solarisael-house-proof/recall-policy.ts", () => ({
  RecallPolicyHostClient: class {
    async inspect() { return { recallPolicy: { requestedMode: "auto", resolvedMode: mode } }; }
    async evaluate() { return { decision: { shouldRecall: false, resolvedMode: mode }, snapshot: { recallPolicy: { requestedMode: "auto", resolvedMode: mode } } }; }
    async invalidateAfterCompaction() { return undefined; }
  },
  isMutateTool: () => false,
  markToolEvidence: () => undefined,
  hasToolEvidence: () => false,
}));
mock.module("../solarisael-house-proof/context.ts", () => ({
  analyzeContext: async () => ({ route: { entityResolutionSuggested: false, intent: "technical_project", terms: [], requiredTerms: [] } }),
  applyRecallViewport: async () => ({ presentation: { found: [], warnings: [], retrievalCandidates: [], canonMatches: [], dateMatches: [] }, diagnostics: {} }),
}));
mock.module("../solarisael-house-proof/feedback.ts", () => ({ showHouseContextFeedback: (_ctx: unknown, feedback: { warnings: string[] }) => warnings.push(feedback.warnings) }));
mock.module("../solarisael-house-proof/lesson-triggers.ts", () => ({
  closeLessonTriggerTransports() {}, filterInterruptedLessonProse: (value: unknown) => value,
  lessonTriggerMessageRenderer() {}, lessonTriggerProseAddition: async () => null,
  lessonTriggerProseStreamUpdate() {}, lessonTriggerToolCall: async () => null,
  prependLessonReminder() {}, processLessonsMessageRenderer() {}, resetLessonTriggerProseStream() {}, takeLessonReminder() {}, toolLessonCard() {},
}));
mock.module("../solarisael-house-proof/conversation-log.ts", () => ({ logConversationWindow: async () => ({ fresh: false, loggedTurns: [] }) }));
mock.module("../giga.ts", () => ({ closeGigaTransports() {}, ingestGigaLoggedTurnsDetached() {} }));
mock.module("../solarisael-house-proof/recall.ts", () => ({ closeRustRecallTransports() {}, recallWithRouting: async () => ({ ok: false, result: null }) }));
mock.module("../solarisael-house-proof/tools.ts", () => ({ closeRustRememberTransports() {}, registerSolarisaelTools() {}, writeRustMemory: async () => undefined }));
mock.module("../solarisael-house-proof/anamnesis.ts", () => ({ closeRustAnamnesisTransports() {}, formatAnamnesisContext: () => "", queryAnamnesis: async () => ({ ok: true }) }));
mock.module("../solarisael-house-proof/hallway.ts", () => ({ projectHallwayInbox: async () => ({ changed: false, inbox: { ok: true, hallways: [] } }) }));
mock.module("../solarisael-house-proof/substrate.ts", () => ({ catchBoat: async () => ({ ok: true, found: false }), closePaperBoatTransports() {}, readQuestBoard: async () => ({ ok: true, quests: [] }), formatQuestBoardSection: () => "" }));
mock.module("../solarisael-house-proof/entity-resolution.ts", () => ({ resolveEntities: async () => ({ ok: true, matches: [] }) }));
mock.module("../solarisael-house-proof/recall-telemetry.ts", () => ({ recordRecallTelemetry: async () => true }));
mock.module("../solarisael-house-proof/triggers.ts", () => ({ processLessonsReminder: async () => null }));

// The context handler answers `undefined` when it neither injected nor changed
// anything — that IS the no-op contract, so these proofs resolve the final
// message array from either shape.
async function context(messages: Array<Record<string, unknown>>) {
  const { default: register } = await import(`../index.ts?packet-${crypto.randomUUID()}`);
  let handler: ((event: any, ctx: any) => Promise<any>) | undefined;
  register({ setLabel() {}, registerMessageRenderer() {}, on(name: string, candidate: any) { if (name === "context") handler = candidate; } });
  const result = await handler!({ messages }, { cwd: process.cwd(), sessionID: crypto.randomUUID(), ui: { notify() {} } });
  return { result, messages: result?.messages ?? messages };
}

const user = (id: string) => ({ role: "user", content: "implement the packet", id });
const packets = (messages: Array<Record<string, unknown>>) => messages.filter((message) => message.customType === "solarisael-lesson-packet");

describe("work-mode lesson packet", () => {
  // Kills: removing the work-mode query or its custom message addition.
  // red-proof: replace `resolvedMode === "work"` with `false`.
  test("injects always-on coding lessons in work mode", async () => {
    mode = "work"; packet = { ok: true, lessons: [{ type: "coding", id: 9, title: "Keep doors narrow", lesson: "A narrow door owns one boundary.", proofPattern: "one boundary" }] };
    const { messages } = await context([user("one")]);
    expect(packets(messages)).toHaveLength(1);
    expect(packets(messages)[0].content).toContain("coding#9 — Keep doors narrow");
  });

  // Kills: ignoring the existing custom-type guard.
  // red-proof: remove `!existingTypes.has("solarisael-lesson-packet")`.
  test("does not duplicate an existing packet", async () => {
    mode = "work";
    const { messages } = await context([user("two"), { role: "custom", customType: "solarisael-lesson-packet", content: "old" }]);
    expect(packets(messages)).toHaveLength(1);
  });

  // Kills: injecting packets outside the resolved work mode.
  // red-proof: remove the resolved-mode condition.
  test("does not inject in conversation mode", async () => {
    mode = "conversation";
    const { messages } = await context([user("three")]);
    expect(packets(messages)).toHaveLength(0);
  });

  // Kills: treating a failed lesson transport as a packet.
  // red-proof: push a packet when `packet.ok` is false.
  test("fails open and warns when lesson transport degrades", async () => {
    mode = "work"; packet = { ok: false, lessons: [], error: "offline" }; warnings.length = 0;
    const { messages } = await context([user("four")]);
    expect(packets(messages)).toHaveLength(0);
    expect(warnings.flat()).toContain("work-mode lesson packet unavailable");
  });
});

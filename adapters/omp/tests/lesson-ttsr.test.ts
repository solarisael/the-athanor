import { beforeEach, describe, expect, mock, test } from "bun:test";

let rows: any[] = [];
mock.module("../house-proof/lesson-context.ts", () => ({
  runLessonQuery: async (_roomDir: string, _room: string, filters: { type: string }) => ({
    ok: true,
    lessons: filters.type === "coding" ? rows : [],
  }),
}));

const { installLessonTtsrBridge, syncLessonTtsr } = await import("../house-proof/lesson-ttsr.ts");

class FakeManager {
  rules: any[] = [];
  addRule(rule: any) { this.rules.push(rule); return true; }
  checkDelta(text: string) { return this.rules.filter((rule) => rule.condition?.some((pattern: string) => new RegExp(pattern, "i").test(text))); }
  checkSnapshot(text: string) { return this.checkDelta(text); }
  async checkAstSnapshot() { return []; }
}

class FakeSession {
  ttsrManager = new FakeManager();
  sessionManager = { getSessionId: () => "native-lesson-session" };
  getContextUsage() { return undefined; }
}

function row(condition: string) {
  return {
    id: 175, type: "coding", title: "Keep reports short", lesson: "Keep reports short.",
    tags: ["ttsr-approved"], condition: [condition], astCondition: [], triggerScope: ["text"],
    interruptMode: "block", repeatCooldownSecs: null, languageKeys: [],
  };
}

beforeEach(() => { rows = [row("forbidden")]; });

describe("native lesson TTSR bridge", () => {
  test("replaces an approved guard without leaving its old condition active", async () => {
    expect(installLessonTtsrBridge({ pi: { AgentSession: FakeSession } })).toBeNull();
    const session = new FakeSession();
    const ctx = { sessionManager: session.sessionManager, getContextUsage: () => session.getContextUsage() };

    expect(await syncLessonTtsr({ ctx, roomDir: process.cwd(), room: "tuner", activeProject: null }))
      .toMatchObject({ active: 1, added: 1, warnings: [] });
    expect(session.ttsrManager.checkDelta("forbidden")).toHaveLength(1);
    expect(session.ttsrManager.checkDelta("ordinary")).toHaveLength(0);

    rows = [row("replacement")];
    await syncLessonTtsr({ ctx, roomDir: process.cwd(), room: "tuner", activeProject: null });
    expect(session.ttsrManager.checkDelta("forbidden")).toHaveLength(0);
    expect(session.ttsrManager.checkDelta("replacement")).toHaveLength(1);
  });

  test("refuses to claim support when OMP hides the capture seam", () => {
    expect(installLessonTtsrBridge({ pi: {} })).toContain("AgentSession.getContextUsage");
  });
});

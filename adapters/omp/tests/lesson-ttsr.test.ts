import { beforeEach, describe, expect, mock, test } from "bun:test";

let rows: any[] = [];
let queryFails = false;
mock.module("../solarisael-house-proof/lesson-context.ts", () => ({
  runLessonQuery: async (_roomDir: string, _room: string, filters: { type: string }) => {
    if (queryFails) return { ok: false, lessons: [], error: "transient query failure" };
    return {
      ok: true,
      lessons: filters.type === "coding" ? rows : [],
    };
  },
}));

const {
  installLessonTtsrBridge,
  installStreamingTranscriptBridge,
  syncLessonTtsr,
} = await import("../solarisael-house-proof/lesson-ttsr.ts");

class FakeManager {
  rules: any[] = [];
  addRule(rule: any) { this.rules.push(rule); return true; }
  checkDelta(text: string) { return this.rules.filter((rule) => rule.condition?.some((pattern: string) => new RegExp(pattern, "i").test(text))); }
  checkSnapshot(text: string) { return this.checkDelta(text); }
  async checkAstSnapshot() { return []; }
}

class FakeSession {
  ttsrManager = new FakeManager();
  sessionManager: { getSessionId: () => string };
  constructor(sessionId = "native-lesson-session") {
    this.sessionManager = { getSessionId: () => sessionId };
  }
  getContextUsage() { return undefined; }
}

class FakeMarkdown {
  transientRenderCache = false;
  constructor(private text: string) {}
  setText(text: string) { this.text = text; }
  getLastRenderStableText() {
    if (!this.transientRenderCache) return "";
    const boundary = this.text.lastIndexOf("\n\n");
    return boundary < 0 ? "" : this.text.slice(0, boundary + 2);
  }
  render(_width?: number) {
    return this.text.trim().split(/\n\n+/).filter(Boolean);
  }
}

class FakeAssistantMessageComponent {
  private text = "";
  private thinking = "";
  updateContent(message: any, _opts?: { transient?: boolean }) {
    const content = Array.isArray(message?.content) ? message.content : [];
    this.text = String(content.find((block: any) => block?.type === "text")?.text ?? "").trim();
    this.thinking = String(content.find((block: any) => block?.type === "thinking")?.thinking ?? "").trim();
  }
  render(_width?: number) {
    const textRows = new FakeMarkdown(this.text).render();
    return this.thinking ? [this.thinking, "", ...textRows] : textRows;
  }
  getTranscriptStableRows() {
    return this.thinking ? [{ key: "native-thinking" }] : [];
  }
  renderTranscriptStableRows(count = 0, _width?: number) {
    return this.thinking && count > 0 ? [this.thinking] : [];
  }
  setTextColorTransform(_transform?: (text: string) => string) {}
}

function row(condition: string) {
  return {
    id: 175, type: "coding", title: "Keep reports short", lesson: "Keep reports short.",
    tags: ["ttsr-approved"], condition: [condition], astCondition: [], triggerScope: ["text"],
    interruptMode: "block", repeatCooldownSecs: null, languageKeys: [],
  };
}

beforeEach(() => {
  rows = [row("forbidden")];
  queryFails = false;
});

describe("native lesson TTSR bridge", () => {
  test("keeps native match methods untouched and freezes the first set until restart", async () => {
    expect(installLessonTtsrBridge({ pi: { AgentSession: FakeSession } })).toBeNull();
    const session = new FakeSession();
    const nativeCheckDelta = session.ttsrManager.checkDelta;
    const ctx = { sessionManager: session.sessionManager, getContextUsage: () => session.getContextUsage() };

    expect(await syncLessonTtsr({ ctx, roomDir: process.cwd(), room: "tuner", activeProject: null }))
      .toMatchObject({ active: 1, added: 1, warnings: [] });
    expect(session.ttsrManager.checkDelta).toBe(nativeCheckDelta);
    expect(session.ttsrManager.checkDelta("forbidden")).toHaveLength(1);
    expect(session.ttsrManager.checkDelta("ordinary")).toHaveLength(0);

    rows = [row("replacement")];
    expect(await syncLessonTtsr({ ctx, roomDir: process.cwd(), room: "tuner", activeProject: null }))
      .toMatchObject({ active: 1, added: 0, warnings: ["native lesson rules changed; restart OMP to apply the new set"] });
    expect(session.ttsrManager.checkDelta("forbidden")).toHaveLength(1);
    expect(session.ttsrManager.checkDelta("replacement")).toHaveLength(0);
  });

  test("keeps every native manager for one resumed session active", async () => {
    installLessonTtsrBridge({ pi: { AgentSession: FakeSession } });
    const first = new FakeSession("shared-resumed-session");
    const second = new FakeSession("shared-resumed-session");
    first.getContextUsage();
    second.getContextUsage();
    const ctx = { sessionManager: first.sessionManager, getContextUsage: () => undefined };

    expect(await syncLessonTtsr({ ctx, roomDir: process.cwd(), room: "tuner", activeProject: null }))
      .toMatchObject({ active: 1, added: 1, warnings: [] });
    expect(first.ttsrManager.checkDelta("forbidden")).toHaveLength(1);
    expect(second.ttsrManager.checkDelta("forbidden")).toHaveLength(1);
  });

  test("hydrates a manager first observed after the session sync", async () => {
    installLessonTtsrBridge({ pi: { AgentSession: FakeSession } });
    const first = new FakeSession("late-manager-session");
    first.getContextUsage();
    const ctx = { sessionManager: first.sessionManager, getContextUsage: () => undefined };
    await syncLessonTtsr({ ctx, roomDir: process.cwd(), room: "tuner", activeProject: null });

    const late = new FakeSession("late-manager-session");
    late.getContextUsage();
    expect(late.ttsrManager.checkDelta("forbidden")).toHaveLength(1);
  });


  test("preserves the last good active set through a transient query failure", async () => {
    installLessonTtsrBridge({ pi: { AgentSession: FakeSession } });
    const session = new FakeSession("query-failure-session");
    session.getContextUsage();
    const ctx = { sessionManager: session.sessionManager, getContextUsage: () => undefined };
    await syncLessonTtsr({ ctx, roomDir: process.cwd(), room: "tuner", activeProject: null });

    queryFails = true;
    const degraded = await syncLessonTtsr({
      ctx,
      roomDir: process.cwd(),
      room: "tuner",
      activeProject: null,
    });
    expect(degraded.active).toBe(1);
    expect(degraded.warnings).toHaveLength(4);
    expect(session.ttsrManager.checkDelta("forbidden")).toHaveLength(1);
  });

  test("refuses to claim support when OMP hides the capture seam", () => {
    expect(installLessonTtsrBridge({ pi: {} })).toContain("AgentSession.getContextUsage");
  });

  test("retires stable text paragraphs while the response continues streaming", () => {
    expect(installStreamingTranscriptBridge({
      pi: {
        AssistantMessageComponent: FakeAssistantMessageComponent,
        Markdown: FakeMarkdown,
        getMarkdownTheme: () => ({}),
      },
    })).toBeNull();
    const component = new FakeAssistantMessageComponent();
    component.updateContent({
      content: [{ type: "text", text: "first paragraph\n\nsecond paragraph" }],
    }, { transient: true });

    expect(component.render()).toEqual(["first paragraph", "second paragraph"]);
    expect(component.getTranscriptStableRows()).toHaveLength(1);
    expect(component.renderTranscriptStableRows(1, 80)).toEqual(["first paragraph"]);

    component.updateContent({
      content: [{ type: "text", text: "first paragraph\n\nsecond paragraph\n\nunfinished tail" }],
    }, { transient: true });
    component.render();
    expect(component.getTranscriptStableRows()).toHaveLength(2);
    expect(component.renderTranscriptStableRows(2, 80)).toEqual(["first paragraph", "second paragraph"]);
  });

  test("extends OMP's native thinking prefix with stable response paragraphs", () => {
    installStreamingTranscriptBridge({
      pi: {
        AssistantMessageComponent: FakeAssistantMessageComponent,
        Markdown: FakeMarkdown,
        getMarkdownTheme: () => ({}),
      },
    });
    const component = new FakeAssistantMessageComponent();
    component.updateContent({
      content: [
        { type: "thinking", thinking: "private reasoning" },
        { type: "text", text: "first paragraph\n\nsecond paragraph" },
      ],
    }, { transient: true });
    component.render();
    expect(component.getTranscriptStableRows()).toHaveLength(2);
    expect(component.renderTranscriptStableRows(2, 80)).toEqual([
      "private reasoning",
      "",
      "first paragraph",
    ]);
  });
});

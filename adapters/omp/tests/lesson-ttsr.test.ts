import { expect, test } from "bun:test";
import { randomUUID } from "node:crypto";
import { TtsrManager } from "@oh-my-pi/pi-coding-agent/export/ttsr";
import { blockLessonRefusal, contextLessonManagerAvailable, installLessonTtsrBridge, syncLessonTtsr } from "../house-proof/lesson-ttsr.ts";
import type { NativeLessonPlan } from "../house-proof/context.ts";

const plan: NativeLessonPlan = {
  token: "native-plan",
  requiresCredential: false,
  turnId: "turn",
  warnings: [],
  rules: [{
    block: true,
    rule: {
      name: "athanor-coding-389-native",
      path: "athanor://lessons/coding/389",
      content: "Do not discard a caught failure.",
      description: "No empty catch",
      astCondition: ["try { $$$BODY } catch ($ERR) { }"],
      scope: ["tool:edit", "tool:write"],
      interruptMode: "always",
      _source: { provider: "athanor-lessons", providerName: "The Athanor", path: "athanor://lessons/coding/389", level: "native" },
    },
  }],
};

test("a supplied native block rule refuses the identical write after the stream interrupt is spent", async () => {
  const id = randomUUID();
  const write = { name: "write", matcherDigest: (args: any) => args.content };
  class AgentSession {
    sessionManager = { getSessionId: () => id };
    ttsrManager = new TtsrManager({ enabled: true, repeatMode: "once" });
    agent = { state: { tools: [write] } };
    getContextUsage() { return {}; }
  }
  installLessonTtsrBridge({ pi: { AgentSession } });
  const ctx = new AgentSession();
  expect(contextLessonManagerAvailable(ctx)).toBe(true);
  expect(syncLessonTtsr({ ctx, plan }).warnings).toEqual([]);
  const attempt = (toolCallId: string, content: string) =>
    blockLessonRefusal({ toolName: "write", toolCallId, input: { path: "a.ts", content } }, ctx);
  const swallowed = "try {\n  run();\n} catch (error) { }\n";
  expect((await attempt("first", swallowed))?.reason).toContain("athanor://lessons/coding/389");
  ctx.ttsrManager.markInjectedByNames(ctx.ttsrManager.getRules().map((rule) => rule.name));
  expect((await attempt("retry", swallowed))?.reason).toContain("athanor://lessons/coding/389");
  expect(await attempt("handled", "try {\n  run();\n} catch (error) {\n  report(error);\n}\n")).toBeUndefined();
  syncLessonTtsr({ ctx, plan: { ...plan, rules: [] } });
  expect(await attempt("retired", swallowed)).toBeUndefined();
});

test("an unavailable OMP manager cannot report an installed native rule", () => {
  const ctx = { sessionID: randomUUID() };
  expect(contextLessonManagerAvailable(ctx)).toBe(false);
  expect(syncLessonTtsr({ ctx, plan }).active).toBe(0);
});

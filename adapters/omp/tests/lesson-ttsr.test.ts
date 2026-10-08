import { expect, test } from "bun:test";
import { randomUUID } from "node:crypto";
import { TtsrManager } from "@oh-my-pi/pi-coding-agent/export/ttsr";
import { spyOn } from "bun:test";
import { blockLessonRefusal, contextLessonManagerAvailable, installLessonTtsrBridge, recordNativeFires, syncLessonTtsr } from "../house-proof/lesson-ttsr.ts";
import type { NativeLessonPlan } from "../house-proof/context.ts";
import * as organ from "../house-proof/organ.ts";

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

// Kills: the guard refusing without writing the ledger, or the native fire recorder
// inventing a pattern when the rule carries several of the matched kind.
test("a block refusal and a native fire both land in the ledger with the matcher that caught them", async () => {
  const remind = {
    block: false,
    rule: {
      name: "athanor-coding-175-native",
      path: "athanor://lessons/coding/175",
      content: "Outcome reports stay short.",
      description: "Short reports",
      condition: ["(?im)^In summary\\b", "(?im)^To summarize\\b"],
      scope: ["text"],
      interruptMode: "never",
      _source: { provider: "athanor-lessons", providerName: "The Athanor", path: "athanor://lessons/coding/175", level: "native" },
    },
  };
  const record = spyOn(organ, "requestOrgan").mockResolvedValue({ ok: true, eventId: 1, fires: 1 });
  try {
    const id = randomUUID();
    const write = { name: "write", matcherDigest: (args: any) => args.content };
    class AgentSession {
      sessionManager = { getSessionId: () => id };
      ttsrManager = new TtsrManager({ enabled: true, repeatMode: "once" });
      agent = { state: { tools: [write] } };
      getContextUsage() { return {}; }
    }
    installLessonTtsrBridge({ pi: { AgentSession } });
    const ctx: any = new AgentSession();
    expect(syncLessonTtsr({ ctx, plan: { ...plan, rules: [...plan.rules, remind] } }).warnings).toEqual([]);
    const binding = { room: "kodo", spirit: "Kodo", session: id };

    const refusal = await blockLessonRefusal(
      { toolName: "write", toolCallId: "blocked", input: { path: "a.ts", content: "try {\n  run();\n} catch (error) { }\n" } },
      ctx,
      binding,
    );
    expect(refusal?.block).toBe(true);
    // The Host adds `room` and `session` from the sender, so the params carry neither.
    expect(record.mock.calls).toEqual([[binding, "lesson_trigger_record", {
      family: "coding", id: 389,
      surface: "tool", tool: "write", path: "a.ts",
      patternKind: "ast", matchedPattern: "try { $$$BODY } catch ($ERR) { }", urgency: "block",
    }, { write: true, timeoutMs: 10_000 }]]);

    // The native manager sees the prose stream through the patched check, then OMP announces the rule.
    record.mockClear();
    const fired = ctx.ttsrManager.checkSnapshot("In summary, done.", { source: "text", streamKey: "turn-1" });
    expect(fired.map((rule: any) => rule.path)).toEqual(["athanor://lessons/coding/175"]);
    expect(await recordNativeFires(binding, fired)).toBe(1);
    expect(record.mock.calls[0]?.[2]).toEqual({
      family: "coding", id: 175,
      surface: "prose", tool: undefined, path: undefined,
      patternKind: "regex", matchedPattern: undefined, urgency: "remind",
    });

    // A foreign rule, an unknown session, and a refused ledger are never fatal.
    record.mockClear();
    expect(await recordNativeFires(binding, [{ name: "omp-own-rule", _source: { provider: "omp" } }])).toBe(0);
    expect(await recordNativeFires({ ...binding, session: "nobody" }, fired)).toBe(0);
    expect(record.mock.calls).toEqual([]);
    record.mockRejectedValue(new Error("substrate away"));
    expect(await recordNativeFires(binding, fired)).toBe(0);
  } finally {
    record.mockRestore();
  }
});

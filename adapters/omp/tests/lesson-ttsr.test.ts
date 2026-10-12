import { expect, test } from "bun:test";
import { randomUUID } from "node:crypto";
import { TtsrManager } from "@oh-my-pi/pi-coding-agent/export/ttsr";
import { spyOn } from "bun:test";
import {
  blockLessonRefusal,
  capturedAgentSession,
  contextLessonManagerAvailable,
  installLessonTtsrBridge,
  recordNativeFires,
  releaseLessonTtsrSession,
  syncLessonTtsr,
} from "../house-proof/lesson-ttsr.ts";
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

// --- Registration truth lives in the OMP manager, one patch per manager ---

const SOURCE = { provider: "athanor-lessons", providerName: "The Athanor", level: "native" };

function remindRule(id: number, pattern: string) {
  const path = `athanor://lessons/coding/${id}`;
  return {
    block: false,
    rule: {
      name: `athanor-coding-${id}-native`, path, content: `Lesson ${id}.`, description: `Lesson ${id}`,
      condition: [pattern], scope: ["text"], interruptMode: "never", _source: { ...SOURCE, path },
    },
  };
}

function planOf(...rules: NativeLessonPlan["rules"]): NativeLessonPlan {
  return { ...plan, rules };
}

const nativeRule = {
  name: "omp-native", path: "/rules/omp-native.md", content: "Native.", description: "Native",
  condition: ["\\bnative-trigger\\b"], scope: ["text"],
  _source: { provider: "omp", providerName: "OMP", path: "/rules/omp-native.md", level: "user" },
};

function ompSession(sessionId: () => string, manager: any = new TtsrManager({ enabled: true, repeatMode: "once" })): any {
  const usage = { tokens: 7 };
  class AgentSession {
    sessionManager = { getSessionId: sessionId };
    ttsrManager = manager;
    agent = { state: { tools: [{ name: "write", matcherDigest: (args: any) => args.content }] } };
    getContextUsage() { return usage; }
  }
  expect(installLessonTtsrBridge({ pi: { AgentSession } })).toBeNull();
  return new AgentSession();
}

const fires = (ctx: any, text: string): string[] =>
  ctx.ttsrManager.checkSnapshot(text, { source: "text", streamKey: randomUUID() }).map((rule: any) => rule.name);

const alpha = remindRule(901, "\\balpha-trigger\\b");
const beta = remindRule(902, "\\bbeta-trigger\\b");

// Kills: a bridge-side "known" set that never re-adds a rule OMP's replaceRules dropped.
test("a rule OMP's rebuild dropped is restored on the next sync and fires again", () => {
  const sessionId = randomUUID();
  const ctx = ompSession(() => sessionId);
  ctx.ttsrManager.addRule(nativeRule);
  expect(syncLessonTtsr({ ctx, plan: planOf(alpha) })).toMatchObject({ active: 1, added: 1, warnings: [] });
  expect(fires(ctx, "alpha-trigger")).toEqual([alpha.rule.name]);

  // OMP rebuilds its rule buckets: only its own rules survive.
  const manager = ctx.ttsrManager;
  manager.replaceRules(manager.getRules().filter((rule: any) => rule._source?.provider !== "athanor-lessons"));
  expect(fires(ctx, "alpha-trigger")).toEqual([]);

  expect(syncLessonTtsr({ ctx, plan: planOf(alpha) })).toMatchObject({ active: 1, added: 1, warnings: [] });
  expect(fires(ctx, "alpha-trigger")).toEqual([alpha.rule.name]);
  expect(fires(ctx, "native-trigger")).toEqual([nativeRule.name]);
  // A sync with nothing dropped adds nothing and reports the same truth.
  expect(syncLessonTtsr({ ctx, plan: planOf(alpha) })).toMatchObject({ active: 1, added: 0, warnings: [] });
});

// Kills: re-patching a manager per session id, where the stale wrapper's filter
// hides a rule the new id activated, and old ids lingering in the lookup.
test("one manager across session ids: patched once, the new id's rules fire, the old id is pruned, switching back works", async () => {
  const first = randomUUID();
  const second = randomUUID();
  let current = first;
  const ctx = ompSession(() => current);
  expect(syncLessonTtsr({ ctx, plan: planOf(alpha) })).toMatchObject({ active: 1, warnings: [] });
  const wrapped = ctx.ttsrManager.checkSnapshot;
  expect(fires(ctx, "alpha-trigger")).toEqual([alpha.rule.name]);

  current = second;
  expect(syncLessonTtsr({ ctx, plan: planOf(beta) })).toMatchObject({ active: 1, added: 1, warnings: [] });
  expect(ctx.ttsrManager.checkSnapshot).toBe(wrapped);
  expect(fires(ctx, "beta-trigger")).toEqual([beta.rule.name]);
  expect(fires(ctx, "alpha-trigger")).toEqual([]);
  expect(capturedAgentSession(first)).toBeNull();
  // The old id's last match left with it: no ledger row can be written under it.
  const record = spyOn(organ, "requestOrgan").mockResolvedValue({ ok: true, eventId: 1, fires: 1 });
  try {
    expect(await recordNativeFires({ room: "kodo", spirit: "Kodo", session: first }, [alpha.rule])).toBe(0);
    expect(record.mock.calls).toEqual([]);
  } finally {
    record.mockRestore();
  }

  current = first;
  expect(syncLessonTtsr({ ctx, plan: planOf(alpha) })).toMatchObject({ active: 1, added: 0, warnings: [] });
  expect(ctx.ttsrManager.checkSnapshot).toBe(wrapped);
  expect(fires(ctx, "alpha-trigger")).toEqual([alpha.rule.name]);
  expect(fires(ctx, "beta-trigger")).toEqual([]);
  expect(capturedAgentSession(second)).toBeNull();
  expect(capturedAgentSession(first)).toBe(ctx);
});

test("independent managers keep their own rules through another manager's sync and rebuild", () => {
  const leftId = randomUUID();
  const rightId = randomUUID();
  const left = ompSession(() => leftId);
  const right = ompSession(() => rightId);
  expect(syncLessonTtsr({ ctx: left, plan: planOf(alpha) }).active).toBe(1);
  expect(syncLessonTtsr({ ctx: right, plan: planOf(beta) }).active).toBe(1);
  expect(fires(left, "alpha-trigger beta-trigger")).toEqual([alpha.rule.name]);
  expect(fires(right, "alpha-trigger beta-trigger")).toEqual([beta.rule.name]);

  right.ttsrManager.replaceRules([]);
  expect(syncLessonTtsr({ ctx: left, plan: planOf(alpha) })).toMatchObject({ active: 1, added: 0 });
  expect(fires(left, "alpha-trigger")).toEqual([alpha.rule.name]);
  expect(fires(right, "beta-trigger")).toEqual([]);
  expect(syncLessonTtsr({ ctx: right, plan: planOf(beta) })).toMatchObject({ active: 1, added: 1 });
  expect(fires(right, "beta-trigger")).toEqual([beta.rule.name]);
  expect(capturedAgentSession(leftId)).toBe(left);
  expect(capturedAgentSession(rightId)).toBe(right);
});

test("native rules keep firing, retired Athanor rules go quiet, and the AST check stays async", async () => {
  const sessionId = randomUUID();
  const ctx = ompSession(() => sessionId);
  ctx.ttsrManager.addRule(nativeRule);
  const swallowed = "try {\n  run();\n} catch (error) { }\n";
  const astCheck = (streamKey: string) =>
    ctx.ttsrManager.checkAstSnapshot(swallowed, { source: "tool", toolName: "write", filePaths: ["a.ts"], streamKey });

  expect(syncLessonTtsr({ ctx, plan: planOf(plan.rules[0]!, alpha) })).toMatchObject({ active: 2, warnings: [] });
  const pending = astCheck("first");
  expect(pending).toBeInstanceOf(Promise);
  expect((await pending).map((rule: any) => rule.name)).toEqual([plan.rules[0]!.rule.name]);

  expect(syncLessonTtsr({ ctx, plan: planOf() })).toMatchObject({ active: 0, warnings: [] });
  expect((await astCheck("second")).map((rule: any) => rule.name)).toEqual([]);
  expect(fires(ctx, "alpha-trigger native-trigger")).toEqual([nativeRule.name]);
});

// Kills: getContextUsage throwing when the manager lacks a matcher method, and the
// methods patched before the missing one staying wrapped.
test("a manager missing a bridge API leaves context usage intact, unwrapped, and reported", () => {
  const sessionId = randomUUID();
  const manager = new TtsrManager({ enabled: true, repeatMode: "once" });
  Object.defineProperty(manager, "checkAstSnapshot", { value: undefined });
  const ctx = ompSession(() => sessionId, manager);
  const usage = ctx.getContextUsage();
  expect(usage).toEqual({ tokens: 7 });
  expect(Object.hasOwn(manager, "checkDelta")).toBe(false);
  expect(Object.hasOwn(manager, "checkSnapshot")).toBe(false);
  expect(contextLessonManagerAvailable(ctx)).toBe(false);
  const synced = syncLessonTtsr({ ctx, plan: planOf(alpha) });
  expect(synced.active).toBe(0);
  expect(synced.warnings).toHaveLength(1);
  expect(manager.getRules()).toEqual([]);

  // A capable manager for the same session clears the report and attaches.
  ctx.ttsrManager = new TtsrManager({ enabled: true, repeatMode: "once" });
  expect(contextLessonManagerAvailable(ctx)).toBe(true);
  expect(syncLessonTtsr({ ctx, plan: planOf(alpha) })).toMatchObject({ active: 1, warnings: [] });
});

test("an unwrappable matcher leaves earlier methods unchanged and context usage available", () => {
  const sessionId = randomUUID();
  const manager = new TtsrManager({ enabled: true, repeatMode: "once" });
  Object.defineProperty(manager, "checkSnapshot", { value: manager.checkSnapshot, configurable: false });
  const ctx = ompSession(() => sessionId, manager);
  expect(ctx.getContextUsage()).toEqual({ tokens: 7 });
  expect(Object.hasOwn(manager, "checkDelta")).toBe(false);
  expect(Object.hasOwn(manager, "checkAstSnapshot")).toBe(false);
  const synced = syncLessonTtsr({ ctx, plan: planOf(alpha) });
  expect(synced.active).toBe(0);
  expect(synced.warnings).toHaveLength(1);
  expect(manager.getRules()).toEqual([]);
});

// Kills: counting every planned rule as active whether or not OMP accepted it.
test("a rule OMP rejects is warned about and never counted active", () => {
  const sessionId = randomUUID();
  const ctx = ompSession(() => sessionId);
  const shadowed = remindRule(903, "\\bgamma-trigger\\b");
  ctx.ttsrManager.addRule({ ...nativeRule, name: shadowed.rule.name });
  const empty = remindRule(904, "");
  empty.rule.condition = [];

  const synced = syncLessonTtsr({ ctx, plan: planOf(alpha, shadowed, empty) });
  expect(synced.active).toBe(1);
  expect(synced.added).toBe(1);
  expect(synced.warnings).toHaveLength(2);
  // The next sync still refuses to count the native rule that holds the name.
  expect(syncLessonTtsr({ ctx, plan: planOf(alpha, shadowed, empty) }).active).toBe(1);
});

test("a shut-down session leaves the lookup and a later capture reuses the same patch", async () => {
  const sessionId = randomUUID();
  const ctx = ompSession(() => sessionId);
  expect(syncLessonTtsr({ ctx, plan: planOf(alpha) }).active).toBe(1);
  const wrapped = ctx.ttsrManager.checkSnapshot;
  expect(fires(ctx, "alpha-trigger")).toEqual([alpha.rule.name]);

  releaseLessonTtsrSession(sessionId);
  expect(capturedAgentSession(sessionId)).toBeNull();
  const record = spyOn(organ, "requestOrgan").mockResolvedValue({ ok: true, eventId: 1, fires: 1 });
  try {
    expect(await recordNativeFires({ room: "kodo", spirit: "Kodo", session: sessionId }, [alpha.rule])).toBe(0);
    expect(record.mock.calls).toEqual([]);
  } finally {
    record.mockRestore();
  }

  expect(syncLessonTtsr({ ctx, plan: planOf(alpha) })).toMatchObject({ active: 1, added: 0 });
  expect(ctx.ttsrManager.checkSnapshot).toBe(wrapped);
  expect(fires(ctx, "alpha-trigger")).toEqual([alpha.rule.name]);
});

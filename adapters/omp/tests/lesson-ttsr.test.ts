import { afterEach, beforeEach, expect, spyOn, test } from "bun:test";
import { randomUUID } from "node:crypto";

import * as lessonContext from "../house-proof/lesson-context.ts";
import { installLessonTtsrBridge, selectPresenceLessons, syncLessonTtsr } from "../house-proof/lesson-ttsr.ts";
import { lessonMaterials } from "../house-proof/presence-materials.ts";
import * as host from "../house-proof/host.ts";
import { compilePresenceContext, type PresenceCompileInput } from "../house-proof/presence.ts";
import { registerTopLevelSession, retireTopLevelSession } from "../house-proof/top-level-session-fence.ts";

let query: ReturnType<typeof spyOn<typeof lessonContext, "runLessonQuery">>;
beforeEach(() => { query = spyOn(lessonContext, "runLessonQuery"); });
afterEach(() => query.mockRestore());

function lessons(rows: Array<Record<string, unknown>>) {
  query.mockImplementation(async (_dir, _room, filters) => ({
    ok: true,
    lessons: rows.filter((row) => row.type === filters.type),
  }));
}

function coding(id: number, extra: Record<string, unknown> = {}) {
  return { id, type: "coding", title: `Lesson ${id}`, lesson: `Craft body ${id}`, ...extra };
}

function session() {
  const rules: Array<Record<string, any>> = [];
  class AgentSession {
    sessionManager = { getSessionId: () => id };
    ttsrManager = {
      addRule: (rule: Record<string, any>) => { rules.push(rule); return true; },
      checkDelta: () => rules,
      checkSnapshot: () => rules,
      checkAstSnapshot: () => rules,
    };
    getContextUsage() { return {}; }
  }
  const id = randomUUID();
  installLessonTtsrBridge({ pi: { AgentSession } });
  return { ctx: new AgentSession(), rules };
}

function sync(ctx: unknown, activeProject: string | null = null) {
  return syncLessonTtsr({ ctx, roomDir: "test-room", room: "kodo", activeProject });
}

function presence(result: Awaited<ReturnType<typeof sync>>, mode: Parameters<typeof selectPresenceLessons>[1]) {
  return lessonMaterials(selectPresenceLessons(result, mode));
}

test("unarmed always-on coding reaches work Presence, not conversation or quiet", async () => {
  lessons([coding(200, { alwaysOn: true }), coding(224), {
    id: 400, type: "writing", title: "Writing", lesson: "Not coding craft", alwaysOn: true,
  }]);
  const { ctx, rules } = session();
  const result = await sync(ctx);
  expect(presence(result, "work").map((material) => material.body)).toEqual(["Craft body 200"]);
  expect(presence(result, "conversation")).toEqual([]);
  expect(presence(result, "quiet")).toEqual([]);
  expect(presence(result, "mixed")).toEqual([]);
  expect(presence(result, undefined)).toEqual([]);
  expect(rules).toEqual([]);
});

test("unapproved triggers never arm while approved guards retain scope and project boundaries", async () => {
  lessons([
    coding(1, { condition: ["forbidden"], triggerScope: ["text"] }),
    coding(2, { tags: ["ttsr-approved"], condition: ["approved"], languageKeys: ["rust"], triggerScope: ["text", "tool:edit"] }),
    { id: 3, type: "project", title: "Project", lesson: "Project rule", project: "other", tags: ["ttsr-approved"], condition: ["project"], triggerScope: ["text", "tool:write"] },
  ]);
  const { ctx, rules } = session();
  const result = await sync(ctx, "active");
  expect(rules.map((rule) => rule.condition)).toEqual([["approved"], ["project"]]);
  expect(rules[0].scope).toEqual(["tool:edit"]);
  expect(rules[0].globs).toEqual(["**/*.rs"]);
  expect(rules[1].scope).toEqual(["tool:write"]);
  expect(rules[1].globs).toEqual(["**/other/**"]);
  expect(presence(result, "conversation").map((material) => material.body)).toEqual(["Craft body 2", "Project rule"]);
});

test("coding baseline survives an unavailable native manager without arming conditional lessons", async () => {
  lessons([coding(200, { alwaysOn: true }), coding(224, { tags: ["ttsr-approved"], condition: ["rename"] })]);
  const result = await sync({ sessionID: randomUUID() });
  expect(presence(result, "work").map((material) => material.body)).toEqual(["Craft body 200"]);
  expect(presence(result, "conversation")).toEqual([]);
  expect(result.active).toBe(0);
  expect(result.warnings).toContain("native OMP TTSR manager unavailable");
});

test("overlapping baseline and armed lesson speaks once in Presence", async () => {
  lessons([coding(9, { alwaysOn: true, tags: ["ttsr-approved"], condition: ["plain line"] })]);
  const { ctx } = session();
  const result = await sync(ctx);
  expect(result.active).toBe(1);
  expect(presence(result, "work").map((material) => material.body)).toEqual(["Craft body 9"]);
  expect(presence(result, "conversation").map((material) => material.body)).toEqual(["Craft body 9"]);
});

test("work craft beyond the eighth lesson receives an enact directive within the wire ceiling", async () => {
  const baseline = Array.from({ length: 10 }, (_, index) => coding(200 + index, {
    alwaysOn: true,
    lesson: `Foundation ${index}: ${"plain line ".repeat(120)}`,
  }));
  const armed = Array.from({ length: 25 }, (_, index) => coding(1 + index, {
    tags: ["ttsr-approved"], condition: [`guard ${index}`],
  }));
  lessons([...armed, ...baseline]);
  const { ctx } = session();
  const result = await sync(ctx);
  const room = `craft-${randomUUID()}`;
  const binding = { room, spirit: "Kodo", session: ctx.sessionManager.getSessionId() };
  const previousHouseId = process.env.ATHANOR_HOST_HOUSE_ID;
  process.env.ATHANOR_HOST_HOUSE_ID = "test-house";
  const requests: PresenceCompileInput[] = [];
  const transport = spyOn(host, "sendHostCommand").mockImplementation(async (command) => {
    requests.push(command.presence_compile as PresenceCompileInput);
    return { result: { operation: "compile", value: { contractId: "craft-contract" } } } as host.HostResponse;
  });
  registerTopLevelSession(room, binding.session);
  try {
    for (const mode of ["work", "conversation"] as const) {
      await compilePresenceContext({
        binding, operator: "Sol", prompt: "Meet this turn", turnId: mode,
        priorFrameId: "craft-frame", lessons: presence(result, mode),
      });
    }

    const work = requests[0].directives!;
    const enacted = work.filter((directive) => directive.sourceIds[0].startsWith("lesson:"));
    expect(work).toHaveLength(32);
    expect(enacted.slice(0, 10).map((directive) => directive.sourceIds[0]))
      .toEqual(baseline.map((row) => `lesson:${row.id}`));
    expect(enacted[9].kind).toBe("enact");
    expect(enacted[9].instruction).toBe(baseline[9].lesson.slice(0, 1000));
    expect(enacted.some((directive) => directive.sourceIds[0] === "lesson:21")).toBe(false);
    expect(requests[1].directives!.filter((directive) => directive.sourceIds[0].startsWith("lesson:"))
      .map((directive) => directive.sourceIds[0])).toEqual(armed.map((row) => `lesson:${row.id}`));
  } finally {
    if (previousHouseId === undefined) delete process.env.ATHANOR_HOST_HOUSE_ID;
    else process.env.ATHANOR_HOST_HOUSE_ID = previousHouseId;
    transport.mockRestore();
    retireTopLevelSession(room, binding.session);
  }
});

import { describe, expect, test } from "bun:test";
import {
  clearLessonWorkingSets,
  deriveLessonWorkingState,
  stateForLessonPrompt,
  updateLessonWorkingSet,
} from "../solarisael-house-proof/lesson-working-state.ts";
import { rankEligibleLessons } from "../solarisael-house-proof/lesson-context.ts";

const project = { root: "C:/work/app", project: "app", source: "marker" as const, candidates: ["C:/work/app/src/a.ts"] };
const lessons = [1, 2, 3, 4, 5, 6, 7].map((id) => ({ id, semantic: { similarity: 1 - id / 100 } }));

describe("Striatum lesson working state", () => {
  test("keeps up to six bounded lessons across tiny wording changes", () => {
    clearLessonWorkingSets();
    const initial = deriveLessonWorkingState({ room: "kintsu", project, toolName: "edit", target: "C:/work/app/src/a.ts" });
    const selected = updateLessonWorkingSet(initial, lessons);
    const wordingOnly = stateForLessonPrompt(initial, "Please carefully adjust the wording in this file.");
    const stable = updateLessonWorkingSet(wordingOnly, lessons.slice().reverse());
    expect(selected.lessons).toHaveLength(6);
    expect(stable.refreshed).toBeFalse();
    expect(stable.lessons.map((lesson) => lesson.id)).toEqual(selected.lessons.map((lesson) => lesson.id));
  });

  test("an explicit phase replaces stale stages while project identity stays exact", () => {
    const initial = deriveLessonWorkingState({
      room: "kintsu", project, toolName: "edit", target: "C:/work/app/src/a.ts", stages: ["implementation"],
    });
    const phase = stateForLessonPrompt(initial, "Move into phase release now.");
    expect(phase.signature).not.toBe(initial.signature);
    expect(phase.project.project).toBe("app");
    expect(phase.stages).toContain("release");
    expect(phase.stages).not.toContain("implementation");
    expect(updateLessonWorkingSet(phase, lessons).refreshed).toBeTrue();
  });

  test("retains the current stage when a prompt declares no transition", () => {
    const initial = deriveLessonWorkingState({
      room: "kintsu", project, toolName: "edit", target: "C:/work/app/src/a.ts", stages: ["implementation"],
    });
    const retained = stateForLessonPrompt(initial, "Keep adjusting the current module.");
    expect(retained.stages).toEqual(["implementation"]);
  });
  test("refreshes when the project stays fixed but the work topic changes abruptly", () => {
    const initial = deriveLessonWorkingState({
      room: "kintsu",
      project,
      prompt: "repair postgres migration rollback",
      toolName: "edit",
      target: "C:/work/app/src/a.ts",
    });
    const changed = stateForLessonPrompt(initial, "redesign discord session ownership isolation");
    expect(changed.signature).not.toBe(initial.signature);
    expect(changed.project.project).toBe("app");
    expect(updateLessonWorkingSet(changed, lessons).refreshed).toBeTrue();
  });
  test("does not activate structurally eligible but semantically distant lessons", async () => {
    const ranked = await rankEligibleLessons(
      "unrelated work",
      [{ id: 9, project: "app" }],
      async () => [0.09],
    );
    expect(ranked).toEqual([]);
  });
  test("fails open when Nemotron is unavailable", async () => {
    await expect(rankEligibleLessons("deploy", [{ id: 9, project: "app" }], async () => null)).resolves.toBeNull();
  });

});

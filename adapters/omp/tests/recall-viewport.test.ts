import { describe, expect, test } from "bun:test";
import { automaticRecallViewport, createRecallViewportSession } from "../solarisael-house-proof/recall-viewport.ts";

const candidate = (overrides: Record<string, unknown> = {}) => ({
  source_path: "memory/example.md",
  title: "Example memory",
  matched_terms: ["alpha", "beta"],
  sources: ["memory"],
  ...overrides,
});

const recall = (retrievalCandidates: unknown[], extra: Record<string, unknown> = {}) => ({
  query: "alpha beta",
  retrievalCandidates,
  ...extra,
});

describe("automatic recall viewport", () => {
  test("suppresses zero-term and glue-only candidates", () => {
    const result = automaticRecallViewport(recall([
      candidate({ matched_terms: [] }),
      candidate({ source_path: "memory/glue.md", matched_terms: ["please", "the"] }),
    ]));
    expect(result.keptCandidates).toHaveLength(0);
    expect(result.suppressions.map((s) => s.reason)).toEqual(["zero-terms", "glue-only"]);
  });

  test("does not treat generic operational terms as exact title evidence", () => {
    const result = automaticRecallViewport({
      query: "did the verification work?",
      retrievalCandidates: [
        candidate({
          source_path: "memory/release-check.md",
          title: "Did the release verification work end-to-end?",
          matched_terms: ["work"],
        }),
        candidate({
          source_path: "memory/service-check.md",
          title: "Background service work completed",
          matched_terms: ["works"],
        }),
      ],
    });

    expect(result.keptCandidates).toHaveLength(0);
    expect(result.suppressions.map((suppression) => suppression.reason)).toEqual(["glue-only", "glue-only"]);
  });

  test("keeps exact entity, project, and date signals", () => {
    const result = automaticRecallViewport(recall([
      candidate({ source_path: "memory/entity.md", matched_terms: ["orchid"], sources: ["entity"] }),
      candidate({ source_path: "memory/project.md", matched_terms: ["northstar"], sources: ["project_lesson"] }),
      candidate({ source_path: "memory/date.md", matched_terms: ["2030-01-02"], sources: ["date"] }),
    ]));
    expect(result.keptCandidates).toHaveLength(3);
  });

  test("keeps candidates with two independent meaningful terms", () => {
    const result = automaticRecallViewport(recall([candidate({ matched_terms: ["alpha", "beta"] })]));
    expect(result.keptCandidates).toHaveLength(1);
  });

  test("saturates repeated identities but permits another relevant result", () => {
    const session = createRecallViewportSession();
    const repeated = candidate({ id: "same" });
    const other = candidate({ id: "other", source_path: "memory/other.md" });
    expect(automaticRecallViewport(recall([repeated]), { session, saturationLimit: 2 }).keptCandidates).toHaveLength(1);
    expect(automaticRecallViewport(recall([repeated]), { session, saturationLimit: 2 }).keptCandidates).toHaveLength(1);
    const third = automaticRecallViewport(recall([repeated, other]), { session, saturationLimit: 2 });
    expect(third.keptCandidates).toEqual([other]);
    expect(third.suppressions[0].reason).toBe("saturated");
  });

  test("returns a successful empty result", () => {
    const result = automaticRecallViewport(recall([]));
    expect(result).toEqual({ candidates: [], keptCandidates: [], suppressions: [], diagnostics: { kept: 0, suppressed: 0, reasons: {} } });
  });
});

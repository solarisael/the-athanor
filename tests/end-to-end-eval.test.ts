import { describe, expect, test } from "bun:test";
import { scoreTrial, summarizeTrials, type JudgeResult } from "../evals/end-to-end-metrics.ts";

const completeJudge = (overrides: Partial<JudgeResult> = {}): JudgeResult => ({
  criteria: [
    { id: "current", status: "met", evidence_available: true },
    { id: "history", status: "met", evidence_available: true },
  ],
  risks: [{ id: "stale", violated: false }],
  unsupported_claims: [],
  temporal_correct: true,
  uncertainty_correct: true,
  citation_support: [{ citation_id: "E1", supported: true }],
  ...overrides,
});

const score = (judge: JudgeResult, citations = ["E1"]) => scoreTrial({
  judge,
  expectedCriterionIds: ["current", "history"],
  expectedRiskIds: ["stale"],
  answerCitations: citations,
  availableEvidenceIds: ["E1"],
  requireCitation: true,
});

describe("end-to-end memory scoring", () => {
  test("passes a complete grounded answer", () => {
    expect(score(completeJudge())).toMatchObject({
      passed: true,
      criterion_coverage: 1,
      evidence_coverage: 1,
      failure_class: "pass",
    });
  });

  test("attributes a miss to retrieval when required evidence was absent", () => {
    const judge = completeJudge({
      criteria: [
        { id: "current", status: "met", evidence_available: true },
        { id: "history", status: "miss", evidence_available: false },
      ],
    });
    expect(score(judge)).toMatchObject({ passed: false, evidence_coverage: 0.5, failure_class: "retrieval" });
  });

  test("attributes a miss to model selection when evidence was available", () => {
    const judge = completeJudge({
      criteria: [
        { id: "current", status: "met", evidence_available: true },
        { id: "history", status: "miss", evidence_available: true },
      ],
    });
    expect(score(judge)).toMatchObject({ passed: false, evidence_coverage: 1, failure_class: "selection-or-synthesis" });
  });

  test("rejects stale claims and invalid provenance", () => {
    const stale = completeJudge({ risks: [{ id: "stale", violated: true }] });
    expect(score(stale).passed).toBe(false);
    const invalid = score(completeJudge(), ["E9"]);
    expect(invalid.invalid_citations).toEqual(["E9"]);
    expect(invalid.passed).toBe(false);
  });

  test("computes first-response memory lift by scenario", () => {
    const passed = score(completeJudge());
    const failed = score(completeJudge({
      criteria: [
        { id: "current", status: "miss", evidence_available: false },
        { id: "history", status: "miss", evidence_available: false },
      ],
    }));
    const summary = summarizeTrials([
      { scenario_id: "one", variant: "full", score: passed },
      { scenario_id: "one", variant: "full", score: passed },
      { scenario_id: "one", variant: "no-recall", score: passed },
      { scenario_id: "one", variant: "no-recall", score: failed },
    ]);
    expect(summary.memory_lift).toEqual([{
      scenario_id: "one",
      full_success_rate: 1,
      no_recall_success_rate: 0.5,
      memory_lift: 0.5,
    }]);
  });
});

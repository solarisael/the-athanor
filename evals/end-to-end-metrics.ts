export type CriterionJudgment = {
  id: string;
  status: "met" | "partial" | "miss";
  evidence_available: boolean;
  explanation?: string;
};

export type RiskJudgment = {
  id: string;
  violated: boolean;
  explanation?: string;
};

export type CitationJudgment = {
  citation_id: string;
  supported: boolean;
  explanation?: string;
};

export type JudgeResult = {
  criteria: CriterionJudgment[];
  risks: RiskJudgment[];
  unsupported_claims: string[];
  temporal_correct: boolean;
  uncertainty_correct: boolean;
  citation_support: CitationJudgment[];
  overall_pass?: boolean;
  summary?: string;
};

export type TrialScore = {
  passed: boolean;
  criterion_total: number;
  criterion_met: number;
  criterion_coverage: number;
  evidence_available: number;
  evidence_coverage: number;
  risk_violations: number;
  unsupported_claims: number;
  invalid_citations: string[];
  unsupported_citations: string[];
  failure_class: "pass" | "retrieval" | "selection-or-synthesis" | "invalid-output";
};

const percent = (part: number, whole: number): number => whole ? part / whole : 1;

export function scoreTrial({
  judge,
  expectedCriterionIds,
  expectedRiskIds,
  answerCitations,
  availableEvidenceIds,
  requireCitation,
}: {
  judge: JudgeResult;
  expectedCriterionIds: string[];
  expectedRiskIds: string[];
  answerCitations: string[];
  availableEvidenceIds: string[];
  requireCitation: boolean;
}): TrialScore {
  const criterionMap = new Map((judge.criteria || []).map((item) => [item.id, item]));
  const riskMap = new Map((judge.risks || []).map((item) => [item.id, item]));
  const citationMap = new Map((judge.citation_support || []).map((item) => [item.citation_id, item]));
  const evidenceIds = new Set(availableEvidenceIds);
  const criterionTotal = expectedCriterionIds.length;
  const criterionMet = expectedCriterionIds.filter((id) => criterionMap.get(id)?.status === "met").length;
  const evidenceAvailable = expectedCriterionIds.filter((id) => criterionMap.get(id)?.evidence_available === true).length;
  const riskViolations = expectedRiskIds.filter((id) => riskMap.get(id)?.violated !== false).length;
  const unsupportedClaims = Array.isArray(judge.unsupported_claims) ? judge.unsupported_claims.length : 1;
  const invalidCitations = answerCitations.filter((id) => !evidenceIds.has(id));
  const unsupportedCitations = answerCitations.filter((id) => citationMap.get(id)?.supported !== true);
  const citationFailure = invalidCitations.length > 0
    || unsupportedCitations.length > 0
    || (requireCitation && answerCitations.length === 0);
  const completeJudgment = criterionMap.size >= criterionTotal && riskMap.size >= expectedRiskIds.length;
  const passed = completeJudgment
    && criterionMet === criterionTotal
    && riskViolations === 0
    && unsupportedClaims === 0
    && judge.temporal_correct === true
    && judge.uncertainty_correct === true
    && !citationFailure;
  const evidenceCoverage = percent(evidenceAvailable, criterionTotal);
  return {
    passed,
    criterion_total: criterionTotal,
    criterion_met: criterionMet,
    criterion_coverage: percent(criterionMet, criterionTotal),
    evidence_available: evidenceAvailable,
    evidence_coverage: evidenceCoverage,
    risk_violations: riskViolations,
    unsupported_claims: unsupportedClaims,
    invalid_citations: invalidCitations,
    unsupported_citations: unsupportedCitations,
    failure_class: passed
      ? "pass"
      : !completeJudgment
        ? "invalid-output"
        : evidenceCoverage < 1
          ? "retrieval"
          : "selection-or-synthesis",
  };
}

export function summarizeTrials<T extends { scenario_id: string; variant: string; score: TrialScore }>(trials: T[]) {
  const groups = new Map<string, T[]>();
  for (const trial of trials) {
    const key = `${trial.scenario_id}\u0000${trial.variant}`;
    const group = groups.get(key) || [];
    group.push(trial);
    groups.set(key, group);
  }
  const summaries = [...groups.values()].map((items) => {
    const passes = items.filter((item) => item.score.passed).length;
    const mean = (selector: (item: T) => number) => items.reduce((sum, item) => sum + selector(item), 0) / items.length;
    const failures: Record<string, number> = {};
    for (const item of items) failures[item.score.failure_class] = (failures[item.score.failure_class] || 0) + 1;
    return {
      scenario_id: items[0].scenario_id,
      variant: items[0].variant,
      trials: items.length,
      passes,
      success_rate: passes / items.length,
      mean_criterion_coverage: mean((item) => item.score.criterion_coverage),
      mean_evidence_coverage: mean((item) => item.score.evidence_coverage),
      mean_risk_violations: mean((item) => item.score.risk_violations),
      mean_unsupported_claims: mean((item) => item.score.unsupported_claims),
      failure_classes: failures,
    };
  }).sort((left, right) => left.scenario_id.localeCompare(right.scenario_id) || left.variant.localeCompare(right.variant));

  const scenarioIds = [...new Set(trials.map((trial) => trial.scenario_id))];
  const memoryLift = scenarioIds.map((scenarioId) => {
    const full = summaries.find((item) => item.scenario_id === scenarioId && item.variant === "full");
    const noRecall = summaries.find((item) => item.scenario_id === scenarioId && item.variant === "no-recall");
    return {
      scenario_id: scenarioId,
      full_success_rate: full?.success_rate ?? null,
      no_recall_success_rate: noRecall?.success_rate ?? null,
      memory_lift: full && noRecall ? full.success_rate - noRecall.success_rate : null,
    };
  });
  return { summaries, memory_lift: memoryLift };
}

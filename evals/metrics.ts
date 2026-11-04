export type QueryMeasurement = {
  recall_at_1: boolean;
  recall_at_5: boolean;
  recall_at_10: boolean;
  mrr: number;
  status: "ok" | "degraded";
  latency_ms: number;
};

export type ExpectedScore = {
  satisfaction_rank: number | null;
  first_expected_rank: number | null;
  hit_memory_ids: number[];
};
export function directMemoryId(candidate: {
  id?: unknown;
  source_table?: unknown;
  source_id?: unknown;
}): number | null {
  const idMatch = /^memories:(\d+)$/.exec(String(candidate.id || ""));
  if (idMatch) {
    const numericId = Number(idMatch[1]);
    if (Number.isInteger(numericId) && numericId > 0) return numericId;
  }
  if (String(candidate.source_table || "").toLowerCase() === "memories") {
    const numericId = Number(candidate.source_id);
    if (Number.isInteger(numericId) && numericId > 0) return numericId;
  }
  return null;
}


export function scoreExpected(
  rankedMemoryIds: Array<number | null>,
  expectedMemoryIds: number[],
  expectedAny: boolean,
): ExpectedScore {
  const expected = [...new Set(expectedMemoryIds)];
  const firstRanks = new Map<number, number>();
  rankedMemoryIds.forEach((memoryId, index) => {
    if (memoryId !== null && expected.includes(memoryId) && !firstRanks.has(memoryId)) {
      firstRanks.set(memoryId, index + 1);
    }
  });

  const hitMemoryIds = expected.filter((memoryId) => firstRanks.has(memoryId));
  const hitRanks = hitMemoryIds.map((memoryId) => firstRanks.get(memoryId) as number);
  const firstExpectedRank = hitRanks.length ? Math.min(...hitRanks) : null;
  const satisfactionRank = expectedAny
    ? firstExpectedRank
    : expected.length > 0 && hitMemoryIds.length === expected.length
      ? Math.max(...hitRanks)
      : null;

  return {
    satisfaction_rank: satisfactionRank,
    first_expected_rank: firstExpectedRank,
    hit_memory_ids: hitMemoryIds,
  };
}

export function wilsonInterval(successes: number, total: number, z = 1.959963984540054): { lower: number; upper: number } {
  if (!total) return { lower: 0, upper: 0 };
  const proportion = successes / total;
  const zSquared = z * z;
  const denominator = 1 + zSquared / total;
  const center = (proportion + zSquared / (2 * total)) / denominator;
  const margin = (z * Math.sqrt((proportion * (1 - proportion) + zSquared / (4 * total)) / total)) / denominator;
  return {
    lower: Math.max(0, center - margin),
    upper: Math.min(1, center + margin),
  };
}

export function nearestRankPercentile(values: number[], percentile: number): number {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const rank = Math.max(1, Math.ceil(percentile * sorted.length));
  return sorted[Math.min(rank - 1, sorted.length - 1)];
}

function mean(values: number[]): number {
  return values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : 0;
}

export function summarizeMetrics(measurements: QueryMeasurement[]) {
  const evaluated = measurements.length;
  const recallAt1 = measurements.filter((item) => item.recall_at_1).length;
  const recallAt5 = measurements.filter((item) => item.recall_at_5).length;
  const recallAt10 = measurements.filter((item) => item.recall_at_10).length;
  const healthy = measurements.filter((item) => item.status === "ok").length;
  const latencies = measurements.map((item) => item.latency_ms);

  return {
    evaluated,
    recall_at_1: evaluated ? recallAt1 / evaluated : 0,
    recall_at_5: evaluated ? recallAt5 / evaluated : 0,
    recall_at_10: evaluated ? recallAt10 / evaluated : 0,
    mrr: evaluated ? mean(measurements.map((item) => item.mrr)) : 0,
    recall_hits: {
      at_1: recallAt1,
      at_5: recallAt5,
      at_10: recallAt10,
    },
    confidence_95: {
      recall_at_1: wilsonInterval(recallAt1, evaluated),
      recall_at_5: wilsonInterval(recallAt5, evaluated),
      recall_at_10: wilsonInterval(recallAt10, evaluated),
    },
    query_health: {
      healthy,
      degraded: evaluated - healthy,
    },
    latency_ms: {
      mean: mean(latencies),
      p50: nearestRankPercentile(latencies, 0.5),
      p95: nearestRankPercentile(latencies, 0.95),
      min: latencies.length ? Math.min(...latencies) : 0,
      max: latencies.length ? Math.max(...latencies) : 0,
      percentile_method: "nearest-rank",
    },
  };
}

export function sliceMetrics<T extends QueryMeasurement>(
  measurements: T[],
  key: (measurement: T) => string,
): Record<string, ReturnType<typeof summarizeMetrics>> {
  const groups = new Map<string, T[]>();
  for (const measurement of measurements) {
    const name = key(measurement);
    groups.set(name, [...(groups.get(name) || []), measurement]);
  }
  return Object.fromEntries(
    [...groups.entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([name, items]) => [name, summarizeMetrics(items)]),
  );
}

import { describe, expect, test } from "bun:test";
import {
  directMemoryId,
  nearestRankPercentile,
  scoreExpected,
  sliceMetrics,
  summarizeMetrics,
  wilsonInterval,
} from "../evals/metrics.ts";

describe("directMemoryId", () => {
  test("rejects zero placeholders so path mapping can recover the real ID", () => {
    expect(directMemoryId({ id: "memories:0", source_table: "memories", source_id: 0 })).toBeNull();
  });

  test("accepts positive memory IDs", () => {
    expect(directMemoryId({ id: "memories:42" })).toBe(42);
    expect(directMemoryId({ source_table: "memories", source_id: 43 })).toBe(43);
  });
});

describe("scoreExpected", () => {
  test("expected-any scores the first matching target", () => {
    expect(scoreExpected([9, 2, 7, 3], [2, 3], true)).toEqual({
      satisfaction_rank: 2,
      first_expected_rank: 2,
      hit_memory_ids: [2, 3],
    });
  });

  test("expected-all scores the rank where every target has appeared", () => {
    expect(scoreExpected([9, 2, 7, 3], [2, 3], false)).toEqual({
      satisfaction_rank: 4,
      first_expected_rank: 2,
      hit_memory_ids: [2, 3],
    });
  });

  test("expected-all remains unsatisfied when one target is absent", () => {
    expect(scoreExpected([9, 2, 7], [2, 3], false)).toEqual({
      satisfaction_rank: null,
      first_expected_rank: 2,
      hit_memory_ids: [2],
    });
  });
});

describe("statistical helpers", () => {
  test("Wilson interval contains the observed proportion", () => {
    const interval = wilsonInterval(28, 45);
    expect(interval.lower).toBeCloseTo(0.476, 2);
    expect(interval.upper).toBeCloseTo(0.749, 2);
    expect(interval.lower).toBeLessThan(28 / 45);
    expect(interval.upper).toBeGreaterThan(28 / 45);
  });

  test("nearest-rank percentile does not interpolate measurements", () => {
    expect(nearestRankPercentile([10, 30, 20, 40], 0.5)).toBe(20);
    expect(nearestRankPercentile([10, 30, 20, 40], 0.95)).toBe(40);
  });
});

describe("summarizeMetrics", () => {
  const measurements = [
    { room: "kodo", recall_at_1: true, recall_at_5: true, recall_at_10: true, mrr: 1, status: "ok" as const, latency_ms: 100 },
    { room: "kodo", recall_at_1: false, recall_at_5: true, recall_at_10: true, mrr: 0.25, status: "ok" as const, latency_ms: 200 },
    { room: "kintsu", recall_at_1: false, recall_at_5: false, recall_at_10: false, mrr: 0, status: "degraded" as const, latency_ms: 900 },
  ];

  test("reports recall, health, and wall-clock latency together", () => {
    const metrics = summarizeMetrics(measurements);
    expect(metrics.evaluated).toBe(3);
    expect(metrics.recall_at_1).toBeCloseTo(1 / 3);
    expect(metrics.recall_at_5).toBeCloseTo(2 / 3);
    expect(metrics.recall_at_10).toBeCloseTo(2 / 3);
    expect(metrics.mrr).toBeCloseTo(1.25 / 3);
    expect(metrics.query_health).toEqual({ healthy: 2, degraded: 1 });
    expect(metrics.latency_ms.p50).toBe(200);
    expect(metrics.latency_ms.p95).toBe(900);
  });

  test("slices retain the same metric contract", () => {
    const slices = sliceMetrics(measurements, (measurement) => measurement.room);
    expect(slices.kodo.evaluated).toBe(2);
    expect(slices.kodo.recall_at_5).toBe(1);
    expect(slices.kintsu.query_health.degraded).toBe(1);
  });
});

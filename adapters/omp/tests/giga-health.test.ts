import { describe, expect, test } from "bun:test";

import type { GigaHealthResult } from "../giga.ts";
import { normalizeGigaHealth } from "../house-proof/tools.ts";

function health(overrides: Partial<GigaHealthResult> = {}): GigaHealthResult {
  return {
    capture_enabled: true,
    classifier_enabled: true,
    store_healthy: true,
    classifier: { consecutive_failures: 0 },
    ...overrides,
  };
}

describe("giga_health", () => {
  test("is green only when capture, store, and classifier are all alive", () => {
    const result = normalizeGigaHealth(health());
    expect(result.isError).toBe(false);
    expect(result.details.dead).toEqual([]);
  });

  test("a disabled classifier is not green and is named", () => {
    const result = normalizeGigaHealth(health({ classifier_enabled: false }));
    expect(result.isError).toBe(true);
    expect(result.details.dead).toEqual(["classifier disabled"]);
  });

  test("a failing classifier is not green and carries its failure count", () => {
    const result = normalizeGigaHealth(health({ classifier: { consecutive_failures: 4 } }));
    expect(result.isError).toBe(true);
    expect(result.details.dead).toEqual(["classifier failing (4 consecutive)"]);
  });

  test("every dead part is listed, not only the first", () => {
    const result = normalizeGigaHealth(
      health({ capture_enabled: false, store_healthy: false, classifier_enabled: false }),
    );
    expect(result.details.dead).toEqual([
      "capture disabled",
      "store unhealthy",
      "classifier disabled",
    ]);
  });
});

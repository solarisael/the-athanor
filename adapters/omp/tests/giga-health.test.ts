import { describe, expect, test } from "bun:test";

import type { GigaHealthResult } from "../giga.ts";
import { normalizeGigaHealth } from "../solarisael-house-proof/tools.ts";

function healthResult(overrides: Partial<GigaHealthResult> = {}): GigaHealthResult {
  return {
    capture_enabled: true,
    classifier_enabled: false,
    store_healthy: true,
    queue_depth: 2,
    oldest_queue_age_seconds: 17,
    processed_count: 11,
    failed_count: 3,
    candidates_by_kind_state: [],
    classifier: {
      provider_type: "ollama",
      model: "model",
      model_digest: "digest",
      prompt_version: "prompt",
      endpoint_scope: "loopback",
      last_error_class: "classifier_unavailable",
      last_error_at: "2026-08-30T12:00:00Z",
      consecutive_failures: 4,
    },
    ...overrides,
  };
}

describe("GIGA health normalization", () => {
  test("reports healthy capture and store when classifier policy is off", () => {
    const result = normalizeGigaHealth(healthResult());

    expect(result.isError).toBe(false);
    expect(result.details).toMatchObject({
      ok: true,
      capture_enabled: true,
      classifier_enabled: false,
      store_healthy: true,
      classifier: {
        last_error_class: "classifier_unavailable",
        consecutive_failures: 4,
      },
    });
  });

  test("refuses health when capture is disabled", () => {
    const result = normalizeGigaHealth(healthResult({ capture_enabled: false }));

    expect(result.isError).toBe(true);
    expect(result.details).toMatchObject({
      ok: false,
      capture_enabled: false,
      classifier_enabled: false,
      store_healthy: true,
    });
  });
});

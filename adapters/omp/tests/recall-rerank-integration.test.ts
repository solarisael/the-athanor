import { expect, test } from "bun:test";

import {
  jevRecallDeadline,
  prepareRecallResultForViewport,
} from "../index.ts";

test("Jev deadline leaves the viewport and commit reserve untouched", () => {
  const now = 1_000;
  const automaticDeadline = 6_000;

  expect(jevRecallDeadline(automaticDeadline, now)).toBe(2_500);
  expect(automaticDeadline - jevRecallDeadline(automaticDeadline, now)!).toBeGreaterThan(500);
  expect(jevRecallDeadline(1_550, now)).toBeNull();
});

test("active zero-winner selection clears raw lanes and found state", () => {
  const prepared = prepareRecallResultForViewport(
    {
      ok: true,
      found: true,
      retrievalCandidates: [{ memory_id: 1 }],
      rerankCandidates: [{ memory_id: 2 }],
      semanticChunks: [{ body: "raw semantic" }],
      contentChunks: [{ body: "raw lexical" }],
      canonMatches: [],
      dateMatches: [],
    },
    {
      retrievalCandidates: [],
      receipt: { status: "active" },
    },
    true,
  );

  expect(prepared.retrievalCandidates).toEqual([]);
  expect(prepared.semanticChunks).toEqual([]);
  expect(prepared.contentChunks).toEqual([]);
  expect(prepared.found).toBe(false);
  expect(prepared).not.toHaveProperty("rerankCandidates");
});

test("fallback preserves baseline found and raw lanes while stripping the sidecar", () => {
  const semanticChunks = [{ body: "raw semantic" }];
  const contentChunks = [{ body: "raw lexical" }];
  const baseline = [{ memory_id: 1 }];
  const prepared = prepareRecallResultForViewport(
    {
      ok: true,
      found: true,
      retrievalCandidates: baseline,
      rerankCandidates: [{ memory_id: 2 }],
      semanticChunks,
      contentChunks,
      canonMatches: [],
      dateMatches: [],
    },
    {
      retrievalCandidates: baseline,
      receipt: { status: "failed" },
    },
    false,
  );

  expect(prepared.retrievalCandidates).toBe(baseline);
  expect(prepared.semanticChunks).toBe(semanticChunks);
  expect(prepared.contentChunks).toBe(contentChunks);
  expect(prepared.found).toBe(true);
  expect(prepared).not.toHaveProperty("rerankCandidates");
});

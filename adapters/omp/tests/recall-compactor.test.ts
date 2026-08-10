import { describe, expect, test } from "bun:test";

import { compactRecall } from "../solarisael-house-proof/recall.ts";

function repeatedText(length: number) {
  return "x".repeat(length);
}

describe("recall compactor", () => {
  test("surfaces fused retrieval candidates with bounded candidate fields", () => {
    const longExcerpt = repeatedText(950);
    const candidates = Array.from({ length: 6 }, (_, index) => ({
      source_path: `memory/candidate-${index}.md`,
      title: `Candidate ${index}`,
      heading_path: `Heading > ${index}`,
      sources: ["semantic", "content", "canon", "date", "overflow"],
      score: 0.9 - index / 100,
      term_coverage: { matched: 2, total: 3 },
      matched_terms: ["one", "two", "three", "four", "five", "six", "seven", "eight", "nine"],
      missing_terms: ["a", "b", "c", "d", "e", "f", "g", "h", "i"],
      reasons: ["r1", "r2", "r3", "r4", "r5", "r6"],
      excerpt: longExcerpt,
      noisy_internal_field: "must not leak",
    }));

    const compact = compactRecall({
      ok: true,
      found: true,
      query: "candidate recall",
      source: "memory",
      retrievalCandidates: candidates,
    });

    expect(compact.retrievalCandidates).toHaveLength(5);
    expect(compact.retrievalCandidates[0]).toEqual({
      source_path: "memory/candidate-0.md",
      title: "Candidate 0",
      heading_path: "Heading > 0",
      sources: ["semantic", "content", "canon", "date"],
      score: 0.9,
      term_coverage: { matched: 2, total: 3 },
      matched_terms: ["one", "two", "three", "four", "five", "six", "seven", "eight"],
      missing_terms: ["a", "b", "c", "d", "e", "f", "g", "h"],
      reasons: ["r1", "r2", "r3", "r4", "r5"],
      excerpt: repeatedText(900),
    });
    expect(compact.retrievalCandidates.map((candidate) => candidate.source_path)).not.toContain(
      "memory/candidate-5.md",
    );
  });

  test("preserves bounded ordered-thread evidence on surfaced memories", () => {
    const compact = compactRecall({
      ok: true,
      found: true,
      query: "continue the work page",
      source: "rust-postgres",
      retrievalCandidates: [{
        source_path: "db-only/work/current",
        title: "Current work-page decision",
        memory_id: 22,
        thread_key: "Solarisael website / Work page",
        excerpt: "Current decision.",
        thread_neighbors: Array.from({ length: 8 }, (_, index) => ({
          thread: "Solarisael website / Work page",
          direction: index === 0 ? "previous" : "next",
          id: 10 + index,
          title: `Neighbor ${index}`,
          source_path: `db-only/work/${index}`,
          excerpt: repeatedText(600),
          authority_state: index === 0 ? "historical" : "active",
          superseded_by: index === 0 ? 21 : null,
        })),
      }],
    });

    expect(compact.retrievalCandidates[0].memory_id).toBe(22);
    expect(compact.retrievalCandidates[0].thread_key).toBe("Solarisael website / Work page");
    expect(compact.retrievalCandidates[0].thread_neighbors).toHaveLength(6);
    expect(compact.retrievalCandidates[0].thread_neighbors[0]).toEqual({
      thread: "Solarisael website / Work page",
      direction: "previous",
      id: 10,
      title: "Neighbor 0",
      source_path: "db-only/work/0",
      excerpt: repeatedText(500),
      authority_state: "historical",
      superseded_by: 21,
    });
    expect(compact.retrievalCandidates[0].thread_neighbors.map((neighbor) => neighbor.id))
      .toEqual([10, 11, 12, 13, 14, 15]);
  });

  test("suppresses raw chunk arrays when fused retrieval candidates exist", () => {
    const compact = compactRecall({
      ok: true,
      found: true,
      query: "prefer fused candidates",
      source: "memory",
      retrievalCandidates: [
        {
          source_path: "memory/fused.md",
          title: "Fused result",
          excerpt: "The fused result is enough context.",
        },
      ],
      semanticChunks: [
        {
          source_path: "memory/raw-semantic.md",
          heading_path: "Raw semantic",
          sim: 0.99,
          body: "Raw semantic chunk should not be injected beside fused candidates.",
        },
      ],
      contentChunks: [
        {
          source_path: "memory/raw-content.md",
          heading_path: "Raw content",
          ws: 0.88,
          body: "Raw content chunk should not be injected beside fused candidates.",
        },
      ],
    });

    expect(compact.retrievalCandidates).toHaveLength(1);
    expect(compact.semanticChunks).toEqual([]);
    expect(compact.contentChunks).toEqual([]);
  });

  test("preserves recall warnings while suppressing raw chunks and defaults omitted warnings", () => {
    const warnings = ["semantic retrieval disabled: embedding model unavailable"];
    const compact = compactRecall({
      ok: true,
      found: true,
      query: "warning retention",
      source: "memory",
      warnings,
      retrievalCandidates: [{
        source_path: "memory/fused.md",
        title: "Fused result",
        excerpt: "Fused context.",
      }],
      semanticChunks: [{ body: "suppressed semantic chunk" }],
      contentChunks: [{ body: "suppressed content chunk" }],
    });

    expect(compact.warnings).toBe(warnings);
    expect(compact.semanticChunks).toEqual([]);
    expect(compact.contentChunks).toEqual([]);
    expect(compactRecall({ ok: true }).warnings).toEqual([]);
  });

  test("filters reverse-canon matches unless directly named or tied to a surfaced candidate path", () => {
    const compact = compactRecall({
      ok: true,
      found: true,
      query: "Explain Alias Gate without unrelated canon noise",
      source: "memory",
      retrievalCandidates: [
        {
          source_path: "house/memory/surfaced-candidate.md",
          title: "Surfaced candidate",
          excerpt: "The candidate source should allow directly connected canon context.",
        },
      ],
      canonMatches: [
        {
          termKey: "canonical-alias-entry",
          entry: {
            type: "project",
            summary: "Kept because the query directly names its alias.",
            aliases: ["Alias Gate"],
            files: [{ file: "memory/alias-entry.md", lines: [1, 4] }],
          },
        },
        {
          termKey: "candidate-linked-entry",
          entry: {
            type: "memory",
            summary: "Kept because one canon file is the surfaced candidate source.",
            aliases: ["Not Named"],
            files: [{ file: "memory/surfaced-candidate.md", lines: [8, 12] }],
          },
        },
        {
          termKey: "reverse-index-noise",
          entry: {
            type: "meta",
            summary: "Filtered because it is neither named nor linked to the surfaced source.",
            aliases: ["Noisy Alias"],
            files: [{ file: "memory/unrelated.md", lines: [20, 30] }],
          },
        },
      ],
    });

    expect(compact.canonMatches).toEqual([
      {
        termKey: "canonical-alias-entry",
        type: "project",
        summary: "Kept because the query directly names its alias.",
        files: [{ file: "memory/alias-entry.md", lines: [1, 4] }],
      },
      {
        termKey: "candidate-linked-entry",
        type: "memory",
        summary: "Kept because one canon file is the surfaced candidate source.",
        files: [{ file: "memory/surfaced-candidate.md", lines: [8, 12] }],
      },
    ]);
  });

  test("nudges cluster rebuilds only when telemetry says clusters are stale", () => {
    const staleCases = [
      {
        name: "never built",
        clusterStaleness: {
          built_at: null,
          chunks_since_build: 12,
          fraction_unseen: 0.4,
        },
        expected: ["never built", "12 chunks since", "40% of corpus unseen", "house/substrate/rebuild_clusters.py"],
      },
      {
        name: "unseen fraction at threshold",
        clusterStaleness: {
          built_at: "2026-07-01T12:34:56Z",
          chunks_since_build: 3,
          fraction_unseen: 0.15,
        },
        expected: ["built 2026-07-01", "3 chunks since", "15% of corpus unseen", "house/substrate/rebuild_clusters.py"],
      },
    ];

    for (const { name, clusterStaleness, expected } of staleCases) {
      const compact = compactRecall({
        ok: true,
        found: true,
        query: name,
        source: "memory",
        clusterStaleness,
      });

      for (const fragment of expected) {
        expect(compact.clusterNudge).toContain(fragment);
      }
    }

    const fresh = compactRecall({
      ok: true,
      found: true,
      query: "fresh clusters",
      source: "memory",
      clusterStaleness: {
        built_at: "2026-07-09T00:00:00Z",
        chunks_since_build: 2,
        fraction_unseen: 0.149,
      },
    });

    expect(fresh).not.toHaveProperty("clusterNudge");
  });

  test("passes through bounded cluster resonance telemetry only when profile data exists", () => {
    const compact = compactRecall({
      ok: true,
      found: true,
      query: "resonance",
      source: "memory",
      clusterResonance: {
        profile: Array.from({ length: 9 }, (_, index) => ({
          label: `cluster-${index}`,
          activation: 0.9 - index / 10,
          member_count: 10 + index,
          internalScore: "must not leak",
        })),
        hot: ["hot-0", "hot-1", "hot-2", "hot-3"],
      },
    });

    expect(compact.clusterResonance).toEqual({
      note: "substrate resonance: what the memory space finds near this query — telemetry, not model-internal state",
      profile: Array.from({ length: 8 }, (_, index) => ({
        label: `cluster-${index}`,
        activation: 0.9 - index / 10,
        members: 10 + index,
      })),
      dormantHot: ["hot-0", "hot-1", "hot-2"],
    });

    const missing = compactRecall({
      ok: true,
      found: true,
      query: "no resonance",
      source: "memory",
      clusterResonance: { hot: ["hot-without-profile"] },
    });

    expect(missing).not.toHaveProperty("clusterResonance");
  });

  test("passes through memory handles while bounding embedded memory bodies", () => {
    const compact = compactRecall({
      ok: true,
      found: true,
      query: "memory handle",
      source: "memory",
      memoryHandle: {
        path: "memory/example.md",
        title: "Example",
        memory: {
          source_path: "memory/example.md",
          body: repeatedText(6100),
          frontmatter: { type: "note" },
        },
      },
    });

    expect(compact.memoryHandle).toEqual({
      path: "memory/example.md",
      title: "Example",
      memory: {
        source_path: "memory/example.md",
        body: repeatedText(6000),
        frontmatter: { type: "note" },
      },
    });

    const missing = compactRecall({
      ok: true,
      found: true,
      query: "missing memory handle",
      source: "memory",
    });

    expect(missing).not.toHaveProperty("memoryHandle");
  });
});

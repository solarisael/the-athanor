import { describe, expect, test } from "bun:test";
import { fuseRetrievalCandidates } from "../src/memory-rank.ts";

const TERMS = ["plugin", "retrieval", "roadmap"];
const QUERY = "plugin retrieval roadmap";

function searchCandidate(overrides = {}) {
  return {
    id: "memories:1",
    source: "memory",
    source_table: "memories",
    source_id: 1,
    room: "house",
    title: "Plugin retrieval roadmap",
    source_path: "memory/plugin-retrieval-roadmap.md",
    heading_path: "## Retrieval",
    excerpt: "Plugin retrieval roadmap notes with exact title and path signals.",
    raw_rank: 0.6,
    score: 4.0,
    term_coverage: 1.0,
    matched_terms: [...TERMS],
    missing_terms: [],
    reasons: ["memory search", "title/path search"],
    ...overrides,
  };
}

function candidateByPath(candidates, sourcePath) {
  const candidate = candidates.find((item) => item.source_path === sourcePath);
  expect(candidate).toBeDefined();
  return candidate;
}

function expectNormalizedCandidate(candidate, sourcePath, expectedSources) {
  expect(candidate.source_path).toBe(sourcePath);
  expect(Array.isArray(candidate.sources)).toBe(true);
  for (const source of expectedSources) {
    expect(candidate.sources).toContain(source);
  }
  expect(Number.isFinite(candidate.score)).toBe(true);
  expect(Number.isFinite(candidate.term_coverage)).toBe(true);
  expect(Array.isArray(candidate.matched_terms)).toBe(true);
  expect(Array.isArray(candidate.missing_terms)).toBe(true);
  expect(Array.isArray(candidate.reasons)).toBe(true);
  expect(candidate.reasons.length).toBeGreaterThan(0);
}

describe("fuseRetrievalCandidates", () => {
  test("normalizes search, semantic, content, and date lanes into the same candidate contract", () => {
    const candidates = fuseRetrievalCandidates({
      searchCandidates: [
        searchCandidate({
          source: "entity",
          source_path: "memory/search-candidate.md",
          reasons: ["named entity", "alias/summary/title search"],
        }),
      ],
      semanticChunks: [
        {
          source_path: "memory/semantic-candidate.md",
          heading_path: "## Semantic lane",
          chunk_index: 2,
          sim: 0.82,
          body: "Semantic plugin retrieval chunk with enough query terms to score.",
        },
      ],
      contentChunks: [
        {
          source_path: "memory/content-candidate.md",
          heading_path: "## Content lane",
          chunk_index: 3,
          ws: 0.76,
          body: "Content roadmap retrieval chunk with trigram-style lexical evidence.",
        },
      ],
      dateMatches: [
        {
          source_path: "memory/date-candidate.md",
          title: "2026-05-23 plugin retrieval roadmap",
          body_excerpt: "The tagged date recorded the plugin retrieval roadmap decision.",
          dates: ["2026-05-23"],
          body_full_chars: 128,
        },
      ],
    }, { query: QUERY, searchTerms: TERMS });

    expect(candidates.length).toBe(4);
    expectNormalizedCandidate(candidateByPath(candidates, "memory/search-candidate.md"), "memory/search-candidate.md", ["entity"]);
    expectNormalizedCandidate(candidateByPath(candidates, "memory/semantic-candidate.md"), "memory/semantic-candidate.md", ["semantic"]);
    expectNormalizedCandidate(candidateByPath(candidates, "memory/content-candidate.md"), "memory/content-candidate.md", ["content"]);
    expectNormalizedCandidate(candidateByPath(candidates, "memory/date-candidate.md"), "memory/date-candidate.md", ["date"]);
  });

  test("fuses lanes with the same source_path and preserves every contributing source and reason", () => {
    const candidates = fuseRetrievalCandidates({
      searchCandidates: [
        searchCandidate({
          source: "thread",
          source_path: "memory/shared-source.md",
          reasons: ["thread/context/path search"],
        }),
      ],
      semanticChunks: [
        {
          source_path: "memory/shared-source.md",
          heading_path: "## Shared semantic",
          chunk_index: 0,
          sim: 0.81,
          body: "Semantic evidence for the plugin retrieval roadmap.",
          reasons: ["semantic neighbor"],
        },
      ],
      contentChunks: [
        {
          source_path: "memory/shared-source.md",
          heading_path: "## Shared content",
          chunk_index: 1,
          ws: 0.74,
          body: "Content evidence for the plugin retrieval roadmap.",
          reasons: ["content trigram"],
        },
      ],
      dateMatches: [
        {
          source_path: "memory/shared-source.md",
          title: "Shared date hit",
          body_excerpt: "Date evidence for the plugin retrieval roadmap.",
          dates: ["2026-05-23"],
          reasons: ["date tag"],
        },
      ],
    }, { query: QUERY, searchTerms: TERMS });

    expect(candidates.length).toBe(1);
    const fused = candidates[0];
    expect(fused.source_path).toBe("memory/shared-source.md");
    for (const source of ["thread", "semantic", "content", "date"]) {
      expect(fused.sources).toContain(source);
    }
    for (const reason of ["thread/context/path search", "semantic neighbor", "content trigram", "date tag"]) {
      expect(fused.reasons).toContain(reason);
    }
  });

  test("ranks multi-term exact title and path evidence ahead of a broad one-term content hit", () => {
    const candidates = fuseRetrievalCandidates({
      searchCandidates: [
        searchCandidate({
          source: "memory",
          source_path: "docs/plugin-retrieval-roadmap.md",
          title: "Plugin retrieval roadmap",
          excerpt: "Exact notes for the plugin retrieval roadmap.",
          score: 0.25,
          reasons: ["exact title/path match"],
        }),
      ],
      contentChunks: [
        {
          source_path: "memory/broad-retrieval-memory.md",
          heading_path: "## Broad retrieval",
          chunk_index: 7,
          ws: 0.99,
          body: "retrieval retrieval retrieval retrieval retrieval; a broad memory hit with only one query term",
          reasons: ["broad content hit"],
        },
      ],
    }, { query: QUERY, searchTerms: TERMS, maxResults: 2 });

    expect(candidates.length).toBe(2);
    expect(candidates[0].source_path).toBe("docs/plugin-retrieval-roadmap.md");
    expect(candidates[0].matched_terms).toEqual(TERMS);
    expect(candidates[0].term_coverage).toBe(1);
    expect(candidates[0].score).toBeGreaterThan(candidates[1].score);
    expect(candidates[1].source_path).toBe("memory/broad-retrieval-memory.md");
    expect(candidates[1].matched_terms).toEqual(["retrieval"]);
  });

  test("diversity caps keep the strongest representative while limiting repeated source types and paths", () => {
    const candidates = fuseRetrievalCandidates({
      searchCandidates: [
        searchCandidate({
          id: "memories:best",
          source: "memory",
          source_path: "memory/best-plugin-retrieval-roadmap.md",
          title: "Plugin retrieval roadmap",
          excerpt: "The exact plugin retrieval roadmap note.",
          score: 9,
          reasons: ["best memory representative"],
        }),
        searchCandidate({
          id: "memories:weaker",
          source: "memory",
          source_path: "memory/weaker-retrieval-only.md",
          title: "Retrieval notes",
          excerpt: "Only retrieval appears here; this is a weaker broad memory hit.",
          score: 8,
          term_coverage: 1 / 3,
          matched_terms: ["retrieval"],
          missing_terms: ["plugin", "roadmap"],
          reasons: ["weaker memory representative"],
        }),
      ],
      semanticChunks: [
        {
          source_path: "memory/best-plugin-retrieval-roadmap.md",
          heading_path: "## Vector confirmation",
          chunk_index: 0,
          sim: 0.88,
          body: "Semantic confirmation for the plugin retrieval roadmap.",
          reasons: ["semantic confirmation for best path"],
        },
        {
          source_path: "memory/semantic-plugin-roadmap.md",
          heading_path: "## Semantic alternative",
          chunk_index: 1,
          sim: 0.7,
          body: "Semantic alternative about the plugin roadmap.",
          reasons: ["semantic alternative"],
        },
      ],
      dateMatches: [
        {
          source_path: "memory/date-plugin-roadmap.md",
          title: "2026-05-23 plugin roadmap",
          body_excerpt: "A dated plugin roadmap record.",
          dates: ["2026-05-23"],
          reasons: ["date alternative"],
        },
      ],
    }, {
      query: QUERY,
      searchTerms: TERMS,
      maxResults: 3,
      maxPerSource: 1,
      maxPerSourcePath: 1,
    });

    const paths = candidates.map((candidate) => candidate.source_path);
    expect(paths).toContain("memory/best-plugin-retrieval-roadmap.md");
    expect(paths).not.toContain("memory/weaker-retrieval-only.md");

    const best = candidateByPath(candidates, "memory/best-plugin-retrieval-roadmap.md");
    expect(best.sources).toContain("memory");
    expect(best.sources).toContain("semantic");
    expect(best.reasons).toContain("best memory representative");
    expect(best.reasons).toContain("semantic confirmation for best path");

    const memoryRepresentatives = candidates.filter((candidate) => candidate.sources.includes("memory"));
    expect(memoryRepresentatives.length).toBe(1);
    expect(candidates.length).toBeLessThanOrEqual(3);
  });

  test("no-embedding query parsing drops conversational fillers while preserving technical terms", () => {
    const query = "wanna see our retrieval candidate fusion query routing without use";
    const technicalTerms = ["retrieval", "candidate", "fusion", "query", "routing"];
    const fillerTerms = ["wanna", "see", "our", "without", "use"];

    const candidates = fuseRetrievalCandidates({
      searchCandidates: [
        searchCandidate({
          source_path: "memory/retrieval-candidate-fusion-query-routing.md",
          title: "Retrieval candidate fusion query routing",
          heading_path: "## Query routing",
          excerpt: "Candidate fusion keeps retrieval query routing evidence together.",
          score: 0.25,
          matched_terms: [],
          missing_terms: [],
          reasons: ["technical exact evidence"],
        }),
      ],
    }, { query, maxResults: 1 });

    expect(candidates.length).toBe(1);
    expect(candidates[0].matched_terms).toEqual(technicalTerms);
    expect(candidates[0].missing_terms).toEqual([]);
    for (const fillerTerm of fillerTerms) {
      expect(candidates[0].matched_terms).not.toContain(fillerTerm);
      expect(candidates[0].missing_terms).not.toContain(fillerTerm);
    }
  });

  test("no-embedding ranking puts strong technical evidence ahead of weak generic wording", () => {
    const query = "wanna see our retrieval candidate fusion query routing without use";

    const candidates = fuseRetrievalCandidates({
      searchCandidates: [
        searchCandidate({
          id: "memories:generic",
          source_path: "memory/wanna-see-use-without-our.md",
          title: "Wanna see use without our notes",
          heading_path: "## Generic wording",
          excerpt: "wanna see use without our generic note",
          score: 0.25,
          matched_terms: [],
          missing_terms: [],
          reasons: ["generic wording only"],
        }),
        searchCandidate({
          id: "memories:technical",
          source_path: "memory/retrieval-candidate-fusion-query-routing.md",
          title: "Retrieval candidate fusion query routing",
          heading_path: "## Query routing",
          excerpt: "Candidate fusion keeps retrieval query routing evidence together.",
          score: 0.25,
          matched_terms: [],
          missing_terms: [],
          reasons: ["technical exact evidence"],
        }),
      ],
    }, { query, maxResults: 2 });

    expect(candidates.length).toBe(2);
    expect(candidates[0].source_path).toBe("memory/retrieval-candidate-fusion-query-routing.md");
    expect(candidates[0].score).toBeGreaterThan(candidates[1].score);

    const generic = candidateByPath(candidates, "memory/wanna-see-use-without-our.md");
    expect(generic.matched_terms).toEqual([]);
    expect(generic.missing_terms).toEqual(["retrieval", "candidate", "fusion", "query", "routing"]);
  });

  test("no-embedding weighty entity metadata promotes only exact or intent-compatible candidates", () => {
    const query = "retrieval candidate fusion query routing";

    const candidates = fuseRetrievalCandidates({
      searchCandidates: [
        searchCandidate({
          id: "entity:exact-neutral",
          source: "entity",
          kind: "memory",
          weighty: false,
          source_path: "entities/exact-neutral-personal.md",
          title: "retrieval candidate fusion query routing",
          heading_path: "## Exact entity",
          excerpt: "retrieval candidate fusion query routing",
          score: 0.1,
          matched_terms: [],
          missing_terms: [],
          reasons: ["exact neutral entity"],
        }),
        searchCandidate({
          id: "entity:exact-weighty",
          source: "entity",
          kind: "memory",
          weighty: true,
          source_path: "entities/exact-weighty-personal.md",
          title: "retrieval candidate fusion query routing",
          heading_path: "## Exact entity",
          excerpt: "retrieval candidate fusion query routing",
          score: 0.1,
          matched_terms: [],
          missing_terms: [],
          reasons: ["exact weighty entity"],
        }),
        searchCandidate({
          id: "entity:intent-compatible",
          source: "entity",
          kind: "meta",
          weighty: true,
          source_path: "entities/technical-routing-meta.md",
          title: "Retrieval routing note",
          heading_path: "## Routing",
          excerpt: "Retrieval routing terms only.",
          score: 0.1,
          matched_terms: [],
          missing_terms: [],
          reasons: ["technical meta entity"],
        }),
        searchCandidate({
          id: "entity:incompatible-weighty",
          source: "entity",
          kind: "memory",
          weighty: true,
          source_path: "entities/personal-weighty-routing.md",
          title: "Retrieval routing note",
          heading_path: "## Routing",
          excerpt: "Retrieval routing terms only.",
          score: 0.1,
          matched_terms: [],
          missing_terms: [],
          reasons: ["incompatible weighty personal entity"],
        }),
        searchCandidate({
          id: "entity:incompatible-neutral",
          source: "entity",
          kind: "memory",
          weighty: false,
          source_path: "entities/personal-neutral-routing.md",
          title: "Retrieval routing note",
          heading_path: "## Routing",
          excerpt: "Retrieval routing terms only.",
          score: 0.1,
          matched_terms: [],
          missing_terms: [],
          reasons: ["incompatible neutral personal entity"],
        }),
      ],
    }, { query, maxResults: 5 });

    const exactWeighty = candidateByPath(candidates, "entities/exact-weighty-personal.md");
    const exactNeutral = candidateByPath(candidates, "entities/exact-neutral-personal.md");
    const intentCompatible = candidateByPath(candidates, "entities/technical-routing-meta.md");
    const incompatibleWeighty = candidateByPath(candidates, "entities/personal-weighty-routing.md");
    const incompatibleNeutral = candidateByPath(candidates, "entities/personal-neutral-routing.md");

    expect(exactWeighty.score).toBeGreaterThan(exactNeutral.score);
    expect(intentCompatible.score).toBeGreaterThan(incompatibleWeighty.score);
    expect(incompatibleNeutral.score).toBeGreaterThan(incompatibleWeighty.score);
    expect(incompatibleWeighty.matched_terms).toEqual(["retrieval", "routing"]);
    expect(incompatibleWeighty.missing_terms).toEqual(["candidate", "fusion", "query"]);
  });

  test("date lookup scoring does not apply technical-memory demotion to personal canon", () => {
    const query = "what happened on 2026-07-04";

    const candidates = fuseRetrievalCandidates({
      searchCandidates: [
        searchCandidate({
          id: "entity:date-personal-neutral",
          source: "entity",
          kind: "memory",
          weighty: false,
          source_path: "entities/date-personal-neutral.md",
          title: "2026 07 04 memory",
          heading_path: "## Date memory",
          excerpt: "2026 07 04 memory",
          score: 0.1,
          matched_terms: [],
          missing_terms: [],
          reasons: ["date lookup personal neutral"],
        }),
        searchCandidate({
          id: "entity:date-personal-weighty",
          source: "entity",
          kind: "memory",
          weighty: true,
          source_path: "entities/date-personal-weighty.md",
          title: "2026 07 04 memory",
          heading_path: "## Date memory",
          excerpt: "2026 07 04 memory",
          score: 0.1,
          matched_terms: [],
          missing_terms: [],
          reasons: ["date lookup personal weighty"],
        }),
      ],
    }, { query, maxResults: 2 });

    const neutral = candidateByPath(candidates, "entities/date-personal-neutral.md");
    const weighty = candidateByPath(candidates, "entities/date-personal-weighty.md");

    expect(weighty.score).toBeGreaterThan(neutral.score);
    expect(weighty.matched_terms).toEqual(["2026", "07", "04"]);
  });
});

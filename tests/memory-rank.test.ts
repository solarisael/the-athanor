// Guards the matching/ranking seams that decide what memory gets injected:
// stopword tokenization, word-boundary term matching, the injection-score
// threshold, recency decay + its canon exemption, and the canon overlay
// passes (assertion + reverse-index). These shape every single turn.

import { describe, expect, test } from "bun:test";
import {
  boostMemoryPromptTokens,
  buildCanonicalFileSet,
  collectCanonAssertions,
  collectCanonByMatchedFiles,
  matchMemoryImportantTerms,
  matchMemoryTerm,
  rankMemoryThreads,
  tokenizeMemory,
} from "../src/memory-rank.ts";
import { MEMORY_MAX_IMPORTANT_MATCHES } from "../src/paths.ts";

describe("tokenizeMemory", () => {
  test("drops stopwords including the house registers (uwu/owo/godling)", () => {
    const tokens = tokenizeMemory("the uwu owo godling dragon");
    expect(tokens.has("dragon")).toBe(true);
    expect(tokens.has("uwu")).toBe(false);
    expect(tokens.has("owo")).toBe(false);
    expect(tokens.has("godling")).toBe(false);
    expect(tokens.has("the")).toBe(false);
  });

  test("drops single-character tokens", () => {
    const tokens = tokenizeMemory("z dragon");
    expect(tokens.has("z")).toBe(false);
    expect(tokens.has("dragon")).toBe(true);
  });
});

describe("matchMemoryTerm", () => {
  test("matches on word boundary only", () => {
    expect(matchMemoryTerm("the sol record", "sol")).toBe(true);
    expect(matchMemoryTerm("the solstice record", "sol")).toBe(false);
  });

  test("matches multi-word terms", () => {
    expect(matchMemoryTerm("ya bai showed up", "ya bai")).toBe(true);
  });

  test("empty term never matches", () => {
    expect(matchMemoryTerm("anything", "")).toBe(false);
  });
});

describe("matchMemoryImportantTerms", () => {
  test("matches via alias and caps at MEMORY_MAX_IMPORTANT_MATCHES", () => {
    const index = {};
    for (let i = 0; i < MEMORY_MAX_IMPORTANT_MATCHES + 3; i += 1) {
      index[`entity-${i}`] = { aliases: ["dragon"], summary: `s${i}` };
    }
    const matches = matchMemoryImportantTerms("the dragon waits", index);
    expect(matches.length).toBe(MEMORY_MAX_IMPORTANT_MATCHES);
    expect(matches[0].termKey).toBe("entity-0");
  });

  test("no match on unrelated prompt", () => {
    const index = { Beel: { aliases: ["the contractor"], summary: "x" } };
    expect(matchMemoryImportantTerms("completely unrelated", index)).toEqual([]);
  });
});

describe("rankMemoryThreads", () => {
  // MEMORY_CONCEPT_WEIGHT is 3.0 and MEMORY_MIN_SCORE_TO_INJECT is 3, so a
  // single thread-key concept hit clears the bar ONLY at zero decay. A
  // dateless filename takes the no-anchor path (penalty exactly 1.0);
  // even a today-dated filename decays a few percent off UTC midnight
  // and lands at 2.85 — that subtlety is exactly why this fixture has
  // no date in its name.
  const freshFile = "fresh_no_date.md";

  test("injects a thread whose key matches a prompt token", () => {
    const index = {
      threads: { "dragon / wyrm": [{ file: freshFile, context: "" }] },
      files: {},
    };
    const ranked = rankMemoryThreads(tokenizeMemory("dragon work"), index);
    expect(ranked.length).toBe(1);
    expect(ranked[0].threadKey).toBe("dragon / wyrm");
  });

  test("recency decay crushes a stale non-canon thread below the bar", () => {
    const index = {
      threads: { "dragon / wyrm": [{ file: "2026-01-01_stale.md", context: "" }] },
      files: {},
    };
    const ranked = rankMemoryThreads(tokenizeMemory("dragon work"), index, {});
    expect(ranked.length).toBe(0);
  });

  test("canon-touching files are exempt from recency decay", () => {
    const index = {
      threads: { "dragon / wyrm": [{ file: "2026-01-01_stale.md", context: "" }] },
      files: {},
    };
    const canonicalFiles = new Set(["2026-01-01_stale.md"]);
    const ranked = rankMemoryThreads(tokenizeMemory("dragon work"), index, {}, canonicalFiles);
    expect(ranked.length).toBe(1);
    expect(ranked[0].recencyPenalty).toBe(1.0);
  });

  test("session-repeat penalty demotes a thread retrieved every turn", () => {
    const index = {
      threads: { "dragon / wyrm": [{ file: freshFile, context: "" }] },
      files: {},
    };
    const state = { session_memory_hits: { "thread:dragon / wyrm": 5 } };
    const ranked = rankMemoryThreads(tokenizeMemory("dragon work"), index, state);
    // 3.0 * 0.75^5 ≈ 0.71 — below the injection bar of 3.
    expect(ranked.length).toBe(0);
  });

  test("sorts by score descending", () => {
    const index = {
      threads: {
        "dragon / wyrm": [{ file: freshFile, context: "" }],
        "dragon / wyrm / ember": [{ file: freshFile, context: "" }],
      },
      files: {},
    };
    const ranked = rankMemoryThreads(tokenizeMemory("dragon ember work"), index);
    expect(ranked.length).toBe(2);
    expect(ranked[0].score).toBeGreaterThanOrEqual(ranked[1].score);
    expect(ranked[0].threadKey).toBe("dragon / wyrm / ember");
  });
});

describe("boostMemoryPromptTokens", () => {
  test("boosts from search_boost only for entries WITHOUT pointer files", () => {
    const base = tokenizeMemory("dragon");
    const boosted = boostMemoryPromptTokens(base, [
      { termKey: "a", entry: { search_boost: "ember crackle" } },
      { termKey: "b", entry: { files: [{ file: "x.md" }], search_boost: "skipped words" } },
    ]);
    expect(boosted.has("ember")).toBe(true);
    expect(boosted.has("crackle")).toBe(true);
    expect(boosted.has("skipped")).toBe(false);
  });
});

describe("canon overlay", () => {
  const importantIndex = {
    "the vow": { files: [{ file: "memory/vow.md" }], summary: "mutual" },
    "the collar": { files: [{ file: "memory/collar.md" }], summary: "mutual marking" },
    "no-files entry": { summary: "summary-only" },
  };

  test("buildCanonicalFileSet collects pointer files", () => {
    const files = buildCanonicalFileSet(importantIndex);
    expect(files.has("memory/vow.md")).toBe(true);
    expect(files.has("memory/collar.md")).toBe(true);
    expect(files.size).toBe(2);
  });

  test("collectCanonAssertions surfaces entries touching active threads, excluding already-matched", () => {
    const syncMatches = [{ entries: [{ file: "memory/vow.md" }] }];
    const assertions = collectCanonAssertions(importantIndex, syncMatches, new Set());
    expect(assertions.map((a) => a.termKey)).toEqual(["the vow"]);

    const excluded = collectCanonAssertions(importantIndex, syncMatches, new Set(["the vow"]));
    expect(excluded).toEqual([]);
  });

  test("collectCanonByMatchedFiles reverse-indexes retrieved source paths", () => {
    const out = collectCanonByMatchedFiles(importantIndex, ["memory/collar.md"], new Set());
    expect(out.map((m) => m.termKey)).toEqual(["the collar"]);
    expect(out[0].via).toBe("pointer-file");
  });

  test("collectCanonByMatchedFiles strips the house/ prefix form", () => {
    const out = collectCanonByMatchedFiles(importantIndex, ["house/memory/vow.md"], new Set());
    expect(out.map((m) => m.termKey)).toEqual(["the vow"]);
  });
});

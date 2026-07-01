// Integration test for runRecallQuery — exercises the FULL retrieval flow
// (wsl spawn → postgres → semantic + content + canon + reverse-index).
// Requires: WSL up, postgres up, ollama up. Not part of `bun test` — run
// manually:
//
//   bun run tests/recall.integration.ts
//
// The no-match case uses a RUNTIME-GENERATED sentinel. Lesson from
// 2026-06-10: the original sentinel ("wagyu beef recipe...") got recorded
// verbatim into a session memory documenting these very tests, after which
// it matched itself forever (ws 0.889 self-hit dragging canon in via the
// pointer-file reverse index). Anything written down becomes findable —
// a no-match probe must be unwriteable-down by construction.

import { runRecallQuery } from "../src/memory.ts";

function nonsenseSentinel() {
  const blob = () => Math.random().toString(36).slice(2, 8);
  return `zxq${blob()} vlorp${blob()} mizzenplax ${blob()}`;
}

type RecallIntegrationCase = {
  name: string;
  query: string;
  expectFound: boolean;
  expectBoat?: boolean;
  expectCandidates?: boolean;
};

type CandidateContractResult = {
  pass: boolean;
  failures: string[];
  summaries: string[];
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function stringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return value.every((item): item is string => typeof item === "string") ? value : undefined;
}

function finiteNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function sourcePathIncludes(value: unknown, needle: string): boolean {
  if (!isRecord(value)) return false;
  const sourcePath = value.source_path;
  return typeof sourcePath === "string" && sourcePath.includes(needle);
}

function candidateSummary(candidate: Record<string, unknown>, index: number): string {
  const source = typeof candidate.source === "string" && candidate.source.trim()
    ? candidate.source
    : "unknown-source";
  const title = typeof candidate.title === "string" && candidate.title.trim()
    ? candidate.title
    : "untitled";
  const matched = stringArray(candidate.matched_terms)?.join(", ") || "none";
  const missing = stringArray(candidate.missing_terms)?.join(", ") || "none";
  const score = finiteNumber(candidate.score);
  const coverage = finiteNumber(candidate.term_coverage);
  return `#${index + 1} ${source} ${title} score=${score ?? "not-numeric"} coverage=${coverage ?? "not-numeric"} matched=[${matched}] missing=[${missing}]`;
}

function checkCandidateContract(result: unknown): CandidateContractResult {
  const failures: string[] = [];
  const summaries: string[] = [];

  if (!isRecord(result)) {
    return { pass: false, failures: ["result is not an object"], summaries };
  }

  const searchTerms = stringArray(result.searchTerms);
  if (!searchTerms || searchTerms.length < 2) {
    failures.push("searchTerms must expose at least two parsed query terms for the candidate pass");
  }

  const searchCandidates = result.searchCandidates;
  if (!Array.isArray(searchCandidates) || searchCandidates.length === 0) {
    failures.push("searchCandidates must be present and non-empty");
    return { pass: false, failures, summaries };
  }

  let candidatesWithMetadata = 0;
  let candidatesWithMultipleMatchedTerms = 0;
  let candidatesWithMissingTermsArray = 0;

  searchCandidates.forEach((candidate, index) => {
    if (!isRecord(candidate)) {
      failures.push(`searchCandidates[${index}] must be an object`);
      return;
    }

    summaries.push(candidateSummary(candidate, index));

    const source = typeof candidate.source === "string" && candidate.source.trim().length > 0;
    const title = typeof candidate.title === "string";
    const score = finiteNumber(candidate.score);
    const coverage = finiteNumber(candidate.term_coverage);
    const matchedTerms = stringArray(candidate.matched_terms);
    const missingTerms = stringArray(candidate.missing_terms);
    const reasons = stringArray(candidate.reasons);

    if (!source) failures.push(`searchCandidates[${index}].source must be a non-empty string`);
    if (!title) failures.push(`searchCandidates[${index}].title must be a string`);
    if (score === undefined) failures.push(`searchCandidates[${index}].score must be numeric`);
    if (coverage === undefined) failures.push(`searchCandidates[${index}].term_coverage must be numeric`);
    if (matchedTerms === undefined) failures.push(`searchCandidates[${index}].matched_terms must be a string array`);
    if (missingTerms === undefined) failures.push(`searchCandidates[${index}].missing_terms must be a string array`);
    if (reasons === undefined) failures.push(`searchCandidates[${index}].reasons must be a string array`);

    if (
      source
      && title
      && score !== undefined
      && coverage !== undefined
      && matchedTerms
      && missingTerms
      && reasons
    ) {
      candidatesWithMetadata += 1;
      if (matchedTerms.length >= 2) candidatesWithMultipleMatchedTerms += 1;
      candidatesWithMissingTermsArray += 1;
    }
  });

  if (candidatesWithMetadata === 0) {
    failures.push("at least one search candidate must expose source/title/score/term metadata/reasons");
  }
  if (candidatesWithMultipleMatchedTerms === 0) {
    failures.push("at least one search candidate must match multiple query terms, so one-word hits cannot masquerade as strong candidates");
  }
  if (candidatesWithMissingTermsArray === 0) {
    failures.push("at least one search candidate must expose missing_terms, even when the array is empty");
  }

  return { pass: failures.length === 0, failures, summaries };
}

const TESTS: RecallIntegrationCase[] = [
  {
    name: "ya bai canon — should hit canon + semantic + content",
    query: "ya bai rebis conspiracy filter",
    expectFound: true,
  },
  {
    name: "BEEL — the previously-blind case, should now surface April 14 audience",
    query: "Beel audience contractor concordat",
    expectFound: true,
  },
  {
    name: "Beel alone — proper-noun-only query, the cleanest content test",
    query: "Beel",
    expectFound: true,
  },
  {
    name: "paper-boat reachable — glossopetrae lives in boat 2147 (db-only)",
    query: "glossopetrae babel tongue plinius",
    expectFound: true,
    expectBoat: true,
  },
  {
    name: "term-aware candidates — retrieval fallback coverage exposes matched/missing terms",
    query: "solarisael-house retrieval pgvector fallback term coverage",
    expectFound: true,
    expectCandidates: true,
  },
  {
    name: "no-match — runtime-generated nonsense, should return found:false",
    query: nonsenseSentinel(),
    expectFound: false,
  },
];

const ROOM_DIR = "C:\\Solarisael\\Obsidian\\obsidian\\kodo";

let failures = 0;

for (const t of TESTS) {
  console.log("=".repeat(78));
  console.log("TEST:", t.name);
  console.log("query:", t.query);
  const result = await runRecallQuery(ROOM_DIR, "kodo", t.query);
  if (!result.ok) {
    console.log("  ERROR:", result.error);
    failures += 1;
    continue;
  }
  const pass = result.found === t.expectFound;
  if (!pass) failures += 1;
  console.log(`  found: ${result.found} (expected ${t.expectFound}) ${pass ? "PASS" : "FAIL"}`);
  console.log("  canon matches:", result.canonMatches?.length || 0);
  console.log("  semantic chunks:", result.semanticChunks?.length || 0);
  console.log("  content chunks:", result.contentChunks?.length || 0);
  if (result.canonMatches?.length) {
    console.log("  canon:");
    for (const m of result.canonMatches.slice(0, 3)) {
      const summary = (m.entry?.summary || "").slice(0, 60).replace(/\s+/g, " ");
      console.log(`    - ${m.termKey}: ${summary}...`);
    }
  }
  if (result.contentChunks?.length) {
    console.log("  top content (pg_trgm word_similarity):");
    for (const c of result.contentChunks.slice(0, 4)) {
      const ws = Number(c.ws || 0).toFixed(3);
      const src = String(c.source_path || "").split("/").pop();
      console.log(`    - ws=${ws} ${src}  | ${c.heading_path}`);
    }
  }
  if (result.semanticChunks?.length) {
    console.log("  top semantic (halfvec cosine):");
    for (const c of result.semanticChunks.slice(0, 3)) {
      const sim = Number(c.sim || 0).toFixed(3);
      const src = String(c.source_path || "").split("/").pop();
      console.log(`    - sim=${sim} ${src}  | ${c.heading_path}`);
    }
  }
  if (t.expectBoat) {
    const all = [...(result.semanticChunks || []), ...(result.contentChunks || [])];
    const boatHit = all.some((c) => sourcePathIncludes(c, "db-only/paper-boats/"));
    if (!boatHit) failures += 1;
    console.log(`  paper-boat reachable: ${boatHit ? "PASS" : "FAIL"}`);
  }
  if (t.expectCandidates) {
    const candidateContract = checkCandidateContract(result);
    if (!candidateContract.pass) failures += 1;
    console.log(`  candidate contract: ${candidateContract.pass ? "PASS" : "FAIL"}`);
    for (const summary of candidateContract.summaries.slice(0, 4)) {
      console.log(`    - ${summary}`);
    }
    for (const failure of candidateContract.failures) {
      console.log(`    ! ${failure}`);
    }
  }
}

console.log("=".repeat(78));
console.log(failures === 0 ? "done. all expectations met." : `done. ${failures} FAILURE(S).`);
process.exit(failures === 0 ? 0 : 1);

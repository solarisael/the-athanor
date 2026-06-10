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

import { runRecallQuery } from "../memory.ts";

function nonsenseSentinel() {
  const blob = () => Math.random().toString(36).slice(2, 8);
  return `zxq${blob()} vlorp${blob()} mizzenplax ${blob()}`;
}

const TESTS = [
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
}

console.log("=".repeat(78));
console.log(failures === 0 ? "done. all expectations met." : `done. ${failures} FAILURE(S).`);
process.exit(failures === 0 ? 0 : 1);

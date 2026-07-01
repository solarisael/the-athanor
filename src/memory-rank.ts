// Pure matching + ranking logic for the memory pipeline: token tooling,
// thread scoring, session-repeat + recency penalties, important-index
// (named-entity) matching, and the canon overlay passes.
//
// Nothing in this module touches the filesystem or spawns a process —
// it ranks and matches data that memory-sources.ts already fetched.

import {
  HOUSE_MEMORY_DIRNAME,
  MEMORY_CONCEPT_WEIGHT, MEMORY_CONTEXT_WEIGHT,
  MEMORY_FILE_ONE_LINE_WEIGHT,
  MEMORY_MAX_IMPORTANT_MATCHES,
  MEMORY_MIN_SCORE_TO_INJECT,
  MEMORY_RECENCY_HALF_LIFE_DAYS,
  MEMORY_SESSION_REPEAT_PENALTY_BASE,
  MEMORY_STOPWORDS, MEMORY_TOKEN_RE,
} from "./paths.ts";
import { escapeRegExp } from "./util.ts";

// ── token tooling ──────────────────────────────────────────────────────────

export function tokenizeMemory(text) {
  const tokens = String(text || "").toLowerCase().match(MEMORY_TOKEN_RE) || [];
  return new Set(tokens.filter((token) => token.length > 1 && !MEMORY_STOPWORDS.has(token)));
}

function extractMemoryConcepts(threadKey) {
  const concepts = new Set();
  for (const variant of String(threadKey || "").split("/")) {
    for (const token of tokenizeMemory(variant.trim())) {
      concepts.add(token);
    }
  }

  return concepts;
}

function countTokenOverlap(left, right) {
  let count = 0;
  for (const token of left) {
    if (right.has(token)) count += 1;
  }

  return count;
}

// ── thread ranking ─────────────────────────────────────────────────────────

function collectMemoryEntryTokens(entries, filesMap) {
  const contextTokens = new Set();
  const fileTokens = new Set();
  for (const entry of Array.isArray(entries) ? entries : []) {
    for (const token of tokenizeMemory(entry?.context || "")) {
      contextTokens.add(token);
    }
    const fileMeta = filesMap?.[entry?.file || ""];
    if (fileMeta?.one_line) {
      for (const token of tokenizeMemory(fileMeta.one_line)) {
        fileTokens.add(token);
      }
    }
  }

  return { contextTokens, fileTokens };
}

function scoreMemoryThread(promptTokens, threadKey, entries, filesMap) {
  const concepts = extractMemoryConcepts(threadKey);
  const { contextTokens, fileTokens } = collectMemoryEntryTokens(entries, filesMap);

  return (
    countTokenOverlap(concepts, promptTokens) * MEMORY_CONCEPT_WEIGHT
    + countTokenOverlap(contextTokens, promptTokens) * MEMORY_CONTEXT_WEIGHT
    + countTokenOverlap(fileTokens, promptTokens) * MEMORY_FILE_ONE_LINE_WEIGHT
  );
}

function sessionRepeatCountForThread(state, threadKey, entries = []) {
  const hits = state?.session_memory_hits || {};
  const counts = [hits[`thread:${threadKey}`] || 0];
  for (const entry of Array.isArray(entries) ? entries : []) {
    if (entry?.file) counts.push(hits[`file:${entry.file}`] || 0);
  }

  return Math.max(0, ...counts.map((value) => Number(value) || 0));
}

function memorySessionRepeatPenalty(repeatCount) {
  return Math.pow(MEMORY_SESSION_REPEAT_PENALTY_BASE, Math.max(0, Number(repeatCount) || 0));
}

// ── recency decay (audit ticket #2) ────────────────────────────────────────
// Threads inherit a recency penalty from their files' `last_touched_at`.
// Touch-based: a thread that's been retrieved every turn keeps `last_touched_at`
// fresh and gets no decay; a thread quietly sitting on disk for weeks
// gets exponentially demoted. Canon-touching files are exempt entirely —
// they're already surfaced via the canon-assertion overlay (#5) and
// decaying them doubly is wrong.
//
// When `state.loaded[file].last_touched_at` is missing (file hasn't been
// retrieved yet in this state's history), fall back to a date parsed
// from the filename (`YYYY-MM-DD_*.md` convention); if no date, return
// no decay (give brand-new content a fair first-shot at the score).

export function buildCanonicalFileSet(importantIndex) {
  const files = new Set();
  for (const entry of Object.values(importantIndex || {})) {
    for (const fileRef of Array.isArray(entry?.files) ? entry.files : []) {
      if (fileRef?.file) files.add(fileRef.file);
    }
  }

  return files;
}

const DATE_IN_FILENAME_RE = /(\d{4}-\d{2}-\d{2})/;
const MS_PER_DAY = 1000 * 60 * 60 * 24;

function resolveLastTouchedAtMs(filePath, loadedEntry) {
  if (loadedEntry?.last_touched_at) {
    const parsed = Date.parse(loadedEntry.last_touched_at);
    if (!Number.isNaN(parsed)) return parsed;
  }

  const match = String(filePath || "").match(DATE_IN_FILENAME_RE);
  if (match) {
    const parsed = Date.parse(match[1]);
    if (!Number.isNaN(parsed)) return parsed;
  }

  return null;
}

function computeThreadRecencyPenalty(entries, state, canonicalFiles) {
  if (!Array.isArray(entries) || !entries.length) return 1.0;

  const loaded = state?.loaded || {};
  let freshestMs = 0;
  for (const entry of entries) {
    const file = entry?.file;
    if (!file) continue;
    // Canon-touching threads return early with full weight — no decay.
    if (canonicalFiles?.has(file)) return 1.0;
    const touchedMs = resolveLastTouchedAtMs(file, loaded[file]);
    if (touchedMs === null) continue;
    if (touchedMs > freshestMs) freshestMs = touchedMs;
  }

  if (freshestMs === 0) return 1.0; // no anchor — give benefit of the doubt

  const ageDays = (Date.now() - freshestMs) / MS_PER_DAY;
  if (ageDays <= 0) return 1.0;
  return Math.exp(-Math.LN2 * ageDays / MEMORY_RECENCY_HALF_LIFE_DAYS);
}

export function rankMemoryThreads(promptTokens, index, state = {}, canonicalFiles = new Set()) {
  const ranked = [];
  const threads = index?.threads || {};
  const filesMap = index?.files || {};

  for (const [threadKey, entries] of Object.entries(threads)) {
    const rawScore = scoreMemoryThread(promptTokens, threadKey, entries, filesMap);
    const sessionRepeatCount = sessionRepeatCountForThread(state, threadKey, entries);
    const sessionPenalty = memorySessionRepeatPenalty(sessionRepeatCount);
    const recencyPenalty = computeThreadRecencyPenalty(entries, state, canonicalFiles);
    const score = rawScore * sessionPenalty * recencyPenalty;
    if (score >= MEMORY_MIN_SCORE_TO_INJECT) {
      ranked.push({
        score, rawScore, sessionPenalty, sessionRepeatCount, recencyPenalty, threadKey, entries,
      });
    }
  }

  ranked.sort((a, b) => b.score - a.score);
  return ranked;
}

// Re-score a prior turn's prefetch queue against this turn's prompt.
export function restoreMemoryPrefetchMatches(prefetch, promptTokens, index, state, canonicalFiles) {
  if (!promptTokens?.size) return [];

  return (Array.isArray(prefetch) ? prefetch : [])
    .map((item) => {
      const threadKey = item?.thread_key || "prefetch";
      const entries = item?.entries || [];
      const rawScore = scoreMemoryThread(promptTokens, threadKey, entries, index?.files || {});
      const recencyPenalty = computeThreadRecencyPenalty(entries, state, canonicalFiles);
      return {
        score: rawScore * recencyPenalty,
        rawScore,
        recencyPenalty,
        threadKey,
        entries,
      };
    })
    .filter((match) => match.score >= MEMORY_MIN_SCORE_TO_INJECT);
}

// ── important-index matching ───────────────────────────────────────────────

export function matchMemoryTerm(promptLower, term) {
  const target = String(term || "").trim().toLowerCase();
  if (!target) return false;

  return new RegExp(`\\b${escapeRegExp(target)}\\b`, "i").test(promptLower);
}

export function matchMemoryImportantTerms(prompt, importantIndex) {
  const promptLower = String(prompt || "").toLowerCase();
  const matches = [];
  const seen = new Set();

  for (const [termKey, entry] of Object.entries(importantIndex || {})) {
    if (seen.has(termKey)) continue;
    const terms = [termKey, ...(Array.isArray(entry?.aliases) ? entry.aliases : [])];
    if (terms.some((term) => matchMemoryTerm(promptLower, term))) {
      matches.push({ termKey, entry });
      seen.add(termKey);
    }
    if (matches.length >= MEMORY_MAX_IMPORTANT_MATCHES) break;
  }

  return matches;
}

export function boostMemoryPromptTokens(promptTokens, importantMatches) {
  const boosted = new Set(promptTokens);
  for (const match of importantMatches) {
    if (Array.isArray(match.entry?.files) && match.entry.files.length) continue;
    for (const token of tokenizeMemory(match.entry?.search_boost || "")) {
      boosted.add(token);
    }
  }

  return boosted;
}


// Candidate fusion is its own pure seam; keep lexical/canon ranking readable here.
export { fuseRetrievalCandidates } from "./retrieval-candidates.ts";

// ── canon-assertion overlay (audit ticket #5) ──────────────────────────────
// importantIndex entries whose pointer_files reference any file in an active
// (top-N sync) thread match get pulled in as load-bearing CONSTRAINTS —
// regardless of whether they appeared in the user's prompt text. This is
// what the 2026-05-11 substrate-audit-ticket called the "biggest single win"
// alongside tiered retrieval: canon never gets crowded out by recent-
// dramatic content, AND canon-touching-the-current-conversation surfaces
// even when the user didn't name it.
//
// Entries already matched via prompt-text (importantMatches) are excluded
// from this overlay so they don't double-inject — they're already going to
// land in the memory-context block via their existing path.

export function collectCanonAssertions(importantIndex, syncMatches, alreadyMatchedTermKeys) {
  const activeFiles = new Set(
    (Array.isArray(syncMatches) ? syncMatches : []).flatMap((match) =>
      (Array.isArray(match?.entries) ? match.entries : [])
        .map((entry) => entry?.file)
        .filter(Boolean),
    ),
  );
  if (!activeFiles.size) return [];

  const assertions = [];
  for (const [termKey, entry] of Object.entries(importantIndex || {})) {
    if (alreadyMatchedTermKeys.has(termKey)) continue;
    const entryFiles = (Array.isArray(entry?.files) ? entry.files : [])
      .map((fileRef) => fileRef?.file)
      .filter(Boolean);
    if (!entryFiles.length) continue;
    if (!entryFiles.some((file) => activeFiles.has(file))) continue;
    assertions.push({ termKey, entry });
  }

  return assertions;
}

// ── canon reverse-index (recall): matched-file → owning-entity ─────────────
// 2026-06-05 fix. recall's canonMatches was NAME-only (matchMemoryImportantTerms
// fires on word-boundary match of an entity's name/aliases). That meant a query
// like "12 inches / be gentle" could pull the bath-scene chunk — a file that IS
// a pointer of `the protection vow` — and STILL report "0 canon entries,"
// because the query never said "the vow." The canon was right there in the
// retrieved file and the link back to the owning entity was simply never made.
//
// This reverses the index: given the source_paths of the chunks recall actually
// surfaced (semantic + content + date), find any canon entry whose pointer
// files include one of them. Deduped against the name-match termKeys so an
// entry matched both ways isn't double-counted. Path normalization mirrors
// the python side's house/ rendering prefix (load_semantic_chunks strips it;
// here we compare both bare and prefixed forms to be safe).
export function collectCanonByMatchedFiles(importantIndex, matchedSourcePaths, alreadyMatchedTermKeys) {
  const matched = new Set();
  for (const raw of Array.isArray(matchedSourcePaths) ? matchedSourcePaths : []) {
    const p = String(raw || "").trim();
    if (!p) continue;
    matched.add(p);
    // also add the house-prefix-stripped form so entity pointers (bare paths)
    // match chunks rendered with the cross-room "house/" prefix.
    if (p.startsWith(`${HOUSE_MEMORY_DIRNAME}/`)) {
      matched.add(p.slice(HOUSE_MEMORY_DIRNAME.length + 1));
    }
  }

  if (!matched.size) return [];

  const out = [];
  for (const [termKey, entry] of Object.entries(importantIndex || {})) {
    if (alreadyMatchedTermKeys?.has(termKey)) continue;
    const entryFiles = (Array.isArray(entry?.files) ? entry.files : [])
      .map((fileRef) => fileRef?.file)
      .filter(Boolean);
    if (!entryFiles.length) continue;
    if (!entryFiles.some((file) => matched.has(file))) continue;
    out.push({ termKey, entry, via: "pointer-file" });
  }

  return out;
}

// ── canon cross-reference annotation (audit ticket #3) ────────────────────
// Conservative lexical version: for each memory excerpt, scan its body for
// any canon term/alias that's already surfaced this turn (via importantMatches
// or canonAssertions). Tag the excerpt with `canon_refs: [termKey, ...]` so
// formatMemoryContextBlock can render a cross-reference hint.
//
// We don't try to detect actual semantic *disagreement* — that's a meaning
// operation, requires LLM help, out of scope for v1. The cross-ref label
// just gives the model the bridge metadata; the model itself decides
// whether canon framing wins in any specific tension.
//
// Self-references skipped per-term, not per-excerpt — an `important_index:X`
// excerpt's body IS the X entry's summary, but it can still reference OTHER
// canon entries that should be flagged.

export function annotateMemoryExcerptsWithCanonRefs(
  finalExcerpts,
  surfacedCanonTermKeys,
  importantIndex,
) {
  if (
    !Array.isArray(finalExcerpts) || !finalExcerpts.length
    || !surfacedCanonTermKeys?.size
    || !importantIndex
  ) return;

  for (const excerpt of finalExcerpts) {
    const body = String(excerpt?.text || "").toLowerCase();
    if (!body) continue;

    const source = String(excerpt?.source || "");
    const selfTermKey = source.startsWith("important_index:")
      ? source.slice("important_index:".length)
      : null;

    const refs = new Set();
    for (const termKey of surfacedCanonTermKeys) {
      if (termKey === selfTermKey) continue;
      const entry = importantIndex[termKey];
      if (!entry) continue;
      const terms = [termKey, ...(Array.isArray(entry?.aliases) ? entry.aliases : [])];
      if (terms.some((term) => matchMemoryTerm(body, term))) {
        refs.add(termKey);
      }
    }

    if (refs.size) {
      excerpt.canon_refs = Array.from(refs);
    }
  }
}

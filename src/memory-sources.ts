// Data sources for the memory pipeline: the postgres-memory-source.py
// spawn wrappers (lexical / semantic / date / content modes) and the
// JSON-index fallback used when the postgres source fails.
//
// Everything here answers one question: "fetch me raw retrieval data."
// Ranking, excerpt shaping, and formatting live elsewhere.

import path from "node:path";
import {
  HOUSE_MEMORY_DIRNAME,
  MEMORY_CONTENT_MIN_SIM, MEMORY_CONTENT_TOP_K,
  MEMORY_IMPORTANT_INDEX_FILENAME, MEMORY_INDEX_FILENAME,
  MEMORY_POSTGRES_SOURCE_SCRIPT, MEMORY_POSTGRES_TIMEOUT_MS,
  MEMORY_SEMANTIC_MIN_SIM, MEMORY_SEMANTIC_TOP_K,
} from "./paths.ts";
import { readJson } from "./util.ts";
import { runWsl, windowsPathToWsl } from "./wsl.ts";

// ── postgres source spawn ──────────────────────────────────────────────────
// Two-stage flow per audit ticket #1 (tiered retrieval):
//   Stage 1 — lexical: load index + importantIndex (Pass 1). No embed.
//   Stage 2 — semantic: embed prompt + narrowed cosine on active-thread files
//             (Pass 2). Only called after plugin has ranked Pass 1 threads.
// spawnPostgresSource is the shared spawn machinery; the wrappers below
// just build the right argv for each mode.

export async function spawnPostgresSource(roomDir, args, prompt) {
  const outcome = await runWsl({
    argv: ["python3", windowsPathToWsl(MEMORY_POSTGRES_SOURCE_SCRIPT), ...args],
    cwd: roomDir,
    // Prompt rides stdin (avoids argv-length issues on long prompts).
    stdin: String(prompt || ""),
    timeoutMs: MEMORY_POSTGRES_TIMEOUT_MS,
  });

  if (outcome.timedOut) return { ok: false, error: "postgres source timed out" };
  if (outcome.spawnError) return { ok: false, error: outcome.spawnError };
  if (outcome.code !== 0) {
    return { ok: false, error: outcome.stderr.trim() || `postgres source exited ${outcome.code}` };
  }

  try {
    return { ok: true, data: JSON.parse(outcome.stdout), stderr: outcome.stderr.trim() };
  } catch (err) {
    return { ok: false, error: err?.message || String(err) };
  }
}

function runMemoryPostgresLexical(roomDir, roomName, prompt) {
  const resolvedRoom = (roomName || path.basename(roomDir) || "").toLowerCase();
  const args = [
    "--room-dir", windowsPathToWsl(roomDir),
    "--mode", "lexical",
  ];
  if (resolvedRoom === "kodo" || resolvedRoom === "kintsu") {
    args.push("--room", resolvedRoom);
  }

  return spawnPostgresSource(roomDir, args, prompt);
}

function runMemoryPostgresSemantic(roomDir, roomName, prompt, scopeFiles) {
  const resolvedRoom = (roomName || path.basename(roomDir) || "").toLowerCase();
  const args = [
    "--room-dir", windowsPathToWsl(roomDir),
    "--mode", "semantic",
    "--semantic-top-k", String(MEMORY_SEMANTIC_TOP_K),
    "--semantic-min-sim", String(MEMORY_SEMANTIC_MIN_SIM),
  ];
  if (resolvedRoom === "kodo" || resolvedRoom === "kintsu") {
    args.push("--room", resolvedRoom);
  }
  if (Array.isArray(scopeFiles) && scopeFiles.length) {
    args.push("--scope-files", scopeFiles.join(","));
  }

  return spawnPostgresSource(roomDir, args, prompt);
}

// Date mode wrapper. Added 2026-05-23. Always GLOBAL — date hits are
// direct authoritative lookups against memories.dates GIN, the user/dragon
// literally named a date and this returns memories tagged with it. No
// embed, no scope-narrowing, cheap due to the GIN index.
function runMemoryPostgresDate(roomDir, roomName, prompt) {
  const resolvedRoom = (roomName || path.basename(roomDir) || "").toLowerCase();
  const args = [
    "--room-dir", windowsPathToWsl(roomDir),
    "--mode", "date",
  ];
  if (resolvedRoom === "kodo" || resolvedRoom === "kintsu") {
    args.push("--room", resolvedRoom);
  }

  return spawnPostgresSource(roomDir, args, prompt);
}

// Content (pg_trgm word_similarity) mode wrapper. Unlike semantic, content
// is GLOBAL by default — the whole point of this pass is to catch chunks
// the lexical thread-ranking missed entirely. Scope-files honored only when
// caller explicitly wants narrowing (matches semantic-pass behavior for
// back-compat).
function runMemoryPostgresContent(roomDir, roomName, prompt, scopeFiles) {
  const resolvedRoom = (roomName || path.basename(roomDir) || "").toLowerCase();
  const args = [
    "--room-dir", windowsPathToWsl(roomDir),
    "--mode", "content",
    "--content-top-k", String(MEMORY_CONTENT_TOP_K),
    "--content-min-sim", String(MEMORY_CONTENT_MIN_SIM),
  ];
  if (resolvedRoom === "kodo" || resolvedRoom === "kintsu") {
    args.push("--room", resolvedRoom);
  }
  if (Array.isArray(scopeFiles) && scopeFiles.length) {
    args.push("--scope-files", scopeFiles.join(","));
  }

  return spawnPostgresSource(roomDir, args, prompt);
}

// ── JSON fallback (when postgres source fails) ────────────────────────────

async function loadMemoryImportantIndex(roomDir) {
  const data = await readJson(path.join(roomDir, MEMORY_IMPORTANT_INDEX_FILENAME), null);
  return data?.entries && typeof data.entries === "object" ? data.entries : {};
}

function prefixHouseMemoryIndex(index) {
  if (!index || typeof index !== "object") return null;

  const files = {};
  for (const [filePath, meta] of Object.entries(index.files || {})) {
    files[`${HOUSE_MEMORY_DIRNAME}/${filePath}`] = meta;
  }

  const threads = {};
  for (const [threadKey, entries] of Object.entries(index.threads || {})) {
    threads[`house / ${threadKey}`] = (Array.isArray(entries) ? entries : []).map((entry) => ({
      ...entry,
      file: `${HOUSE_MEMORY_DIRNAME}/${entry?.file || ""}`,
      context: `Shared house memory. ${entry?.context || ""}`.trim(),
    }));
  }

  return { files, threads };
}

function mergeMemoryIndexes(...indexes) {
  const merged = { files: {}, threads: {} };
  for (const index of indexes) {
    if (!index) continue;
    Object.assign(merged.files, index.files || {});
    for (const [threadKey, entries] of Object.entries(index.threads || {})) {
      if (!merged.threads[threadKey]) {
        merged.threads[threadKey] = entries;
        continue;
      }
      merged.threads[threadKey] = [
        ...merged.threads[threadKey],
        ...(Array.isArray(entries) ? entries : []),
      ];
    }
  }

  return merged;
}

async function loadHouseMemoryIndex(roomDir) {
  const sharedRoot = path.dirname(roomDir);
  const houseIndexPath = path.join(sharedRoot, HOUSE_MEMORY_DIRNAME, MEMORY_INDEX_FILENAME);
  const index = await readJson(houseIndexPath, null);
  return prefixHouseMemoryIndex(index);
}

async function loadMemoryIndexFromJson(roomDir) {
  const roomIndex = await readJson(path.join(roomDir, MEMORY_INDEX_FILENAME), null);
  if (!roomIndex) return null;

  return mergeMemoryIndexes(roomIndex, await loadHouseMemoryIndex(roomDir));
}

export function preferJsonMemorySource() {
  const value = String(process.env.SOLARISAEL_MEMORY_SOURCE || "").toLowerCase();
  return value === "json" || value === "1" || value === "true"
    || process.env.SOLARISAEL_HOUSE_DISABLE_POSTGRES === "1";
}

// ── load-source wrappers (postgres first, fallback second) ─────────────────

export async function loadMemoryLexicalSources(roomDir, roomName, prompt) {
  if (preferJsonMemorySource()) {
    return {
      index: await loadMemoryIndexFromJson(roomDir),
      importantIndex: await loadMemoryImportantIndex(roomDir),
      taxonomy: null,
      indexSource: "json",
      importantSource: "json",
      fallbackReason: "forced json memory source",
    };
  }

  const fromPostgres = await runMemoryPostgresLexical(roomDir, roomName, prompt);
  if (fromPostgres.ok && fromPostgres.data?.index) {
    return {
      index: fromPostgres.data.index,
      importantIndex: fromPostgres.data.importantIndex || {},
      taxonomy: fromPostgres.data.taxonomy || null,
      indexSource: "postgres",
      importantSource: "postgres",
      fallbackReason: null,
    };
  }

  return {
    index: await loadMemoryIndexFromJson(roomDir),
    importantIndex: await loadMemoryImportantIndex(roomDir),
    taxonomy: null,
    indexSource: "json",
    importantSource: "json",
    fallbackReason: fromPostgres.error || "postgres source unavailable",
  };
}

export async function loadMemorySemanticSource(roomDir, roomName, prompt, scopeFiles) {
  // Skip the call entirely when there's nothing to embed against or no scope
  // to narrow against. The audit-ticket #1 design says semantic Pass 2 only
  // fires for files in active threads; if the prompt produced zero ranked
  // matches, there's no scope, so we skip semantic entirely (saves an embed
  // roundtrip AND keeps the off-axis "find anything kinda related" hits
  // from leaking back in via room-wide fallback).
  if (!prompt || !Array.isArray(scopeFiles) || !scopeFiles.length) {
    return {
      semanticChunks: [],
      semanticSource: "skip-no-scope",
      semanticStderr: "",
    };
  }

  if (preferJsonMemorySource()) {
    return {
      semanticChunks: [],
      semanticSource: "skip-forced-json",
      semanticStderr: "",
    };
  }

  const fromPostgres = await runMemoryPostgresSemantic(roomDir, roomName, prompt, scopeFiles);
  if (fromPostgres.ok && Array.isArray(fromPostgres.data?.semanticChunks)) {
    const chunks = fromPostgres.data.semanticChunks;
    return {
      semanticChunks: chunks,
      semanticSource: chunks.length ? "postgres-narrowed" : "postgres-empty",
      semanticStderr: fromPostgres.stderr || "",
    };
  }

  return {
    semanticChunks: [],
    semanticSource: "unavailable",
    semanticStderr: fromPostgres.error || "postgres source unavailable",
  };
}

// Date source (added 2026-05-23 — date-aware retrieval fix). Always GLOBAL,
// no embed. Returns memories whose `dates` array intersects any YYYY-MM-DD
// token extracted from the prompt. Fail-open: any failure produces an empty
// array. Skip entirely when prompt has no date tokens — the python side
// also short-circuits but skipping here avoids the WSL spawn overhead.
export async function loadMemoryDateSource(roomDir, roomName, prompt) {
  if (!prompt) {
    return {
      dateMatches: [],
      queryDates: [],
      dateSource: "skip-no-prompt",
      dateStderr: "",
    };
  }

  if (preferJsonMemorySource()) {
    return {
      dateMatches: [],
      queryDates: [],
      dateSource: "skip-forced-json",
      dateStderr: "",
    };
  }

  // Cheap pre-flight: if no YYYY-MM-DD token in the prompt, skip the spawn.
  // The python side regex is authoritative; this mirror just saves the
  // wsl.exe roundtrip when the user prompt has no date at all.
  if (!/\b\d{4}-\d{2}-\d{2}\b/.test(prompt)) {
    return {
      dateMatches: [],
      queryDates: [],
      dateSource: "skip-no-date-token",
      dateStderr: "",
    };
  }

  const fromPostgres = await runMemoryPostgresDate(roomDir, roomName, prompt);
  if (fromPostgres.ok && Array.isArray(fromPostgres.data?.dateMatches)) {
    const matches = fromPostgres.data.dateMatches;
    const queryDates = Array.isArray(fromPostgres.data?.queryDates)
      ? fromPostgres.data.queryDates
      : [];
    return {
      dateMatches: matches,
      queryDates,
      dateSource: matches.length ? "postgres-hit" : "postgres-empty",
      dateStderr: fromPostgres.stderr || "",
    };
  }

  return {
    dateMatches: [],
    queryDates: [],
    dateSource: "unavailable",
    dateStderr: fromPostgres.error || "postgres source unavailable",
  };
}

// Content source (added 2026-05-19 zeal pass). Always GLOBAL — the entire
// purpose of this pass is to catch chunks the lexical-thread-ranking missed
// (off-axis files with isolated proper-noun mentions). Cheap due to the
// pg_trgm GIN on memory_chunks.body, no embed roundtrip needed.
export async function loadMemoryContentSource(roomDir, roomName, prompt) {
  if (!prompt) {
    return {
      contentChunks: [],
      contentSource: "skip-no-prompt",
      contentStderr: "",
    };
  }

  if (preferJsonMemorySource()) {
    return {
      contentChunks: [],
      contentSource: "skip-forced-json",
      contentStderr: "",
    };
  }

  // No scopeFiles passed — content runs global.
  const fromPostgres = await runMemoryPostgresContent(roomDir, roomName, prompt, null);
  if (fromPostgres.ok && Array.isArray(fromPostgres.data?.contentChunks)) {
    const chunks = fromPostgres.data.contentChunks;
    return {
      contentChunks: chunks,
      contentSource: chunks.length ? "postgres-global" : "postgres-empty",
      contentStderr: fromPostgres.stderr || "",
    };
  }

  return {
    contentChunks: [],
    contentSource: "unavailable",
    contentStderr: fromPostgres.error || "postgres source unavailable",
  };
}

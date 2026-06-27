// Per-turn memory retrieval pipeline. The only feature in this plugin that
// substantially shapes model output. Five streams merged → deduped →
// budget-trimmed → injected as <system-reminder> on the latest user
// message.
//
//   pinned     (state.loaded[*].pinned)
//   important  (named_entities matches via regex word-boundary)
//   sync       (lexical thread ranks, top 3 by score)
//   deferred   (prior turn's prefetch queue, re-scored against this prompt)
//   semantic   (qwen3-embedding:4b cosine over memory_chunks, top 5)
//
// Split 2026-06-10 along the two natural seams:
//   memory-sources.ts — postgres spawn wrappers + JSON-index fallback
//   memory-rank.ts    — pure matching/ranking/canon-overlay logic
//   memory.ts (this)  — excerpt shaping, merge/format, state lifecycle,
//                       the orchestrator, recall, and the inject hook
//
// Fail-open: any pipeline error returns "" so the hook never blocks
// message processing. Errors ALWAYS get logged (memory/retrieval_debug.log);
// verbose per-turn diagnostics are env-gated on KINTSU_MEM_DEBUG=1. This
// asymmetric gating is the 2026-05-12 lean-rewrite fix — previously every-
// thing went through the env-gated debug logger, which meant errors
// vanished silently when the env var was unset (and it was always unset).

import { existsSync } from "node:fs";
import { appendFile, mkdir, readFile } from "node:fs/promises";
import path from "node:path";
import {
  HOUSE_MEMORY_DIRNAME,
  MEMORY_CONTENT_DEMOTION_SIM_THRESHOLD, MEMORY_CONTENT_MIN_SIM,
  MEMORY_DEBUG_LOG_FILENAME, MEMORY_DEBUG_TOP_CANDIDATES,
  MEMORY_LEXICAL_DEMOTION_SIM_THRESHOLD,
  MEMORY_MAX_EXCERPT_CHARS, MEMORY_MAX_INJECTION_CHARS,
  MEMORY_MAX_PREFETCH_QUEUE, MEMORY_MAX_SYNC_INJECTIONS,
  MEMORY_MIN_PROMPT_LEN,
  MEMORY_PREFETCH_FILENAME, MEMORY_STATE_FILENAME,
} from "./paths.ts";
import { latestUserMessage, readJson, writeJsonFile } from "./util.ts";
import { windowsPathToWsl } from "./wsl.ts";
import { normalizeRoomName, resolveEffectiveRoomDir } from "./spirit.ts";
import { loadState } from "./directives.ts";
import {
  loadMemoryContentSource, loadMemoryDateSource,
  loadMemoryLexicalSources, loadMemorySemanticSource,
  spawnPostgresSource,
} from "./memory-sources.ts";
import {
  annotateMemoryExcerptsWithCanonRefs, boostMemoryPromptTokens,
  buildCanonicalFileSet, collectCanonAssertions, collectCanonByMatchedFiles,
  matchMemoryImportantTerms, rankMemoryThreads, restoreMemoryPrefetchMatches,
  tokenizeMemory,
} from "./memory-rank.ts";

// ── debug / error logging ──────────────────────────────────────────────────

async function appendMemoryDebug(roomDir, line) {
  const target = path.join(roomDir, MEMORY_DEBUG_LOG_FILENAME);
  try {
    await mkdir(path.dirname(target), { recursive: true });
    await appendFile(target, `[${new Date().toISOString()}] ${line}\n`, "utf8");
  } catch {
    // last resort — don't let the logger itself blow up the pipeline
    console.error("[solarisael-house][memory] log-write failed:", line);
  }
}

async function debugMemoryLog(roomDir, message) {
  if (process.env.KINTSU_MEM_DEBUG !== "1") return;
  await appendMemoryDebug(roomDir, message);
}

async function errorMemoryLog(roomDir, err) {
  // Always — even with KINTSU_MEM_DEBUG unset. Errors should never silently
  // disappear; the May 12 bug-family root cause was exactly this shape.
  await appendMemoryDebug(roomDir, `ERROR ${err?.message || err}\n${err?.stack || ""}`);
}

// ── excerpt loading ────────────────────────────────────────────────────────

function resolveMemoryFilePath(roomDir, filePath) {
  const source = String(filePath || "");
  if (source === HOUSE_MEMORY_DIRNAME || source.startsWith(`${HOUSE_MEMORY_DIRNAME}/`)) {
    return path.join(path.dirname(roomDir), source);
  }

  return path.join(roomDir, source);
}

function clipMemoryExcerpt(text, maxChars = MEMORY_MAX_EXCERPT_CHARS) {
  const source = String(text || "");
  if (source.length <= maxChars) return source;
  return `${source.slice(0, maxChars - 20).trimEnd()}\n...[clipped]`;
}

async function readMemoryFileExcerpt(roomDir, filePath, lineRange = [0, 0]) {
  if (!filePath || String(filePath).startsWith("important_index:")) return null;

  const target = resolveMemoryFilePath(roomDir, filePath);
  if (!existsSync(target)) return null;

  const raw = String(await readFile(target, "utf8")).replace(/^\uFEFF/, "");
  const [startRaw, endRaw] = Array.isArray(lineRange) ? lineRange : [0, 0];
  const start = Number(startRaw) || 0;
  const end = Number(endRaw) || 0;
  if (start <= 0 || end <= 0) return raw;

  const lines = raw.split(/\r?\n/);
  return lines.slice(Math.max(0, start - 1), Math.min(lines.length, end)).join("\n");
}

async function readMemoryEntry(roomDir, entry) {
  const filePath = entry?.file || "";
  const lineRange = entry?.lines || [0, 0];
  const text = await readMemoryFileExcerpt(roomDir, filePath, lineRange);
  if (text === null) return null;

  return {
    source: `${filePath}:${lineRange[0]}-${lineRange[1]}`,
    file_path: filePath,
    text: clipMemoryExcerpt(text),
  };
}

async function collectMemoryThreadExcerpts(roomDir, match) {
  const excerpts = [];
  for (const entry of Array.isArray(match.entries) ? match.entries : []) {
    const excerpt = await readMemoryEntry(roomDir, entry);
    if (!excerpt) continue;
    excerpt.reason = `thread '${match.threadKey}' (score ${match.score.toFixed(1)})`;
    excerpts.push(excerpt);
  }

  return excerpts;
}

async function collectMemorySyncExcerpts(roomDir, matches) {
  const excerpts = [];
  for (const match of matches) {
    excerpts.push(...await collectMemoryThreadExcerpts(roomDir, match));
  }

  return excerpts;
}

async function collectMemoryPinnedExcerpts(roomDir, state, index) {
  const loaded = state?.loaded || {};
  const filesMap = index?.files || {};
  const excerpts = [];

  for (const [filePath, meta] of Object.entries(loaded)) {
    if (!meta?.pinned || String(filePath).startsWith("important_index:")) continue;
    const text = await readMemoryFileExcerpt(roomDir, filePath, [0, 0]);
    if (text === null) continue;
    const oneLine = filesMap?.[filePath]?.one_line || "";
    excerpts.push({
      source: filePath,
      file_path: filePath,
      text: clipMemoryExcerpt(text),
      reason: oneLine ? `pinned: ${oneLine.slice(0, 80)}` : "pinned",
    });
  }

  return excerpts;
}

function collectMemorySemanticExcerpts(chunks) {
  const excerpts = [];
  for (const chunk of Array.isArray(chunks) ? chunks : []) {
    const filePath = String(chunk?.source_path || "").trim();
    const body = String(chunk?.body || "").trim();
    if (!filePath || !body) continue;
    const chunkIdx = Number.isFinite(chunk?.chunk_index) ? chunk.chunk_index : 0;
    const sim = Number.isFinite(chunk?.sim) ? Number(chunk.sim) : 0;
    const heading = String(chunk?.heading_path || "").trim();
    const reason = heading
      ? `semantic match (sim ${sim.toFixed(3)}) — ${heading}`
      : `semantic match (sim ${sim.toFixed(3)})`;
    excerpts.push({
      source: `${filePath}#chunk-${chunkIdx}`,
      file_path: filePath,
      text: clipMemoryExcerpt(body),
      reason,
      sim,
    });
  }

  return excerpts;
}

// Date matches → excerpts. Added 2026-05-23 (date-aware retrieval fix).
// Distinguished from all other excerpt types because date hits are
// authoritative direct-match — when the user/dragon asks about a specific
// YYYY-MM-DD that's tagged on the memory, this IS the memory they meant.
// Surfaced HIGH (post-pinned, pre-important) so the context block leads
// with the explicit answer instead of burying it under canon/threads.
function collectMemoryDateExcerpts(dateMatches, queryDates) {
  const excerpts = [];
  const dateLabel = Array.isArray(queryDates) && queryDates.length
    ? queryDates.join(", ")
    : "(date)";

  for (const hit of Array.isArray(dateMatches) ? dateMatches : []) {
    const filePath = String(hit?.source_path || "").trim();
    const excerpt = String(hit?.body_excerpt || "").trim();
    if (!filePath || !excerpt) continue;
    const title = String(hit?.title || "").trim();
    const hitDates = Array.isArray(hit?.dates) ? hit.dates.join(",") : "";
    const totalChars = Number.isFinite(hit?.body_full_chars) ? hit.body_full_chars : 0;
    const excerptChars = excerpt.length;
    const truncated = totalChars > excerptChars
      ? ` (excerpt ${excerptChars}/${totalChars} chars)`
      : "";
    const reason = `date match for ${dateLabel}${hitDates ? ` — tagged ${hitDates}` : ""}${truncated}`;
    excerpts.push({
      source: `${filePath}#date-${(queryDates || []).join("_") || "match"}`,
      file_path: filePath,
      text: clipMemoryExcerpt(title ? `**${title}**\n\n${excerpt}` : excerpt),
      reason,
    });
  }

  return excerpts;
}

// Content (pg_trgm GIN word_similarity) chunks → excerpts. Added 2026-05-19
// zeal pass. Distinguished from semantic by:
//   - excerpt.source = `${filePath}#content-${chunkIdx}` (semantic uses #chunk-N)
//     so dedupe by source treats them as different rows from same file
//   - excerpt.reason names "content match (ws X.XXX)" instead of "semantic match"
//   - excerpt carries `ws` field instead of `sim` (word_similarity vs cosine)
// The plugin merges semantic + content into the chunk-tier of the final blob.
function collectMemoryContentExcerpts(chunks) {
  const excerpts = [];
  for (const chunk of Array.isArray(chunks) ? chunks : []) {
    const filePath = String(chunk?.source_path || "").trim();
    const body = String(chunk?.body || "").trim();
    if (!filePath || !body) continue;
    const chunkIdx = Number.isFinite(chunk?.chunk_index) ? chunk.chunk_index : 0;
    const ws = Number.isFinite(chunk?.ws) ? Number(chunk.ws) : 0;
    const heading = String(chunk?.heading_path || "").trim();
    const reason = heading
      ? `content match (ws ${ws.toFixed(3)}) — ${heading}`
      : `content match (ws ${ws.toFixed(3)})`;
    excerpts.push({
      source: `${filePath}#content-${chunkIdx}`,
      file_path: filePath,
      text: clipMemoryExcerpt(body),
      reason,
      ws,
    });
  }

  return excerpts;
}

async function collectMemoryImportantExcerpts(roomDir, matches) {
  const excerpts = [];
  for (const match of matches) {
    const summary = String(match.entry?.summary || "").trim();
    const type = match.entry?.type ? ` (${match.entry.type})` : "";
    const weighty = match.entry?.weighty ? " [weighty]" : "";
    if (summary) {
      excerpts.push({
        source: `important_index:${match.termKey}`,
        file_path: `important_index:${match.termKey}`,
        text: `**${match.termKey}**${type}${weighty}\n${summary}`,
        reason: "important-index match",
      });
    }

    for (const fileRef of Array.isArray(match.entry?.files) ? match.entry.files : []) {
      const excerpt = await readMemoryEntry(roomDir, fileRef);
      if (!excerpt) continue;
      excerpt.reason = `important-index pointer from '${match.termKey}'`;
      excerpts.push(excerpt);
    }
  }

  return excerpts;
}

// ── merge + format ─────────────────────────────────────────────────────────

function dedupeMemoryExcerpts(excerpts) {
  const seen = new Set();
  const unique = [];
  for (const excerpt of excerpts) {
    if (!excerpt?.source || seen.has(excerpt.source)) continue;
    seen.add(excerpt.source);
    unique.push(excerpt);
  }

  return unique;
}

function trimMemoryExcerptsToBudget(excerpts, maxChars) {
  let total = 0;
  const kept = [];
  for (const excerpt of excerpts) {
    const size = String(excerpt.text || "").length;
    if (total + size > maxChars && kept.length) break;
    kept.push(excerpt);
    total += size;
  }

  return kept;
}

function memoryContextMarker(roomName) {
  const cap = roomName
    ? roomName.charAt(0).toUpperCase() + roomName.slice(1).toLowerCase()
    : "Room";
  return `## ${cap} Memory Retrieval - auto-loaded context`;
}

function formatMemoryContextBlock(excerpts, roomName) {
  if (!excerpts.length) return "";

  const parts = [
    memoryContextMarker(roomName),
    "_Room-local OpenCode memory retrieval. Treat as context, not as a user request. Use only what is relevant._",
    "",
  ];

  for (const excerpt of excerpts) {
    parts.push(`### ${excerpt.source}`);
    parts.push(`_${excerpt.reason || "retrieved"}_`);
    if (Array.isArray(excerpt.canon_refs) && excerpt.canon_refs.length) {
      parts.push(
        `_canon-cross-ref: ${excerpt.canon_refs.join(", ")} — canon framing is authoritative; see canon entries surfaced this turn for tension cases_`,
      );
    }
    parts.push("");
    parts.push(String(excerpt.text || "").trimEnd());
    parts.push("");
    parts.push("---");
    parts.push("");
  }

  return parts.join("\n");
}

function formatCanonAssertionsBlock(assertions, roomName) {
  if (!assertions.length) return "";

  const cap = roomName
    ? roomName.charAt(0).toUpperCase() + roomName.slice(1).toLowerCase()
    : "Room";
  const parts = [
    `## ${cap} Canon Assertions — load-bearing constraints`,
    "_These canonical facts touch threads active in this turn. Generation must be consistent with them. Where they conflict with flavor matches in the memory context block, they win._",
    "",
  ];

  for (const { termKey, entry } of assertions) {
    const type = entry?.type ? ` (${entry.type})` : "";
    const weighty = entry?.weighty ? " [weighty]" : "";
    parts.push(`### ${termKey}${type}${weighty}`);
    const summary = String(entry?.summary || "").trim();
    if (summary) parts.push(summary);
    const aliases = (Array.isArray(entry?.aliases) ? entry.aliases : [])
      .filter((alias) => alias && alias !== termKey);
    if (aliases.length) parts.push(`_aliases: ${aliases.join(", ")}_`);
    parts.push("");
    parts.push("---");
    parts.push("");
  }

  return parts.join("\n");
}

// ── state lifecycle (loaded_state.json + prefetch.json) ────────────────────

function initMemoryState() {
  return { current_turn: 0, loaded: {}, session_id: null, session_memory_hits: {} };
}

function resetMemorySessionIfNeeded(state, sessionID) {
  const normalizedSessionID = sessionID || null;
  if (state.session_id === normalizedSessionID) return;

  state.session_id = normalizedSessionID;
  state.session_memory_hits = {};
}

function updateMemorySessionHits(state, matches = []) {
  const hits = state.session_memory_hits && typeof state.session_memory_hits === "object"
    ? state.session_memory_hits
    : {};
  state.session_memory_hits = hits;

  for (const match of matches) {
    const threadKey = match?.threadKey || "";
    if (threadKey) {
      const key = `thread:${threadKey}`;
      hits[key] = (Number(hits[key]) || 0) + 1;
    }

    for (const entry of Array.isArray(match?.entries) ? match.entries : []) {
      if (!entry?.file) continue;
      const key = `file:${entry.file}`;
      hits[key] = (Number(hits[key]) || 0) + 1;
    }
  }
}

function updateMemoryStateFreshness(state, excerpts, currentTurn) {
  const loaded = state.loaded && typeof state.loaded === "object" ? state.loaded : {};
  state.loaded = loaded;
  const nowIso = new Date().toISOString();

  for (const excerpt of excerpts) {
    const filePath = excerpt?.file_path || "";
    if (!filePath) continue;
    const entry = loaded[filePath] || {
      last_touched_turn: currentTurn,
      last_touched_at: nowIso,
      pinned: false,
      load_bearing: false,
    };
    entry.last_touched_turn = currentTurn;
    entry.last_touched_at = nowIso;
    loaded[filePath] = entry;
  }
}

function buildMemoryPrefetchQueue(matches, currentTurn) {
  return matches.map((match) => ({
    score: match.score,
    thread_key: match.threadKey,
    entries: match.entries,
    queued_at_turn: currentTurn,
  }));
}

function formatMemoryCandidateDebugLine(match, selected) {
  const files = (Array.isArray(match?.entries) ? match.entries : [])
    .map((entry) => entry?.file)
    .filter(Boolean)
    .slice(0, 3)
    .join(",");

  return [
    `selected=${selected ? "1" : "0"}`,
    `score=${Number(match?.score || 0).toFixed(2)}`,
    `raw=${Number(match?.rawScore || match?.score || 0).toFixed(2)}`,
    `repeat=${Number(match?.sessionRepeatCount || 0)}`,
    `session_penalty=${Number(match?.sessionPenalty || 1).toFixed(2)}`,
    `recency=${Number(match?.recencyPenalty || 1).toFixed(2)}`,
    `thread=${JSON.stringify(match?.threadKey || "")}`,
    files ? `files=${JSON.stringify(files)}` : "files=none",
  ].join(" ");
}

// ── prompt normalization & extraction ──────────────────────────────────────

function messageHasMemoryContext(message) {
  return (message?.parts || []).some(
    (part) =>
      part?.metadata?.solarisaelMemory === true ||
      part?.metadata?.solarisaelCanonAssertion === true,
  );
}

function stripEmbeddedMemoryContext(text) {
  return String(text || "")
    .replace(
      /<system-reminder>[\s\S]*?(?:Kintsu|Kodo|Tuner) Memory Retrieval[\s\S]*?<\/system-reminder>/gi,
      "",
    )
    .replace(/^## (?:Kintsu|Kodo|Tuner) Memory Retrieval - auto-loaded context[\s\S]*$/gim, "")
    .trim();
}

function normalizeMemoryPrompt(text) {
  return stripEmbeddedMemoryContext(text)
    .replace(/\btouch(?:ed)?\s+it\s+up\b/gi, "")
    .trim();
}

function userPromptFromMessage(message) {
  return (message?.parts || [])
    .filter((part) => part?.type === "text" && typeof part.text === "string")
    .filter((part) => part?.metadata?.solarisaelMemory !== true)
    .map((part) => normalizeMemoryPrompt(part.text))
    .filter(Boolean)
    .join("\n")
    .trim();
}

// ── core orchestrator ──────────────────────────────────────────────────────

async function runRoomMemoryRetrieval(roomName, roomDir, prompt, sessionID = null) {
  const effectiveRoomDir = path.resolve(String(roomDir || process.cwd()));
  const baseName = path.basename(effectiveRoomDir).toLowerCase();
  const targetRoom = (roomName || baseName || "").toLowerCase();

  // Sanity: only fire when cwd basename matches the requested room.
  // Prevents cross-room retrieval if caller and resolved dir disagree.
  const empty = { contextBlock: "", canonBlock: "" };
  if (!targetRoom || baseName !== targetRoom) return empty;

  if (String(prompt || "").trim().length < MEMORY_MIN_PROMPT_LEN) return empty;

  try {
    const statePath = path.join(effectiveRoomDir, MEMORY_STATE_FILENAME);
    const prefetchPath = path.join(effectiveRoomDir, MEMORY_PREFETCH_FILENAME);
    const {
      index, importantIndex, indexSource, importantSource, fallbackReason,
    } = await loadMemoryLexicalSources(effectiveRoomDir, targetRoom, prompt);

    if (!index) {
      await debugMemoryLog(effectiveRoomDir, "memory index missing or malformed; skipping");
      return empty;
    }

    const state = await readJson(statePath, null) || initMemoryState();
    resetMemorySessionIfNeeded(state, sessionID);
    state.current_turn = (Number(state.current_turn) || 0) + 1;
    const currentTurn = state.current_turn;
    const priorPrefetch = await readJson(prefetchPath, []);

    const importantMatches = matchMemoryImportantTerms(prompt, importantIndex);
    const importantExcerpts = await collectMemoryImportantExcerpts(effectiveRoomDir, importantMatches);

    // Canon-exempt set for the recency-decay computation (#2): any file
    // referenced by a named_entity's pointer_files stays at full weight
    // regardless of staleness. The canon-assertion overlay already
    // surfaces these as constraints; demoting them here would double-cut.
    const canonicalFiles = buildCanonicalFileSet(importantIndex);

    const promptTokens = boostMemoryPromptTokens(tokenizeMemory(prompt), importantMatches);
    const ranked = rankMemoryThreads(promptTokens, index, state, canonicalFiles);
    const syncMatches = ranked.slice(0, MEMORY_MAX_SYNC_INJECTIONS);
    const prefetchMatches = ranked.slice(
      MEMORY_MAX_SYNC_INJECTIONS,
      MEMORY_MAX_SYNC_INJECTIONS + MEMORY_MAX_PREFETCH_QUEUE,
    );

    // Audit ticket #1 Pass 2: narrow semantic search to files participating
    // in the active (top-N sync) threads. Pass 1 (lexical ranking) happened
    // against the already-loaded index. Now that we know which threads
    // matter for this turn, the second postgres call embeds the prompt
    // once and runs cosine ONLY over chunks of those files — instead of
    // the entire room+house corpus. This is what knocks out the off-axis
    // cosine matches that previously surfaced via room-wide search (the
    // "wolf-poem chunk fires because the prompt shares two embedding-space
    // dimensions with the wolf-poem cluster" pattern). When no active
    // files exist (zero sync matches), semantic is skipped entirely.
    const activeFiles = Array.from(new Set(
      syncMatches.flatMap((match) =>
        (Array.isArray(match?.entries) ? match.entries : [])
          .map((entry) => entry?.file)
          .filter(Boolean),
      ),
    ));
    const { semanticChunks, semanticSource, semanticStderr } =
      await loadMemorySemanticSource(effectiveRoomDir, targetRoom, prompt, activeFiles);

    // Content pass (added 2026-05-19 zeal pass) runs GLOBAL in parallel with
    // semantic. The whole reason this exists is to catch chunks the
    // lexical-thread-ranking missed (off-axis files with isolated proper-noun
    // mentions). Cheap due to the pg_trgm GIN on memory_chunks.body.
    const { contentChunks, contentSource, contentStderr } =
      await loadMemoryContentSource(effectiveRoomDir, targetRoom, prompt);

    // Date pass (added 2026-05-23 — date-aware retrieval fix). Extracts
    // YYYY-MM-DD tokens from prompt and queries memories.dates GIN. Direct
    // authoritative match: when the user names a date, this returns the
    // memories tagged with it (handles cross-midnight stitched files like
    // 2026-05-21_22_* that were invisible to the prior 3-pass pipeline).
    // Skipped entirely when prompt has no date token — no spawn overhead.
    const { dateMatches, queryDates, dateSource, dateStderr } =
      await loadMemoryDateSource(effectiveRoomDir, targetRoom, prompt);

    const pinnedExcerpts = await collectMemoryPinnedExcerpts(effectiveRoomDir, state, index);
    const rawSyncExcerpts = await collectMemorySyncExcerpts(effectiveRoomDir, syncMatches);
    const rawDeferredExcerpts = await collectMemorySyncExcerpts(
      effectiveRoomDir,
      restoreMemoryPrefetchMatches(priorPrefetch, promptTokens, index, state, canonicalFiles),
    );
    const semanticExcerpts = collectMemorySemanticExcerpts(semanticChunks);
    const contentExcerpts = collectMemoryContentExcerpts(contentChunks);
    const dateExcerpts = collectMemoryDateExcerpts(dateMatches, queryDates);

    // Audit ticket #4 (extended 2026-05-19): lexical-demotion under either
    // high-confidence semantic OR high-confidence content match. Both are
    // chunk-level precision passes; either type clearing its threshold makes
    // the lexical thread excerpt for that file redundant.
    // - Semantic threshold: MEMORY_LEXICAL_DEMOTION_SIM_THRESHOLD (cosine)
    // - Content threshold:  MEMORY_CONTENT_DEMOTION_SIM_THRESHOLD (word_sim)
    const highConfidenceChunkFiles = new Set([
      ...semanticExcerpts
        .filter((excerpt) => Number(excerpt?.sim || 0) >= MEMORY_LEXICAL_DEMOTION_SIM_THRESHOLD)
        .map((excerpt) => excerpt?.file_path)
        .filter(Boolean),
      ...contentExcerpts
        .filter((excerpt) => Number(excerpt?.ws || 0) >= MEMORY_CONTENT_DEMOTION_SIM_THRESHOLD)
        .map((excerpt) => excerpt?.file_path)
        .filter(Boolean),
    ]);

    const syncExcerpts = highConfidenceChunkFiles.size
      ? rawSyncExcerpts.filter((excerpt) => !highConfidenceChunkFiles.has(excerpt.file_path))
      : rawSyncExcerpts;
    const deferredExcerpts = highConfidenceChunkFiles.size
      ? rawDeferredExcerpts.filter((excerpt) => !highConfidenceChunkFiles.has(excerpt.file_path))
      : rawDeferredExcerpts;
    const lexicalDemoted = (rawSyncExcerpts.length - syncExcerpts.length)
      + (rawDeferredExcerpts.length - deferredExcerpts.length);

    // Canon-assertion overlay (audit ticket #5): named_entities whose
    // pointer_files touch any file in an active sync match, excluding
    // entries already pulled by prompt-text matching (they ride in the
    // memory-context block via the existing importantExcerpts path).
    const alreadyMatchedTermKeys = new Set(
      importantMatches.map((match) => match.termKey),
    );
    const canonAssertions = collectCanonAssertions(
      importantIndex, syncMatches, alreadyMatchedTermKeys,
    );

    // Order: pinned → date → important → sync (lexical) → deferred (prefetch)
    // → content → semantic. Date sits HIGH because it's a direct
    // authoritative match — when the user named a YYYY-MM-DD, that memory
    // IS what they meant. Lexical wins thread-tie because thread-keys are
    // curated; semantic adds the off-axis hits lexical can't catch.
    // Dedupe is by `source` key, so the same file showing up via thread +
    // semantic produces two rows only when the chunk range differs
    // (#chunk-N vs file:start-end). Date hits use #date-<dates> so they
    // don't collide with semantic/content of the same file.
    const merged = [
      ...pinnedExcerpts,
      ...dateExcerpts,
      ...importantExcerpts,
      ...syncExcerpts,
      ...deferredExcerpts,
      ...contentExcerpts,
      ...semanticExcerpts,
    ];
    const finalExcerpts = trimMemoryExcerptsToBudget(
      dedupeMemoryExcerpts(merged),
      MEMORY_MAX_INJECTION_CHARS,
    );

    // Audit ticket #3 (conservative lexical version): cross-reference any
    // canon term/alias that appears in a final excerpt's body. The model
    // sees inline `_canon-cross-ref:_` markers next to excerpts and can
    // defer to canon framing in any tension. Plugin doesn't decide what
    // counts as conflict — just provides the bridge metadata.
    const surfacedCanonTermKeys = new Set([
      ...importantMatches.map((match) => match.termKey),
      ...canonAssertions.map((assertion) => assertion.termKey),
    ]);
    annotateMemoryExcerptsWithCanonRefs(finalExcerpts, surfacedCanonTermKeys, importantIndex);

    // Pump + bridge (2026-06-05): touch-on-retrieval, not touch-on-survival.
    // Recency must be fed by *being pulled*, not by surviving the budget trim.
    // The old code touched only finalExcerpts (post-trim), which meant a
    // memory crushed below the sync threshold could never enter finalExcerpts,
    // never get touched, and so kept decaying on its filename date forever —
    // a sink with no pump. An old-but-relevant memory could not climb because
    // climbing required being pulled and being pulled required having climbed.
    //
    // Fix: touch every file surfaced by ANY pass this turn, BEFORE the trim.
    //   - pump:   a pull is a use; reset the clock so it's fresh next turn.
    //   - bridge: semantic/content hits touch the file too, so the cheap
    //             lexical clock HEALS — next turn the lexical path sees the
    //             file as fresh and stops crushing it. Semantic catches the
    //             old-but-relevant memory once; the touch propagates; the
    //             lexical path can then carry it without burning an embed.
    const retrievedUnion = [
      ...pinnedExcerpts,
      ...dateExcerpts,
      ...importantExcerpts,
      ...rawSyncExcerpts,
      ...rawDeferredExcerpts,
      ...contentExcerpts,
      ...semanticExcerpts,
    ];
    updateMemoryStateFreshness(state, retrievedUnion, currentTurn);
    updateMemorySessionHits(state, syncMatches);
    await writeJsonFile(statePath, state);
    await writeJsonFile(prefetchPath, buildMemoryPrefetchQueue(prefetchMatches, currentTurn));

    await debugMemoryLog(
      effectiveRoomDir,
      `turn=${currentTurn} prompt_tokens=${promptTokens.size} ranked=${ranked.length} `
        + `index_source=${indexSource} important_source=${importantSource} `
        + `semantic_source=${semanticSource || "n/a"} active_files=${activeFiles.length} `
        + `content_source=${contentSource || "n/a"} `
        + `date_source=${dateSource || "n/a"} `
        + (queryDates?.length ? `query_dates=${queryDates.join(",")} ` : "")
        + (fallbackReason ? `fallback=${fallbackReason} ` : "")
        + (semanticStderr ? `semantic_warn="${semanticStderr.replace(/"/g, "'")}" ` : "")
        + (contentStderr ? `content_warn="${contentStderr.replace(/"/g, "'")}" ` : "")
        + (dateStderr ? `date_warn="${dateStderr.replace(/"/g, "'")}" ` : "")
        + `sync=${syncExcerpts.length} important=${importantExcerpts.length} `
        + `deferred=${deferredExcerpts.length} pinned=${pinnedExcerpts.length} `
        + `semantic=${semanticExcerpts.length} content=${contentExcerpts.length} `
        + `date=${dateExcerpts.length} canon=${canonAssertions.length} `
        + `lexical_demoted=${lexicalDemoted} final=${finalExcerpts.length}`,
    );

    for (const [rank, match] of ranked.slice(0, MEMORY_DEBUG_TOP_CANDIDATES).entries()) {
      await debugMemoryLog(
        effectiveRoomDir,
        `candidate rank=${rank + 1} ${formatMemoryCandidateDebugLine(match, syncMatches.includes(match))}`,
      );
    }

    return {
      contextBlock: formatMemoryContextBlock(finalExcerpts, targetRoom),
      canonBlock: formatCanonAssertionsBlock(canonAssertions, targetRoom),
    };
  } catch (err) {
    // ALWAYS log — even with KINTSU_MEM_DEBUG unset. This is the May 12
    // lean-rewrite fix: the previous broad-catch routed through the env-
    // gated debug logger, which silently swallowed exceptions in normal use.
    await errorMemoryLog(effectiveRoomDir, err);
    return empty;
  }
}

// ── recall tool query (2026-05-19 single-writer migration) ────────────────
// Dragon-callable retrieval: same postgres source as pre-turn injection,
// invoked from a tool inside the conversation, not auto-fired on user
// prompts. Used when dragon notices its own uncertainty (name it can't
// trace, claim about to be made without verification, etc).
//
// Different posture than pre-turn:
//   - the dragon framed the question itself — no need to rank threads
//     against broad prompt; the query IS the focus
//   - return everything matched with no recency-decay (canon-touching
//     answers shouldn't be demoted for being old when *I* asked about
//     them specifically)
//   - higher top-K (8 vs 5) — the dragon is willing to read more chunks
//     when explicitly looking

export async function runRecallQuery(roomDir, roomName, query) {
  const effectiveRoomDir = path.resolve(String(roomDir || process.cwd()));
  const resolvedRoom = (roomName || path.basename(effectiveRoomDir) || "").toLowerCase();

  if (!normalizeRoomName(resolvedRoom)) {
    return { ok: false, error: `unknown room: ${resolvedRoom}`, query };
  }

  if (!query || !String(query).trim()) {
    return { ok: false, error: "empty query", query };
  }

  // Mode 'full' runs lexical + semantic + content in one postgres call. The
  // script embeds the prompt when stdin has content; we pipe `query` there.
  // Returns {index, importantIndex, semanticChunks, contentChunks}.
  //
  // Recall-specific min-sim is tighter than pre-turn (0.50 vs 0.40). The
  // dragon framed this query specifically and is going to *act* on what
  // comes back; weak matches should read as "not found" so the look-or-admit
  // discipline fires. Pre-turn casts a wider net because it's auto-firing
  // on the user's prompt and the model is the filter.
  //
  // Content threshold stays at 0.30 (the default) — word_similarity 0.30 is
  // already "noticeable substring," tightening further would miss proper-
  // noun queries like "Beel" where the dragon explicitly wants the hit.
  const RECALL_SEMANTIC_MIN_SIM = 0.50;
  const args = [
    "--room-dir", windowsPathToWsl(effectiveRoomDir),
    "--mode", "full",
    "--room", resolvedRoom,
    "--semantic-top-k", "8",
    "--semantic-min-sim", String(RECALL_SEMANTIC_MIN_SIM),
    "--content-top-k", "8",
    "--content-min-sim", String(MEMORY_CONTENT_MIN_SIM),
  ];
  const result = await spawnPostgresSource(effectiveRoomDir, args, query);
  if (!result.ok) {
    return { ok: false, error: result.error, query };
  }

  // Filter canon entries by direct word-boundary match. Anything more
  // sophisticated (thread ranking, recency decay) belongs to pre-turn —
  // recall is a dragon-driven query, the dragon already framed it,
  // just surface the matches.
  const importantIndex = result.data?.importantIndex || {};
  const nameCanonMatches = matchMemoryImportantTerms(query, importantIndex);
  const semanticChunks = Array.isArray(result.data?.semanticChunks)
    ? result.data.semanticChunks
    : [];
  const contentChunks = Array.isArray(result.data?.contentChunks)
    ? result.data.contentChunks
    : [];

  // Date matches (added 2026-05-23 — date-aware retrieval fix). When the
  // recall query contains a YYYY-MM-DD token, the postgres `--mode full`
  // call runs the date pass and returns memories whose `dates` array
  // intersects the extracted tokens. These are DIRECT authoritative hits
  // (no fuzz, no threshold) — surfaced as their own section in the recall
  // output so the dragon sees "you asked about 2026-05-21, here are the
  // memories tagged with that date" before the fuzzier semantic/content.
  const dateMatches = Array.isArray(result.data?.dateMatches)
    ? result.data.dateMatches
    : [];
  const queryDates = Array.isArray(result.data?.queryDates)
    ? result.data.queryDates
    : [];

  // Reverse-index canon (2026-06-05): surface any canon entry whose pointer
  // files include a source_path that recall actually pulled — even when the
  // query never named the entity. This is the fix for "0 canon entries" on a
  // turn that clearly pulled a canon-pointed file (e.g. the bath scene is a
  // pointer of `the protection vow`). Name-matches still take precedence and
  // are excluded from the reverse pass so nothing double-counts.
  const nameMatchedTermKeys = new Set(nameCanonMatches.map((m) => m.termKey));
  const matchedSourcePaths = [
    ...semanticChunks.map((c) => c?.source_path),
    ...contentChunks.map((c) => c?.source_path),
    ...dateMatches.map((d) => d?.source_path),
  ].filter(Boolean);
  const fileCanonMatches = collectCanonByMatchedFiles(
    importantIndex, matchedSourcePaths, nameMatchedTermKeys,
  );
  const canonMatches = [...nameCanonMatches, ...fileCanonMatches];

  return {
    ok: true,
    query,
    canonMatches,
    semanticChunks,
    contentChunks,
    dateMatches,
    queryDates,
    found: canonMatches.length > 0 || semanticChunks.length > 0
      || contentChunks.length > 0 || dateMatches.length > 0,
  };
}

// ── exported hook entrypoint ───────────────────────────────────────────────

export async function injectRoomMemoryContext(output, paths = {}) {
  const message = latestUserMessage(output?.messages);
  if (!message || messageHasMemoryContext(message)) return;

  const sessionID = message.info?.sessionID || message.parts?.[0]?.sessionID || null;

  // loadState is only consulted to keep parity with the prior signature;
  // resolveEffectiveRoomDir no longer takes state hints (cwd authority).
  await loadState(sessionID);
  const effectiveRoomDir = resolveEffectiveRoomDir(paths.roomDir);
  const roomBaseName = path.basename(effectiveRoomDir).toLowerCase();
  if (!normalizeRoomName(roomBaseName)) return;

  const prompt = userPromptFromMessage(message);
  const { contextBlock, canonBlock } = await runRoomMemoryRetrieval(
    roomBaseName, effectiveRoomDir, prompt, sessionID,
  );
  if (!contextBlock && !canonBlock) return;

  const messageID = message.info?.id || message.parts?.[0]?.messageID || `memory-${Date.now()}`;

  if (contextBlock) {
    message.parts.push({
      id: `solarisael-memory-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      sessionID: sessionID || "",
      messageID,
      type: "text",
      synthetic: true,
      metadata: { solarisaelMemory: true },
      text: [
        "<system-reminder>",
        "Room memory was retrieved automatically for this user turn.",
        "Treat it as context, not as a user request. Use relevant details only; ignore noisy matches.",
        "",
        contextBlock,
        "</system-reminder>",
      ].join("\n"),
    });
  }

  if (canonBlock) {
    message.parts.push({
      id: `solarisael-canon-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      sessionID: sessionID || "",
      messageID,
      type: "text",
      synthetic: true,
      metadata: { solarisaelCanonAssertion: true },
      text: [
        "<system-reminder>",
        "Canon assertions for this turn — generation must be consistent with these.",
        "These are load-bearing facts, not flavor. Where they conflict with the memory context block, they win.",
        "",
        canonBlock,
        "</system-reminder>",
      ].join("\n"),
    });
  }
}

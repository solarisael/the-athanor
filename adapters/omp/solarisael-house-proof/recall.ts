// Recall adapter for OMP.
// AKASHA and Vault both route through Rust. Vault uses a database-free
// filesystem method; OMP only selects the installed profile and presents it.

import { RustJsonlTransport, RustTransportError } from "../rust-transport.ts";
import { discoverRustExecutable } from "../discovery.ts";

function text(value) {
  if (value === null || value === undefined) return "";
  return String(value).trim();
}

const RECALL_VALIDATOR_SYMBOL = "validRustRecallResult";

// Semantic floor for recall, calibrated to the embedding model in use.
// Nemotron-3-Embed-1B at Q4 has a compressed cosine scale: measured
// 2026-07-25 over 3981 chunks, correct rank-1 matches land at 0.42-0.56 and
// unrelated queries top out at 0.24. The previous 0.50 discarded correct top
// hits and returned a confident empty answer, which reads to the caller as a
// true "no canonical match". Re-measure when the model or quantization
// changes; this is calibration, not taste.
const RECALL_SEMANTIC_MIN_SIM = 0.40;
// word_similarity 0.30 is already "noticeable substring". Tightening it would
// miss short proper-noun queries the caller asked for on purpose.
const RECALL_CONTENT_MIN_SIM = 0.30;

function observedShape(value) {
  if (value === null) return { type: "null" };
  if (Array.isArray(value)) return { type: "array", length: value.length };
  if (typeof value !== "object") return { type: typeof value };
  const entries = Object.keys(value).sort().slice(0, 32).map((key) => {
    const field = value[key];
    return [key, field === null ? "null" : Array.isArray(field) ? "array" : typeof field];
  });
  return {
    type: "object",
    fields: Object.fromEntries(entries),
    ...(Object.keys(value).length > entries.length ? { fields_truncated: true } : {}),
  };
}

function diagnosticDetails({ category, stage, operation, owner, expected, observed, evidence, targets, nextChecks, execution }) {
  return {
    category,
    stage,
    operation,
    owner,
    expected,
    observed,
    evidence,
    targets,
    next_checks: nextChecks,
    execution,
  };
}

function invalidRustRecallFailure(validationError, value) {
  return {
    ok: false,
    error: `invalid Rust recall result: ${validationError}`,
    code: "invalid_rust_result",
    retryable: true,
    details: diagnosticDetails({
      category: "protocol",
      stage: "validation",
      operation: "recall",
      owner: {
        component: "solarisael-house-omp",
        path: "solarisael-house-proof/recall.ts",
        symbol: RECALL_VALIDATOR_SYMBOL,
      },
      expected: { validator: RECALL_VALIDATOR_SYMBOL, result: "valid Rust recall response" },
      observed: observedShape(value),
      evidence: [{ kind: "validator_failure", symbol: RECALL_VALIDATOR_SYMBOL, reason: validationError }],
      targets: ["solarisael-house-proof/recall.ts#validRustRecallResult"],
      nextChecks: [{ action: "inspect", target: "solarisael-house-proof/recall.ts#validRustRecallResult" }],
      execution: { request_dispatched: true, write_outcome: "not_started", retry: "safe_now" },
    }),
  };
}

function boundedStderr(stderr) {
  return String(stderr || "")
    .replace(/([a-z][a-z0-9+.-]*:\/\/)[^\s@/]+@/gi, "$1[redacted]@")
    .replace(/(\b(?:token|password|authorization)\s*[=:]\s*)(?:Bearer\s+)?\S+/gi, "$1[redacted]")
    .slice(0, 2000);
}

function rustRecallFailure(error, transport) {
  const stderr = boundedStderr(error instanceof RustTransportError ? error.stderr : transport.stderrDiagnostics);
  if (error instanceof RustTransportError) {
    const details = error.details && typeof error.details === "object" && !Array.isArray(error.details)
      ? {
        ...error.details,
        ...(stderr ? {
          evidence: [
            ...(Array.isArray(error.details.evidence) ? error.details.evidence : []),
            { kind: "stderr", text: stderr },
          ],
        } : {}),
      }
      : stderr ? diagnosticDetails({
        category: "transport",
        stage: "request_parse",
        operation: "recall",
        owner: { component: "solarisael-house-omp", path: "solarisael-house-proof/recall.ts", symbol: "rustRecallFailure" },
        expected: { transport: "a structured Rust response or transport error" },
        observed: { transport_details: observedShape(error.details) },
        evidence: [{ kind: "stderr", text: stderr }],
        targets: ["rust-transport.ts#RustJsonlTransport.request"],
        nextChecks: [{ action: "inspect", target: "rust-transport.ts#RustJsonlTransport.request" }],
        execution: { request_dispatched: true, write_outcome: "not_started", retry: error.retryable ? "safe_now" : "after_change" },
      })
      : error.details;
    return { ok: false, error: error.message, code: error.code, retryable: error.retryable, ...(details === undefined ? {} : { details }) };
  }
  return {
    ok: false,
    error: "Rust transport request failed",
    code: "rust_transport_failure",
    retryable: true,
    details: diagnosticDetails({
      category: "transport",
      stage: "request_parse",
      operation: "recall",
      owner: { component: "solarisael-house-omp", path: "solarisael-house-proof/recall.ts", symbol: "rustRecallFailure" },
      expected: { transport: "a structured Rust response or transport error" },
      observed: { error_type: error instanceof Error ? error.name : typeof error },
      evidence: stderr ? [{ kind: "stderr", text: stderr }] : [],
      targets: ["rust-transport.ts#RustJsonlTransport.request"],
      nextChecks: [{ action: "inspect", target: "rust-transport.ts#RustJsonlTransport.request" }],
      execution: { request_dispatched: true, write_outcome: "not_started", retry: "safe_now" },
    }),
  };
}

function strings(value) {
  if (!Array.isArray(value)) return [];
  const out = [];
  const seen = new Set();
  for (const item of value) {
    const normalized = text(item);
    if (!normalized || seen.has(normalized)) continue;
    seen.add(normalized);
    out.push(normalized);
  }
  return out;
}

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function sourcePathKey(value) {
  return text(value).replace(/\\/g, "/").replace(/^house\//i, "").toLowerCase();
}

function matchesQueryTerm(query, term) {
  const needle = text(term).toLowerCase();
  if (!needle) return false;
  return new RegExp(`(^|[^\\p{L}\\p{N}_])${escapeRegExp(needle)}($|[^\\p{L}\\p{N}_])`, "iu").test(text(query).toLowerCase());
}

function isDirectCanonMatch(match, query) {
  return [match?.termKey, ...strings(match?.entry?.aliases)].some((term) => matchesQueryTerm(query, term));
}

function canonFiles(match) {
  return Array.isArray(match?.entry?.files)
    ? match.entry.files.map((entry) => sourcePathKey(entry?.file)).filter(Boolean)
    : [];
}

function canonTouchesCandidate(match, candidatePaths) {
  if (!candidatePaths.size) return false;
  return canonFiles(match).some((file) => candidatePaths.has(file));
}

function compactThreadNeighbors(value) {
  return Array.isArray(value)
    ? value.slice(0, 6).map((neighbor) => ({
      thread: neighbor?.thread,
      direction: neighbor?.direction,
      id: neighbor?.id,
      title: neighbor?.title,
      source_path: neighbor?.source_path,
      excerpt: text(neighbor?.excerpt).slice(0, 500),
      authority_state: neighbor?.authority_state,
      superseded_by: neighbor?.superseded_by,
    }))
    : [];
}

function compactThreadEvidence(value) {
  const memoryId = value?.memory_id;
  const threadKey = text(value?.thread_key).trim();
  const neighbors = compactThreadNeighbors(value?.thread_neighbors);

  return {
    ...(memoryId !== undefined && memoryId !== null ? { memory_id: memoryId } : {}),
    ...(threadKey ? { thread_key: threadKey } : {}),
    ...(neighbors.length ? { thread_neighbors: neighbors } : {}),
  };
}

function compactRetrievalCandidates(result) {
  return Array.isArray(result?.retrievalCandidates)
    ? result.retrievalCandidates.slice(0, 5).map((candidate) => ({
      source_path: candidate?.source_path,
      title: candidate?.title,
      heading_path: candidate?.heading_path,
      ...compactThreadEvidence(candidate),
      sources: strings(candidate?.sources).slice(0, 4),
      score: candidate?.score,
      term_coverage: candidate?.term_coverage,
      matched_terms: strings(candidate?.matched_terms).slice(0, 8),
      missing_terms: strings(candidate?.missing_terms).slice(0, 8),
      reasons: strings(candidate?.reasons).slice(0, 5),
      excerpt: text(candidate?.excerpt).slice(0, 900),
    }))
    : [];
}

function compactTaxonomy(taxonomy) {
  if (!taxonomy || typeof taxonomy !== "object") return null;
  const memoryTypes = Array.isArray(taxonomy.memoryTypes)
    ? taxonomy.memoryTypes.slice(0, 12)
    : [];
  const threadKeys = Array.isArray(taxonomy.threadKeys)
    ? taxonomy.threadKeys.slice(0, 12)
    : [];
  const namedEntities = Array.isArray(taxonomy.namedEntities)
    ? taxonomy.namedEntities.slice(0, 12)
    : [];
  if (!memoryTypes.length && !threadKeys.length && !namedEntities.length && !Array.isArray(taxonomy.fileTypes)) return null;
  return {
    rooms: Array.isArray(taxonomy.rooms) ? taxonomy.rooms : [],
    memoryTypes,
    threadKeys,
    namedEntities,
    fileTypes: Array.isArray(taxonomy.fileTypes) ? taxonomy.fileTypes.slice(0, 12) : [],
  };
}

export function compactRecall(result, { includeTaxonomy = false } = {}) {
  const retrievalCandidates = compactRetrievalCandidates(result);
  const candidatePaths = new Set(
    retrievalCandidates.map((candidate) => sourcePathKey(candidate?.source_path)).filter(Boolean),
  );
  const canonMatches = Array.isArray(result?.canonMatches)
    ? result.canonMatches
      .filter((match) => isDirectCanonMatch(match, result?.query) || canonTouchesCandidate(match, candidatePaths))
      .slice(0, 6)
      .map((m) => ({
        termKey: m?.termKey,
        type: m?.entry?.type,
        summary: m?.entry?.summary,
        files: Array.isArray(m?.entry?.files) ? m.entry.files.slice(0, 3) : [],
      }))
    : [];
  const includeRawChunks = retrievalCandidates.length === 0;
  const semanticChunks = includeRawChunks && Array.isArray(result?.semanticChunks)
    ? result.semanticChunks.slice(0, 5).map((c) => ({
      source_path: c?.source_path,
      heading_path: c?.heading_path,
      sim: c?.sim,
      body: String(c?.body || "").slice(0, 900),
    }))
    : [];
  const contentChunks = includeRawChunks && Array.isArray(result?.contentChunks)
    ? result.contentChunks.slice(0, 5).map((c) => ({
      source_path: c?.source_path,
      heading_path: c?.heading_path,
      ws: c?.ws,
      body: String(c?.body || "").slice(0, 900),
    }))
    : [];
  const dateMatches = Array.isArray(result?.dateMatches)
    ? result.dateMatches.slice(0, 5).map((d) => ({
      source_path: d?.source_path,
      title: d?.title,
      dates: d?.dates,
      body_excerpt: String(d?.body_excerpt || "").slice(0, 900),
    }))
    : [];
  const taxonomy = includeTaxonomy ? compactTaxonomy(result?.taxonomy) : null;

  // Cluster telemetry is advisory and fail-open: malformed or absent
  // substrate fields must never affect the base recall payload.
  const staleness = validClusterStaleness(result?.clusterStaleness) ? result.clusterStaleness : null;
  const clusterNudge = staleness && (staleness.built_at === null || staleness.fraction_unseen >= 0.15)
    ? `clusters: ${staleness.built_at === null ? "never built" : `built ${staleness.built_at.slice(0, 10)}`}, `
      + `${staleness.chunks_since_build} chunks since (${Math.round(staleness.fraction_unseen * 100)}% of corpus unseen) — `
      + `wanna do clusters, dummies? (house/substrate/rebuild_clusters.py)`
    : null;

  // Resonance is similarly advisory. Keep the existing eight-profile/three-hot
  // bounds, but never serialize partially malformed telemetry.
  const resonance = validClusterResonance(result?.clusterResonance)
    ? {
      note: "substrate resonance: what the memory space finds near this query — telemetry, not model-internal state",
      profile: result.clusterResonance.profile.slice(0, 8).map((p) => ({
        label: p.label,
        activation: p.activation,
        members: p.member_count,
      })),
      dormantHot: result.clusterResonance.hot.slice(0, 3),
    }
    : null;

  return {
    ok: Boolean(result?.ok),
    query: result?.query,
    found: Boolean(result?.found),
    source: result?.source,
    ...(result?.source === "vault-files" ? {
      vault: {
        authority: result?.authority,
        roots: strings(result?.roots).slice(0, 8),
        scannedFiles: Number.isInteger(result?.scannedFiles) ? result.scannedFiles : 0,
        indexedDocuments: Number.isInteger(result?.indexedDocuments) ? result.indexedDocuments : 0,
      },
    } : {}),
    // Substrate self-diagnostics (recall.rs RecallResult.warnings) carry the
    // reason a lane went absent, e.g. "semantic lane absent: embedding
    // disabled". Dropping them turns a one-call diagnosis into a silent empty
    // array. Fail-open: omitted entirely when the substrate reports nothing.
    ...(Array.isArray(result?.warnings) && result.warnings.length
      ? { warnings: result.warnings.slice(0, 8).map((w) => String(w).slice(0, 300)) }
      : {}),
    canonMatches,
    retrievalCandidates,
    semanticChunks,
    contentChunks,
    dateMatches,
    queryDates: Array.isArray(result?.queryDates) ? result.queryDates : [],
    ...(taxonomy ? { taxonomy } : {}),
    ...(clusterNudge ? { clusterNudge } : {}),
    ...(resonance ? { clusterResonance: resonance } : {}),
    ...(result?.memoryHandle ? {
      memoryHandle: {
        ...result.memoryHandle,
        memory: result.memoryHandle.memory
          ? { ...result.memoryHandle.memory, body: String(result.memoryHandle.memory.body || "").slice(0, 6000) }
          : null,
      },
    } : {}),
  };
}

function validClusterStaleness(value) {
  const builtAtValid = value?.built_at === null
    || (typeof value?.built_at === "string" && Number.isFinite(Date.parse(value.built_at)));
  return value && typeof value === "object" && !Array.isArray(value)
    && builtAtValid
    && Number.isInteger(value.chunks_since_build) && value.chunks_since_build >= 0
    && Number.isFinite(value.fraction_unseen) && value.fraction_unseen >= 0 && value.fraction_unseen <= 1;
}

function validClusterHot(value) {
  if (typeof value === "string") return value.length > 0;
  return value && typeof value === "object" && !Array.isArray(value)
    && Number.isInteger(value.cluster_id) && value.cluster_id >= 0
    && typeof value.label === "string"
    && Array.isArray(value.chunks)
    && value.chunks.every((chunk) => chunk && typeof chunk === "object" && !Array.isArray(chunk)
      && typeof chunk.source_path === "string"
      && (chunk.heading_path === null || typeof chunk.heading_path === "string")
      && Number.isFinite(chunk.sim) && chunk.sim >= -1 && chunk.sim <= 1);
}

function validClusterResonance(value) {
  return value && typeof value === "object" && !Array.isArray(value)
    && Array.isArray(value.profile) && value.profile.length > 0
    && value.profile.every((entry) => entry && typeof entry === "object" && !Array.isArray(entry)
      && typeof entry.label === "string" && entry.label.length > 0
      && Number.isFinite(entry.activation) && entry.activation >= -1 && entry.activation <= 1
      && Number.isInteger(entry.member_count) && entry.member_count >= 0)
    && Array.isArray(value.hot) && value.hot.every(validClusterHot);
}


const rustRecallTransports = new Map();

function rustRecallTransport() {
  const executable = discoverRustExecutable();
  if (!executable) return null;
  let transport = rustRecallTransports.get(executable);
  if (!transport) {
    transport = new RustJsonlTransport({ executable });
    rustRecallTransports.set(executable, transport);
  }
  return transport;
}

function evictRustRecallTransport(executable, transport) {
  if (rustRecallTransports.get(executable) !== transport) return;
  rustRecallTransports.delete(executable);
  void transport.close().catch(() => {});
}

export function closeRustRecallTransports() {
  for (const [executable, transport] of rustRecallTransports) {
    rustRecallTransports.delete(executable);
    void transport.close().catch(() => {});
  }
}

const RECALL_TIMEOUT_MS = 120_000;

function stringArray(value) {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function validRustRecallCandidate(candidate) {
  return candidate && typeof candidate === "object" && !Array.isArray(candidate)
    && typeof candidate.source_path === "string"
    && typeof candidate.title === "string"
    && typeof candidate.heading_path === "string"
    && typeof candidate.excerpt === "string"
    && stringArray(candidate.sources)
    && Number.isFinite(candidate.score)
    && Number.isFinite(candidate.term_coverage)
    && stringArray(candidate.matched_terms)
    && stringArray(candidate.missing_terms)
    && stringArray(candidate.reasons);
}

function validRustRecallDateMatch(dateMatch) {
  return dateMatch && typeof dateMatch === "object" && !Array.isArray(dateMatch)
    && typeof dateMatch.body_excerpt === "string";
}

function validRustRecallResult(value, query) {
  if (!value || typeof value !== "object" || Array.isArray(value)) return "result must be an object";
  const result = value;
  if (result.ok !== true || result.query !== query || typeof result.found !== "boolean" || typeof result.source !== "string") {
    return "result has invalid ok, query, found, or source";
  }
  for (const field of ["retrievalCandidates", "canonMatches", "semanticChunks", "contentChunks", "dateMatches", "queryDates"]) {
    if (!Array.isArray(result[field])) return `result.${field} must be an array`;
  }
  if (result.warnings !== undefined && !stringArray(result.warnings)) {
    return "result.warnings must be an array of strings";
  }
  if (result.dateMatches.length > 5) return "result.dateMatches must contain at most 5 entries";
  if (!result.taxonomy || typeof result.taxonomy !== "object" || Array.isArray(result.taxonomy)) {
    return "result.taxonomy must be an object";
  }
  if (!result.retrievalCandidates.every(validRustRecallCandidate)) {
    return "result.retrievalCandidates entries must contain the exact compactor candidate fields";
  }
  if (!result.dateMatches.every(validRustRecallDateMatch)) {
    return "result.dateMatches entries must contain a string body_excerpt";
  }
  return null;
}


function stripInvalidClusterTelemetry(result) {
  if (!result || typeof result !== "object") return result;
  const safe = { ...result };
  if (safe.clusterStaleness !== undefined && !validClusterStaleness(safe.clusterStaleness)) {
    delete safe.clusterStaleness;
  }
  if (safe.clusterResonance !== undefined && !validClusterResonance(safe.clusterResonance)) {
    delete safe.clusterResonance;
  }
  return safe;
}

function temporalDecayUnsupported(error) {
  return error instanceof RustTransportError
    && error.code === "invalid_params"
    && error.message.includes("temporal_decay");
}

export async function recallWithRouting(effectiveRoomDir, room, query, { signal, temporalDecay = false } = {}) {
  const executable = discoverRustExecutable();
  const transport = rustRecallTransport();
  if (!transport) {
    return {
      ok: false,
      result: {
        ok: false,
        query,
        error: "Rust Vault/AKASHA runtime is unavailable",
        code: "rust_runtime_unavailable",
        retryable: false,
      },
    };
  }
  const vaultProfile = !String(process.env.ATHANOR_SUBSTRATE_ROOT || "").trim();
  const baseParams = vaultProfile
    ? { room, room_dir: effectiveRoomDir, query }
    : {
      room,
      query,
      semantic_top_k: 8,
      semantic_min_similarity: RECALL_SEMANTIC_MIN_SIM,
      content_top_k: 8,
      content_min_similarity: RECALL_CONTENT_MIN_SIM,
    };
  const params = temporalDecay && !vaultProfile ? { ...baseParams, temporal_decay: true } : baseParams;
  const requestOptions = { signal, timeoutMs: RECALL_TIMEOUT_MS };
  try {
    let result;
    try {
      result = await transport.request(vaultProfile ? "vault_recall" : "recall", params, requestOptions);
    } catch (error) {
      if (vaultProfile || !temporalDecay || !temporalDecayUnsupported(error)) throw error;
      result = await transport.request("recall", baseParams, requestOptions);
    }
    const validationError = validRustRecallResult(result, query);
    if (validationError) {
      evictRustRecallTransport(executable, transport);
      return { ok: false, result: { query, ...invalidRustRecallFailure(validationError, result) } };
    }
    return { ok: true, result: stripInvalidClusterTelemetry(result) };
  } catch (error) {
    if (!transport.usable) evictRustRecallTransport(executable, transport);
    return { ok: false, result: { ok: false, query, ...rustRecallFailure(error, transport) } };
  }
}


import { MEMORY_STOPWORDS, MEMORY_TOKEN_RE } from "./paths.ts";

// Pure retrieval-candidate contract. This module owns the cross-lane ranking
// seam so `memory-rank.ts` can stay focused on lexical/canon ranking.

const DEFAULT_RRF_K = 60;
const DEFAULT_RRF_WEIGHT = 30;
const DEFAULT_MAX_RESULTS = 12;

const SOURCE_PRIOR = Object.freeze({
  date: 4.0,
  entity: 3.4,
  coding_lesson: 3.0,
  project_lesson: 3.0,
  thread: 2.4,
  memory: 2.0,
  content: 1.8,
  semantic: 1.4,
});

const FIELD_BOOST = Object.freeze({
  title: 1.2,
  path: 0.8,
  heading: 0.5,
  excerpt: 0.2,
  exact: 2.0,
  sourceCount: 0.6,
  broadMemoryPenalty: 1.4,
});

const CANDIDATE_QUERY_STOPWORDS = new Set([
  "alright",
  "also",
  "basically",
  "could",
  "enabled",
  "even",
  "good",
  "guess",
  "into",
  "just",
  "kinda",
  "like",
  "looksie",
  "maybe",
  "much",
  "need",
  "our",
  "pretty",
  "probably",
  "really",
  "sample",
  "see",
  "seems",
  "should",
  "size",
  "stuff",
  "thing",
  "things",
  "think",
  "tool",
  "try",
  "use",
  "wanna",
  "want",
  "well",
  "without",
  "working",
  "would",
]);

const TECHNICAL_MEMORY_TERMS = new Set([
  "adapter",
  "bm25",
  "candidate",
  "candidates",
  "content",
  "debug",
  "embedding",
  "embeddings",
  "fusion",
  "index",
  "lexical",
  "memory",
  "parsing",
  "pgvector",
  "postgres",
  "query",
  "rank",
  "ranking",
  "recall",
  "retrieval",
  "routing",
  "semantic",
  "term",
  "terms",
]);

const PERSONAL_CANON_KINDS = new Set(["memory", "person"]);
const TECHNICAL_CANON_KINDS = new Set(["meta", "project"]);

function text(value) {
  if (value === null || value === undefined) return "";
  return String(value).trim();
}

function finiteNumber(value, fallback = 0) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function roundScore(value) {
  return Math.round(finiteNumber(value) * 10000) / 10000;
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

function pushUnique(list, value) {
  const normalized = text(value);
  if (normalized && !list.includes(normalized)) list.push(normalized);
}

function cleanTerm(value) {
  const term = text(value)
    .toLowerCase()
    .replace(/^[._:/+#-]+|[._:/+#-]+$/g, "");

  if (term.length <= 1 || MEMORY_STOPWORDS.has(term) || CANDIDATE_QUERY_STOPWORDS.has(term)) return "";
  return term;
}

function classifyQueryIntent(terms, query) {
  const haystack = `${terms.join(" ")} ${text(query).toLowerCase()}`;
  const technicalHits = terms.filter((term) => TECHNICAL_MEMORY_TERMS.has(term)).length;
  if (technicalHits >= 2 || /\b(candidate|retrieval|recall|embedding|semantic|pgvector|postgres|query)\b/.test(haystack)) {
    return "technical_memory";
  }

  return "general";
}

function termsFrom(query, explicitTerms, laneArrays) {
  const terms = [];
  const seen = new Set();

  const add = (value) => {
    const term = cleanTerm(value);
    if (!term || seen.has(term)) return;

    seen.add(term);
    terms.push(term);
  };

  const addWithParts = (value) => {
    add(value);
    for (const part of text(value).split(/[-_./:+#]+/)) add(part);
  };

  for (const term of strings(explicitTerms)) addWithParts(term);

  if (!terms.length) {
    const queryTokens = text(query).toLowerCase().match(MEMORY_TOKEN_RE) || [];
    for (const token of queryTokens) addWithParts(token);
  }

  if (!terms.length) {
    for (const lane of laneArrays) {
      for (const candidate of Array.isArray(lane) ? lane : []) {
        for (const term of strings(candidate?.matched_terms)) addWithParts(term);
        for (const term of strings(candidate?.missing_terms)) addWithParts(term);
      }
    }
  }

  return terms.slice(0, 16);
}

function termHits(terms, value) {
  const haystack = text(value).toLowerCase();
  if (!haystack) return [];

  return terms.filter((term) => haystack.includes(term));
}

function fieldEvidence(terms, fields, query) {
  const title = termHits(terms, fields.title);
  const path = termHits(terms, fields.source_path);
  const heading = termHits(terms, fields.heading_path);
  const excerpt = termHits(terms, fields.excerpt);
  const matched_terms = strings([...title, ...path, ...heading, ...excerpt]);

  const exactNeedle = text(query).toLowerCase();
  const exact_match = exactNeedle.length > 1 && [
    fields.title,
    fields.source_path,
    fields.heading_path,
  ].some((field) => {
    const haystack = text(field).toLowerCase();
    return haystack === exactNeedle || haystack.includes(exactNeedle);
  });

  return { title, path, heading, excerpt, matched_terms, exact_match };
}

function laneCandidate(item, lane, rank, terms, options) {
  if (!item || typeof item !== "object") return null;

  const candidate = {
    id: "",
    source: lane,
    lane,
    source_table: "",
    source_id: "",
    room: "",
    title: "",
    source_path: "",
    heading_path: "",
    excerpt: "",
    raw_score: 0,
    matched_terms: [],
    reasons: [],
    kind: "",
    weighty: false,
  };

  if (lane === "search") {
    candidate.id = text(item.id);
    candidate.source = text(item.source) || "search";
    candidate.source_table = text(item.source_table);
    candidate.source_id = text(item.source_id);
    candidate.room = text(item.room);
    candidate.title = text(item.title);
    candidate.source_path = text(item.source_path);
    candidate.heading_path = text(item.heading_path);
    candidate.excerpt = text(item.excerpt);
    candidate.raw_score = finiteNumber(item.score, finiteNumber(item.raw_rank));
    candidate.matched_terms = strings(item.matched_terms).map(cleanTerm).filter(Boolean);
    candidate.reasons = strings(item.reasons);
    candidate.kind = text(item.kind);
    candidate.weighty = Boolean(item.weighty);
  } else if (lane === "semantic") {
    candidate.source = "semantic";
    candidate.room = text(item.room);
    candidate.source_path = text(item.source_path);
    candidate.heading_path = text(item.heading_path);
    candidate.title = candidate.heading_path || candidate.source_path;
    candidate.excerpt = text(item.body);
    candidate.raw_score = finiteNumber(item.sim) * 5;
    candidate.reasons = strings(item.reasons);
    pushUnique(candidate.reasons, `semantic match (sim ${finiteNumber(item.sim).toFixed(3)})`);
  } else if (lane === "content") {
    candidate.source = "content";
    candidate.room = text(item.room);
    candidate.source_path = text(item.source_path);
    candidate.heading_path = text(item.heading_path);
    candidate.title = candidate.heading_path || candidate.source_path;
    candidate.excerpt = text(item.body);
    candidate.raw_score = finiteNumber(item.ws) * 6;
    candidate.reasons = strings(item.reasons);
    pushUnique(candidate.reasons, `content match (ws ${finiteNumber(item.ws).toFixed(3)})`);
  } else if (lane === "date") {
    candidate.source = "date";
    candidate.source_table = "memories";
    candidate.source_id = text(item.id);
    candidate.room = text(item.room);
    candidate.title = text(item.title);
    candidate.source_path = text(item.source_path);
    candidate.excerpt = text(item.body_excerpt);
    candidate.raw_score = 6;
    candidate.reasons = strings(item.reasons);

    const dates = strings(item.dates);
    pushUnique(candidate.reasons, `date match${dates.length ? ` (${dates.join(", ")})` : ""}`);
  }

  if (!candidate.reasons.length) candidate.reasons = [candidate.source];

  const evidence = fieldEvidence(terms, candidate, options.query);
  const matched_terms = strings([...candidate.matched_terms, ...evidence.matched_terms]);
  const term_coverage = terms.length ? matched_terms.length / terms.length : 0;
  const scoreParts = scoreCandidate(candidate, evidence, term_coverage, rank, options);

  return {
    ...candidate,
    key: candidateKey(candidate),
    sources: [candidate.source],
    lanes: [lane],
    matched_terms,
    missing_terms: terms.filter((term) => !matched_terms.includes(term)),
    term_coverage: roundScore(term_coverage),
    exact_match: evidence.exact_match,
    score: scoreParts.score,
    best_lane_score: scoreParts.score,
    raw_score: roundScore(candidate.raw_score),
    rrf_score: scoreParts.rrf_score,
    source_prior: scoreParts.source_prior,
  };
}

function scoreCandidate(candidate, evidence, termCoverage, rank, options) {
  const sourcePrior = {
    ...SOURCE_PRIOR,
    ...(options.sourcePriors || {}),
  };
  const fieldBoost = {
    ...FIELD_BOOST,
    ...(options.fieldBoosts || {}),
  };

  const prior = finiteNumber(sourcePrior[candidate.source], finiteNumber(sourcePrior[candidate.lane], 1));
  const rrfK = Math.max(1, finiteNumber(options.rrfK, DEFAULT_RRF_K));
  const rrfWeight = finiteNumber(options.rrfWeight, DEFAULT_RRF_WEIGHT);
  const rrf_score = rrfWeight / (rrfK + Math.max(1, rank));
  const field_score = (evidence.title.length ? fieldBoost.title : 0)
    + (evidence.path.length ? fieldBoost.path : 0)
    + (evidence.heading.length ? fieldBoost.heading : 0)
    + (evidence.excerpt.length ? fieldBoost.excerpt : 0);
  const exact_score = evidence.exact_match ? finiteNumber(fieldBoost.exact) : 0;

  let broadPenalty = 0;
  if (["memory", "content", "semantic"].includes(candidate.source)
    && !evidence.exact_match
    && !evidence.title.length
    && !evidence.path.length
    && termCoverage < 0.5) {
    broadPenalty = finiteNumber(fieldBoost.broadMemoryPenalty);
  }

  const intent = options.intent || "general";
  let weightyAdjustment = 0;
  if (candidate.weighty) {
    if (evidence.exact_match) {
      weightyAdjustment = 1.2;
    } else if (intent === "technical_memory" && PERSONAL_CANON_KINDS.has(candidate.kind)) {
      weightyAdjustment = -1.8;
    } else if (intent === "technical_memory" && TECHNICAL_CANON_KINDS.has(candidate.kind)) {
      weightyAdjustment = 0.6;
    } else {
      weightyAdjustment = 0.4;
    }
  }

  const score = prior
    + candidate.raw_score
    + rrf_score
    + (termCoverage * 6)
    + field_score
    + exact_score
    + weightyAdjustment
    - broadPenalty;
  return {
    score,
    rrf_score: roundScore(rrf_score),
    source_prior: roundScore(prior),
  };
}

function candidateKey(candidate) {
  if (candidate.source_path) return `path:${candidate.source_path}`;
  if (candidate.id) return `id:${candidate.id}`;
  if (candidate.source_table || candidate.source_id) {
    return `source:${candidate.source_table || candidate.source}:${candidate.source_id}`;
  }

  return `anon:${candidate.source}:${candidate.title}:${candidate.heading_path}`;
}

function mergeCandidateEvidence(existing, incoming) {
  existing.score += incoming.score;
  existing.raw_score = roundScore(existing.raw_score + incoming.raw_score);
  existing.rrf_score = roundScore(existing.rrf_score + incoming.rrf_score);
  existing.source_prior = roundScore(existing.source_prior + incoming.source_prior);
  existing.exact_match = existing.exact_match || incoming.exact_match;

  for (const source of incoming.sources) pushUnique(existing.sources, source);
  for (const lane of incoming.lanes) pushUnique(existing.lanes, lane);
  for (const reason of incoming.reasons) pushUnique(existing.reasons, reason);
  for (const term of incoming.matched_terms) pushUnique(existing.matched_terms, term);

  if (incoming.best_lane_score <= existing.best_lane_score) return;

  existing.best_lane_score = incoming.best_lane_score;
  existing.id = incoming.id || existing.id;
  existing.source = incoming.source;
  existing.source_table = incoming.source_table || existing.source_table;
  existing.source_id = incoming.source_id || existing.source_id;
  existing.room = incoming.room || existing.room;
  existing.title = incoming.title || existing.title;
  existing.heading_path = incoming.heading_path || existing.heading_path;
  existing.excerpt = incoming.excerpt || existing.excerpt;
  existing.source_path = incoming.source_path || existing.source_path;
}

function finalizeCandidate(candidate, terms, options) {
  const fieldBoost = {
    ...FIELD_BOOST,
    ...(options.fieldBoosts || {}),
  };
  const matched_terms = strings(candidate.matched_terms);
  const term_coverage = terms.length ? matched_terms.length / terms.length : 0;
  const source_count_score = Math.max(0, candidate.sources.length - 1)
    * finiteNumber(fieldBoost.sourceCount);

  return {
    id: candidate.id,
    source: candidate.source,
    sources: candidate.sources,
    lanes: candidate.lanes,
    source_table: candidate.source_table,
    source_id: candidate.source_id,
    room: candidate.room,
    title: candidate.title,
    source_path: candidate.source_path,
    heading_path: candidate.heading_path,
    excerpt: candidate.excerpt,
    score: roundScore(candidate.score + source_count_score),
    raw_score: roundScore(candidate.raw_score),
    rrf_score: roundScore(candidate.rrf_score),
    source_prior: roundScore(candidate.source_prior),
    term_coverage: roundScore(term_coverage),
    matched_terms,
    missing_terms: terms.filter((term) => !matched_terms.includes(term)),
    exact_match: candidate.exact_match,
    kind: candidate.kind,
    weighty: candidate.weighty,
    reasons: strings(candidate.reasons),
  };
}

function capCandidateDiversity(candidates, options) {
  const maxPerSourcePath = finiteNumber(options.maxPerSourcePath, Infinity);
  const maxPerSource = finiteNumber(options.maxPerSourceType, finiteNumber(options.maxPerSource, Infinity));
  if (!Number.isFinite(maxPerSourcePath) && !Number.isFinite(maxPerSource)) return candidates;

  const pathCounts = new Map();
  const sourceCounts = new Map();
  const kept = [];

  for (const candidate of candidates) {
    const pathKey = candidate.source_path;
    const sourceKey = candidate.source;

    if (pathKey && Number.isFinite(maxPerSourcePath)) {
      const pathCount = pathCounts.get(pathKey) || 0;
      if (pathCount >= maxPerSourcePath) continue;
      pathCounts.set(pathKey, pathCount + 1);
    }

    if (sourceKey && Number.isFinite(maxPerSource)) {
      const sourceCount = sourceCounts.get(sourceKey) || 0;
      if (sourceCount >= maxPerSource) continue;
      sourceCounts.set(sourceKey, sourceCount + 1);
    }

    kept.push(candidate);
  }

  return kept;
}

export function fuseRetrievalCandidates(lanes = {}, options = {}) {
  const laneSets = Array.isArray(lanes) ? { searchCandidates: lanes } : (lanes || {});
  const searchCandidates = Array.isArray(laneSets.searchCandidates || laneSets.candidates)
    ? laneSets.searchCandidates || laneSets.candidates
    : [];
  const semanticChunks = Array.isArray(laneSets.semanticChunks) ? laneSets.semanticChunks : [];
  const contentChunks = Array.isArray(laneSets.contentChunks) ? laneSets.contentChunks : [];
  const dateMatches = Array.isArray(laneSets.dateMatches) ? laneSets.dateMatches : [];
  const terms = termsFrom(options.query, options.searchTerms, [
    searchCandidates,
    semanticChunks,
    contentChunks,
    dateMatches,
  ]);
  const intent = options.intent || classifyQueryIntent(terms, options.query);
  const scoringOptions = { ...options, intent };

  const normalized = [
    ...searchCandidates.map((item, index) => laneCandidate(item, "search", index + 1, terms, scoringOptions)),
    ...semanticChunks.map((item, index) => laneCandidate(item, "semantic", index + 1, terms, scoringOptions)),
    ...contentChunks.map((item, index) => laneCandidate(item, "content", index + 1, terms, scoringOptions)),
    ...dateMatches.map((item, index) => laneCandidate(item, "date", index + 1, terms, scoringOptions)),
  ].filter(Boolean);

  const byKey = new Map();
  for (const candidate of normalized) {
    const existing = byKey.get(candidate.key);
    if (existing) mergeCandidateEvidence(existing, candidate);
    else byKey.set(candidate.key, candidate);
  }

  const fused = Array.from(byKey.values())
    .map((candidate) => finalizeCandidate(candidate, terms, scoringOptions))
    .sort((a, b) => (
      b.score - a.score
      || b.term_coverage - a.term_coverage
      || b.sources.length - a.sources.length
      || text(a.source_path || a.title).localeCompare(text(b.source_path || b.title))
    ));
  const diversified = capCandidateDiversity(fused, options);
  const maxResults = Math.max(0, finiteNumber(options.maxResults, finiteNumber(options.topK, DEFAULT_MAX_RESULTS)));

  return diversified.slice(0, maxResults);
}

use house_protocol::{
    RecallCandidate, RecallCanonMatch, RecallPresentation, RecallPresentationCandidate,
    RecallPresentationCanonMatch, RecallPresentationClusterProfile,
    RecallPresentationClusterResonance, RecallPresentationDateMatch,
    RecallPresentationMemoryHandle, RecallPresentationMemoryRecord, RecallPresentationRawChunk,
    RecallPresentationTaxonomy, RecallPresentationVault, RecallResultInput,
    RecallViewportDiagnostics, RecallViewportMode, RecallViewportResult, RecallViewportSuppression,
};
use std::collections::{HashMap, HashSet};

const DEFAULT_GLUE_TERMS: &[&str] = &[
    "a",
    "an",
    "and",
    "are",
    "as",
    "assignment",
    "at",
    "be",
    "below",
    "by",
    "can",
    "change",
    "create",
    "do",
    "for",
    "from",
    "how",
    "i",
    "in",
    "is",
    "it",
    "me",
    "my",
    "of",
    "on",
    "only",
    "or",
    "please",
    "read",
    "restart",
    "restarted",
    "review",
    "show",
    "target",
    "that",
    "the",
    "this",
    "to",
    "we",
    "what",
    "when",
    "where",
    "which",
    "with",
    "wonder",
    "work",
    "works",
    "you",
    "your",
];
const NON_DISTINCTIVE_EXACT_TERMS: &[&str] = &[
    "before",
    "change",
    "restarted",
    "review",
    "same",
    "tool",
    "tools",
    "wonder",
    "work",
    "works",
];

#[derive(Debug, Default)]
pub struct ViewportSession {
    exposures: HashMap<String, u64>,
    last_nudge_band: u64,
}

impl ViewportSession {
    pub fn last_nudge_band(&self) -> u64 {
        self.last_nudge_band
    }

    pub fn set_last_nudge_band(&mut self, band: u64) {
        self.last_nudge_band = self.last_nudge_band.max(band);
    }
}

fn bounded(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn path_key(value: &str) -> String {
    let normalized = key(value).replace('\\', "/");
    normalized
        .strip_prefix("house/")
        .unwrap_or(&normalized)
        .to_owned()
}

fn contains_term(value: &str, term: &str) -> bool {
    let haystack = key(value);
    let needle = key(term);
    needle.len() >= 2 && (haystack == needle || haystack.contains(&needle))
}

fn distinctive_exact_term(value: &str) -> bool {
    let term = key(value);
    term.len() >= 7 && !NON_DISTINCTIVE_EXACT_TERMS.contains(&term.as_str())
}

fn candidate_identity(candidate: &RecallCandidate, index: usize) -> String {
    if let Some(explicit) = candidate
        .id
        .as_deref()
        .or(candidate.identity.as_deref())
        .or(candidate.candidate_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return explicit.to_owned();
    }
    let path = path_key(&candidate.source_path);
    let title = key(&candidate.title);
    if path.is_empty() && title.is_empty() {
        format!("candidate:{index}")
    } else {
        format!("{path}|{title}")
    }
}

fn canon_file_paths(matches: &[RecallCanonMatch]) -> Vec<String> {
    matches
        .iter()
        .flat_map(|entry| entry.entry.files.iter())
        .map(|file| path_key(&file.file))
        .filter(|path| !path.is_empty())
        .collect()
}

fn date_paths(result: &RecallResultInput) -> Vec<String> {
    result
        .date_matches
        .iter()
        .map(|entry| path_key(&entry.source_path))
        .filter(|path| !path.is_empty())
        .collect()
}

fn exact_signals(
    candidate: &RecallCandidate,
    query: &str,
    canon_files: &[String],
    canon_terms: &[String],
    dates: &[String],
) -> HashSet<&'static str> {
    let sources = candidate
        .sources
        .iter()
        .map(|value| key(value))
        .collect::<Vec<_>>();
    let reasons = candidate
        .reasons
        .iter()
        .map(|value| key(value))
        .collect::<Vec<_>>();
    let path = path_key(&candidate.source_path);
    let mut signals = HashSet::new();
    if sources.iter().any(|value| value.contains("canon"))
        || reasons.iter().any(|value| value.contains("canon"))
        || canon_files.iter().any(|value| value == &path)
        || canon_terms.iter().any(|term| contains_term(query, term))
    {
        signals.insert("canon");
    }
    if sources.iter().any(|value| value.contains("entity"))
        || reasons.iter().any(|value| value.contains("named entity"))
    {
        signals.insert("entity");
    }
    if sources.iter().any(|value| value.contains("date"))
        || dates.iter().any(|value| value == &path)
    {
        signals.insert("date");
    }
    if sources.iter().any(|value| value.contains("project"))
        || reasons.iter().any(|value| value.contains("project"))
    {
        signals.insert("project");
    }
    if (key(&candidate.title).len() >= 7 && contains_term(query, &candidate.title))
        || candidate
            .matched_terms
            .iter()
            .any(|term| distinctive_exact_term(term) && contains_term(&candidate.title, term))
    {
        signals.insert("title");
    }
    if candidate
        .matched_terms
        .iter()
        .any(|term| distinctive_exact_term(term) && contains_term(&path, term))
        || (path.len() >= 7 && contains_term(query, &path))
    {
        signals.insert("path");
    }
    signals
}

fn compact_candidate(candidate: &RecallCandidate) -> RecallPresentationCandidate {
    RecallPresentationCandidate {
        source_path: bounded(&candidate.source_path, 2048),
        title: bounded(&candidate.title, 512),
        heading_path: bounded(&candidate.heading_path, 1024),
        memory_id: candidate.memory_id,
        thread_key: candidate
            .thread_key
            .as_deref()
            .map(|value| bounded(value, 512)),
        thread_neighbors: candidate
            .thread_neighbors
            .iter()
            .take(6)
            .map(
                |neighbor| house_protocol::RecallPresentationThreadNeighbor {
                    thread: bounded(&neighbor.thread, 512),
                    direction: bounded(&neighbor.direction, 32),
                    id: neighbor.id,
                    title: bounded(&neighbor.title, 512),
                    source_path: bounded(&neighbor.source_path, 2048),
                    excerpt: bounded(&neighbor.excerpt, 500),
                    authority_state: bounded(&neighbor.authority_state, 64),
                    superseded_by: neighbor.superseded_by,
                },
            )
            .collect(),
        sources: candidate
            .sources
            .iter()
            .take(4)
            .map(|value| bounded(value, 256))
            .collect(),
        score: candidate.score,
        term_coverage: candidate.term_coverage.clone(),
        matched_terms: candidate
            .matched_terms
            .iter()
            .take(8)
            .map(|value| bounded(value, 128))
            .collect(),
        missing_terms: candidate
            .missing_terms
            .iter()
            .take(8)
            .map(|value| bounded(value, 128))
            .collect(),
        reasons: candidate
            .reasons
            .iter()
            .take(5)
            .map(|value| bounded(value, 256))
            .collect(),
        excerpt: bounded(&candidate.excerpt, 900),
    }
}

fn direct_canon_match(entry: &RecallCanonMatch, query: &str) -> bool {
    std::iter::once(entry.term_key.as_str())
        .chain(entry.entry.aliases.iter().map(String::as_str))
        .any(|term| contains_term(query, term))
}

fn compact_canon(
    matches: &[RecallCanonMatch],
    query: &str,
    candidate_paths: &HashSet<String>,
) -> Vec<RecallPresentationCanonMatch> {
    matches
        .iter()
        .filter(|entry| {
            direct_canon_match(entry, query)
                || entry
                    .entry
                    .files
                    .iter()
                    .any(|file| candidate_paths.contains(&path_key(&file.file)))
        })
        .take(6)
        .map(|entry| RecallPresentationCanonMatch {
            term_key: bounded(&entry.term_key, 512),
            entry_type: bounded(&entry.entry.entry_type, 128),
            summary: bounded(&entry.entry.summary, 480),
            files: entry.entry.files.iter().take(3).cloned().collect(),
        })
        .collect()
}

fn compact_raw_chunks(
    values: &[house_protocol::RecallRawChunk],
    semantic: bool,
) -> Vec<RecallPresentationRawChunk> {
    values
        .iter()
        .take(5)
        .map(|entry| RecallPresentationRawChunk {
            source_path: bounded(&entry.source_path, 2048),
            heading_path: bounded(&entry.heading_path, 1024),
            score: if semantic { entry.sim } else { entry.ws },
            body: bounded(&entry.body, 900),
        })
        .collect()
}

pub fn apply_viewport(
    result: RecallResultInput,
    session: &mut ViewportSession,
    mode: RecallViewportMode,
) -> RecallViewportResult {
    let glue = DEFAULT_GLUE_TERMS.iter().copied().collect::<HashSet<_>>();
    let canon_files = canon_file_paths(&result.canon_matches);
    let canon_terms = result
        .canon_matches
        .iter()
        .map(|entry| entry.term_key.clone())
        .collect::<Vec<_>>();
    let dates = date_paths(&result);
    let mut kept = Vec::new();
    let mut suppressions = Vec::new();
    let mut reason_counts = HashMap::<String, u64>::new();

    for (index, candidate) in result.retrieval_candidates.iter().enumerate() {
        let identity = candidate_identity(candidate, index);
        let meaningful = candidate
            .matched_terms
            .iter()
            .filter(|term| !glue.contains(key(term).as_str()))
            .collect::<Vec<_>>();
        let mut reason =
            if mode == RecallViewportMode::Automatic && candidate.matched_terms.is_empty() {
                Some("zero-terms")
            } else if mode == RecallViewportMode::Automatic && meaningful.is_empty() {
                Some("glue-only")
            } else {
                None
            };
        let exact = exact_signals(candidate, &result.query, &canon_files, &canon_terms, &dates);
        let independent = meaningful
            .iter()
            .map(|term| key(term))
            .chain(exact.iter().map(|signal| format!("exact:{signal}")))
            .collect::<HashSet<_>>();
        if mode == RecallViewportMode::Automatic
            && reason.is_none()
            && exact.is_empty()
            && independent.len() < 2
        {
            reason = Some("insufficient-evidence");
        }
        let exposures = session.exposures.get(&identity).copied().unwrap_or(0);
        if mode == RecallViewportMode::Automatic && reason.is_none() && exposures >= 1 {
            reason = Some("saturated");
        }
        if let Some(reason) = reason {
            suppressions.push(RecallViewportSuppression {
                identity,
                reason: reason.into(),
            });
            *reason_counts.entry(reason.into()).or_default() += 1;
        } else if kept.len() < 5 {
            if mode == RecallViewportMode::Automatic {
                session.exposures.insert(identity, exposures + 1);
            }
            kept.push(compact_candidate(candidate));
        }
    }

    let candidate_paths = kept
        .iter()
        .map(|entry| path_key(&entry.source_path))
        .collect::<HashSet<_>>();
    let canon_matches = compact_canon(&result.canon_matches, &result.query, &candidate_paths);
    let include_raw = kept.is_empty();
    let semantic_chunks = include_raw
        .then(|| compact_raw_chunks(&result.semantic_chunks, true))
        .unwrap_or_default();
    let content_chunks = include_raw
        .then(|| compact_raw_chunks(&result.content_chunks, false))
        .unwrap_or_default();
    let date_matches = result
        .date_matches
        .iter()
        .take(5)
        .map(|entry| RecallPresentationDateMatch {
            source_path: bounded(&entry.source_path, 2048),
            title: bounded(&entry.title, 512),
            dates: entry
                .dates
                .iter()
                .take(16)
                .map(|value| bounded(value, 32))
                .collect(),
            body_excerpt: bounded(&entry.body_excerpt, 900),
        })
        .collect::<Vec<_>>();
    let warnings = result
        .warnings
        .iter()
        .take(8)
        .map(|value| bounded(value, 300))
        .collect::<Vec<_>>();
    let taxonomy = (mode == RecallViewportMode::Manual).then(|| RecallPresentationTaxonomy {
        rooms: result
            .taxonomy
            .rooms
            .iter()
            .take(12)
            .map(|value| bounded(value, 128))
            .collect(),
        memory_types: result
            .taxonomy
            .memory_types
            .iter()
            .take(12)
            .map(|value| bounded(value, 128))
            .collect(),
        thread_keys: result
            .taxonomy
            .thread_keys
            .iter()
            .take(12)
            .map(|value| bounded(value, 256))
            .collect(),
        named_entities: result
            .taxonomy
            .named_entities
            .iter()
            .take(12)
            .map(|value| bounded(value, 256))
            .collect(),
        file_types: result
            .taxonomy
            .file_types
            .iter()
            .take(12)
            .map(|value| bounded(value, 64))
            .collect(),
    });
    let cluster_nudge = (mode == RecallViewportMode::Manual)
        .then(|| result.cluster_staleness.as_ref())
        .flatten()
        .filter(|value| value.built_at.is_none() || value.fraction_unseen >= 0.15)
        .map(|value| {
            format!(
                "clusters: {}, {} chunks since ({}% of corpus unseen) — Rust cluster maintenance is due",
                value.built_at.as_deref().map(|date| format!("built {}", bounded(date, 10))).unwrap_or_else(|| "never built".into()),
                value.chunks_since_build,
                (value.fraction_unseen * 100.0).round() as u64,
            )
        });
    let cluster_resonance = (mode == RecallViewportMode::Manual)
        .then(|| result.cluster_resonance.as_ref())
        .flatten()
        .filter(|value| !value.profile.is_empty())
        .map(|value| RecallPresentationClusterResonance {
            note: "substrate resonance: what the memory space finds near this query — telemetry, not model-internal state".into(),
            profile: value.profile.iter().take(8).map(|entry| RecallPresentationClusterProfile {
                label: bounded(&entry.label, 256),
                activation: entry.activation,
                members: entry.member_count,
            }).collect(),
            dormant_hot: value.hot.iter().take(3).cloned().collect(),
        });
    let memory_handle = (mode == RecallViewportMode::Manual)
        .then(|| result.memory_handle.as_ref())
        .flatten()
        .map(|handle| RecallPresentationMemoryHandle {
            path: bounded(&handle.path, 2048),
            title: bounded(&handle.title, 512),
            memory: handle
                .memory
                .as_ref()
                .map(|memory| RecallPresentationMemoryRecord {
                    source_path: bounded(&memory.source_path, 2048),
                    body: bounded(&memory.body, 6000),
                    frontmatter: memory.extra.get("frontmatter").cloned().unwrap_or_default(),
                }),
        });
    let vault = (result.source == "vault-files").then(|| RecallPresentationVault {
        authority: bounded(result.authority.as_deref().unwrap_or_default(), 128),
        roots: result
            .roots
            .iter()
            .take(8)
            .map(|value| bounded(value, 2048))
            .collect(),
        scanned_files: result.scanned_files.unwrap_or_default(),
        indexed_documents: result.indexed_documents.unwrap_or_default(),
    });
    let presentation = RecallPresentation {
        ok: result.ok,
        query: bounded(&result.query, 262_144),
        found: result.found,
        source: bounded(&result.source, 128),
        vault,
        warnings,
        canon_matches,
        retrieval_candidates: kept.clone(),
        semantic_chunks,
        content_chunks,
        date_matches,
        query_dates: result
            .query_dates
            .iter()
            .take(16)
            .map(|value| bounded(value, 32))
            .collect(),
        taxonomy,
        cluster_nudge,
        cluster_resonance,
        memory_handle,
    };
    RecallViewportResult {
        kept_candidates: kept,
        suppressions: suppressions.clone(),
        diagnostics: RecallViewportDiagnostics {
            kept: presentation.retrieval_candidates.len() as u64,
            suppressed: suppressions.len() as u64,
            reasons: reason_counts,
        },
        presentation,
    }
}

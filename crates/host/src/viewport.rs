use protocol::{
    RecallCandidate, RecallCanonMatch, RecallPresentation, RecallPresentationCandidate,
    RecallPresentationCanonMatch, RecallPresentationClusterProfile,
    RecallPresentationClusterResonance, RecallPresentationDateMatch,
    RecallPresentationMemoryHandle, RecallPresentationMemoryRecord, RecallPresentationRawChunk,
    RecallPresentationTaxonomy, RecallPresentationVault, RecallResultInput,
    RecallViewportDiagnostics, RecallViewportMode, RecallViewportResult, RecallViewportSuppression,
};
use std::collections::{HashMap, HashSet};

// enough: one hardcoded English list serving every room. The way up is the
// docketed per-room vocabulary work — a room owning its own glue and stopword
// lists where it is configured — not another literal added to this array.
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

// A term shorter than this is usually a shared English word, so matching it
// exactly says nothing about which memory the operator meant.
// enough: a hand-set character floor standing in for distinctiveness. The way
// up is deriving it from the corpus — document frequency over the room's own
// memories — after which this floor and NON_DISTINCTIVE_EXACT_TERMS both go.
const DISTINCTIVE_TERM_MIN_CHARS: usize = 7;

// Presentation caps for one recall candidate. A cap trims, it never refuses:
// the viewport hands the client a bounded card, and the Host stays the place a
// whole record is read from.
const MAX_SOURCE_PATH_CHARS: usize = 2048;
const MAX_TITLE_CHARS: usize = 512;
const MAX_HEADING_PATH_CHARS: usize = 1024;
const MAX_THREAD_KEY_CHARS: usize = 512;
// Six neighbors is a thread's visible shoulder: enough to see where a memory
// sits in its thread without redrawing the thread inside a card.
const MAX_THREAD_NEIGHBORS: usize = 6;
const MAX_THREAD_NAME_CHARS: usize = 512;
// Direction and authority state are closed vocabularies, never prose.
const MAX_DIRECTION_CHARS: usize = 32;
const MAX_AUTHORITY_STATE_CHARS: usize = 64;
// A neighbor is a pointer, so it shows less body than the candidate it hangs off.
const MAX_NEIGHBOR_EXCERPT_CHARS: usize = 500;
// Sources, terms, and reasons are the evidence line under a card: they are read
// at a glance, so the counts stay small and the strings stay short.
const MAX_SOURCES: usize = 4;
const MAX_SOURCE_CHARS: usize = 256;
const MAX_TERMS: usize = 8;
const MAX_TERM_CHARS: usize = 128;
const MAX_REASONS: usize = 5;
const MAX_REASON_CHARS: usize = 256;
const MAX_CANDIDATE_EXCERPT_CHARS: usize = 900;
// A canon row the caller named is authority, not a card: it is shown whole.
// This ceiling exists only so one pathological row cannot swallow a turn, and
// crossing it is always marked with the deterministic full read.
const MAX_CANON_ASSERTION_CHARS: usize = 6000;
// A similarity-tier canon row is a hint the caller did not name; it stays a
// short card, and the cut is marked the same way.
const MAX_CANON_HINT_CHARS: usize = 480;

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
    term.len() >= DISTINCTIVE_TERM_MIN_CHARS
        && !NON_DISTINCTIVE_EXACT_TERMS.contains(&term.as_str())
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
    if sources.iter().any(|value| value == "exact_id")
        || reasons.iter().any(|value| value == "exact memory id")
    {
        signals.insert("id");
    }
    if sources.iter().any(|value| value.contains("project"))
        || reasons.iter().any(|value| value.contains("project"))
    {
        signals.insert("project");
    }
    if (key(&candidate.title).len() >= DISTINCTIVE_TERM_MIN_CHARS
        && contains_term(query, &candidate.title))
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
        || (path.len() >= DISTINCTIVE_TERM_MIN_CHARS && contains_term(query, &path))
    {
        signals.insert("path");
    }
    signals
}

fn compact_candidate(candidate: &RecallCandidate) -> RecallPresentationCandidate {
    RecallPresentationCandidate {
        source_path: bounded(&candidate.source_path, MAX_SOURCE_PATH_CHARS),
        title: bounded(&candidate.title, MAX_TITLE_CHARS),
        heading_path: bounded(&candidate.heading_path, MAX_HEADING_PATH_CHARS),
        memory_id: candidate.memory_id,
        thread_key: candidate
            .thread_key
            .as_deref()
            .map(|value| bounded(value, MAX_THREAD_KEY_CHARS)),
        thread_neighbors: candidate
            .thread_neighbors
            .iter()
            .take(MAX_THREAD_NEIGHBORS)
            .map(|neighbor| protocol::RecallPresentationThreadNeighbor {
                thread: bounded(&neighbor.thread, MAX_THREAD_NAME_CHARS),
                direction: bounded(&neighbor.direction, MAX_DIRECTION_CHARS),
                id: neighbor.id,
                title: bounded(&neighbor.title, MAX_TITLE_CHARS),
                source_path: bounded(&neighbor.source_path, MAX_SOURCE_PATH_CHARS),
                excerpt: bounded(&neighbor.excerpt, MAX_NEIGHBOR_EXCERPT_CHARS),
                authority_state: bounded(&neighbor.authority_state, MAX_AUTHORITY_STATE_CHARS),
                superseded_by: neighbor.superseded_by,
            })
            .collect(),
        sources: candidate
            .sources
            .iter()
            .take(MAX_SOURCES)
            .map(|value| bounded(value, MAX_SOURCE_CHARS))
            .collect(),
        score: candidate.score,
        term_coverage: candidate.term_coverage.clone(),
        matched_terms: candidate
            .matched_terms
            .iter()
            .take(MAX_TERMS)
            .map(|value| bounded(value, MAX_TERM_CHARS))
            .collect(),
        missing_terms: candidate
            .missing_terms
            .iter()
            .take(MAX_TERMS)
            .map(|value| bounded(value, MAX_TERM_CHARS))
            .collect(),
        reasons: candidate
            .reasons
            .iter()
            .take(MAX_REASONS)
            .map(|value| bounded(value, MAX_REASON_CHARS))
            .collect(),
        excerpt: bounded(&candidate.excerpt, MAX_CANDIDATE_EXCERPT_CHARS),
    }
}

fn direct_canon_match(entry: &RecallCanonMatch, query: &str) -> bool {
    std::iter::once(entry.term_key.as_str())
        .chain(entry.entry.aliases.iter().map(String::as_str))
        .any(|term| contains_term(query, term))
}

fn canon_identity(entry: &RecallCanonMatch) -> String {
    match entry.entry.id {
        Some(id) => format!("canon:{id}"),
        None => format!("canon:{}", key(&entry.term_key)),
    }
}

fn canon_summary(entry: &RecallCanonMatch, exact: bool) -> (String, bool, Option<String>) {
    let limit = if exact {
        MAX_CANON_ASSERTION_CHARS
    } else {
        MAX_CANON_HINT_CHARS
    };
    let upstream_cut = entry.entry.truncated;
    let cut_here = entry.entry.summary.chars().count() > limit;
    let truncated = upstream_cut || cut_here;
    let full_read = if truncated {
        entry
            .entry
            .full_read
            .clone()
            .or_else(|| entry.entry.id.map(|id| format!("canon_read {id}")))
            .or_else(|| Some(format!("canon_read name={}", entry.term_key)))
    } else {
        None
    };
    (bounded(&entry.entry.summary, limit), truncated, full_read)
}

fn compact_canon(
    matches: &[RecallCanonMatch],
    query: &str,
    candidate_paths: &HashSet<String>,
    session: &mut ViewportSession,
    mode: RecallViewportMode,
    suppressions: &mut Vec<RecallViewportSuppression>,
    reason_counts: &mut HashMap<String, u64>,
) -> Vec<RecallPresentationCanonMatch> {
    let mut kept = Vec::new();
    for entry in matches {
        let exact = entry.entry.exact || direct_canon_match(entry, query);
        let by_file = entry
            .entry
            .files
            .iter()
            .any(|file| candidate_paths.contains(&path_key(&file.file)));
        if !(exact || by_file) {
            continue;
        }
        if kept.len() >= 6 {
            break;
        }
        // The same entity version already sits in this session's context;
        // repeating a whole assertion says nothing new. Compaction clears the
        // session, so a reorientation after it sees the row again in full.
        let identity = canon_identity(entry);
        let exposures = session.exposures.get(&identity).copied().unwrap_or(0);
        if mode == RecallViewportMode::Automatic && exposures >= 1 {
            suppressions.push(RecallViewportSuppression {
                identity,
                reason: "saturated".into(),
            });
            *reason_counts.entry("saturated".into()).or_default() += 1;
            continue;
        }
        if mode == RecallViewportMode::Automatic {
            session.exposures.insert(identity, exposures + 1);
        }
        let (summary, truncated, full_read) = canon_summary(entry, exact);
        kept.push(RecallPresentationCanonMatch {
            term_key: bounded(&entry.term_key, 512),
            id: entry.entry.id,
            entry_type: bounded(&entry.entry.entry_type, 128),
            weighty: entry.entry.weighty,
            exact,
            summary,
            truncated,
            full_read,
            files: entry.entry.files.iter().take(3).cloned().collect(),
        });
    }
    kept
}

fn compact_raw_chunks(
    values: &[protocol::RecallRawChunk],
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
    let canon_matches = compact_canon(
        &result.canon_matches,
        &result.query,
        &candidate_paths,
        session,
        mode,
        &mut suppressions,
        &mut reason_counts,
    );
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

#[cfg(test)]
mod tests {
    use super::{MAX_CANON_ASSERTION_CHARS, ViewportSession, apply_viewport};
    use protocol::{RecallResultInput, RecallViewportMode};

    fn result(
        query: &str,
        canon: serde_json::Value,
        candidates: serde_json::Value,
    ) -> RecallResultInput {
        serde_json::from_value(serde_json::json!({
            "ok": true,
            "query": query,
            "found": true,
            "source": "rust-postgres",
            "retrievalCandidates": candidates,
            "canonMatches": canon,
            "semanticChunks": [],
            "contentChunks": [],
            "dateMatches": [],
            "queryDates": [],
            "taxonomy": {},
        }))
        .expect("fixture must deserialize")
    }

    fn athanor(summary: &str) -> serde_json::Value {
        serde_json::json!([{
            "termKey": "The Athanor",
            "entry": {
                "id": 41,
                "type": "platform",
                "summary": summary,
                "aliases": ["Athanor"],
                "weighty": true,
                "exact": true,
                "truncated": false,
                "full_read": null,
                "files": []
            }
        }])
    }

    #[test]
    fn exact_canon_is_shown_whole_and_suppressed_only_for_the_same_version() {
        let summary =
            "The Athanor is the House platform; silent typing is not a truncation. ".repeat(20);
        assert!(summary.chars().count() > 480);
        let mut session = ViewportSession::default();
        let first = apply_viewport(
            result(
                "reorient me on The Athanor",
                athanor(&summary),
                serde_json::json!([]),
            ),
            &mut session,
            RecallViewportMode::Automatic,
        );
        let entry = &first.presentation.canon_matches[0];
        assert_eq!(entry.id, Some(41));
        assert!(entry.exact && entry.weighty);
        assert_eq!(entry.summary, summary, "a named entity is never clipped");
        assert!(!entry.truncated);
        assert_eq!(entry.full_read, None);

        let repeat = apply_viewport(
            result(
                "reorient me on The Athanor",
                athanor(&summary),
                serde_json::json!([]),
            ),
            &mut session,
            RecallViewportMode::Automatic,
        );
        assert!(repeat.presentation.canon_matches.is_empty());
        assert!(
            repeat
                .suppressions
                .iter()
                .any(|s| s.identity == "canon:41" && s.reason == "saturated")
        );

        let manual = apply_viewport(
            result(
                "reorient me on The Athanor",
                athanor(&summary),
                serde_json::json!([]),
            ),
            &mut session,
            RecallViewportMode::Manual,
        );
        assert_eq!(manual.presentation.canon_matches[0].summary, summary);

        let mut fresh = ViewportSession::default();
        let after_compaction = apply_viewport(
            result(
                "reorient me on The Athanor",
                athanor(&summary),
                serde_json::json!([]),
            ),
            &mut fresh,
            RecallViewportMode::Automatic,
        );
        assert_eq!(
            after_compaction.presentation.canon_matches[0].summary,
            summary
        );
    }

    #[test]
    fn a_forced_canon_cut_is_marked_with_its_full_read() {
        let summary = "x".repeat(MAX_CANON_ASSERTION_CHARS + 10);
        let viewport = apply_viewport(
            result("The Athanor", athanor(&summary), serde_json::json!([])),
            &mut ViewportSession::default(),
            RecallViewportMode::Automatic,
        );
        let entry = &viewport.presentation.canon_matches[0];
        assert_eq!(entry.summary.chars().count(), MAX_CANON_ASSERTION_CHARS);
        assert!(entry.truncated);
        assert_eq!(entry.full_read.as_deref(), Some("canon_read 41"));

        let mut upstream = athanor("clipped upstream…");
        upstream[0]["entry"]["exact"] = serde_json::json!(false);
        upstream[0]["entry"]["truncated"] = serde_json::json!(true);
        upstream[0]["entry"]["full_read"] = serde_json::json!("canon_read 41");
        let viewport = apply_viewport(
            result("The Athanor", upstream, serde_json::json!([])),
            &mut ViewportSession::default(),
            RecallViewportMode::Automatic,
        );
        let entry = &viewport.presentation.canon_matches[0];
        assert!(entry.truncated);
        assert_eq!(entry.full_read.as_deref(), Some("canon_read 41"));
    }

    #[test]
    fn an_exact_memory_id_row_survives_automatic_evidence_gating() {
        let candidates = serde_json::json!([{
            "memory_id": 4197,
            "source_path": "kodo/2026-08-28-analysis.md",
            "title": "analysis with Kintsu",
            "heading_path": "",
            "excerpt": "Analysis Sol made with Kintsu.",
            "sources": ["kodo/2026-08-28-analysis.md"],
            "term_coverage": 1.0,
            "matched_terms": ["4197"],
            "missing_terms": [],
            "score": 1.0,
            "reasons": ["exact memory id"],
            "source": "exact_id",
            "chunk_index": 0
        }]);
        let viewport = apply_viewport(
            result("memory 4197", serde_json::json!([]), candidates),
            &mut ViewportSession::default(),
            RecallViewportMode::Automatic,
        );
        assert_eq!(viewport.kept_candidates.len(), 1);
        assert_eq!(viewport.kept_candidates[0].memory_id, Some(4197));
        assert!(viewport.suppressions.is_empty());
    }
}

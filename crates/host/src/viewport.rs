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
// A memory's threads are a map of where to read next, not the threads
// themselves: a card names a few threads and a few next members in each.
const MAX_CANDIDATE_THREADS: usize = 8;
const MAX_THREAD_NEXT_MEMORIES: usize = 8;
// Amendments point to other records. A card names a few in each direction;
// each note keeps the length that the store allows.
const MAX_AMENDMENTS: usize = 8;
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
const MAX_KEPT_CANDIDATES: usize = 5;
// Warnings are one line each. An automatic turn reads a handful under its
// budget; a manual read keeps every warning whole, because each missing,
// refused, or dropped ID line is part of the answer.
const MAX_WARNINGS_AUTOMATIC: usize = 8;
const MAX_WARNING_CHARS_AUTOMATIC: usize = 300;
const MAX_DATE_BODY_EXCERPT_CHARS: usize = 900;
// A canon row the caller named is authority, not a card: it is shown whole.
// This ceiling exists only so one pathological row cannot swallow a turn, and
// crossing it is always marked with the deterministic full read.
const MAX_CANON_ASSERTION_CHARS: usize = 6000;
// A similarity-tier canon row is a hint the caller did not name; it stays a
// short card, and the cut is marked the same way.
const MAX_CANON_HINT_CHARS: usize = 480;

#[derive(Debug, Default)]
pub struct ViewportSession {
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

fn candidate_identity(candidate: &RecallCandidate) -> Option<String> {
    if let Some(id) = candidate.memory_id {
        return Some(format!("memory:{id}"));
    }

    if let Some(explicit) = [
        candidate.id.as_deref(),
        candidate.identity.as_deref(),
        candidate.candidate_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|value| !value.is_empty())
    {
        return Some(explicit.to_owned());
    }

    let path = path_key(&candidate.source_path);

    if path.is_empty() {
        None
    } else {
        Some(format!("source:{path}"))
    }
}

fn canon_file_paths(matches: &[RecallCanonMatch]) -> Vec<String> {
    matches
        .iter()
        .filter(|entry| entry.entry.exact)
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

fn has_exact_signal(
    candidate: &RecallCandidate,
    query: &str,
    canon_files: &[String],
    dates: &[String],
) -> bool {
    let path = path_key(&candidate.source_path);
    let exact_id = candidate.source.as_deref() == Some("exact_id")
        || candidate
            .reasons
            .iter()
            .any(|reason| reason == "exact memory id");

    exact_id
        || canon_files.iter().any(|value| value == &path)
        || dates.iter().any(|value| value == &path)
        || (key(&candidate.title).len() >= DISTINCTIVE_TERM_MIN_CHARS
            && contains_term(query, &candidate.title))
        || candidate.matched_terms.iter().any(|term| {
            distinctive_exact_term(term)
                && (contains_term(&candidate.title, term) || contains_term(&path, term))
        })
        || (path.len() >= DISTINCTIVE_TERM_MIN_CHARS && contains_term(query, &path))
}

fn compact_candidate(
    candidate: &RecallCandidate,
    mode: RecallViewportMode,
) -> RecallPresentationCandidate {
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
        threads: candidate
            .threads
            .iter()
            .take(MAX_CANDIDATE_THREADS)
            .map(|thread| protocol::RecallThreadMembership {
                key: bounded(&thread.key, MAX_THREAD_KEY_CHARS),
                previous_memory_id: thread.previous_memory_id,
                next_memory_ids: thread
                    .next_memory_ids
                    .iter()
                    .take(MAX_THREAD_NEXT_MEMORIES)
                    .copied()
                    .collect(),
            })
            .collect(),
        amended_by: candidate
            .amended_by
            .iter()
            .take(MAX_AMENDMENTS)
            .map(|entry| protocol::RecallAmendedBy {
                memory_id: entry.memory_id,
                changed: bounded(&entry.changed, hearth::MAX_AMENDMENT_CHARS),
            })
            .collect(),
        amends: candidate
            .amends
            .iter()
            .take(MAX_AMENDMENTS)
            .map(|entry| protocol::RecallAmends {
                memory_id: entry.memory_id,
                changed: bounded(&entry.changed, hearth::MAX_AMENDMENT_CHARS),
                keeps: entry
                    .keeps
                    .as_deref()
                    .map(|keeps| bounded(keeps, hearth::MAX_AMENDMENT_CHARS)),
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
        passage: candidate.passage.as_ref().map(|passage| {
            let mut passage = passage.clone();
            passage.char_end = passage.char_end.min(
                passage.char_start
                    + candidate
                        .excerpt
                        .chars()
                        .take(MAX_CANDIDATE_EXCERPT_CHARS)
                        .count() as u64,
            );
            passage
        }),
        // A manual read is the whole selected record; the count cap upstream
        // is the only budget. An automatic working set never carries a body.
        body: match mode {
            RecallViewportMode::Manual => candidate.body.clone(),
            RecallViewportMode::Automatic | RecallViewportMode::AutomaticJudged => None,
        },
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
    suppressions: &mut Vec<RecallViewportSuppression>,
    reason_counts: &mut HashMap<String, u64>,
    mode: RecallViewportMode,
) -> Vec<RecallPresentationCanonMatch> {
    let mut kept = Vec::new();
    let mut identities = HashSet::new();
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
        if mode != RecallViewportMode::Manual && kept.len() >= 6 {
            break;
        }
        let identity = canon_identity(entry);

        if !identities.insert(identity.clone()) {
            suppressions.push(RecallViewportSuppression {
                identity,
                reason: "duplicate".into(),
            });
            *reason_counts.entry("duplicate".into()).or_default() += 1;
            continue;
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
        .map(|entry| RecallPresentationRawChunk {
            source_path: bounded(&entry.source_path, 2048),
            heading_path: bounded(&entry.heading_path, 1024),
            score: if semantic { entry.sim } else { entry.ws },
            body: bounded(&entry.body, 900),
            passage: entry.passage.clone(),
        })
        .collect()
}

pub fn apply_viewport(
    result: RecallResultInput,
    _session: &mut ViewportSession,
    mode: RecallViewportMode,
) -> RecallViewportResult {
    let canon_files = canon_file_paths(&result.canon_matches);
    let dates = date_paths(&result);
    let mut kept = Vec::new();
    let mut suppressions = Vec::new();
    let mut reason_counts = HashMap::<String, u64>::new();
    let mut identities = HashSet::new();
    let keep_cap = match mode {
        RecallViewportMode::Manual => usize::MAX,
        RecallViewportMode::Automatic | RecallViewportMode::AutomaticJudged => MAX_KEPT_CANDIDATES,
    };
    let (warning_cap, warning_chars) = match mode {
        RecallViewportMode::Manual => (usize::MAX, usize::MAX),
        RecallViewportMode::Automatic | RecallViewportMode::AutomaticJudged => {
            (MAX_WARNINGS_AUTOMATIC, MAX_WARNING_CHARS_AUTOMATIC)
        }
    };

    for (index, candidate) in result.retrieval_candidates.iter().enumerate() {
        let Some(identity) = candidate_identity(candidate) else {
            suppressions.push(RecallViewportSuppression {
                identity: format!("candidate:{index}"),
                reason: "missing-identity".into(),
            });
            *reason_counts.entry("missing-identity".into()).or_default() += 1;
            continue;
        };
        let reason = if identities.contains(&identity) {
            Some("duplicate")
        } else if mode != RecallViewportMode::Automatic
            || has_exact_signal(candidate, &result.query, &canon_files, &dates)
        {
            None
        } else if candidate.matched_terms.is_empty() {
            Some("zero-terms")
        } else {
            let meaningful = candidate
                .matched_terms
                .iter()
                .map(|term| key(term))
                .filter(|term| !DEFAULT_GLUE_TERMS.contains(&term.as_str()))
                .collect::<HashSet<_>>();

            if meaningful.is_empty() {
                Some("glue-only")
            } else if meaningful.len() < 2 {
                Some("insufficient-evidence")
            } else {
                None
            }
        };

        if let Some(reason) = reason {
            suppressions.push(RecallViewportSuppression {
                identity,
                reason: reason.into(),
            });
            *reason_counts.entry(reason.into()).or_default() += 1;
        } else if kept.len() < keep_cap {
            identities.insert(identity);
            kept.push(compact_candidate(candidate, mode));
        } else {
            suppressions.push(RecallViewportSuppression {
                identity,
                reason: "record-cap".into(),
            });
            *reason_counts.entry("record-cap".into()).or_default() += 1;
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
        &mut suppressions,
        &mut reason_counts,
        mode,
    );
    let include_raw = mode == RecallViewportMode::Manual && kept.is_empty();
    let semantic_chunks = include_raw
        .then(|| compact_raw_chunks(&result.semantic_chunks, true))
        .unwrap_or_default();
    let content_chunks = include_raw
        .then(|| compact_raw_chunks(&result.content_chunks, false))
        .unwrap_or_default();
    let date_matches = result
        .date_matches
        .iter()
        .take(if mode == RecallViewportMode::Manual {
            usize::MAX
        } else {
            5
        })
        .map(|entry| RecallPresentationDateMatch {
            source_path: bounded(&entry.source_path, 2048),
            title: bounded(&entry.title, 512),
            dates: entry
                .dates
                .iter()
                .take(16)
                .map(|value| bounded(value, 32))
                .collect(),
            body_excerpt: bounded(&entry.body_excerpt, MAX_DATE_BODY_EXCERPT_CHARS),
        })
        .collect::<Vec<_>>();
    let warnings = result
        .warnings
        .iter()
        .take(warning_cap)
        .map(|value| bounded(value, warning_chars))
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
        found: !kept.is_empty()
            || !canon_matches.is_empty()
            || !semantic_chunks.is_empty()
            || !content_chunks.is_empty()
            || !date_matches.is_empty()
            || memory_handle
                .as_ref()
                .is_some_and(|handle| handle.memory.is_some()),
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
        projection: result.projection,
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
    use super::{
        MAX_AMENDMENTS, MAX_CANDIDATE_EXCERPT_CHARS, MAX_CANDIDATE_THREADS,
        MAX_CANON_ASSERTION_CHARS, MAX_KEPT_CANDIDATES, MAX_THREAD_KEY_CHARS,
        MAX_THREAD_NEXT_MEMORIES, MAX_WARNINGS_AUTOMATIC, ViewportSession, apply_viewport,
    };
    use hearth::RecallProjection;
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
    fn exact_canon_is_shown_whole_on_each_snapshot() {
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
        assert_eq!(repeat.presentation.canon_matches[0].summary, summary);
        assert!(repeat.suppressions.is_empty());

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

    #[test]
    fn a_card_keeps_bounded_threads_with_exact_neighbor_ids() {
        let mut candidate = record(4520, "body");
        candidate["threads"] = serde_json::json!(
            (0..12)
                .map(|index| serde_json::json!({
                    "key": format!("{}{index}", "k".repeat(600)),
                    "previousMemoryId": 4519,
                    "nextMemoryIds": (4521..4541).collect::<Vec<i64>>(),
                }))
                .collect::<Vec<_>>()
        );
        let viewport = apply_viewport(
            result(
                "memory 4520",
                serde_json::json!([]),
                serde_json::json!([candidate]),
            ),
            &mut ViewportSession::default(),
            RecallViewportMode::Automatic,
        );
        let threads = &viewport.presentation.retrieval_candidates[0].threads;

        assert_eq!(threads.len(), MAX_CANDIDATE_THREADS);
        assert!(
            threads
                .iter()
                .all(|thread| thread.key.chars().count() == MAX_THREAD_KEY_CHARS)
        );
        assert!(
            threads
                .iter()
                .all(|thread| thread.previous_memory_id == Some(4519))
        );
        assert_eq!(
            threads[0].next_memory_ids,
            (4521..4521 + MAX_THREAD_NEXT_MEMORIES as i64).collect::<Vec<_>>()
        );

        let wire = serde_json::to_value(&viewport.presentation).unwrap();
        let first = &wire["retrievalCandidates"][0]["threads"][0];

        assert_eq!(first["previousMemoryId"], 4519);
        assert_eq!(first["nextMemoryIds"][0], 4521);
    }

    #[test]
    fn a_card_keeps_bounded_amendments_in_both_directions() {
        let mut candidate = record(4520, "body");
        let note = "n".repeat(hearth::MAX_AMENDMENT_CHARS + 40);
        candidate["amendedBy"] = serde_json::json!(
            (4600..4612)
                .map(|id| serde_json::json!({"memoryId": id, "changed": note}))
                .collect::<Vec<_>>()
        );
        candidate["amends"] = serde_json::json!([
            {"memoryId": 4400, "changed": "The date moved.", "keeps": "The budget."},
            {"memoryId": 4401, "changed": "The owner changed.", "keeps": null}
        ]);
        let viewport = apply_viewport(
            result(
                "memory 4520",
                serde_json::json!([]),
                serde_json::json!([candidate]),
            ),
            &mut ViewportSession::default(),
            RecallViewportMode::Automatic,
        );
        let card = &viewport.presentation.retrieval_candidates[0];

        assert_eq!(card.amended_by.len(), MAX_AMENDMENTS);
        assert_eq!(card.amended_by[0].memory_id, 4600);
        assert!(
            card.amended_by
                .iter()
                .all(|entry| entry.changed.chars().count() == hearth::MAX_AMENDMENT_CHARS)
        );

        let wire = serde_json::to_value(&viewport.presentation).unwrap();
        let card = &wire["retrievalCandidates"][0];

        assert_eq!(card["amendedBy"][0]["memoryId"], 4600);
        assert_eq!(
            card["amends"],
            serde_json::json!([
                {"memoryId": 4400, "changed": "The date moved.", "keeps": "The budget."},
                {"memoryId": 4401, "changed": "The owner changed.", "keeps": null}
            ])
        );
    }

    fn record(id: i64, body: &str) -> serde_json::Value {
        serde_json::json!({
            "memory_id": id,
            "source_path": format!("kodo/record-{id}.md"),
            "title": format!("record {id}"),
            "heading_path": "",
            "excerpt": body.chars().take(1200).collect::<String>(),
            "body": body,
            "sources": [format!("kodo/record-{id}.md")],
            "term_coverage": 1.0,
            "matched_terms": [id.to_string()],
            "missing_terms": [],
            "score": 1.0,
            "reasons": ["exact memory id"],
            "source": "exact_id",
            "chunk_index": 0
        })
    }

    #[test]
    fn a_manual_read_carries_the_whole_record_and_the_auto_working_set_never_does() {
        let body = "tail marker follows the twelve-hundredth character ".repeat(60);
        assert!(body.chars().count() > 1200 + MAX_CANDIDATE_EXCERPT_CHARS);
        let mut input = result(
            "memory 4520",
            serde_json::json!([]),
            serde_json::json!([record(4520, &body)]),
        );
        input.projection = Some(RecallProjection::Manual);
        let manual = apply_viewport(
            input,
            &mut ViewportSession::default(),
            RecallViewportMode::Manual,
        );
        let card = &manual.presentation.retrieval_candidates[0];
        assert_eq!(
            card.body.as_deref(),
            Some(body.as_str()),
            "manual keeps the full body"
        );
        assert!(
            card.body
                .as_deref()
                .is_some_and(|value| value.ends_with(&body[body.len() - 40..])),
            "the tail past 1200 characters survives"
        );
        assert_eq!(card.excerpt.chars().count(), MAX_CANDIDATE_EXCERPT_CHARS);
        assert_eq!(
            manual.presentation.projection,
            Some(RecallProjection::Manual)
        );

        let mut input = result(
            "memory 4520",
            serde_json::json!([]),
            serde_json::json!([record(4520, &body)]),
        );
        input.projection = Some(RecallProjection::Auto);
        let auto = apply_viewport(
            input,
            &mut ViewportSession::default(),
            RecallViewportMode::Automatic,
        );
        let card = &auto.presentation.retrieval_candidates[0];
        assert_eq!(
            card.body, None,
            "an automatic working set stays excerpt-bounded"
        );
        assert_eq!(card.excerpt.chars().count(), MAX_CANDIDATE_EXCERPT_CHARS);
        let rendered = serde_json::to_string(&auto.presentation).expect("presentation serializes");
        assert!(
            !rendered.contains("\"body\":"),
            "no body key leaks into the auto presentation"
        );
    }

    #[test]
    fn a_manual_read_keeps_requested_records_and_every_id_warning() {
        let count = 75;
        let candidates = (1..=count)
            .map(|id| record(id, "short body"))
            .collect::<Vec<_>>();
        let mut input = result(
            "memories 1, 2, 3, 4, 5, 6, 7 and 8",
            serde_json::json!([]),
            serde_json::json!(candidates),
        );
        input.projection = Some(RecallProjection::Manual);
        input.warnings = (100..100 + MAX_WARNINGS_AUTOMATIC as i64 + 4)
            .map(|id| format!("memory {id} not found"))
            .collect();
        let expected_warnings = input.warnings.clone();
        let manual = apply_viewport(
            input.clone(),
            &mut ViewportSession::default(),
            RecallViewportMode::Manual,
        );
        assert_eq!(
            manual.presentation.retrieval_candidates.len(),
            count as usize
        );
        assert_eq!(
            manual
                .presentation
                .retrieval_candidates
                .iter()
                .map(|card| card.memory_id)
                .collect::<Vec<_>>(),
            (1..=count).map(Some).collect::<Vec<_>>(),
            "the viewport preserves every selected record in reference order"
        );
        assert_eq!(manual.presentation.warnings, expected_warnings);
        assert!(manual.suppressions.is_empty());

        let auto = apply_viewport(
            input,
            &mut ViewportSession::default(),
            RecallViewportMode::Automatic,
        );
        assert_eq!(
            auto.presentation.warnings.len(),
            MAX_WARNINGS_AUTOMATIC,
            "the automatic budget stays"
        );
        // The automatic seat count is its own number; the overflow is whatever
        // the manual-sized input leaves past it.
        assert_eq!(
            auto.presentation.retrieval_candidates.len(),
            MAX_KEPT_CANDIDATES
        );
        assert_eq!(
            auto.diagnostics.reasons.get("record-cap"),
            Some(&((count as usize - MAX_KEPT_CANDIDATES) as u64)),
            "automatic overflow is accounted in diagnostics"
        );
        assert!(
            !auto
                .presentation
                .warnings
                .iter()
                .any(|w| w.starts_with("record cap")),
            "the automatic working set keeps its warning budget for substrate warnings"
        );
    }

    #[test]
    fn named_canon_only_admits_a_candidate_with_linked_provenance() {
        let mut unrelated = record(71, "An unrelated passage.");
        unrelated["source"] = serde_json::json!("semantic");
        unrelated["matched_terms"] = serde_json::json!([]);
        unrelated["reasons"] = serde_json::json!([]);
        let mut linked = unrelated.clone();
        linked["memory_id"] = serde_json::json!(72);
        linked["source_path"] = serde_json::json!("memory/athanor-policy.md");
        let mut canon = athanor("The platform's active definition.");
        canon[0]["entry"]["files"] =
            serde_json::json!([{"file":"memory/athanor-policy.md","lines":[]}]);
        let viewport = apply_viewport(
            result("The Athanor", canon, serde_json::json!([unrelated, linked])),
            &mut ViewportSession::default(),
            RecallViewportMode::Automatic,
        );

        assert_eq!(
            viewport
                .presentation
                .retrieval_candidates
                .iter()
                .map(|candidate| candidate.memory_id)
                .collect::<Vec<_>>(),
            vec![Some(72)]
        );
        assert_eq!(viewport.presentation.canon_matches[0].id, Some(41));
    }

    #[test]
    fn judged_relevance_does_not_require_overlap_or_restore_raw_chunks() {
        let mut candidate = record(71, "A semantic paraphrase without literal query terms.");
        candidate["matched_terms"] = serde_json::json!([]);
        candidate["reasons"] = serde_json::json!([]);
        candidate["source"] = serde_json::json!("semantic");
        let mut input = result(
            "unrelated wording",
            serde_json::json!([]),
            serde_json::json!([candidate]),
        );
        input.semantic_chunks = serde_json::from_value(serde_json::json!([{
            "source_path": "raw.md", "body": "must not return"
        }]))
        .unwrap();
        input.content_chunks = input.semantic_chunks.clone();

        let conservative = apply_viewport(
            input.clone(),
            &mut ViewportSession::default(),
            RecallViewportMode::Automatic,
        );
        assert!(conservative.kept_candidates.is_empty());
        assert!(!conservative.presentation.found);
        assert!(conservative.presentation.semantic_chunks.is_empty());
        assert!(conservative.presentation.content_chunks.is_empty());

        input.found = false;
        let judged = apply_viewport(
            input.clone(),
            &mut ViewportSession::default(),
            RecallViewportMode::AutomaticJudged,
        );
        assert_eq!(judged.kept_candidates.len(), 1);
        assert!(judged.presentation.found);
        assert!(judged.kept_candidates[0].body.is_none());

        input.retrieval_candidates.clear();
        let empty = apply_viewport(
            input.clone(),
            &mut ViewportSession::default(),
            RecallViewportMode::AutomaticJudged,
        );
        assert!(!empty.presentation.found);
        assert!(empty.presentation.semantic_chunks.is_empty());
        assert!(empty.presentation.content_chunks.is_empty());

        let manual = apply_viewport(
            input,
            &mut ViewportSession::default(),
            RecallViewportMode::Manual,
        );
        assert!(manual.presentation.found);
        assert_eq!(manual.presentation.semantic_chunks.len(), 1);
    }

    #[test]
    fn exact_records_with_no_terms_repeat_but_duplicate_records_do_not() {
        let mut first = record(72, "An exact record.");
        first["matched_terms"] = serde_json::json!([]);
        first["id"] = serde_json::json!("first-card");
        let mut duplicate = first.clone();
        duplicate["id"] = serde_json::json!("other-card");
        duplicate["source_path"] = serde_json::json!("another-source.md");
        let input = result(
            "72",
            serde_json::json!([]),
            serde_json::json!([first, duplicate]),
        );
        let mut session = ViewportSession::default();

        for mode in [
            RecallViewportMode::Automatic,
            RecallViewportMode::AutomaticJudged,
            RecallViewportMode::Manual,
        ] {
            let viewport = apply_viewport(input.clone(), &mut session, mode);
            assert_eq!(viewport.kept_candidates.len(), 1);
            assert_eq!(viewport.diagnostics.reasons.get("duplicate"), Some(&1));
            assert!(viewport.presentation.found);
        }
    }

    #[test]
    fn judged_selection_still_requires_identity_and_obeys_payload_caps() {
        let mut input = result(
            "semantic",
            serde_json::json!([]),
            serde_json::json!([{
                "source_path": "", "title": "A title is not a record identity",
                "excerpt": "unsupported"
            }]),
        );
        let refused = apply_viewport(
            input.clone(),
            &mut ViewportSession::default(),
            RecallViewportMode::AutomaticJudged,
        );
        assert!(!refused.presentation.found);
        assert_eq!(
            refused.diagnostics.reasons.get("missing-identity"),
            Some(&1)
        );

        input.retrieval_candidates = serde_json::from_value(serde_json::json!(
            (1..=(MAX_KEPT_CANDIDATES as i64 + 1))
                .map(|id| record(id, "body"))
                .collect::<Vec<_>>()
        ))
        .unwrap();
        let capped = apply_viewport(
            input,
            &mut ViewportSession::default(),
            RecallViewportMode::AutomaticJudged,
        );
        assert_eq!(capped.kept_candidates.len(), MAX_KEPT_CANDIDATES);
        assert_eq!(capped.diagnostics.reasons.get("record-cap"), Some(&1));
    }

    #[test]
    fn canon_deduplicates_within_each_result_without_historical_suppression() {
        let mut canon = athanor("Authoritative topical context.");
        let duplicate = canon[0].clone();
        canon.as_array_mut().unwrap().push(duplicate);
        let input = result("The Athanor", canon, serde_json::json!([]));
        let mut session = ViewportSession::default();

        for _ in 0..2 {
            let viewport =
                apply_viewport(input.clone(), &mut session, RecallViewportMode::Automatic);
            assert_eq!(viewport.presentation.canon_matches.len(), 1);
            assert!(viewport.presentation.found);
            assert_eq!(viewport.diagnostics.reasons.get("duplicate"), Some(&1));
        }
    }

    #[test]
    fn excerpt_trimming_preserves_unicode_passage_offsets() {
        let mut candidate = record(73, &"界".repeat(MAX_CANDIDATE_EXCERPT_CHARS + 20));
        candidate["passage"] = serde_json::json!({
            "char_start": 40,
            "char_end": 40 + MAX_CANDIDATE_EXCERPT_CHARS + 20,
            "chunk_index": 3
        });
        let viewport = apply_viewport(
            result("73", serde_json::json!([]), serde_json::json!([candidate])),
            &mut ViewportSession::default(),
            RecallViewportMode::AutomaticJudged,
        );
        let card = &viewport.kept_candidates[0];
        let passage = card.passage.as_ref().unwrap();
        assert_eq!(passage.char_start, 40);
        assert_eq!(
            passage.char_end - passage.char_start,
            card.excerpt.chars().count() as u64
        );
        assert_eq!(passage.chunk_index, Some(3));
    }
}

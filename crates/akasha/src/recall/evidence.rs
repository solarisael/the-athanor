use super::memory_reference::MemoryReferences;
use crate::bm25f;
use crate::config::QUERY_DATE_RE;
use chrono::NaiveDate;
use std::collections::BTreeSet;

pub(super) fn query_dates(query: &str) -> Vec<NaiveDate> {
    QUERY_DATE_RE
        .captures_iter(query)
        .filter_map(|c| {
            NaiveDate::from_ymd_opt(c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
/// Canon lookup terms: every alphanumeric fragment and whitespace compound,
/// filler included, because a canonical name can be any word (`Like`).
pub(super) fn lookup_terms(query: &str) -> Vec<String> {
    words_to_terms(query.split_whitespace(), |_| true)
}

/// Ranking and coverage terms: the lookup terms of words that carry a subject.
pub(crate) fn query_terms(query: &str) -> Vec<String> {
    words_to_terms(bm25f::content_words(query), bm25f::is_content_term)
}

fn words_to_terms<'a>(words: impl Iterator<Item = &'a str>, keep: fn(&str) -> bool) -> Vec<String> {
    let mut terms = BTreeSet::new();
    for word in words {
        terms.extend(
            word.split(|c: char| !c.is_alphanumeric())
                .map(str::to_lowercase)
                .filter(|term| term.len() >= 2 && keep(term)),
        );
        // Compound tokens split only on whitespace so `-`, `/`, `.`, `_` survive
        // inside a token (pais/mais, queue-maintenance); edge punctuation that is
        // not part of a compound (quotes, commas, trailing periods) is trimmed.
        let compound = word
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        if compound.len() >= 2 && keep(&compound) {
            terms.insert(compound);
        }
    }
    terms.into_iter().collect()
}

pub(super) fn like_patterns(terms: &[String]) -> Vec<String> {
    terms.iter().map(|term| format!("%{term}%")).collect()
}
/// The text of the one semantic retry: subject tokens in query order, first
/// occurrence kept, exact-reference tokens gone. `None` when the query is all
/// filler or holds no filler, because the retry would embed nothing or embed
/// the same meaning again.
pub(super) fn subject_retry_text(query: &str, references: &MemoryReferences) -> Option<String> {
    let is_reference = |token: &String| {
        references
            .terms
            .contains(token.trim_matches(|c: char| !c.is_alphanumeric()))
    };
    let tokens = bm25f::content_tokens(query);
    let mut seen = BTreeSet::new();
    let subject = tokens
        .iter()
        .filter(|token| !is_reference(token) && seen.insert(token.as_str()))
        .map(String::as_str)
        .collect::<Vec<_>>();
    let distinct_tokens = bm25f::tokens(query)
        .into_iter()
        .filter(|token| token.len() >= 2 && !is_reference(token))
        .collect::<BTreeSet<_>>();
    (!subject.is_empty() && subject.len() < distinct_tokens.len()).then(|| subject.join(" "))
}
pub(crate) fn term_evidence(terms: &[String], fields: &[&str]) -> (Vec<String>, Vec<String>) {
    let haystack = fields.join(" ").to_lowercase();
    let matched = terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let missing = terms
        .iter()
        .filter(|term| !matched.contains(term))
        .cloned()
        .collect::<Vec<_>>();
    (matched, missing)
}

const EXCERPT_MAX_CHARS: usize = 1200;

pub(crate) fn exceeds_excerpt(body: &str) -> bool {
    body.chars().count() > EXCERPT_MAX_CHARS
}

pub(crate) fn bounded_excerpt(body: &str) -> String {
    let excerpt: String = body.chars().take(EXCERPT_MAX_CHARS).collect();
    if exceeds_excerpt(body) {
        format!("{excerpt}…")
    } else {
        excerpt
    }
}

pub(crate) fn candidate_terms(
    terms: &[String],
    fields: &[&str],
) -> (Vec<String>, Vec<String>, f64) {
    let (matched, missing) = term_evidence(terms, fields);
    let coverage = if terms.is_empty() {
        0.0
    } else {
        matched.len() as f64 / terms.len() as f64
    };
    (matched, missing, coverage)
}

/// Keep the reranking sidecar deliberately narrower than the regular recall
/// result. It receives only candidate cards with bounded excerpts; raw lane
/// arrays, record bodies, canon, taxonomy, and neighbors never enter it.
fn bounded_rerank_candidate(candidate: &serde_json::Value) -> Option<serde_json::Value> {
    let memory_id = candidate["memory_id"].as_i64()?;
    let excerpt = candidate
        .get("excerpt")
        .or_else(|| candidate.get("body"))
        .and_then(serde_json::Value::as_str)?;
    let mut projected = serde_json::Map::new();
    for key in [
        "memory_id",
        "source_path",
        "title",
        "heading_path",
        "sources",
        "term_coverage",
        "matched_terms",
        "missing_terms",
        "score",
        "semantic_score",
        "content_score",
        "bm25f_score",
        "bm25f_fields",
        "semantic_lexical_score",
        "semantic_lexical_fields",
        "durability",
        "temporal_weight",
        "reasons",
        "source",
        "chunk_index",
    ] {
        if let Some(value) = candidate.get(key) {
            projected.insert(key.to_owned(), value.clone());
        }
    }
    projected.insert("memory_id".to_owned(), serde_json::Value::from(memory_id));
    projected.insert(
        "excerpt".to_owned(),
        serde_json::Value::String(bounded_excerpt(excerpt)),
    );
    if !projected.contains_key("score") {
        let score = candidate
            .get("bm25f_score")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or_default();
        projected.insert("score".to_owned(), serde_json::Value::from(score));
    }
    Some(serde_json::Value::Object(projected))
}

pub(super) fn should_build_rerank_candidates(
    projection: hearth::RecallProjection,
    top_k: u32,
) -> bool {
    projection.is_auto() && top_k > 0
}

pub(super) fn build_rerank_candidates(
    baseline: &[serde_json::Value],
    broad: &[serde_json::Value],
    exact_memory_ids: &BTreeSet<i64>,
    top_k: usize,
) -> Vec<serde_json::Value> {
    let mut seen = BTreeSet::new();
    baseline
        .iter()
        .chain(broad)
        .filter_map(bounded_rerank_candidate)
        .filter(|candidate| {
            let Some(memory_id) = candidate["memory_id"].as_i64() else {
                return false;
            };
            !exact_memory_ids.contains(&memory_id) && seen.insert(memory_id)
        })
        .take(top_k)
        .collect()
}

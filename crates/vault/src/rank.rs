use crate::config::VaultSettings;
use crate::documents::VaultDocument;
use crate::index::VaultIndex;
use crate::model::VaultCandidate;
use crate::text::{STOPWORDS, normalized_text, tokens};
use regex::Regex;
use std::collections::{BTreeSet, HashMap, HashSet};

pub(crate) const FIELD_COUNT: usize = 7;
pub(crate) const DEFAULT_MAX_RESULTS: usize = 8;
pub(crate) const DEFAULT_EXCERPT_CHARS: usize = 900;

// Term-frequency saturation, shared on purpose with akasha's BM25F
// (crates/akasha/src/bm25f.rs): both rankers answer the same operator over the
// same prose, so a repeated term must not bend differently by mode. Moving one
// value alone is a deliberate divergence, never a tidy-up.
const K1: f64 = 1.2;

// A verbatim phrase in a short labelled field (path, title, heading, keys,
// tags, metadata) is a stronger claim than the same phrase buried in a body,
// so a body hit earns the smaller boost.
const EXACT_PHRASE_BOOST: f64 = 2.25;
const EXACT_PHRASE_BODY_BOOST: f64 = 1.5;

// Floor for reading a separator-bearing token as a compound identifier
// (HINGE-PROTOCOL-77, crates/vault/src). Shorter tokens like "v1.0" or "a-b"
// would gate every result on punctuation.
const COMPOUND_TERM_MIN_CHARS: usize = 4;

// Floor for an exact-phrase claim: one- and two-character phrases occur in
// nearly every document and would hand the phrase boost to all of them.
const EXACT_PHRASE_MIN_CHARS: usize = 3;

// The three tables below are index-aligned with `Field` and with the per-field
// arrays on `VaultDocument`; a room override in .solarisael-room.json is keyed
// by these names.
pub(crate) const FIELD_NAMES: [&str; FIELD_COUNT] = [
    "path", "title", "heading", "keys", "tags", "body", "metadata",
];
// Domain tuning a room may override: path and title outrank body because a
// vault room is a file tree an operator named by hand, and the name he chose
// carries more intent than a line inside the file.
pub(crate) const DEFAULT_FIELD_WEIGHTS: [f64; FIELD_COUNT] = [4.2, 3.8, 3.4, 2.6, 2.8, 1.0, 1.4];
// BM25 b per field: how hard length dilutes a match. Short labelled fields stay
// near zero because their length says nothing about relevance; body sits high.
pub(crate) const DEFAULT_FIELD_LENGTH_NORMALIZATIONS: [f64; FIELD_COUNT] =
    [0.2, 0.25, 0.3, 0.45, 0.3, 0.75, 0.5];

#[derive(Clone, Copy)]
enum Field {
    Path,
    Title,
    Heading,
    Keys,
    Tags,
    Body,
    Metadata,
}
const FIELDS: [Field; FIELD_COUNT] = [
    Field::Path,
    Field::Title,
    Field::Heading,
    Field::Keys,
    Field::Tags,
    Field::Body,
    Field::Metadata,
];
impl Field {
    fn index(self) -> usize {
        self as usize
    }
    fn name(self) -> &'static str {
        FIELD_NAMES[self.index()]
    }
}

fn query_terms(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let all = tokens(query)
        .into_iter()
        .filter(|term| seen.insert(term.clone()))
        .collect::<Vec<_>>();
    let meaningful = all
        .iter()
        .filter(|term| term.len() > 1 && !STOPWORDS.contains(&term.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if meaningful.is_empty() {
        all
    } else {
        meaningful
    }
}
fn quoted_terms(query: &str) -> Vec<String> {
    let quote = Regex::new(r#"[\"“”]([^\"“”]+)[\"“”]"#).expect("static quote regex");
    quote
        .captures_iter(query)
        .filter_map(|capture| capture.get(1).map(|value| normalized_text(value.as_str())))
        .filter(|value| !value.is_empty())
        .collect()
}
fn excerpt(document: &VaultDocument, terms: &[String], excerpt_chars: usize) -> String {
    let body = document
        .body
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if body.chars().count() <= excerpt_chars {
        return body;
    }
    let lower = normalized_text(&body);
    let position = terms
        .iter()
        .filter_map(|term| lower.find(term))
        .min()
        .unwrap_or(0);
    let desired_start = position.saturating_sub(excerpt_chars / 3);
    let mut start = desired_start.min(body.len());
    while start > 0 && !body.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = start;
    for (offset, character) in body[start..].char_indices().take(excerpt_chars) {
        end = start + offset + character.len_utf8();
    }
    let clipped = body[start..end].trim();
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        clipped,
        if end < body.len() { "…" } else { "" }
    )
}
pub(crate) fn rank(
    index: &VaultIndex,
    query: &str,
    settings: &VaultSettings,
) -> Vec<VaultCandidate> {
    let terms = query_terms(query);
    if terms.is_empty() || index.documents.is_empty() {
        return Vec::new();
    }
    let compound_terms = terms
        .iter()
        .filter(|term| {
            term.len() >= COMPOUND_TERM_MIN_CHARS
                && term
                    .chars()
                    .any(|character| matches!(character, '_' | ':' | '+' | '#' | '.' | '/' | '-'))
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut averages = [1.0; FIELD_COUNT];
    for field in FIELDS {
        averages[field.index()] = (index
            .documents
            .iter()
            .map(|document| document.lengths[field.index()])
            .sum::<usize>() as f64
            / index.documents.len() as f64)
            .max(1.0);
    }
    let frequencies = terms
        .iter()
        .map(|term| {
            let count = index
                .documents
                .iter()
                .filter(|document| document.terms.iter().any(|field| field.contains_key(term)))
                .count();
            (term.clone(), count)
        })
        .collect::<HashMap<_, _>>();
    let exact_phrases = std::iter::once(normalized_text(query).trim().to_owned())
        .chain(quoted_terms(query))
        .filter(|term| term.chars().count() >= EXACT_PHRASE_MIN_CHARS)
        .collect::<BTreeSet<_>>();
    let total = index.documents.len() as f64;
    let mut ranked = Vec::new();
    for document in &index.documents {
        let mut score = 0.0;
        let mut matched_terms = Vec::new();
        let mut matched_fields = BTreeSet::new();
        for term in &terms {
            let mut combined_tf = 0.0;
            for field in FIELDS {
                let tf = *document.terms[field.index()].get(term).unwrap_or(&0) as f64;
                if tf <= 0.0 {
                    continue;
                }
                matched_fields.insert(field.index());
                let b = settings.field_length_normalizations[field.index()];
                combined_tf += settings.field_weights[field.index()] * tf
                    / (1.0 - b
                        + b * document.lengths[field.index()] as f64 / averages[field.index()]);
            }
            if combined_tf <= 0.0 {
                continue;
            }
            matched_terms.push(term.clone());
            let frequency = *frequencies.get(term).unwrap_or(&0) as f64;
            score += (1.0 + (total - frequency + 0.5) / (frequency + 0.5)).ln()
                * ((K1 + 1.0) * combined_tf)
                / (K1 + combined_tf);
        }
        let mut exact_fields = BTreeSet::new();
        for phrase in &exact_phrases {
            for field in FIELDS {
                if normalized_text(&document.fields[field.index()]).contains(phrase) {
                    exact_fields.insert(field.index());
                    score += settings.field_weights[field.index()]
                        * if matches!(field, Field::Body) {
                            EXACT_PHRASE_BODY_BOOST
                        } else {
                            EXACT_PHRASE_BOOST
                        };
                }
            }
        }
        if score <= 0.0
            || (!compound_terms.is_empty()
                && !compound_terms
                    .iter()
                    .any(|term| matched_terms.contains(term)))
        {
            continue;
        }
        let reasons = [
            (!matched_fields.is_empty()).then(|| {
                format!(
                    "BM25F fields: {}",
                    matched_fields
                        .iter()
                        .map(|index| FIELDS[*index].name())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }),
            (!exact_fields.is_empty()).then(|| {
                format!(
                    "exact content fields: {}",
                    exact_fields
                        .iter()
                        .map(|index| FIELDS[*index].name())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }),
        ]
        .into_iter()
        .flatten()
        .collect();
        let missing_terms = terms
            .iter()
            .filter(|term| !matched_terms.contains(term))
            .cloned()
            .collect();
        ranked.push((score, document, matched_terms, missing_terms, reasons));
    }
    ranked.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.source_path.cmp(&right.1.source_path))
            .then_with(|| left.1.heading_path.cmp(&right.1.heading_path))
    });
    ranked
        .into_iter()
        .take(settings.max_results)
        .map(
            |(score, document, matched_terms, missing_terms, reasons)| VaultCandidate {
                source_path: document.source_path.clone(),
                title: document.title.clone(),
                heading_path: document.heading_path.clone(),
                sources: vec![document.source_path.clone()],
                score,
                term_coverage: matched_terms.len() as f64 / terms.len().max(1) as f64,
                excerpt: excerpt(document, &matched_terms, settings.excerpt_chars),
                matched_terms,
                missing_terms,
                reasons,
            },
        )
        .collect()
}

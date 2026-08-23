use crate::documents::VaultDocument;
use crate::index::VaultIndex;
use crate::model::VaultCandidate;
use crate::text::{STOPWORDS, normalized_text, tokens};
use regex::Regex;
use std::collections::{BTreeSet, HashMap, HashSet};

pub(crate) const MAX_RESULTS: usize = 8;
pub(crate) const EXCERPT_CHARS: usize = 900;

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
const FIELDS: [Field; 7] = [
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
        match self {
            Self::Path => "path",
            Self::Title => "title",
            Self::Heading => "heading",
            Self::Keys => "keys",
            Self::Tags => "tags",
            Self::Body => "body",
            Self::Metadata => "metadata",
        }
    }
    fn weight(self) -> f64 {
        match self {
            Self::Path => 4.2,
            Self::Title => 3.8,
            Self::Heading => 3.4,
            Self::Keys => 2.6,
            Self::Tags => 2.8,
            Self::Body => 1.0,
            Self::Metadata => 1.4,
        }
    }
    fn length_normalization(self) -> f64 {
        match self {
            Self::Path => 0.2,
            Self::Title => 0.25,
            Self::Heading => 0.3,
            Self::Keys => 0.45,
            Self::Tags => 0.3,
            Self::Body => 0.75,
            Self::Metadata => 0.5,
        }
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
fn excerpt(document: &VaultDocument, terms: &[String]) -> String {
    let body = document
        .body
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if body.chars().count() <= EXCERPT_CHARS {
        return body;
    }
    let lower = normalized_text(&body);
    let position = terms
        .iter()
        .filter_map(|term| lower.find(term))
        .min()
        .unwrap_or(0);
    let desired_start = position.saturating_sub(EXCERPT_CHARS / 3);
    let mut start = desired_start.min(body.len());
    while start > 0 && !body.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = start;
    for (offset, character) in body[start..].char_indices().take(EXCERPT_CHARS) {
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
pub(crate) fn rank(index: &VaultIndex, query: &str) -> Vec<VaultCandidate> {
    let terms = query_terms(query);
    if terms.is_empty() || index.documents.is_empty() {
        return Vec::new();
    }
    let compound_terms = terms
        .iter()
        .filter(|term| {
            term.len() >= 4
                && term
                    .chars()
                    .any(|character| matches!(character, '_' | ':' | '+' | '#' | '.' | '/' | '-'))
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut averages = [1.0; 7];
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
        .filter(|term| term.chars().count() >= 3)
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
                let b = field.length_normalization();
                combined_tf += field.weight() * tf
                    / (1.0 - b
                        + b * document.lengths[field.index()] as f64 / averages[field.index()]);
            }
            if combined_tf <= 0.0 {
                continue;
            }
            matched_terms.push(term.clone());
            let frequency = *frequencies.get(term).unwrap_or(&0) as f64;
            score += (1.0 + (total - frequency + 0.5) / (frequency + 0.5)).ln()
                * (2.2 * combined_tf)
                / (1.2 + combined_tf);
        }
        let mut exact_fields = BTreeSet::new();
        for phrase in &exact_phrases {
            for field in FIELDS {
                if normalized_text(&document.fields[field.index()]).contains(phrase) {
                    exact_fields.insert(field.index());
                    score += field.weight()
                        * if matches!(field, Field::Body) {
                            1.5
                        } else {
                            2.25
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
        .take(MAX_RESULTS)
        .map(
            |(score, document, matched_terms, missing_terms, reasons)| VaultCandidate {
                source_path: document.source_path.clone(),
                title: document.title.clone(),
                heading_path: document.heading_path.clone(),
                sources: vec![document.source_path.clone()],
                score,
                term_coverage: matched_terms.len() as f64 / terms.len().max(1) as f64,
                excerpt: excerpt(document, &matched_terms),
                matched_terms,
                missing_terms,
                reasons,
            },
        )
        .collect()
}

use regex::Regex;
use std::{collections::BTreeMap, sync::LazyLock};

static TOKEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{L}\p{N}_:+#./-]+").expect("BM25F token regex must compile"));

const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "but", "by", "da", "das", "de", "do", "dos",
    "e", "em", "for", "from", "had", "has", "have", "i", "if", "in", "is", "it", "its", "na",
    "nas", "no", "nos", "o", "of", "on", "or", "os", "para", "por", "que", "se", "so", "than",
    "that", "the", "their", "them", "then", "this", "to", "um", "uma", "was", "we", "were", "with",
    "you",
];

const K1: f64 = 1.2;

#[derive(Clone, Copy, Debug)]
pub(crate) struct FieldConfig {
    pub weight: f64,
    pub length_normalization: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct FieldValue<'a> {
    pub name: &'static str,
    pub text: &'a str,
    pub average_length: f64,
    pub config: FieldConfig,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Score {
    pub value: f64,
    pub matched_terms: Vec<String>,
    pub matched_fields: Vec<&'static str>,
}

pub(crate) const TITLE: FieldConfig = FieldConfig {
    weight: 4.0,
    length_normalization: 0.30,
};
pub(crate) const HEADING: FieldConfig = FieldConfig {
    weight: 2.5,
    length_normalization: 0.50,
};
pub(crate) const SOURCE_PATH: FieldConfig = FieldConfig {
    weight: 2.0,
    length_normalization: 0.20,
};
pub(crate) const THREADS: FieldConfig = FieldConfig {
    weight: 2.0,
    length_normalization: 0.30,
};
pub(crate) const BODY: FieldConfig = FieldConfig {
    weight: 1.0,
    length_normalization: 0.75,
};
pub(crate) const MEMORY_TYPE: FieldConfig = FieldConfig {
    weight: 0.5,
    length_normalization: 0.0,
};

pub(crate) fn tokens(text: &str) -> Vec<String> {
    TOKEN_RE
        .find_iter(text)
        .map(|token| token.as_str().to_lowercase())
        .collect()
}

pub(crate) fn query_terms(query: &str) -> Vec<String> {
    tokens(query)
        .into_iter()
        .filter(|term| term.len() >= 2 && STOPWORDS.binary_search(&term.as_str()).is_err())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}
pub(crate) fn score(
    query_terms: &[String],
    document_count: u64,
    document_frequency: &BTreeMap<String, u64>,
    fields: &[FieldValue<'_>],
) -> Score {
    if query_terms.is_empty() || document_count == 0 {
        return Score::default();
    }

    let tokenized_fields = fields
        .iter()
        .map(|field| (field, tokens(field.text)))
        .collect::<Vec<_>>();
    let mut value = 0.0;
    let mut matched_terms = Vec::new();
    let mut matched_fields = Vec::new();

    for term in query_terms {
        let mut weighted_frequency = 0.0;
        let mut term_fields = Vec::new();
        for (field, field_tokens) in &tokenized_fields {
            let frequency = field_tokens
                .iter()
                .filter(|token| token.as_str() == term)
                .count() as f64;
            if frequency == 0.0 {
                continue;
            }
            let length = field_tokens.len() as f64;
            let average_length = field.average_length.max(1.0);
            let normalization = 1.0 - field.config.length_normalization
                + field.config.length_normalization * (length / average_length);
            weighted_frequency += field.config.weight * frequency / normalization.max(f64::EPSILON);
            term_fields.push(field.name);
        }
        if weighted_frequency == 0.0 {
            continue;
        }

        let frequency = document_frequency.get(term).copied().unwrap_or(0) as f64;
        let corpus_size = document_count as f64;
        let inverse_document_frequency =
            (1.0 + (corpus_size - frequency + 0.5) / (frequency + 0.5)).ln();
        value += inverse_document_frequency
            * ((K1 + 1.0) * weighted_frequency / (K1 + weighted_frequency));
        matched_terms.push(term.clone());
        for field in term_fields {
            if !matched_fields.contains(&field) {
                matched_fields.push(field);
            }
        }
    }

    Score {
        value,
        matched_terms,
        matched_fields,
    }
}

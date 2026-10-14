use regex::Regex;
use std::{collections::BTreeMap, sync::LazyLock};

static TOKEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{L}\p{N}_:+#./-]+").expect("BM25F token regex must compile"));

// enough: repeated laughter syllables and the operator's `xzclkm` keysmash
// cluster only; a new spelling joins the alternation, never a heuristic that
// could swallow an identifier.
static LAUGHTER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:k{3,}|(?:ha|he|hi|ah|eh|ja|je|rs|ks){2,}h?|(?:(?:xz|zx)(?:clkm)+)+)$")
        .expect("laughter regex must compile")
});

// Keep the bilingual vocabulary tabular; `remember` is also a tool name.
#[rustfmt::skip]
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "but", "by", "da", "das", "de", "do", "dos",
    "e", "em", "for", "from", "had", "has", "have", "i", "if", "in", "is", "it", "its", "na", "nas",
    "no", "nos", "o", "of", "on", "or", "os", "para", "por", "que", "se", "so", "than", "that", "the",
    "their", "them", "then", "this", "to", "um", "uma", "was", "we", "were", "with", "you",
    "about", "actually", "again", "all", "also", "am", "any", "around", "ass", "basically", "being",
    "bro", "can", "cant", "could", "damn", "did", "didnt", "does", "doesnt", "doing", "dont", "dude",
    "dummy", "even", "ever", "exactly", "fuck", "fucking", "get", "gonna", "got", "gotta", "he", "her",
    "here", "hey", "him", "his", "hmm", "how", "im", "into", "isnt", "ive", "just", "kek", "kinda",
    "know", "like", "literally", "ll", "lmao", "lmfao", "lol", "man", "many", "maybe", "me", "much",
    "my", "nah", "never", "now", "oh", "ok", "okay", "our", "owo", "pretty", "probably", "quite", "re",
    "really", "rofl", "she", "shit", "should", "some", "something", "sorta", "still", "stuff", "sure",
    "thats", "there", "these", "they", "thing", "things", "think", "those", "too", "uh", "us", "uwu",
    "ve", "very", "wanna", "well", "what", "whats", "when", "where", "which", "who", "why", "will",
    "would", "xd", "yeah", "yep", "yes", "your", "youre",
    "acho", "aí", "assim", "cara", "coisa", "coisas", "ela", "elas", "ele", "eles", "entao", "então",
    "essa", "esse", "eu", "foi", "gente", "isso", "isto", "já", "kk", "lá", "mano", "mas", "mesmo",
    "meu", "minha", "muito", "nao", "não", "ne", "né", "nem", "porque", "pq", "pra", "sei", "sim",
    "ta", "tá", "tambem", "também", "tb", "tem", "ter", "tipo", "vc", "voce", "você",
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

/// True when a lowercased query word or term names a subject: not grammar, not
/// casual filler, not keyboard laughter. A contraction answers for its stem,
/// and every `n't` is a negated auxiliary, so `won't` is filler while `sol's`
/// stays a subject.
pub(crate) fn is_content_term(term: &str) -> bool {
    let bare = term.trim_matches(|c: char| !c.is_alphanumeric());
    let stem = match bare.split_once(['\'', '’']) {
        Some((stem, "t")) if stem.ends_with('n') => return false,
        Some((stem, "t" | "s" | "m" | "d" | "re" | "ve" | "ll")) => stem,
        _ => bare,
    };
    !STOPWORDS.contains(&stem) && !LAUGHTER_RE.is_match(bare)
}

pub(crate) fn content_words(query: &str) -> impl Iterator<Item = &str> {
    query
        .split_whitespace()
        .scan(false, |negated, word| {
            let lower = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            let reminiscing = *negated && lower == "remember";
            *negated = matches!(
                lower.as_str(),
                "don't" | "don’t" | "dont" | "can't" | "can’t" | "cannot"
            );
            Some((!reminiscing && is_content_term(&lower)).then_some(word))
        })
        .flatten()
}

/// Subject tokens in query order. Filler is judged per whitespace word before
/// tokenizing, so a contraction never leaves a fragment (`won` of `won't`).
pub(crate) fn content_tokens(query: &str) -> Vec<String> {
    content_words(query)
        .flat_map(tokens)
        .filter(|term| term.len() >= 2 && is_content_term(term))
        .collect()
}

pub(crate) fn query_terms(query: &str) -> Vec<String> {
    content_tokens(query)
        .into_iter()
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

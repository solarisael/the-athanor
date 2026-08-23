use std::collections::HashMap;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

pub(crate) const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "do", "for", "from", "how", "i", "in",
    "is", "it", "of", "on", "or", "that", "the", "this", "to", "was", "we", "what", "when",
    "where", "which", "with", "you", "ao", "aos", "com", "como", "da", "das", "de", "do", "dos",
    "e", "em", "eu", "na", "nas", "no", "nos", "o", "os", "ou", "para", "por", "que", "se", "um",
    "uma",
];

pub(crate) fn normalized_text(value: &str) -> String {
    value
        .nfkd()
        .filter(|character| !is_combining_mark(*character))
        .flat_map(char::to_lowercase)
        .collect()
}
fn token_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | ':' | '+' | '#' | '.' | '/' | '-')
}
fn trim_token(value: &str) -> &str {
    value.trim_matches(|character| matches!(character, '_' | ':' | '+' | '#' | '.' | '/' | '-'))
}
pub(crate) fn tokens(value: &str) -> Vec<String> {
    let normalized = normalized_text(value);
    let mut result = Vec::new();
    for raw in normalized.split(|character| !token_character(character)) {
        let token = trim_token(raw);
        if token.is_empty() {
            continue;
        }
        result.push(token.to_owned());
        for part in token.split(['_', ':', '+', '#', '.', '/', '-']) {
            if !part.is_empty() && part != token {
                result.push(part.to_owned());
            }
        }
    }
    result
}
pub(crate) fn term_frequency(value: &str) -> HashMap<String, usize> {
    let mut frequencies = HashMap::new();
    for term in tokens(value) {
        *frequencies.entry(term).or_insert(0) += 1;
    }
    frequencies
}

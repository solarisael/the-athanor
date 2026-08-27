use std::collections::HashSet;

pub(crate) fn normalize_strings(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty() && seen.insert(*value))
        .map(str::to_string)
        .collect()
}

pub(crate) fn normalize_threads(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty() && seen.insert(*value))
        .map(str::to_string)
        .collect()
}

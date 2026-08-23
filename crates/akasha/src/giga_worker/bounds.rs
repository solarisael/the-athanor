
pub(super) const GIGA_MAX_MODEL_RATIONALE_BYTES: usize = 4 * 1024;
pub(super) const GIGA_MAX_STORED_RATIONALE_BYTES: usize = 1_200;
pub(super) const GIGA_RATIONALE_TRUNCATION_MARKER: &str =
    "\n[truncated: classifier rationale is diagnostic, not evidence]";

pub(super) fn bounded_trimmed(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && value.trim() == value
}

pub(super) fn truncate_with_marker(value: &str, maximum: usize, marker: &str) -> String {
    if value.len() <= maximum {
        return value.to_owned();
    }
    if marker.len() >= maximum {
        return marker.chars().take(maximum).collect();
    }
    let mut content_end = maximum - marker.len();
    while !value.is_char_boundary(content_end) {
        content_end -= 1;
    }
    let mut truncated = String::with_capacity(maximum);
    truncated.push_str(&value[..content_end]);
    truncated.push_str(marker);
    truncated
}

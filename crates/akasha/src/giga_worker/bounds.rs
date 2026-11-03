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

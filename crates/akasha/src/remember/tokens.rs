pub(crate) fn token_estimate(text: &str) -> i32 {
    (text.chars().count() / 4).max(1) as i32
}

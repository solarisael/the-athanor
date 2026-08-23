
use super::error::{BoatError, BoatResult};

pub const MAX_TITLE_BYTES: usize = 512;
pub const MAX_SOURCE_PATH_BYTES: usize = 2_048;
pub const MAX_KIND_BYTES: usize = 128;
pub const MAX_WARNING_BYTES: usize = 4_096;

const NON_POSITIVE_ID: &str = "paper boat database ID must be positive";

pub fn positive_id(id: i64) -> BoatResult<u64> {
    u64::try_from(id)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| BoatError::Invalid(NON_POSITIVE_ID.into()))
}

pub fn bounded_utf8(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_owned(), false);
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_utf8_never_splits_a_character() {
        assert_eq!(bounded_utf8("ab💛cd", 5), ("ab".into(), true));
    }
}

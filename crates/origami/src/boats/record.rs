//! Boat record hygiene: bounded fields and positive database IDs.
//!
//! A boat crosses the wire as a record with hard byte ceilings. Clipping
//! happens here, once, and it never splits a UTF-8 character; a clipped
//! field is always announced as a warning by the caller that clipped it.
//!
//! Absorbs house-substrate/src/paper_boat.rs:14 (field ceilings), :197
//! `positive_id`, and :213 `bounded_utf8`.

use super::error::{BoatError, BoatResult};

/// Byte ceiling for a record title.
pub const MAX_TITLE_BYTES: usize = 512;
/// Byte ceiling for a record source path.
pub const MAX_SOURCE_PATH_BYTES: usize = 2_048;
/// Byte ceiling for a record memory kind.
pub const MAX_KIND_BYTES: usize = 128;
/// Byte ceiling for one warning line on a boat receipt.
pub const MAX_WARNING_BYTES: usize = 4_096;

/// The refusal when a database ID cannot address a boat.
const NON_POSITIVE_ID: &str = "paper boat database ID must be positive";

/// Narrow a database ID to the positive range the wire contract uses.
pub fn positive_id(id: i64) -> BoatResult<u64> {
    u64::try_from(id)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| BoatError::Invalid(NON_POSITIVE_ID.into()))
}

/// Clip `value` to at most `max_bytes` UTF-8 bytes on a character
/// boundary. The flag reports whether anything was dropped, so the
/// caller can warn instead of lying about a complete field.
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

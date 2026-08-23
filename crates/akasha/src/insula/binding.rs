use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::{InsulaError, bad};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TrustedBinding {
    pub house_id: String,
    pub room: String,
    pub spirit: String,
    pub session_id: String,
}

pub(super) fn is_house(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.bytes().enumerate().all(|(i, b)| {
            b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || (i > 0 && matches!(b, b'_' | b'.' | b':' | b'-'))
        })
}
pub(super) fn is_room(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && !v.starts_with('-')
        && !v.ends_with('-')
        && !v.contains("--")
        && v.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
pub(super) fn atom(v: &str, n: usize) -> bool {
    !v.is_empty()
        && v.len() <= n
        && v.bytes().enumerate().all(|(i, b)| {
            b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || (i > 0 && matches!(b, b'_' | b'.' | b':' | b'-'))
        })
}
pub(super) fn opaque(v: &str, n: usize) -> bool {
    !v.is_empty()
        && v.len() <= n
        && v.is_ascii()
        && v.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'/' | b'-' | b'@')
        })
}
pub(super) fn uuid(f: &'static str, v: &str) -> Result<(), InsulaError> {
    let u = Uuid::parse_str(v).map_err(|_| bad(f, "malformed_uuid"))?;
    if u.to_string() != v {
        return Err(bad(f, "noncanonical_uuid"));
    }
    Ok(())
}
pub(super) fn binding(v: &TrustedBinding) -> Result<(), InsulaError> {
    if !is_house(&v.house_id) {
        return Err(bad("houseId", "invalid_house_key"));
    }
    if !is_room(&v.room) {
        return Err(bad("room", "invalid_room_key"));
    }
    if !opaque(&v.spirit, 64) {
        return Err(bad("spirit", "invalid_identity_atom"));
    }
    if !opaque(&v.session_id, 128) {
        return Err(bad("sessionId", "invalid_session_id"));
    }
    Ok(())
}
pub fn validate_trusted_binding(value: &TrustedBinding) -> Result<(), InsulaError> {
    binding(value)
}

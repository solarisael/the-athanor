use std::fmt;

use crate::error::DomainError;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RoomKey(String);

impl RoomKey {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        Self::build(value.into(), false)
    }

    pub fn for_anamnesis(value: impl Into<String>) -> Result<Self, DomainError> {
        Self::build(value.into(), true)
    }
    /// Memory writes may target the shared house commons; lesson writes may not.
    pub fn for_memory_write(value: impl Into<String>) -> Result<Self, DomainError> {
        Self::build(value.into(), true)
    }
    /// Canon may be room-local or shared in the House commons.
    pub fn for_canon(value: impl Into<String>) -> Result<Self, DomainError> {
        Self::build(value.into(), true)
    }

    fn build(value: String, allow_house: bool) -> Result<Self, DomainError> {
        if value == "house" && !allow_house {
            return Err(DomainError::ReservedRoomKey);
        }
        if !is_safe_key(&value) {
            return Err(DomainError::InvalidRoomKey(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Lowercase ASCII words joined by single hyphens; room and operator keys share it.
pub(crate) fn is_safe_key(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !value.contains("--")
}

impl fmt::Display for RoomKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

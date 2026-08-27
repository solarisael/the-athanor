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
        let valid = !value.is_empty()
            && !value.starts_with('-')
            && !value.ends_with('-')
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !value.contains("--");
        if !valid {
            return Err(DomainError::InvalidRoomKey(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RoomKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_room_keys_and_reserved_house() {
        assert!(RoomKey::new("living-room2").is_ok());
        for invalid in ["", "Living", "-room", "room-", "two--rooms", "house"] {
            assert!(RoomKey::new(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn shared_house_doors_stay_purpose_scoped() {
        assert!(RoomKey::for_anamnesis("house").is_ok());
        assert!(RoomKey::for_memory_write("house").is_ok());
        assert!(RoomKey::for_memory_write("living-room2").is_ok());
        assert!(RoomKey::for_anamnesis("Living").is_err());
        assert!(RoomKey::for_memory_write("Living").is_err());
        assert!(RoomKey::new("house").is_err());
    }
    #[test]
    fn accepts_canonical_room_keys_longer_than_63_bytes() {
        let room = "a".repeat(64);
        assert!(RoomKey::new(room).is_ok());
    }
}

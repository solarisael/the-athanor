use std::fmt;

#[derive(Debug)]
pub enum VaultError {
    EmptyQuery,
    InvalidRoomDirectory(String),
    RoomMismatch { requested: String, actual: String },
}
impl VaultError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmptyQuery => "empty_query",
            Self::InvalidRoomDirectory(_) => "invalid_room_directory",
            Self::RoomMismatch { .. } => "room_mismatch",
        }
    }
}
impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyQuery => f.write_str("empty query"),
            Self::InvalidRoomDirectory(message) => f.write_str(message),
            Self::RoomMismatch { requested, actual } => {
                write!(f, "room name/path mismatch: {requested} != {actual}")
            }
        }
    }
}
impl std::error::Error for VaultError {}

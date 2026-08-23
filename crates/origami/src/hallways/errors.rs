//! Truthful-refusal discipline: a [`HallwayError::Refusal`] carries a
//! stable code and static text that names the rule the caller broke.
//! Refusals never interpolate state, never guess, and are only raised
//! for a rule that was actually checked — a room genuinely not allowed,
//! a session genuinely bound to another spirit, an idempotency key
//! genuinely reused with different content. Anything the House merely
//! failed to establish is [`HallwayError::Invalid`], not a refusal.
//!
//! These are deliberately the same four shapes the substrate's
//! `AppError` uses for this family, so the adapter in
//! `house-substrate/src/hallway.rs` is a rename and never a reading.

use std::fmt;

#[derive(Debug)]
pub enum HallwayError {
    Invalid(String),
    Refusal {
        code: &'static str,
        message: &'static str,
    },
    Config(String),
    Database(sqlx::Error),
}

impl fmt::Display for HallwayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(formatter, "invalid request: {message}"),
            Self::Refusal { message, .. } => write!(formatter, "refused: {message}"),
            Self::Config(message) => write!(formatter, "configuration error: {message}"),
            Self::Database(error) => write!(formatter, "database error: {error}"),
        }
    }
}

impl std::error::Error for HallwayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            _ => None,
        }
    }
}

impl From<sqlx::Error> for HallwayError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

pub(crate) fn invalid(message: impl Into<String>) -> HallwayError {
    HallwayError::Invalid(message.into())
}

pub(crate) fn refusal(code: &'static str, message: &'static str) -> HallwayError {
    HallwayError::Refusal { code, message }
}

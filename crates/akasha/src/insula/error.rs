use thiserror::Error;

#[derive(Debug, Error)]
pub enum InsulaError {
    #[error("invalid Insula field {field}: {code}")]
    Validation {
        field: &'static str,
        code: &'static str,
    },
    #[error("Insula database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Insula persistence invariant failed: {0}")]
    Invariant(&'static str),
}
pub(super) fn bad(field: &'static str, code: &'static str) -> InsulaError {
    InsulaError::Validation { field, code }
}

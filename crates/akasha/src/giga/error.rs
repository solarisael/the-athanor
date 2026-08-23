use crate::AppError;

pub(super) fn domain_error(error: impl std::fmt::Display) -> AppError {
    AppError::Invalid(error.to_string())
}

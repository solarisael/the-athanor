use crate::config::{AppError, ROOM_KEY_RE};
use uuid::Uuid;

pub(super) fn required<'a>(value: &'a Option<String>, field: &str) -> Result<&'a str, AppError> {
    let value = value
        .as_deref()
        .ok_or_else(|| AppError::Invalid(format!("{field} is required")))?;
    nonempty(value, field)?;
    Ok(value)
}

pub(super) fn nonempty(value: &str, field: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        Err(AppError::Invalid(format!("{field} must not be empty")))
    } else {
        Ok(())
    }
}

pub(super) fn validate_uuid(value: &str, field: &str) -> Result<(), AppError> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| AppError::Invalid(format!("{field} must be a UUID")))
}

pub(super) fn reject_action_fields(fields: &[(&str, bool)]) -> Result<(), AppError> {
    if let Some((field, _)) = fields.iter().find(|(_, present)| *present) {
        Err(AppError::Invalid(format!(
            "{field} is not valid for this action"
        )))
    } else {
        Ok(())
    }
}

pub(super) fn validate_identity(room: &str, spirit: &str, session: &str) -> Result<(), AppError> {
    if !ROOM_KEY_RE.is_match(room) {
        return Err(AppError::Invalid("room must be a lowercase slug".into()));
    }
    nonempty(spirit, "spirit")?;
    nonempty(session, "session")
}

pub(super) fn validate_write_identity(
    room: &str,
    spirit: &str,
    session: &str,
    capability: &str,
    idempotency_key: &str,
) -> Result<(), AppError> {
    validate_identity(room, spirit, session)?;
    nonempty(capability, "capability")?;
    nonempty(idempotency_key, "idempotencyKey")
}

pub(super) fn looks_like_iso8601_duration(value: &str) -> bool {
    value.starts_with('P')
        && value.bytes().any(|byte| byte.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || matches!(byte, b'P' | b'T' | b'D' | b'H' | b'M' | b'S' | b'.')
        })
}

pub(super) fn principal(room: &str, spirit: &str) -> String {
    format!("{room}:{spirit}")
}

pub(super) fn refusal(code: &'static str, message: &'static str) -> AppError {
    AppError::Refusal { code, message }
}

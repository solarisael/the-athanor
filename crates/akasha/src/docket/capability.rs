use super::digest::{constant_time_equal, sha256_hex};
use super::validate::refusal;
use crate::config::AppError;
use sqlx::PgPool;

/// Gate a Docket write before any cooperation-plane state is read or changed.
pub async fn require_docket_capability(
    pool: &PgPool,
    room: &str,
    capability: &str,
) -> Result<(), AppError> {
    let expected: Option<String> = sqlx::query_scalar(
        "SELECT capability_hash FROM docket.room_capabilities WHERE room=$1 AND operation_class='docket_write'",
    )
    .bind(room)
    .fetch_optional(pool)
    .await?;
    let supplied = sha256_hex(capability.as_bytes());
    if expected
        .as_deref()
        .is_none_or(|hash| !constant_time_equal(supplied.as_bytes(), hash.as_bytes()))
    {
        return Err(refusal(
            "docket_capability",
            "the room capability does not authorize Docket writes",
        ));
    }
    Ok(())
}

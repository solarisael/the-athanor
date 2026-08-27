use crate::AppError;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};

pub(super) fn timestamp(value: &str) -> Result<DateTime<Utc>, AppError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| AppError::Invalid("GIGA timestamp is invalid".into()))
}

pub(crate) async fn database_now(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<DateTime<Utc>, AppError> {
    sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
}

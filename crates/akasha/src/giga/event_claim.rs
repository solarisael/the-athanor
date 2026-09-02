use super::clock::database_now;
use super::error::domain_error;
use super::event_store::event_from_store;
use crate::AppError;
use chrono::{DateTime, Duration, Utc};
use hearth::{GIGA_MAX_EVENT_ATTEMPTS, GigaEventClaimReceipt, GigaEventClaimRequest};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};

// Keys are giga_event_attempts column names; the table is the only schema
// (0004_giga_runtime.sql:33-50). `SELECT *` from the record writes over column
// DEFAULTs, so every column is named here: candidate_count carries its DDL
// default 0 and the four outcome columns carry NULL. A new column must be added
// here; the first claim after the migration refuses loudly otherwise.
fn attempt_row(
    event_id: &str,
    replay_count: i32,
    attempt_count: i32,
    room: &str,
    worker_id: &str,
    claimed_at: DateTime<Utc>,
    lease_expires_at: DateTime<Utc>,
) -> Value {
    json!({
        "event_id": event_id,
        "replay_count": replay_count,
        "attempt_count": attempt_count,
        "room": room,
        "worker_id": worker_id,
        "claimed_at": claimed_at,
        "lease_expires_at": lease_expires_at,
        "outcome": null,
        "candidate_count": 0,
        "error_class": null,
        "available_at": null,
        "finished_at": null,
    })
}

pub async fn giga_event_claim(
    pool: &PgPool,
    request: GigaEventClaimRequest,
) -> Result<GigaEventClaimReceipt, AppError> {
    let room = request.room().to_string();
    let mut tx = pool.begin().await?;
    let claimed_at = database_now(&mut tx).await?;
    let lease_expires_at = claimed_at + Duration::seconds(i64::from(request.lease_seconds()));

    let exhausted = sqlx::query(
        "SELECT event_id,replay_count,attempt_count FROM giga_events
         WHERE room=$1 AND queue_state='running' AND lease_expires_at<=$2
           AND attempt_count>=$3
         ORDER BY lease_expires_at,created_at,event_id
         FOR UPDATE SKIP LOCKED",
    )
    .bind(&room)
    .bind(claimed_at)
    .bind(i32::try_from(GIGA_MAX_EVENT_ATTEMPTS).unwrap_or(i32::MAX))
    .fetch_all(&mut *tx)
    .await?;
    for row in exhausted {
        let event_id: String = row.try_get("event_id")?;
        let attempt_count: i32 = row.try_get("attempt_count")?;
        let replay_count: i32 = row.try_get("replay_count")?;
        sqlx::query(
            "UPDATE giga_event_attempts
             SET outcome='lease_expired',error_class='lease_expired_retry_exhausted',
                 finished_at=$4
             WHERE event_id=$1 AND replay_count=$2 AND attempt_count=$3
               AND finished_at IS NULL",
        )
        .bind(&event_id)
        .bind(replay_count)
        .bind(attempt_count)
        .bind(claimed_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE giga_events
             SET queue_state='failed',locked_by=NULL,locked_at=NULL,lease_expires_at=NULL,
                 last_error='lease_expired_retry_exhausted',processed_at=$2,last_finished_at=$2,
                 updated_at=$2
             WHERE event_id=$1",
        )
        .bind(&event_id)
        .bind(claimed_at)
        .execute(&mut *tx)
        .await?;
    }

    let selected = sqlx::query(
        "SELECT event_id,queue_state,replay_count,attempt_count FROM giga_events
         WHERE room=$1 AND attempt_count<$3 AND (
           (queue_state='pending' AND available_at<=$2)
           OR (queue_state='running' AND lease_expires_at<=$2)
         )
         ORDER BY
           CASE WHEN queue_state='pending' THEN 0 ELSE 1 END,
           CASE WHEN queue_state='pending' THEN available_at ELSE lease_expires_at END,
           created_at,event_id
         LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .bind(&room)
    .bind(claimed_at)
    .bind(i32::try_from(GIGA_MAX_EVENT_ATTEMPTS).unwrap_or(i32::MAX))
    .fetch_optional(&mut *tx)
    .await?;
    let Some(selected) = selected else {
        let receipt = GigaEventClaimReceipt::new(
            request.room().clone(),
            request.worker_id().into(),
            claimed_at.to_rfc3339(),
            None,
            None,
            None,
        )
        .map_err(domain_error)?;
        tx.commit().await?;
        return Ok(receipt);
    };

    let event_id: String = selected.try_get("event_id")?;
    let previous_state: String = selected.try_get("queue_state")?;
    let replay_count: i32 = selected.try_get("replay_count")?;
    let previous_attempt: i32 = selected.try_get("attempt_count")?;
    if previous_state == "running" {
        sqlx::query(
            "UPDATE giga_event_attempts
             SET outcome='lease_expired',error_class='lease_expired',finished_at=$4
             WHERE event_id=$1 AND replay_count=$2 AND attempt_count=$3
               AND finished_at IS NULL",
        )
        .bind(&event_id)
        .bind(replay_count)
        .bind(previous_attempt)
        .bind(claimed_at)
        .execute(&mut *tx)
        .await?;
    }
    let attempt_count = previous_attempt + 1;
    // A lease mutation, not a row image: attempt_count/retry_count advance from
    // the stored row and $3 is a predicate inside the CASE, so these scalars
    // stay normal parameters (giga_events, 0004_giga_runtime.sql:3-9).
    sqlx::query(
        "UPDATE giga_events
         SET queue_state='running',attempt_count=$2,
             retry_count=retry_count+CASE WHEN $3='running' THEN 1 ELSE 0 END,
             locked_by=$4,locked_at=$5,lease_expires_at=$6,candidate_count=0,
             processed_at=NULL,last_finished_at=NULL,updated_at=$5
         WHERE event_id=$1",
    )
    .bind(&event_id)
    .bind(attempt_count)
    .bind(&previous_state)
    .bind(request.worker_id())
    .bind(claimed_at)
    .bind(lease_expires_at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO giga_event_attempts
         SELECT * FROM jsonb_populate_record(NULL::giga_event_attempts,$1)",
    )
    .bind(attempt_row(
        &event_id,
        replay_count,
        attempt_count,
        &room,
        request.worker_id(),
        claimed_at,
        lease_expires_at,
    ))
    .execute(&mut *tx)
    .await?;
    let event = event_from_store(&mut tx, &event_id).await?;
    let receipt =
        GigaEventClaimReceipt::new(
            request.room().clone(),
            request.worker_id().into(),
            claimed_at.to_rfc3339(),
            Some(event),
            Some(lease_expires_at.to_rfc3339()),
            Some(u32::try_from(attempt_count).map_err(|_| {
                AppError::Invalid("GIGA attempt count exceeds protocol bounds".into())
            })?),
        )
        .map_err(domain_error)?;
    tx.commit().await?;
    Ok(receipt)
}

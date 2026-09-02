use super::candidate_store::giga_candidate_store_tx;
use super::clock::database_now;
use super::error::domain_error;
use crate::AppError;
use chrono::{DateTime, Duration, Utc};
use hearth::{
    GIGA_MAX_EVENT_ATTEMPTS, GigaCandidate, GigaEventFinishOutcome, GigaEventFinishReceipt,
    GigaEventFinishRequest, GigaQueueState,
};
use sqlx::{PgPool, Postgres, Row, Transaction};

async fn giga_event_finish_tx(
    tx: &mut Transaction<'_, Postgres>,
    request: &GigaEventFinishRequest,
) -> Result<GigaEventFinishReceipt, AppError> {
    let room = request.room().to_string();
    let event = sqlx::query(
        "SELECT room,queue_state,locked_by,locked_at,lease_expires_at,replay_count,attempt_count
         FROM giga_events WHERE event_id=$1 FOR UPDATE",
    )
    .bind(request.event_id())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::Invalid("GIGA event does not exist".into()))?;
    let finished_at = database_now(tx).await?;
    if event.try_get::<Option<String>, _>("room")?.as_deref() != Some(room.as_str()) {
        return Err(AppError::Invalid(
            "GIGA event finish crosses the room boundary".into(),
        ));
    }
    let queue_state: String = event.try_get("queue_state")?;
    let attempt_count: i32 = event.try_get("attempt_count")?;
    let replay_count: i32 = event.try_get("replay_count")?;
    if queue_state != "running" {
        let history = sqlx::query(
            "SELECT worker_id,outcome,candidate_count,error_class,available_at,finished_at
             FROM giga_event_attempts
             WHERE event_id=$1 AND replay_count=$2 AND attempt_count=$3",
        )
        .bind(request.event_id())
        .bind(replay_count)
        .bind(attempt_count)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(history) = history {
            let stored_finished: Option<DateTime<Utc>> = history.try_get("finished_at")?;
            let stored_available: Option<DateTime<Utc>> = history.try_get("available_at")?;
            let stored_retry_delay = match (&stored_available, &stored_finished) {
                (Some(available), Some(finished)) => u32::try_from(
                    available
                        .signed_duration_since(finished.clone())
                        .num_seconds(),
                )
                .ok(),
                _ => None,
            };
            let requested_retry_delay = (request.outcome() == GigaEventFinishOutcome::Retry)
                .then_some(request.retry_after_seconds().unwrap_or(0));
            let matches = history.try_get::<String, _>("worker_id")? == request.worker_id()
                && history.try_get::<Option<String>, _>("outcome")?.as_deref()
                    == Some(request.outcome().as_str())
                && history.try_get::<i32, _>("candidate_count")?
                    == i32::try_from(request.candidate_count()).unwrap_or(i32::MAX)
                && history
                    .try_get::<Option<String>, _>("error_class")?
                    .as_deref()
                    == request.error_class()
                && stored_retry_delay == requested_retry_delay
                && stored_finished.is_some();
            if matches {
                return GigaEventFinishReceipt::new(
                    request.room().clone(),
                    request.event_id().into(),
                    request.worker_id().into(),
                    request.outcome(),
                    GigaQueueState::parse(&queue_state).map_err(domain_error)?,
                    u32::try_from(attempt_count).map_err(|_| {
                        AppError::Invalid("GIGA attempt count exceeds protocol bounds".into())
                    })?,
                    request.candidate_count(),
                    stored_available.map(|value| value.to_rfc3339()),
                    stored_finished.unwrap().to_rfc3339(),
                )
                .map_err(domain_error);
            }
        }
        return Err(AppError::Invalid(
            "GIGA event is not owned by an active lease".into(),
        ));
    }
    if event.try_get::<Option<String>, _>("locked_by")?.as_deref() != Some(request.worker_id()) {
        return Err(AppError::Invalid(
            "GIGA event lease is owned by another worker".into(),
        ));
    }
    let locked_at: DateTime<Utc> = event
        .try_get::<Option<DateTime<Utc>>, _>("locked_at")?
        .ok_or_else(|| AppError::Invalid("GIGA event lease has no claim time".into()))?;
    if finished_at < locked_at {
        return Err(AppError::Invalid(
            "database time precedes the GIGA lease claim".into(),
        ));
    }
    let lease_expires_at: DateTime<Utc> = event
        .try_get::<Option<DateTime<Utc>>, _>("lease_expires_at")?
        .ok_or_else(|| AppError::Invalid("GIGA event lease has no expiry".into()))?;
    if finished_at >= lease_expires_at {
        return Err(AppError::Invalid(
            "GIGA event lease expired before finish".into(),
        ));
    }
    if request.outcome() == GigaEventFinishOutcome::Retry
        && attempt_count >= i32::try_from(GIGA_MAX_EVENT_ATTEMPTS).unwrap_or(i32::MAX)
    {
        return Err(AppError::Invalid(
            "GIGA retry limit is exhausted; finish the event as failed".into(),
        ));
    }
    let stored_candidates: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM giga_candidates WHERE event_id=$1")
            .bind(request.event_id())
            .fetch_one(&mut **tx)
            .await?;
    if stored_candidates != i64::from(request.candidate_count()) {
        return Err(AppError::Invalid(
            "GIGA candidate_count does not match durable candidates for the event".into(),
        ));
    }
    let (next_state, available_at) = match request.outcome() {
        GigaEventFinishOutcome::Succeeded => ("succeeded", None),
        GigaEventFinishOutcome::Retry => (
            "pending",
            Some(
                finished_at.clone()
                    + Duration::seconds(i64::from(request.retry_after_seconds().unwrap_or(0))),
            ),
        ),
        GigaEventFinishOutcome::Failed => ("failed", None),
    };
    let attempt = sqlx::query(
        "UPDATE giga_event_attempts
         SET outcome=$4,candidate_count=$5,error_class=$6,available_at=$7,finished_at=$8
         WHERE event_id=$1 AND replay_count=$2 AND attempt_count=$3
           AND finished_at IS NULL",
    )
    .bind(request.event_id())
    .bind(replay_count)
    .bind(attempt_count)
    .bind(request.outcome().as_str())
    .bind(i32::try_from(request.candidate_count()).unwrap_or(i32::MAX))
    .bind(request.error_class())
    .bind(available_at.clone())
    .bind(finished_at.clone())
    .execute(&mut **tx)
    .await?;
    if attempt.rows_affected() != 1 {
        return Err(AppError::Invalid(
            "GIGA active attempt could not be finished exactly once".into(),
        ));
    }
    sqlx::query(
        "UPDATE giga_events
         SET queue_state=$2,candidate_count=$3,last_error=$4,available_at=COALESCE($5,available_at),
             retry_count=retry_count+CASE WHEN $2='pending' THEN 1 ELSE 0 END,
             locked_by=NULL,locked_at=NULL,lease_expires_at=NULL,
             processed_at=CASE WHEN $2 IN ('succeeded','failed') THEN $6 ELSE NULL END,
             last_finished_at=$6,updated_at=$6
         WHERE event_id=$1",
    )
    .bind(request.event_id())
    .bind(next_state)
    .bind(i32::try_from(request.candidate_count()).unwrap_or(i32::MAX))
    .bind(request.error_class())
    .bind(available_at.clone())
    .bind(finished_at.clone())
    .execute(&mut **tx)
    .await?;
    GigaEventFinishReceipt::new(
        request.room().clone(),
        request.event_id().into(),
        request.worker_id().into(),
        request.outcome(),
        GigaQueueState::parse(next_state).map_err(domain_error)?,
        u32::try_from(attempt_count)
            .map_err(|_| AppError::Invalid("GIGA attempt count exceeds protocol bounds".into()))?,
        request.candidate_count(),
        available_at.map(|value| value.to_rfc3339()),
        finished_at.to_rfc3339(),
    )
    .map_err(domain_error)
}

pub async fn giga_event_finish(
    pool: &PgPool,
    request: GigaEventFinishRequest,
) -> Result<GigaEventFinishReceipt, AppError> {
    let mut tx = pool.begin().await?;
    let receipt = giga_event_finish_tx(&mut tx, &request).await?;
    tx.commit().await?;
    Ok(receipt)
}

pub(crate) async fn giga_candidate_store_and_finish(
    pool: &PgPool,
    candidate: GigaCandidate,
    finish: GigaEventFinishRequest,
) -> Result<GigaEventFinishReceipt, AppError> {
    let mut tx = pool.begin().await?;
    giga_candidate_store_tx(&mut tx, &candidate).await?;
    let receipt = giga_event_finish_tx(&mut tx, &finish).await?;
    tx.commit().await?;
    Ok(receipt)
}

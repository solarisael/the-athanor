use crate::AppError;
use chrono::{DateTime, Utc};
use hearth::{GigaEventReplayReceipt, GigaEventReplayRequest, GigaQueueState};
use sqlx::{PgPool, Row};
use super::clock::database_now;
use super::error::domain_error;

pub async fn giga_event_replay(
    pool: &PgPool,
    request: GigaEventReplayRequest,
) -> Result<GigaEventReplayReceipt, AppError> {
    let room = request.room().to_string();
    let mut tx = pool.begin().await?;
    let event = sqlx::query(
        "SELECT room,queue_state,replay_count FROM giga_events WHERE event_id=$1 FOR UPDATE",
    )
    .bind(request.event_id())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Invalid("GIGA event does not exist".into()))?;
    let replayed_at = database_now(&mut tx).await?;
    if event.try_get::<Option<String>, _>("room")?.as_deref() != Some(room.as_str()) {
        return Err(AppError::Invalid(
            "GIGA event replay crosses the room boundary".into(),
        ));
    }
    let queue_state: String = event.try_get("queue_state")?;
    let replay_count: i32 = event.try_get("replay_count")?;
    if queue_state == "pending" {
        let replay = sqlx::query(
            "SELECT replayed_at FROM giga_event_replays
             WHERE event_id=$1 AND replay_count=$2 AND room=$3 AND operator_identity=$4
               AND authorization_basis=$5",
        )
        .bind(request.event_id())
        .bind(replay_count)
        .bind(&room)
        .bind(request.operator_identity())
        .bind(request.authorization_basis())
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(replay) = replay {
            let stored_replayed_at: DateTime<Utc> = replay.try_get("replayed_at")?;
            let receipt = GigaEventReplayReceipt::new(
                request.room().clone(),
                request.event_id().into(),
                request.operator_identity().into(),
                GigaQueueState::Failed,
                GigaQueueState::Pending,
                0,
                stored_replayed_at.to_rfc3339(),
            )
            .map_err(domain_error)?;
            tx.commit().await?;
            return Ok(receipt);
        }
    }
    if queue_state != "failed" {
        return Err(AppError::Invalid(
            "only failed GIGA work can be replayed".into(),
        ));
    }
    let missing_source_roles: bool = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM giga_event_sources WHERE event_id=$1 AND source_role IS NULL
         )",
    )
    .bind(request.event_id())
    .fetch_one(&mut *tx)
    .await?;
    if missing_source_roles {
        return Err(AppError::Invalid(
            "GIGA event cannot be replayed because its pre-0004 source roles are unavailable"
                .into(),
        ));
    }
    let next_replay_count = replay_count + 1;
    sqlx::query(
        "INSERT INTO giga_event_replays
         (event_id,replay_count,room,operator_identity,authorization_basis,previous_state,replayed_at)
         VALUES ($1,$2,$3,$4,$5,'failed',$6)",
    )
    .bind(request.event_id())
    .bind(next_replay_count)
    .bind(&room)
    .bind(request.operator_identity())
    .bind(request.authorization_basis())
    .bind(replayed_at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE giga_events
         SET queue_state='pending',attempt_count=0,retry_count=0,candidate_count=0,
             available_at=$2,locked_by=NULL,locked_at=NULL,lease_expires_at=NULL,
             processed_at=NULL,last_finished_at=NULL,replay_count=$3,updated_at=$2
         WHERE event_id=$1",
    )
    .bind(request.event_id())
    .bind(replayed_at)
    .bind(next_replay_count)
    .execute(&mut *tx)
    .await?;
    let receipt = GigaEventReplayReceipt::new(
        request.room().clone(),
        request.event_id().into(),
        request.operator_identity().into(),
        GigaQueueState::Failed,
        GigaQueueState::Pending,
        0,
        replayed_at.to_rfc3339(),
    )
    .map_err(domain_error)?;
    tx.commit().await?;
    Ok(receipt)
}

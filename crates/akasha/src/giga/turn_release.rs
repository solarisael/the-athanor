use super::clock::database_now;
use crate::AppError;
use chrono::Duration;
use hearth::RoomKey;
use sqlx::PgPool;

pub(crate) const GIGA_MAX_BATCH_TURNS: u32 = 64;
pub(crate) const GIGA_MAX_BATCH_WAIT_SECONDS: u32 = 3_600;

/// Releases every waiting turn of each conversation (room + session) that holds
/// at least `batch_turns` waiting turns or whose oldest waiting turn has waited
/// `wait_seconds`. The deadline is the oldest turn's capture time, so new turns
/// never postpone it. Release is durable: a released turn stays claimable
/// whatever happens to the rest of its batch. Returns the released turn count.
pub async fn giga_turn_release(
    pool: &PgPool,
    room: &RoomKey,
    batch_turns: u32,
    wait_seconds: u32,
) -> Result<u64, AppError> {
    if !(1..=GIGA_MAX_BATCH_TURNS).contains(&batch_turns)
        || !(1..=GIGA_MAX_BATCH_WAIT_SECONDS).contains(&wait_seconds)
    {
        return Err(AppError::Invalid(format!(
            "GIGA batch release needs 1..={GIGA_MAX_BATCH_TURNS} turns and 1..={GIGA_MAX_BATCH_WAIT_SECONDS} seconds"
        )));
    }
    let mut tx = pool.begin().await?;
    let released_at = database_now(&mut tx).await?;
    let deadline = released_at - Duration::seconds(i64::from(wait_seconds));
    let released = sqlx::query(
        "WITH ready AS (
           SELECT session_id FROM giga_events
           WHERE room=$1 AND waiting_since IS NOT NULL
           GROUP BY session_id
           HAVING COUNT(*)>=$2 OR MIN(waiting_since)<=$3
         )
         UPDATE giga_events e SET waiting_since=NULL,updated_at=$4
         FROM ready
         WHERE e.room=$1 AND e.session_id=ready.session_id AND e.waiting_since IS NOT NULL",
    )
    .bind(room.as_str())
    .bind(i64::from(batch_turns))
    .bind(deadline)
    .bind(released_at)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(released)
}

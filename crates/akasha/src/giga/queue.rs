use crate::AppError;
use hearth::{GigaQueueMaintenanceOperation, GigaQueueMaintenanceRequest, GigaQueueMaintenanceScope};
use protocol::{GigaQueueMaintenanceResult, GigaQueueStateCount};
use sqlx::{PgPool, Postgres, Row, Transaction};

const GIGA_ELIGIBLE_ALL_SQL: &str = "SELECT COUNT(*)::bigint FROM giga_events e
     WHERE (e.queue_state IN ('pending','failed')
            OR (e.queue_state='running' AND e.lease_expires_at <= NOW()))
       AND NOT EXISTS (SELECT 1 FROM giga_candidates c WHERE c.event_id=e.event_id)
       AND NOT EXISTS (SELECT 1 FROM giga_review_resonances r WHERE r.event_id=e.event_id)";

const GIGA_ELIGIBLE_ROOM_SQL: &str = "SELECT COUNT(*)::bigint FROM giga_events e
     WHERE e.room=$1
       AND (e.queue_state IN ('pending','failed')
            OR (e.queue_state='running' AND e.lease_expires_at <= NOW()))
       AND NOT EXISTS (SELECT 1 FROM giga_candidates c WHERE c.event_id=e.event_id)
       AND NOT EXISTS (SELECT 1 FROM giga_review_resonances r WHERE r.event_id=e.event_id)";

const GIGA_ATTEMPTS_ALL_SQL: &str = "SELECT COUNT(*)::bigint FROM giga_event_attempts a
     JOIN giga_events e ON e.event_id=a.event_id
     WHERE (e.queue_state IN ('pending','failed')
            OR (e.queue_state='running' AND e.lease_expires_at <= NOW()))
       AND NOT EXISTS (SELECT 1 FROM giga_candidates c WHERE c.event_id=e.event_id)
       AND NOT EXISTS (SELECT 1 FROM giga_review_resonances r WHERE r.event_id=e.event_id)";

const GIGA_ATTEMPTS_ROOM_SQL: &str = "SELECT COUNT(*)::bigint FROM giga_event_attempts a
     JOIN giga_events e ON e.event_id=a.event_id
     WHERE e.room=$1
       AND (e.queue_state IN ('pending','failed')
            OR (e.queue_state='running' AND e.lease_expires_at <= NOW()))
       AND NOT EXISTS (SELECT 1 FROM giga_candidates c WHERE c.event_id=e.event_id)
       AND NOT EXISTS (SELECT 1 FROM giga_review_resonances r WHERE r.event_id=e.event_id)";

async fn giga_queue_state_counts(
    tx: &mut Transaction<'_, Postgres>,
    room: Option<&str>,
) -> Result<Vec<GigaQueueStateCount>, AppError> {
    let rows = match room {
        Some(room) => {
            sqlx::query(
                "SELECT queue_state,COUNT(*)::bigint AS count
                 FROM giga_events WHERE room=$1 GROUP BY queue_state ORDER BY queue_state",
            )
            .bind(room)
            .fetch_all(&mut **tx)
            .await?
        }
        None => {
            sqlx::query(
                "SELECT queue_state,COUNT(*)::bigint AS count
                 FROM giga_events GROUP BY queue_state ORDER BY queue_state",
            )
            .fetch_all(&mut **tx)
            .await?
        }
    };

    rows.into_iter()
        .map(|row| {
            Ok(GigaQueueStateCount {
                queue_state: row.try_get("queue_state")?,
                count: row.try_get::<i64, _>("count")? as u64,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(AppError::from)
}

async fn giga_queue_count(
    tx: &mut Transaction<'_, Postgres>,
    room: Option<&str>,
    all_sql: &str,
    room_sql: &str,
) -> Result<u64, AppError> {
    let count = match room {
        Some(room) => {
            sqlx::query_scalar::<_, i64>(room_sql)
                .bind(room)
                .fetch_one(&mut **tx)
                .await?
        }
        None => {
            sqlx::query_scalar::<_, i64>(all_sql)
                .fetch_one(&mut **tx)
                .await?
        }
    };

    u64::try_from(count).map_err(|_| AppError::Invalid("GIGA queue count is invalid".into()))
}

pub async fn giga_queue_maintenance(
    pool: &PgPool,
    request: GigaQueueMaintenanceRequest,
) -> Result<GigaQueueMaintenanceResult, AppError> {
    let room = request.room().to_string();
    let scoped_room = match request.scope() {
        GigaQueueMaintenanceScope::Room => Some(room.as_str()),
        GigaQueueMaintenanceScope::All => None,
    };
    let mut tx = pool.begin().await?;

    if request.operation() == GigaQueueMaintenanceOperation::PurgeStuck {
        sqlx::query(
            "SELECT pg_advisory_xact_lock(hashtextextended('solarisael.giga_queue_maintenance', 42))",
        )
        .execute(&mut *tx)
        .await?;
    }

    let before = giga_queue_state_counts(&mut tx, scoped_room).await?;
    let eligible_events = giga_queue_count(
        &mut tx,
        scoped_room,
        GIGA_ELIGIBLE_ALL_SQL,
        GIGA_ELIGIBLE_ROOM_SQL,
    )
    .await?;
    let non_succeeded = giga_queue_count(
        &mut tx,
        scoped_room,
        "SELECT COUNT(*)::bigint FROM giga_events WHERE queue_state <> 'succeeded'",
        "SELECT COUNT(*)::bigint FROM giga_events
         WHERE room=$1 AND queue_state <> 'succeeded'",
    )
    .await?;
    let blocked_events = non_succeeded.saturating_sub(eligible_events);
    let deleted_attempts = if request.operation() == GigaQueueMaintenanceOperation::PurgeStuck {
        giga_queue_count(
            &mut tx,
            scoped_room,
            GIGA_ATTEMPTS_ALL_SQL,
            GIGA_ATTEMPTS_ROOM_SQL,
        )
        .await?
    } else {
        0
    };
    let preserved_candidates = giga_queue_count(
        &mut tx,
        scoped_room,
        "SELECT COUNT(*)::bigint FROM giga_candidates",
        "SELECT COUNT(*)::bigint FROM giga_candidates WHERE room=$1",
    )
    .await?;
    let deleted_events = if request.operation() == GigaQueueMaintenanceOperation::PurgeStuck {
        match scoped_room {
            Some(room) => sqlx::query(
                "DELETE FROM giga_events e
                     WHERE e.room=$1
                       AND (e.queue_state IN ('pending','failed')
                            OR (e.queue_state='running' AND e.lease_expires_at <= NOW()))
                       AND NOT EXISTS (
                           SELECT 1 FROM giga_candidates c WHERE c.event_id=e.event_id
                       )
                       AND NOT EXISTS (
                           SELECT 1 FROM giga_review_resonances r WHERE r.event_id=e.event_id
                       )",
            )
            .bind(room)
            .execute(&mut *tx)
            .await?
            .rows_affected(),
            None => sqlx::query(
                "DELETE FROM giga_events e
                     WHERE (e.queue_state IN ('pending','failed')
                            OR (e.queue_state='running' AND e.lease_expires_at <= NOW()))
                       AND NOT EXISTS (
                           SELECT 1 FROM giga_candidates c WHERE c.event_id=e.event_id
                       )
                       AND NOT EXISTS (
                           SELECT 1 FROM giga_review_resonances r WHERE r.event_id=e.event_id
                       )",
            )
            .execute(&mut *tx)
            .await?
            .rows_affected(),
        }
    } else {
        0
    };
    let after = giga_queue_state_counts(&mut tx, scoped_room).await?;

    tx.commit().await?;

    Ok(GigaQueueMaintenanceResult {
        ok: true,
        operation: request.operation().as_str().into(),
        scope: request.scope().as_str().into(),
        room,
        eligible_events,
        blocked_events,
        deleted_events,
        deleted_attempts,
        preserved_candidates,
        before,
        after,
    })
}

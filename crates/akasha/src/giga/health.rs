use crate::{
    AppError,
    giga_worker::{giga_classifier_enabled, giga_classifier_health},
};
use chrono::{DateTime, Utc};
use protocol::{GigaHealthCount, GigaHealthRequest, GigaHealthResult};
use sqlx::{PgPool, Row};

pub async fn giga_health(
    pool: &PgPool,
    request: GigaHealthRequest,
) -> Result<GigaHealthResult, AppError> {
    let room = request.room().to_string();
    let event = sqlx::query(
        "SELECT COUNT(*) FILTER (WHERE queue_state IN ('pending','running','failed'))::bigint AS queue_depth,
                EXTRACT(EPOCH FROM (NOW()-MIN(created_at) FILTER (WHERE queue_state IN ('pending','running','failed'))))::bigint AS oldest_age,
                COUNT(*) FILTER (WHERE queue_state='succeeded')::bigint AS processed_count,
                COUNT(*) FILTER (WHERE queue_state='failed')::bigint AS failed_count,
                (SELECT latest.last_error FROM giga_events latest
                 WHERE latest.room=$1 AND latest.last_error IS NOT NULL
                 ORDER BY latest.updated_at DESC,latest.event_id LIMIT 1) AS last_error,
                (SELECT latest.updated_at FROM giga_events latest
                 WHERE latest.room=$1 AND latest.last_error IS NOT NULL
                 ORDER BY latest.updated_at DESC,latest.event_id LIMIT 1) AS last_error_at,
                COALESCE((
                    SELECT COUNT(*)::bigint
                    FROM (
                        SELECT outcome,
                               SUM(CASE WHEN outcome='succeeded' THEN 1 ELSE 0 END)
                                   OVER (ORDER BY finished_at DESC,event_id,replay_count,attempt_count
                                         ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS successes
                        FROM giga_event_attempts
                        WHERE room=$1 AND finished_at IS NOT NULL
                    ) recent
                    WHERE recent.successes=0 AND recent.outcome <> 'succeeded'
                ),0)::bigint AS consecutive_failures
         FROM giga_events WHERE room=$1",
    )
    .bind(&room)
    .fetch_one(pool)
    .await?;
    let rows = sqlx::query(
        "SELECT kind,review_state,COUNT(*)::bigint AS count FROM giga_candidates
         WHERE room=$1 GROUP BY kind,review_state ORDER BY kind,review_state",
    )
    .bind(&room)
    .fetch_all(pool)
    .await?;
    let candidates_by_kind_state = rows
        .into_iter()
        .map(|row| {
            Ok(GigaHealthCount {
                kind: row.try_get("kind")?,
                review_state: row.try_get("review_state")?,
                count: row.try_get::<i64, _>("count")? as u64,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let oldest: Option<i64> = event.try_get("oldest_age")?;
    let last_error: Option<String> = event.try_get("last_error")?;
    let last_error_at: Option<DateTime<Utc>> = event.try_get("last_error_at")?;
    let consecutive_failures: u64 = event.try_get::<i64, _>("consecutive_failures")? as u64;
    Ok(GigaHealthResult {
        enabled: giga_classifier_enabled(),
        store_healthy: true,
        queue_depth: event.try_get::<i64, _>("queue_depth")? as u64,
        oldest_queue_age_seconds: oldest.map(|age| age.max(0) as u64),
        processed_count: event.try_get::<i64, _>("processed_count")? as u64,
        failed_count: event.try_get::<i64, _>("failed_count")? as u64,
        candidates_by_kind_state,
        classifier: giga_classifier_health(
            last_error,
            last_error_at.map(|value| value.to_rfc3339()),
            consecutive_failures,
        ),
    })
}

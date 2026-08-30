use crate::{
    AppError,
    giga_worker::{giga_capability_state, giga_classifier_health},
};
use chrono::{DateTime, Utc};
use protocol::{GigaHealthCount, GigaHealthRequest, GigaHealthResult};
use sqlx::PgPool;

pub async fn giga_health(
    pool: &PgPool,
    request: GigaHealthRequest,
) -> Result<GigaHealthResult, AppError> {
    let room = request.room().to_string();
    let (queue_depth, oldest_age, processed_count, failed_count, last_error, last_error_at, consecutive_failures): (
        i64, Option<i64>, i64, i64, Option<String>, Option<DateTime<Utc>>, i64,
    ) = sqlx::query_as(
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
    let candidates_by_kind_state = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT kind,review_state,COUNT(*)::bigint AS count FROM giga_candidates
         WHERE room=$1 GROUP BY kind,review_state ORDER BY kind,review_state",
    )
    .bind(&room)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(kind, review_state, count)| GigaHealthCount {
        kind,
        review_state,
        count: count as u64,
    })
    .collect();
    let capabilities = giga_capability_state();
    Ok(GigaHealthResult {
        capture_enabled: capabilities.capture_enabled,
        classifier_enabled: capabilities.classifier_enabled,
        store_healthy: true,
        queue_depth: queue_depth as u64,
        oldest_queue_age_seconds: oldest_age.map(|age| age.max(0) as u64),
        processed_count: processed_count as u64,
        failed_count: failed_count as u64,
        candidates_by_kind_state,
        classifier: giga_classifier_health(
            last_error,
            last_error_at.map(|value| value.to_rfc3339()),
            consecutive_failures as u64,
        ),
    })
}

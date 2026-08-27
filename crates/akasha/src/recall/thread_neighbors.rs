use super::bounded_excerpt;
use crate::config::AppError;
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;

pub(super) async fn load_thread_neighbors(
    pool: &PgPool,
    memory_ids: &[i64],
) -> Result<BTreeMap<i64, Vec<serde_json::Value>>, AppError> {
    if memory_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let rows = sqlx::query(
        r#"WITH neighbor_edges AS (
             SELECT current_event.memory_id AS candidate_memory_id,
                    t.thread_key,
                    'previous'::text AS direction,
                    0 AS direction_order,
                    neighbor_event.id AS neighbor_event_id,
                    neighbor.id,coalesce(neighbor.title,'') AS title,
                    neighbor.source_path,left(coalesce(neighbor.body,''),1200) AS body,
                    CASE
                      WHEN neighbor.archived_at IS NULL AND neighbor.superseded_by IS NULL
                      THEN 'active' ELSE 'historical'
                    END AS authority_state,
                    neighbor.superseded_by
             FROM thread_events current_event
             JOIN threads t ON t.id=current_event.thread_id
             JOIN thread_event_links link
               ON link.thread_id=current_event.thread_id
              AND link.next_event_id=current_event.id
             JOIN thread_events neighbor_event
               ON neighbor_event.thread_id=link.thread_id
              AND neighbor_event.id=link.previous_event_id
             JOIN memories neighbor ON neighbor.id=neighbor_event.memory_id
             WHERE current_event.memory_id = ANY($1::bigint[])
             UNION ALL
             SELECT current_event.memory_id AS candidate_memory_id,
                    t.thread_key,
                    'next'::text AS direction,
                    1 AS direction_order,
                    neighbor_event.id AS neighbor_event_id,
                    neighbor.id,coalesce(neighbor.title,'') AS title,
                    neighbor.source_path,left(coalesce(neighbor.body,''),1200) AS body,
                    CASE
                      WHEN neighbor.archived_at IS NULL AND neighbor.superseded_by IS NULL
                      THEN 'active' ELSE 'historical'
                    END AS authority_state,
                    neighbor.superseded_by
             FROM thread_events current_event
             JOIN threads t ON t.id=current_event.thread_id
             JOIN thread_event_links link
               ON link.thread_id=current_event.thread_id
              AND link.previous_event_id=current_event.id
             JOIN thread_events neighbor_event
               ON neighbor_event.thread_id=link.thread_id
              AND neighbor_event.id=link.next_event_id
             JOIN memories neighbor ON neighbor.id=neighbor_event.memory_id
             WHERE current_event.memory_id = ANY($1::bigint[])
           ), ranked AS (
             SELECT *,
                    row_number() OVER (
                      PARTITION BY candidate_memory_id
                      ORDER BY thread_key,direction_order,neighbor_event_id,id
                    ) AS ordinal
             FROM neighbor_edges
           )
           SELECT candidate_memory_id,thread_key,direction,id,title,source_path,body,
                  authority_state,superseded_by
           FROM ranked WHERE ordinal <= 6
           ORDER BY candidate_memory_id,ordinal"#,
    )
    .bind(memory_ids)
    .fetch_all(pool)
    .await?;
    let mut by_memory = BTreeMap::new();
    for row in rows {
        let candidate_memory_id: i64 = row.try_get("candidate_memory_id")?;
        let body: String = row.try_get("body")?;
        by_memory
            .entry(candidate_memory_id)
            .or_insert_with(Vec::new)
            .push(serde_json::json!({
                "thread": row.try_get::<String, _>("thread_key")?,
                "direction": row.try_get::<String, _>("direction")?,
                "id": row.try_get::<i64, _>("id")?,
                "title": row.try_get::<String, _>("title")?,
                "source_path": row.try_get::<String, _>("source_path")?,
                "excerpt": bounded_excerpt(&body),
                "authority_state": row.try_get::<String, _>("authority_state")?,
                "superseded_by": row.try_get::<Option<i64>, _>("superseded_by")?,
            }));
    }
    Ok(by_memory)
}

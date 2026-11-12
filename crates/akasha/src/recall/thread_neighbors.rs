use super::Phase;
use super::evidence::bounded_excerpt;
use super::scope::ScopeFilter;
use crate::config::AppError;
use crate::insula_writer::EmitterSpan;
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};

async fn load_thread_neighbors(
    pool: &PgPool,
    memory_ids: &[i64],
    scope: &ScopeFilter,
) -> Result<BTreeMap<i64, Vec<serde_json::Value>>, AppError> {
    if memory_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let neighbor_scope = scope.predicate("neighbor", 2);
    let neighbor_successor = scope.visible_successor("neighbor", 2);
    let sql = format!(
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
                    {neighbor_successor} AS superseded_by
             FROM thread_events current_event
             JOIN threads t ON t.id=current_event.thread_id
             JOIN thread_event_links link
               ON link.thread_id=current_event.thread_id
              AND link.next_event_id=current_event.id
             JOIN thread_events neighbor_event
               ON neighbor_event.thread_id=link.thread_id
              AND neighbor_event.id=link.previous_event_id
             JOIN memories neighbor ON neighbor.id=neighbor_event.memory_id
             WHERE current_event.memory_id = ANY($1::bigint[]){neighbor_scope}
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
                    {neighbor_successor} AS superseded_by
             FROM thread_events current_event
             JOIN threads t ON t.id=current_event.thread_id
             JOIN thread_event_links link
               ON link.thread_id=current_event.thread_id
              AND link.previous_event_id=current_event.id
             JOIN thread_events neighbor_event
               ON neighbor_event.thread_id=link.thread_id
              AND neighbor_event.id=link.next_event_id
             JOIN memories neighbor ON neighbor.id=neighbor_event.memory_id
             WHERE current_event.memory_id = ANY($1::bigint[]){neighbor_scope}
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
           ORDER BY candidate_memory_id,ordinal"#
    );
    let rows = scope
        .bind(sqlx::query(&sql).bind(memory_ids))
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

/// Every active thread of each memory, keyed by memory ID: the thread key, the
/// directly linked previous member, and the next members. A neighbor the reader
/// cannot read is omitted. One query serves the whole candidate set.
async fn load_thread_memberships(
    pool: &PgPool,
    memory_ids: &[i64],
    scope: &ScopeFilter,
) -> Result<BTreeMap<i64, Vec<serde_json::Value>>, AppError> {
    if memory_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let sql = format!(
        r#"SELECT member.memory_id,t.thread_key,
                  (SELECT previous_memory.id
                   FROM thread_event_links link
                   JOIN thread_events previous_event
                     ON previous_event.thread_id=link.thread_id
                    AND previous_event.id=link.previous_event_id
                   JOIN memories previous_memory ON previous_memory.id=previous_event.memory_id
                   WHERE link.thread_id=member.thread_id
                     AND link.next_event_id=member.id{previous_scope}) AS previous_memory_id,
                  ARRAY(SELECT next_memory.id
                        FROM thread_event_links link
                        JOIN thread_events next_event
                          ON next_event.thread_id=link.thread_id
                         AND next_event.id=link.next_event_id
                        JOIN memories next_memory ON next_memory.id=next_event.memory_id
                        WHERE link.thread_id=member.thread_id
                          AND link.previous_event_id=member.id{next_scope}
                        ORDER BY next_event.id) AS next_memory_ids
           FROM thread_events member
           JOIN threads t ON t.id=member.thread_id
           WHERE member.memory_id = ANY($1::bigint[])
           ORDER BY member.memory_id,t.thread_key"#,
        previous_scope = scope.predicate("previous_memory", 2),
        next_scope = scope.predicate("next_memory", 2),
    );
    let rows = scope
        .bind(sqlx::query(&sql).bind(memory_ids))
        .fetch_all(pool)
        .await?;
    let mut by_memory = BTreeMap::new();
    for row in rows {
        let memory_id: i64 = row.try_get("memory_id")?;
        by_memory
            .entry(memory_id)
            .or_insert_with(Vec::new)
            .push(serde_json::json!({
                "key": row.try_get::<String, _>("thread_key")?,
                "previousMemoryId": row.try_get::<Option<i64>, _>("previous_memory_id")?,
                "nextMemoryIds": row.try_get::<Vec<i64>, _>("next_memory_ids")?,
            }));
    }
    Ok(by_memory)
}

/// Amendment edges `(amendedBy, amends)` of each memory, keyed by memory ID,
/// in one query. `amendedBy` names newer memories that correct or update the
/// memory; `amends` names older memories that the memory corrects or updates.
/// An edge to a memory that the reader cannot read is omitted. The newest
/// edge comes first.
async fn load_amendments(
    pool: &PgPool,
    memory_ids: &[i64],
    scope: &ScopeFilter,
) -> Result<BTreeMap<i64, (Vec<serde_json::Value>, Vec<serde_json::Value>)>, AppError> {
    if memory_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let other_scope = scope.predicate("other", 2);
    let sql = format!(
        r#"SELECT a.older_id AS memory_id,true AS amended_by,a.newer_id AS other_id,
                  a.changed,a.keeps
           FROM memory_amendments a
           JOIN memories other ON other.id=a.newer_id
           WHERE a.older_id = ANY($1::bigint[]){other_scope}
           UNION ALL
           SELECT a.newer_id,false,a.older_id,a.changed,a.keeps
           FROM memory_amendments a
           JOIN memories other ON other.id=a.older_id
           WHERE a.newer_id = ANY($1::bigint[]){other_scope}
           ORDER BY 1,2,3 DESC"#
    );
    let rows = scope
        .bind(sqlx::query(&sql).bind(memory_ids))
        .fetch_all(pool)
        .await?;
    let mut by_memory: BTreeMap<i64, (Vec<_>, Vec<_>)> = BTreeMap::new();
    for row in rows {
        let edges = by_memory.entry(row.try_get("memory_id")?).or_default();
        let other_id: i64 = row.try_get("other_id")?;
        let changed: String = row.try_get("changed")?;

        if row.try_get::<bool, _>("amended_by")? {
            edges.0.push(serde_json::json!({
                "memoryId": other_id,
                "changed": changed,
            }));
        } else {
            edges.1.push(serde_json::json!({
                "memoryId": other_id,
                "changed": changed,
                "keeps": row.try_get::<Option<String>, _>("keeps")?,
            }));
        }
    }
    Ok(by_memory)
}

/// Record links for every candidate that owns a memory: `thread_neighbors`
/// is the visible shoulder of each thread, `threads` names every active thread
/// with the exact previous and next memory IDs to follow, and `amendedBy` and
/// `amends` name the amendment edges. An amendment never changes the ranking.
pub(super) async fn attach_memory_links(
    pool: &PgPool,
    span: Option<&EmitterSpan>,
    scope: &ScopeFilter,
    retrieval_candidates: &mut [serde_json::Value],
) -> Result<(), AppError> {
    let memory_ids = retrieval_candidates
        .iter()
        .filter_map(|candidate| candidate["memory_id"].as_i64())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let phase = Phase::start(span, "recall.neighbors");
    let mut neighbors = load_thread_neighbors(pool, &memory_ids, scope).await?;
    let threads = load_thread_memberships(pool, &memory_ids, scope).await?;
    let amendments = load_amendments(pool, &memory_ids, scope).await?;
    for candidate in retrieval_candidates.iter_mut() {
        let memory_id = candidate["memory_id"].as_i64();
        candidate["thread_neighbors"] = serde_json::Value::Array(
            memory_id
                .and_then(|memory_id| neighbors.remove(&memory_id))
                .unwrap_or_default(),
        );
        if let Some(memory_id) = memory_id {
            let (amended_by, amends) = amendments.get(&memory_id).cloned().unwrap_or_default();

            candidate["threads"] =
                serde_json::Value::Array(threads.get(&memory_id).cloned().unwrap_or_default());
            candidate["amendedBy"] = serde_json::Value::Array(amended_by);
            candidate["amends"] = serde_json::Value::Array(amends);
        }
    }
    phase.ok();
    Ok(())
}

use super::thread_join::{lock_room_memories, memory_ids};
use crate::config::AppError;
use crate::recall::scope::ScopeFilter;
use hearth::RoomKey;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};

/// A reason explains one archive decision; it is a note, not a record body.
const MAX_REASON_CHARS: usize = 500;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MemoryThreadLeaveParams {
    pub thread: String,
    pub memory_ids: Vec<String>,
    pub room: String,
    pub reason: String,
    #[serde(default)]
    pub operator: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryThreadLeaveMemory {
    pub id: i64,
    pub title: String,
    pub was_member: bool,
    /// The predecessor that the successors now continue. Absent when the
    /// memory was the first member or the caller cannot read the predecessor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relinked_to: Option<i64>,
    /// The readable successors that this call moved off the memory.
    pub relinked_successors: Vec<i64>,
}

#[derive(Serialize)]
pub struct MemoryThreadLeaveReceipt {
    pub ok: bool,
    pub room: String,
    pub thread: String,
    pub reason: String,
    pub memories: Vec<MemoryThreadLeaveMemory>,
}

/// Moves one active membership into `thread_event_archive` and keeps the
/// active chain whole: the successors continue from the predecessor, and the
/// archive row keeps the original links. Returns `None` when the memory has no
/// active membership; otherwise the readable predecessor and successors.
async fn archive_membership(
    tx: &mut Transaction<'_, Postgres>,
    thread_id: i64,
    memory_id: i64,
    reason: &str,
    operator: Option<&str>,
    scope: &ScopeFilter,
) -> Result<Option<(Option<i64>, Vec<i64>)>, AppError> {
    let archived: Option<(i64, Option<i64>, Vec<i64>)> = sqlx::query_as(
        "INSERT INTO thread_event_archive
           (event_id,thread_id,memory_id,joined_at,previous_memory_id,next_memory_ids,
            refs,archived_by,reason)
         SELECT e.id,e.thread_id,e.memory_id,e.created_at,
                (SELECT previous_event.memory_id
                 FROM thread_event_links link
                 JOIN thread_events previous_event
                   ON previous_event.thread_id=link.thread_id
                  AND previous_event.id=link.previous_event_id
                 WHERE link.thread_id=e.thread_id AND link.next_event_id=e.id),
                ARRAY(SELECT next_event.memory_id
                      FROM thread_event_links link
                      JOIN thread_events next_event
                        ON next_event.thread_id=link.thread_id
                       AND next_event.id=link.next_event_id
                      WHERE link.thread_id=e.thread_id AND link.previous_event_id=e.id
                      ORDER BY next_event.id),
                coalesce((SELECT jsonb_agg(to_jsonb(ref)-'event_id' ORDER BY ref.id)
                          FROM memory_thread_refs ref WHERE ref.event_id=e.id),'[]'::jsonb),
                $3,$4
         FROM thread_events e
         WHERE e.thread_id=$1 AND e.memory_id=$2
         RETURNING event_id,previous_memory_id,next_memory_ids",
    )
    .bind(thread_id)
    .bind(memory_id)
    .bind(operator)
    .bind(reason)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((event_id, previous_memory_id, next_memory_ids)) = archived else {
        return Ok(None);
    };

    // The successors move onto the predecessor before the event goes, so the
    // delete cascades only the link into the archived event. Without a
    // predecessor, the delete cascades the successor links too, and the
    // successors become first members.
    sqlx::query(
        "UPDATE thread_event_links link
         SET previous_event_id=incoming.previous_event_id
         FROM thread_event_links incoming
         WHERE link.thread_id=$1 AND link.previous_event_id=$2
           AND incoming.thread_id=$1 AND incoming.next_event_id=$2",
    )
    .bind(thread_id)
    .bind(event_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM thread_events WHERE id=$1")
        .bind(event_id)
        .execute(&mut **tx)
        .await?;

    let neighbors: Vec<i64> = previous_memory_id
        .into_iter()
        .chain(next_memory_ids.iter().copied())
        .collect();

    if neighbors.is_empty() {
        return Ok(Some((None, Vec::new())));
    }
    // The receipt names only the memories that the caller can read.
    let sql = format!(
        "SELECT visible.id FROM memories visible WHERE visible.id=ANY($1){}",
        scope.predicate("visible", 2)
    );
    let readable: Vec<i64> = scope
        .bind_scalar(sqlx::query_scalar(&sql).bind(&neighbors))
        .fetch_all(&mut **tx)
        .await?;

    Ok(Some((
        previous_memory_id.filter(|id| readable.contains(id)),
        next_memory_ids
            .into_iter()
            .filter(|id| readable.contains(id))
            .collect(),
    )))
}

pub async fn memory_thread_leave(
    pool: &PgPool,
    p: MemoryThreadLeaveParams,
) -> Result<MemoryThreadLeaveReceipt, AppError> {
    let thread = p.thread.trim();
    let reason = p.reason.trim();

    if thread.is_empty() {
        return Err(AppError::Invalid("thread must not be empty".into()));
    }
    if reason.is_empty() {
        return Err(AppError::Invalid("reason must not be empty".into()));
    }
    if reason.chars().count() > MAX_REASON_CHARS {
        return Err(AppError::Invalid(format!(
            "reason must not exceed {MAX_REASON_CHARS} characters"
        )));
    }
    let ids = memory_ids(&p.memory_ids)?;
    RoomKey::for_memory_write(&p.room).map_err(|error| AppError::Invalid(error.to_string()))?;

    let writer_scope = ScopeFilter::load(pool, p.operator.as_deref(), &mut Vec::new()).await?;
    let mut tx = pool.begin().await?;
    let rows = lock_room_memories(&mut tx, &ids, &p.room, thread, &writer_scope).await?;
    let thread_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM threads WHERE room=$1 AND thread_key=$2")
            .bind(&p.room)
            .bind(thread)
            .fetch_optional(&mut *tx)
            .await?;

    if let Some(thread_id) = thread_id {
        // The same lock as the link cycle trigger: one graph change per thread.
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(thread_id)
            .execute(&mut *tx)
            .await?;
    }
    let mut memories = Vec::with_capacity(rows.len());
    // Membership is read again inside the lock, so a concurrent join or leave
    // cannot make the receipt stale.
    for (id, title, _) in rows {
        let archived = match thread_id {
            Some(thread_id) => {
                archive_membership(
                    &mut tx,
                    thread_id,
                    id,
                    reason,
                    p.operator.as_deref(),
                    &writer_scope,
                )
                .await?
            }
            None => None,
        };
        let was_member = archived.is_some();
        let (relinked_to, relinked_successors) = archived.unwrap_or_default();

        memories.push(MemoryThreadLeaveMemory {
            id,
            title,
            was_member,
            relinked_to,
            relinked_successors,
        });
    }
    sqlx::query(
        "UPDATE memories SET threads=array_remove(threads,$2)
         WHERE id=ANY($1) AND $2=ANY(threads)",
    )
    .bind(&ids)
    .bind(thread)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(MemoryThreadLeaveReceipt {
        ok: true,
        room: p.room,
        thread: thread.to_owned(),
        reason: reason.to_owned(),
        memories,
    })
}

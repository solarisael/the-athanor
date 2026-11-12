use super::memory_write::continue_by_date_tx;
use crate::config::AppError;
use crate::recall::scope::ScopeFilter;
use hearth::RoomKey;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MemoryThreadJoinParams {
    pub thread: String,
    pub memory_ids: Vec<String>,
    pub room: String,
    #[serde(default)]
    pub operator: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryThreadJoinMemory {
    pub id: i64,
    pub title: String,
    pub already_member: bool,
    /// The predecessor this call linked the memory to; absent when the memory
    /// kept an earlier link or is the first member by date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_after: Option<i64>,
}

#[derive(Serialize)]
pub struct MemoryThreadJoinReceipt {
    pub ok: bool,
    pub room: String,
    pub thread: String,
    pub memories: Vec<MemoryThreadJoinMemory>,
}

/// One positive numeric memory ID from the parameter `field`.
pub(super) fn memory_id(field: &str, text: &str) -> Result<i64, AppError> {
    let valid = text
        .as_bytes()
        .first()
        .is_some_and(|byte| matches!(byte, b'1'..=b'9'))
        && text.bytes().all(|byte| byte.is_ascii_digit());

    if !valid {
        return Err(AppError::Invalid(format!(
            "{field}: each ID must be a positive numeric memory ID"
        )));
    }
    text.parse::<i64>()
        .map_err(|_| AppError::Invalid(format!("{field}: each ID must fit PostgreSQL BIGINT")))
}

/// Positive numeric memory IDs, sorted and without duplicates.
pub(super) fn memory_ids(texts: &[String]) -> Result<Vec<i64>, AppError> {
    if texts.is_empty() {
        return Err(AppError::Invalid("memoryIds must not be empty".into()));
    }
    let mut ids = texts
        .iter()
        .map(|text| memory_id("memoryIds", text))
        .collect::<Result<Vec<_>, _>>()?;
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

/// Locks each requested memory for the call. Returns (ID, title, member of the
/// thread) in ID order. Every ID must name a readable, unarchived memory of the
/// room, so one bad ID refuses the whole call.
pub(super) async fn lock_room_memories(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[i64],
    room: &str,
    thread: &str,
    scope: &ScopeFilter,
) -> Result<Vec<(i64, String, bool)>, AppError> {
    let sql = format!(
        "SELECT m.id,coalesce(m.title,'') AS title,EXISTS (
             SELECT 1 FROM thread_events e JOIN threads t ON t.id=e.thread_id
             WHERE e.memory_id=m.id AND t.room=$2 AND t.thread_key=$3
         ) AS member
         FROM memories m
         WHERE m.id=ANY($1) AND m.room=$2 AND m.archived_at IS NULL{}
         ORDER BY m.id FOR UPDATE OF m",
        scope.predicate("m", 4)
    );
    let rows = scope
        .bind(sqlx::query(&sql).bind(ids).bind(room).bind(thread))
        .fetch_all(&mut **tx)
        .await?
        .iter()
        .map(|row| {
            Ok((
                row.try_get("id")?,
                row.try_get("title")?,
                row.try_get("member")?,
            ))
        })
        .collect::<Result<Vec<(i64, String, bool)>, sqlx::Error>>()?;
    let absent: Vec<String> = ids
        .iter()
        .filter(|id| rows.binary_search_by_key(*id, |row| row.0).is_err())
        .map(ToString::to_string)
        .collect();

    if !absent.is_empty() {
        return Err(AppError::Invalid(format!(
            "memories are unavailable in target room {room}: {}",
            absent.join(", ")
        )));
    }
    Ok(rows)
}

pub async fn memory_thread_join(
    pool: &PgPool,
    p: MemoryThreadJoinParams,
) -> Result<MemoryThreadJoinReceipt, AppError> {
    let thread = p.thread.trim();

    if thread.is_empty() {
        return Err(AppError::Invalid("thread must not be empty".into()));
    }
    let ids = memory_ids(&p.memory_ids)?;
    RoomKey::for_memory_write(&p.room).map_err(|error| AppError::Invalid(error.to_string()))?;

    let writer_scope = ScopeFilter::load(pool, p.operator.as_deref(), &mut Vec::new()).await?;
    let mut tx = pool.begin().await?;
    let mut memories = lock_room_memories(&mut tx, &ids, &p.room, thread, &writer_scope)
        .await?
        .into_iter()
        .map(|(id, title, already_member)| MemoryThreadJoinMemory {
            id,
            title,
            already_member,
            linked_after: None,
        })
        .collect::<Vec<_>>();

    let thread_id: i64 = sqlx::query_scalar(
        "INSERT INTO threads (room,thread_key) VALUES ($1,$2)
         ON CONFLICT (room,thread_key) DO UPDATE SET thread_key=EXCLUDED.thread_key
         RETURNING id",
    )
    .bind(&p.room)
    .bind(thread)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO thread_events (thread_id,memory_id)
         SELECT $1,unnest($2::bigint[])
         ON CONFLICT (thread_id,memory_id) DO NOTHING",
    )
    .bind(thread_id)
    .bind(&ids)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO memory_thread_refs (event_id)
         SELECT e.id FROM thread_events e
         WHERE e.thread_id=$1 AND e.memory_id=ANY($2)
           AND NOT EXISTS (SELECT 1 FROM memory_thread_refs r WHERE r.event_id=e.id)",
    )
    .bind(thread_id)
    .bind(&ids)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE memories SET threads=array_append(threads,$2)
         WHERE id=ANY($1) AND NOT ($2=ANY(threads))",
    )
    .bind(&ids)
    .bind(thread)
    .execute(&mut *tx)
    .await?;
    let links = continue_by_date_tx(&mut tx, &ids, Some(thread_id), &writer_scope).await?;
    tx.commit().await?;

    for memory in &mut memories {
        memory.linked_after = links
            .iter()
            .find(|(memory_id, _)| *memory_id == memory.id)
            .map(|(_, previous_memory_id)| *previous_memory_id);
    }

    Ok(MemoryThreadJoinReceipt {
        ok: true,
        room: p.room,
        thread: thread.to_owned(),
        memories,
    })
}

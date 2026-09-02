use super::{chunk_body, derive_dates, embed, normalize_strings, token_estimate};
use crate::config::{AppError, Config, EmbeddingMode, HTTP_CLIENT};
use crate::settings::RoomSettings;
use chrono::NaiveDate;
use hearth::ThreadContinuation;
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use std::collections::{BTreeSet, HashSet};

/// Memory IDs are PostgreSQL BIGINT; the domain carries them as u64.
fn bigint(id: u64, field: &str) -> Result<i64, AppError> {
    i64::try_from(id)
        .map_err(|_| AppError::Invalid(format!("{field} is out of PostgreSQL BIGINT range")))
}

pub(crate) struct PreparedMemoryWrite {
    primary_date: NaiveDate,
    dates: Vec<NaiveDate>,
    threads: Vec<String>,
    chunks: Vec<(String, usize, usize, Option<String>)>,
    vectors: Option<Vec<Vec<f32>>>,
    pub(crate) warnings: Vec<String>,
}

pub(crate) async fn prepare_memory_write(
    cfg: &Config,
    settings: &RoomSettings,
    source_path: &str,
    body: &str,
    threads: &[String],
    primary_date: NaiveDate,
) -> Result<PreparedMemoryWrite, AppError> {
    let threads = normalize_strings(threads);
    let dates = derive_dates(source_path, primary_date);
    let chunks = chunk_body(body, settings);
    let mut warnings = Vec::new();
    let vectors = match cfg.embedding_mode {
        EmbeddingMode::Disabled => {
            warnings
                .push("semantic embeddings disabled in production; lexical chunks retained".into());
            None
        }
        EmbeddingMode::DisabledForTest => {
            warnings.push(
                "semantic embeddings disabled for isolated test; lexical chunks retained".into(),
            );
            None
        }
        EmbeddingMode::Required => {
            let url = cfg
                .embed_url
                .as_deref()
                .ok_or_else(|| AppError::Embedding("embedding endpoint is required".into()))?;
            let vectors = embed(
                &HTTP_CLIENT,
                url,
                &cfg.embed_model,
                &chunks,
                cfg.embed_dimension,
            )
            .await?;
            if vectors.len() != chunks.len() {
                return Err(AppError::Embedding("embedding count mismatch".into()));
            }
            Some(vectors)
        }
    };
    Ok(PreparedMemoryWrite {
        primary_date,
        dates,
        threads,
        chunks,
        vectors,
        warnings,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_memory_tx(
    tx: &mut Transaction<'_, Postgres>,
    room: &str,
    memory_type: &str,
    title: &str,
    source_path: &str,
    body: &str,
    supersedes: &[u64],
    meta: Value,
    prepared: &PreparedMemoryWrite,
) -> Result<(i64, bool), AppError> {
    // One jsonb row serves both branches: keys are `memories` column names
    // (0001_initial.sql:19), matched by NAME. The column lists stay written out
    // because `body_tsv` (0001_initial.sql:28) and `bm25f_meta_tsv`
    // (0009_bm25f_memory_search.sql:26) are GENERATED.
    let row = json!({
        "room": room,
        "type": memory_type,
        "date": prepared.primary_date,
        "dates": prepared.dates,
        "title": title,
        "source_path": source_path,
        "body": body,
        "threads": prepared.threads,
        "meta": meta,
    });

    let (memory_id, inserted) = if memory_type == origami::boats::MEMORY_KIND {
        let inserted_id: Option<i64> = sqlx::query_scalar(
            "INSERT INTO memories
             (room,type,date,dates,title,source_path,body,threads,meta)
             SELECT room,type,date,dates,title,source_path,body,threads,meta
             FROM jsonb_populate_record(NULL::memories, $1)
             ON CONFLICT (room,source_path) DO NOTHING
             RETURNING id",
        )
        .bind(row)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(memory_id) = inserted_id {
            (memory_id, true)
        } else {
            let existing: Option<(i64, String, String)> = sqlx::query_as(
                "SELECT id,type,body FROM memories
                 WHERE room=$1 AND source_path=$2
                 FOR KEY SHARE",
            )
            .bind(room)
            .bind(source_path)
            .fetch_optional(&mut **tx)
            .await?;
            match existing {
                Some((memory_id, existing_type, existing_body))
                    if existing_type == memory_type && existing_body == body =>
                {
                    return Ok((memory_id, false));
                }
                _ => {
                    return Err(AppError::Invalid(
                        "paper boat source identity conflicts with a different record".into(),
                    ));
                }
            }
        }
    } else {
        sqlx::query_as(
            "INSERT INTO memories
             (room,type,date,dates,title,source_path,body,threads,meta)
             SELECT room,type,date,dates,title,source_path,body,threads,meta
             FROM jsonb_populate_record(NULL::memories, $1)
             ON CONFLICT (room,source_path) DO UPDATE
             SET type=EXCLUDED.type,date=EXCLUDED.date,dates=EXCLUDED.dates,title=EXCLUDED.title,
                 body=EXCLUDED.body,threads=EXCLUDED.threads,meta=EXCLUDED.meta
             RETURNING id,(xmax=0) AS inserted",
        )
        .bind(row)
        .fetch_one(&mut **tx)
        .await?
    };
    for thread_key in &prepared.threads {
        let thread_id: i64 = sqlx::query_scalar(
            "INSERT INTO threads (room,thread_key) VALUES ($1,$2)
             ON CONFLICT (room,thread_key) DO UPDATE SET thread_key=EXCLUDED.thread_key
             RETURNING id",
        )
        .bind(room)
        .bind(thread_key)
        .fetch_one(&mut **tx)
        .await?;
        let event_id: i64 = sqlx::query_scalar(
            "INSERT INTO thread_events (thread_id,memory_id) VALUES ($1,$2)
             ON CONFLICT (thread_id,memory_id) DO UPDATE SET memory_id=EXCLUDED.memory_id
             RETURNING id",
        )
        .bind(thread_id)
        .bind(memory_id)
        .fetch_one(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO memory_thread_refs (event_id)
             SELECT $1 WHERE NOT EXISTS (
                 SELECT 1 FROM memory_thread_refs WHERE event_id=$1
             )",
        )
        .bind(event_id)
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query(
        "DELETE FROM thread_events e USING threads t
         WHERE e.thread_id=t.id AND e.memory_id=$1 AND t.room=$2
           AND NOT (t.thread_key = ANY($3::text[]))",
    )
    .bind(memory_id)
    .bind(room)
    .bind(&prepared.threads)
    .execute(&mut **tx)
    .await?;
    for old_id in supersedes.iter().copied().collect::<BTreeSet<_>>() {
        sqlx::query("UPDATE memories SET superseded_by=$1 WHERE id=$2 AND id<>$1")
            .bind(memory_id)
            .bind(bigint(old_id, "supersedes ID")?)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query("DELETE FROM memory_chunks WHERE memory_id=$1")
        .bind(memory_id)
        .execute(&mut **tx)
        .await?;
    for (index, (text, start, end, heading)) in prepared.chunks.iter().enumerate() {
        let vector_text = prepared.vectors.as_ref().map(|vectors| {
            format!(
                "[{}]",
                vectors[index]
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        });
        // enough: positional binds, not one jsonb row; `body_embedding` only
        // becomes vector(2048) through 0002_nemotron_2048.sql's guarded ALTER,
        // and the `$8::vector` cast below is what pins that. Convert it once a
        // live DB proves jsonb_populate_record's input path for the column.
        sqlx::query(
            "INSERT INTO memory_chunks
             (memory_id,chunk_index,heading_path,body,char_start,char_end,token_estimate,
              body_embedding,embedded_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8::vector,
                     CASE WHEN $8 IS NULL THEN NULL ELSE NOW() END)",
        )
        .bind(memory_id)
        .bind(
            i32::try_from(index)
                .map_err(|_| AppError::Invalid("memory has too many semantic chunks".into()))?,
        )
        .bind(heading)
        .bind(text)
        .bind(
            i32::try_from(*start).map_err(|_| {
                AppError::Invalid("memory chunk range exceeds database bounds".into())
            })?,
        )
        .bind(
            i32::try_from(*end).map_err(|_| {
                AppError::Invalid("memory chunk range exceeds database bounds".into())
            })?,
        )
        .bind(token_estimate(text))
        .bind(vector_text)
        .execute(&mut **tx)
        .await?;
    }
    Ok((memory_id, inserted))
}

pub(super) async fn write_continuations_tx(
    tx: &mut Transaction<'_, Postgres>,
    room: &str,
    memory_id: i64,
    continuations: &[ThreadContinuation],
) -> Result<(), AppError> {
    let mut seen = HashSet::new();
    for continuation in continuations {
        let thread_key = continuation.thread.trim();
        if !seen.insert(thread_key) {
            continue;
        }
        let previous_memory_id = bigint(continuation.previous_memory_id, "previousMemoryId")?;
        if previous_memory_id == memory_id {
            return Err(AppError::Invalid("a memory cannot continue itself".into()));
        }
        let events = sqlx::query(
            "SELECT current_event.thread_id,
                    current_event.id AS next_event_id,
                    previous_event.id AS previous_event_id
             FROM threads t
             JOIN thread_events current_event
               ON current_event.thread_id=t.id AND current_event.memory_id=$3
             JOIN thread_events previous_event
               ON previous_event.thread_id=t.id AND previous_event.memory_id=$4
             JOIN memories previous_memory ON previous_memory.id=previous_event.memory_id
             WHERE t.room=$1 AND t.thread_key=$2 AND previous_memory.room=$1
             FOR KEY SHARE OF t,current_event,previous_event,previous_memory",
        )
        .bind(room)
        .bind(thread_key)
        .bind(memory_id)
        .bind(previous_memory_id)
        .fetch_optional(&mut **tx)
        .await?;
        let Some(events) = events else {
            return Err(AppError::Invalid(format!(
                "previous memory {previous_memory_id} must share room {room} and thread {thread_key}"
            )));
        };
        let thread_id: i64 = sqlx::Row::try_get(&events, "thread_id")?;
        let next_event_id: i64 = sqlx::Row::try_get(&events, "next_event_id")?;
        let previous_event_id: i64 = sqlx::Row::try_get(&events, "previous_event_id")?;
        sqlx::query(
            "INSERT INTO thread_event_links (thread_id,previous_event_id,next_event_id)
             VALUES ($1,$2,$3)
             ON CONFLICT (thread_id,next_event_id) DO UPDATE
             SET previous_event_id=EXCLUDED.previous_event_id",
        )
        .bind(thread_id)
        .bind(previous_event_id)
        .bind(next_event_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

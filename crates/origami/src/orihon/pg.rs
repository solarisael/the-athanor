use super::book::advance;
use super::error::{Error, refusal};
use super::prepare::{self, PreparedRecord};
use super::read::read_bounds;
use chrono::{DateTime, SecondsFormat, Utc};
use hearth::orihon::{CaptureBatch, CaptureReceipt, Kind, ReadReceipt, ReadRequest, Record};
use serde_json::json;
use sqlx::{Acquire, PgPool, Postgres, Row, Transaction, postgres::PgRow};

fn record_from_row(row: PgRow) -> Result<Record, Error> {
    Ok(Record {
        room: row.try_get("room")?,
        seq: row.try_get("seq")?,
        turn_no: row.try_get("turn_no")?,
        session_id: row.try_get("session_id")?,
        harness: row.try_get("harness")?,
        source_session: row.try_get("source_session")?,
        source_id: row.try_get("source_id")?,
        parent_user_id: row.try_get("parent_user_id")?,
        kind: prepare::parse_kind(row.try_get::<&str, _>("kind")?)?,
        speaker: row.try_get("speaker")?,
        text: row.try_get("text")?,
        said_at: row.try_get("source_time")?,
        timestamp_observed: row.try_get("timestamp_observed")?,
        written_at: row
            .try_get::<DateTime<Utc>, _>("written_at")?
            .to_rfc3339_opts(SecondsFormat::AutoSi, true),
        truncated: row.try_get("truncated")?,
        redacted: row.try_get("redacted")?,
        content_digest: row.try_get("content_digest")?,
    })
}

async fn existing(
    transaction: &mut Transaction<'_, Postgres>,
    room: &str,
    harness: &str,
    source_session: &str,
    source_id: &str,
) -> Result<Option<Record>, Error> {
    sqlx::query(
        "SELECT * FROM orihon WHERE room=$1 AND harness=$2 AND source_session=$3 AND source_id=$4",
    )
    .bind(room)
    .bind(harness)
    .bind(source_session)
    .bind(source_id)
    .fetch_optional(&mut **transaction)
    .await?
    .map(record_from_row)
    .transpose()
}

async fn parent_turn(
    transaction: &mut Transaction<'_, Postgres>,
    room: &str,
    harness: &str,
    source_session: &str,
    parent_id: &str,
) -> Result<i64, Error> {
    let row = sqlx::query(
        "SELECT kind,turn_no FROM orihon
         WHERE room=$1 AND harness=$2 AND source_session=$3 AND source_id=$4",
    )
    .bind(room)
    .bind(harness)
    .bind(source_session)
    .bind(parent_id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| {
        refusal(
            "missing_parent",
            "parentUserId must identify a stored or earlier user record",
        )
    })?;
    if row.try_get::<&str, _>("kind")? != "user" {
        return Err(refusal(
            "invalid_parent",
            "parentUserId identifies a non-user record",
        ));
    }
    Ok(row.try_get("turn_no")?)
}

async fn insert(
    transaction: &mut Transaction<'_, Postgres>,
    room: &str,
    writer_session: &str,
    harness: &str,
    source_session: &str,
    prepared: PreparedRecord,
    seq: i64,
    turn_no: i64,
) -> Result<Record, Error> {
    let source = &prepared.capture;
    let row = sqlx::query(
        "INSERT INTO orihon(
            room,seq,turn_no,session_id,harness,source_session,source_id,parent_user_id,
            kind,speaker,text,source_time,said_at,timestamp_observed,truncated,redacted,content_digest
         )
         SELECT room,seq,turn_no,session_id,harness,source_session,source_id,parent_user_id,
                kind,speaker,text,source_time,said_at,timestamp_observed,truncated,redacted,content_digest
         FROM jsonb_populate_record(NULL::orihon,$1)
         RETURNING written_at",
    )
    .bind(json!({
        "room":room, "seq":seq, "turn_no":turn_no, "session_id":writer_session,
        "harness":harness, "source_session":source_session, "source_id":source.source_id,
        "parent_user_id":source.parent_user_id, "kind":prepare::kind_name(source.kind),
        "speaker":source.speaker, "text":source.text, "source_time":source.said_at,
        "said_at":source.said_at, "timestamp_observed":source.timestamp_observed,
        "truncated":prepared.truncated, "redacted":source.redacted,
        "content_digest":prepared.content_digest,
    }))
    .fetch_one(&mut **transaction).await?;
    let written_at = row
        .try_get::<DateTime<Utc>, _>("written_at")?
        .to_rfc3339_opts(SecondsFormat::AutoSi, true);
    Ok(prepared.into_record(
        room,
        writer_session,
        harness,
        source_session,
        seq,
        turn_no,
        written_at,
    ))
}

pub async fn append_pg(
    pool: &PgPool,
    room: &str,
    writer_session: &str,
    batch: CaptureBatch,
) -> Result<CaptureReceipt, Error> {
    let mut transaction = pool.begin().await?;
    let receipt = append_pg_in_transaction(&mut transaction, room, writer_session, batch).await?;
    transaction.commit().await?;
    Ok(receipt)
}

pub async fn append_pg_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    room: &str,
    writer_session: &str,
    batch: CaptureBatch,
) -> Result<CaptureReceipt, Error> {
    let batch = prepare::prepare(room, writer_session, batch)?;
    // Host can hold its mode lease on this same connection. A savepoint keeps
    // a refused batch from leaving a valid prefix in the caller's transaction.
    let mut savepoint = transaction.begin().await?;
    let receipt = append_prepared(&mut savepoint, room, writer_session, batch).await?;
    savepoint.commit().await?;
    Ok(receipt)
}

async fn append_prepared(
    transaction: &mut Transaction<'_, Postgres>,
    room: &str,
    writer_session: &str,
    batch: prepare::PreparedBatch,
) -> Result<CaptureReceipt, Error> {
    sqlx::query("INSERT INTO orihon_cursor(room) VALUES($1) ON CONFLICT(room) DO NOTHING")
        .bind(room)
        .execute(&mut **transaction)
        .await?;
    let cursor =
        sqlx::query("SELECT next_seq,next_turn FROM orihon_cursor WHERE room=$1 FOR UPDATE")
            .bind(room)
            .fetch_one(&mut **transaction)
            .await?;
    let mut next_seq: i64 = cursor.try_get("next_seq")?;
    let mut next_turn: i64 = cursor.try_get("next_turn")?;
    let mut accepted_records = Vec::with_capacity(batch.records.len());
    let mut accepted_source_ids = Vec::with_capacity(batch.records.len());
    let mut appended = 0;
    for prepared in batch.records {
        let stored = existing(
            transaction,
            room,
            &batch.harness,
            &batch.source_session,
            &prepared.capture.source_id,
        )
        .await?;
        let record = if let Some(stored) = stored {
            if !prepared.matches(&stored) {
                return Err(refusal(
                    "idempotency_reuse",
                    "source identity was reused with different normalized content",
                ));
            }
            stored
        } else {
            let turn_no = if prepared.capture.kind == Kind::User {
                let turn = next_turn;
                next_turn = advance(next_turn)?;
                turn
            } else {
                parent_turn(
                    transaction,
                    room,
                    &batch.harness,
                    &batch.source_session,
                    prepared.capture.parent_user_id.as_deref().ok_or_else(|| {
                        refusal("missing_parent", "a non-user record must name parentUserId")
                    })?,
                )
                .await?
            };
            let seq = next_seq;
            next_seq = advance(next_seq)?;
            let record = insert(
                transaction,
                room,
                writer_session,
                &batch.harness,
                &batch.source_session,
                prepared,
                seq,
                turn_no,
            )
            .await?;
            appended += 1;
            record
        };
        accepted_source_ids.push(record.source_id.clone());
        accepted_records.push(record);
    }
    sqlx::query("UPDATE orihon_cursor SET next_seq=$2,next_turn=$3 WHERE room=$1")
        .bind(room)
        .bind(next_seq)
        .bind(next_turn)
        .execute(&mut **transaction)
        .await?;
    let replayed = accepted_records.len() - appended;
    Ok(CaptureReceipt {
        accepted_source_ids,
        appended,
        replayed,
        accepted_records,
    })
}

pub async fn read_pg(
    pool: &PgPool,
    room: &str,
    request: ReadRequest,
) -> Result<ReadReceipt, Error> {
    let bounds = read_bounds(room, request)?;
    let rows = sqlx::query(
        "SELECT * FROM orihon WHERE room=$1
         AND ($2::bigint IS NULL OR seq>$2)
         AND ($3::bigint IS NULL OR seq<=$3)
         AND ($4::bigint IS NULL OR turn_no=$4)
         AND ($5::text IS NULL OR left(source_time,10)=$5)
         ORDER BY seq LIMIT $6",
    )
    .bind(room)
    .bind(bounds.request.after_seq)
    .bind(bounds.request.until_seq)
    .bind(bounds.request.turn_no)
    .bind(bounds.request.day.as_deref())
    .bind((bounds.request.limit + 1) as i64)
    .fetch_all(pool)
    .await?;
    let rows = rows
        .into_iter()
        .map(record_from_row)
        .collect::<Result<_, _>>()?;
    Ok(bounds.receipt(rows))
}

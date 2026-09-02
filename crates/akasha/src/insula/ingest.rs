// The write door of Insula. insula_writer.rs batches what the substrate
// observes about itself and pushes it through here; the vitals counters
// (Pulse GUI) and the trace reads are the only consumers downstream.

use std::collections::HashSet;

use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use super::event::{TrustedBinding, validate_trusted_binding};
use super::{InsulaError, bad};
use super::event::{ObservationEvent, validate_event};
use super::event::{derive_idempotency_key_v1, derive_semantic_hash_v1};
use super::lock;

pub const INSULA_MAX_BATCH_EVENTS: usize = 512;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IngestBatch {
    pub events: Vec<ObservationEvent>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IngestConflictKind {
    LogicalKeyReuse,
    EventIdReuse,
    WriterSequenceReuse,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IngestConflict {
    pub event_index: u32,
    pub kind: IngestConflictKind,
    pub event_id: String,
    pub incumbent_event_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IngestReceipt {
    pub schema_version: i16,
    pub accepted_count: u32,
    pub duplicate_count: u32,
    pub conflicts: Vec<IngestConflict>,
}

pub async fn ingest_batch(
    pool: &PgPool,
    b: &TrustedBinding,
    mut batch: IngestBatch,
) -> Result<IngestReceipt, InsulaError> {
    validate_trusted_binding(b)?;
    if batch.events.is_empty() {
        return Err(bad("events", "empty_batch"));
    }
    if batch.events.len() > INSULA_MAX_BATCH_EVENTS {
        return Err(bad("events", "batch_too_large"));
    }

    // Validate and stamp both hashes before any transaction opens: a bad
    // event refuses the whole batch without touching the database.
    // observed_at truncates to microseconds first: timestamptz stores micros,
    // and a nanosecond tail rounds on the jsonb text path while the vitals
    // binary bind truncates — half a microsecond apart, retention's coverage
    // proof refuses the sweep (coding#251, lived 2026-08-24).
    let mut expiries = Vec::with_capacity(batch.events.len());
    for e in &mut batch.events {
        e.observed_at = e
            .observed_at
            .with_nanosecond(e.observed_at.nanosecond() / 1_000 * 1_000)
            .unwrap_or(e.observed_at);
        expiries.push(validate_event(e)?);
        e.idempotency_key = derive_idempotency_key_v1(b, e)?;
        e.semantic_hash = derive_semantic_hash_v1(b, e)?;
    }

    // Concurrent batches with overlapping identities can deadlock on the
    // unique indexes; Postgres detects that (40P01) and the batch replays.
    // This replaces the previous 3-advisory-locks-per-event ladder.
    let mut attempts = 0;
    loop {
        match write_batch(pool, b, &batch, &expiries).await {
            Err(InsulaError::Database(e))
                if e.as_database_error().and_then(|d| d.code()).as_deref() == Some("40P01")
                    && attempts < 2 =>
            {
                attempts += 1;
            }
            other => return other,
        }
    }
}

async fn write_batch(
    pool: &PgPool,
    b: &TrustedBinding,
    batch: &IngestBatch,
    expiries: &[DateTime<Utc>],
) -> Result<IngestReceipt, InsulaError> {
    let mut tx = pool.begin().await?;

    // Shared against retention's exclusive sweep (retention.rs:174): the
    // sweep computes coverage hashes over a frozen window, so ingest and
    // sweep never interleave.
    lock(&mut tx, &b.house_id, false).await?;

    let mut accepted = insert_new(&mut tx, b, &batch.events, expiries).await?;

    let mut out = IngestReceipt {
        schema_version: 1,
        accepted_count: 0,
        duplicate_count: 0,
        conflicts: vec![],
    };

    for (index, e) in batch.events.iter().enumerate() {
        // remove, not contains: the same event_id twice in one batch is one
        // inserted row; the second occurrence settles as duplicate or conflict.
        if accepted.remove(&e.event_id) {
            roll_vitals(&mut tx, b, e).await?;
            out.accepted_count += 1;
            continue;
        }
        settle_loser(&mut tx, b, e, index, &mut out).await?;
    }

    tx.commit().await?;
    Ok(out)
}

// One statement for the whole batch. jsonb_populate_recordset matches keys
// to columns by NAME, so a wrong key dies as a NOT NULL refusal, never as a
// silent positional swap; ON CONFLICT DO NOTHING covers all three unique
// constraints at once (0022_insula.sql:236,364,367). A future insula.log
// column must be added to row() below — the failure is loud at first insert.
async fn insert_new(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    b: &TrustedBinding,
    events: &[ObservationEvent],
    expiries: &[DateTime<Utc>],
) -> Result<HashSet<String>, InsulaError> {
    let rows: Vec<serde_json::Value> = events
        .iter()
        .zip(expiries.iter().copied())
        .map(|(e, x)| row(b, e, x))
        .collect();

    // ingested_at merges server-side: NOW() is transaction time, one value for
    // the whole batch — a client clock here skews retention's replay proof.
    let inserted = sqlx::query(
        "INSERT INTO insula.log
         SELECT * FROM jsonb_populate_recordset(
             NULL::insula.log,
             (SELECT jsonb_agg(e || jsonb_build_object('ingested_at', NOW()))
                FROM jsonb_array_elements($1) e)
         )
         ON CONFLICT DO NOTHING
         RETURNING event_id::text",
    )
    .bind(serde_json::Value::Array(rows))
    .fetch_all(&mut **tx)
    .await?;

    inserted
        .into_iter()
        .map(|r| r.try_get("event_id"))
        .collect::<Result<HashSet<String>, sqlx::Error>>()
        .map_err(Into::into)
}

// Keys are insula.log column names; the table itself is the only schema.
fn row(b: &TrustedBinding, e: &ObservationEvent, x: DateTime<Utc>) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "event_id": e.event_id,
        "span_id": e.span_id,
        "trace_id": e.trace_id,
        "parent_span_id": e.parent_span_id,
        "writer_id": e.writer_id,
        "writer_sequence": e.writer_sequence,
        "house_id": b.house_id,
        "room": b.room,
        "spirit": b.spirit,
        "session_id": b.session_id,
        "component": e.component,
        "layer": e.layer,
        "operation": e.operation,
        "phase": e.phase.as_str(),
        "observed_at": e.observed_at,
        "duration_us": e.duration_us,
        "outcome_class": e.outcome_class.as_str(),
        "error_class": e.error_class,
        "bytes_in": e.bytes_in,
        "bytes_out": e.bytes_out,
        "tokens_in": e.tokens_in,
        "tokens_out": e.tokens_out,
        "tool_call_id": e.tool_call_id,
        "provider_request_id": e.provider_request_id,
        "idempotency_version": e.idempotency_version,
        "idempotency_scope": e.idempotency_scope.as_str(),
        "idempotency_key": e.idempotency_key,
        "receipt_kind": e.receipt_kind,
        "receipt_id": e.receipt_id,
        "semantic_hash": e.semantic_hash,
        "drop_count": e.drop_count,
        "expires_at": x,
        "duplicate_count": 0,
        "last_duplicate_at": null,
    })
}

// A lost insert is judged against its incumbents; it is never retried.
async fn settle_loser(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    b: &TrustedBinding,
    e: &ObservationEvent,
    index: usize,
    out: &mut IngestReceipt,
) -> Result<(), InsulaError> {
    let found = existing(tx, b, e).await?;

    // Same logical event, same content: a redelivery, counted on the
    // incumbent row so the writer can see its own retry pressure.
    let twin = found
        .iter()
        .find(|i| logical(i, e) && i.semantic == e.semantic_hash);
    if let Some(i) = twin {
        sqlx::query(
            "UPDATE insula.log
                SET duplicate_count = duplicate_count + 1,
                    last_duplicate_at = NOW()
              WHERE event_id = $1::uuid",
        )
        .bind(&i.event_id)
        .execute(&mut **tx)
        .await?;
        out.duplicate_count += 1;
        return Ok(());
    }

    let named: Vec<IngestConflict> = found
        .iter()
        .filter_map(|i| {
            conflict_kind(i, e).map(|kind| IngestConflict {
                event_index: index as u32,
                kind,
                event_id: e.event_id.clone(),
                incumbent_event_id: i.event_id.clone(),
            })
        })
        .collect();
    if named.is_empty() {
        return Err(InsulaError::Invariant(
            "conflicting insert had no reloadable incumbent",
        ));
    }

    out.conflicts.extend(named);
    Ok(())
}

fn conflict_kind(i: &Existing, e: &ObservationEvent) -> Option<IngestConflictKind> {
    if logical(i, e) {
        return Some(IngestConflictKind::LogicalKeyReuse);
    }
    if i.event_id == e.event_id {
        return Some(IngestConflictKind::EventIdReuse);
    }
    if i.writer_id == e.writer_id && i.sequence == e.writer_sequence {
        return Some(IngestConflictKind::WriterSequenceReuse);
    }
    None
}

#[derive(Debug, sqlx::FromRow)]
struct Existing {
    event_id: String,
    writer_id: String,
    #[sqlx(rename = "writer_sequence")]
    sequence: i64,
    #[sqlx(rename = "idempotency_version")]
    version: i16,
    #[sqlx(rename = "idempotency_scope")]
    scope: String,
    #[sqlx(rename = "idempotency_key")]
    key: String,
    #[sqlx(rename = "semantic_hash")]
    semantic: String,
}

fn logical(i: &Existing, e: &ObservationEvent) -> bool {
    i.version == e.idempotency_version
        && i.scope == e.idempotency_scope.as_str()
        && i.key == e.idempotency_key
}

// One union read over the same three identities the insert can lose to,
// locked FOR UPDATE so the duplicate counter lands on a stable incumbent.
async fn existing(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    b: &TrustedBinding,
    e: &ObservationEvent,
) -> Result<Vec<Existing>, InsulaError> {
    let rows = sqlx::query_as::<_, Existing>(
        "SELECT event_id::text event_id, writer_id::text writer_id,
                writer_sequence, idempotency_version, idempotency_scope,
                idempotency_key, semantic_hash
           FROM insula.log
          WHERE (house_id = $1
                 AND idempotency_version = $2
                 AND idempotency_scope = $3
                 AND idempotency_key = $4)
             OR event_id = $5::uuid
             OR (writer_id = $6::uuid AND writer_sequence = $7)
          ORDER BY event_id
            FOR UPDATE",
    )
    .bind(&b.house_id)
    .bind(e.idempotency_version)
    .bind(e.idempotency_scope.as_str())
    .bind(&e.idempotency_key)
    .bind(&e.event_id)
    .bind(&e.writer_id)
    .bind(e.writer_sequence)
    .fetch_all(&mut **tx)
    .await?;

    Ok(rows)
}

async fn roll_vitals(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    b: &TrustedBinding,
    e: &ObservationEvent,
) -> Result<(), InsulaError> {
    sqlx::query(
        r#"INSERT INTO insula.vitals_minute(
               query_name,query_version,minute,house_id,room,spirit,component,layer,operation,
               phase,outcome_class,event_count,duration_us_sum,duration_us_max,bytes_in_sum,
               bytes_out_sum,tokens_in_sum,tokens_out_sum,drop_count_sum,source_first_sequence,
               source_last_sequence,source_first_observed_at,source_last_observed_at,
               source_coverage_hash
           )
           VALUES(
               'insula.vitals.minute',1,
               date_trunc('minute',$1::timestamptz AT TIME ZONE 'UTC') AT TIME ZONE 'UTC',
               $2,$3,$4,$5,$6,$7,$8,$9,1,COALESCE($10,0),$10,$11,$12,$13,$14,$15,$16,$16,$1,$1,$17
           )
           ON CONFLICT(
               query_name,query_version,minute,house_id,room,spirit,component,layer,operation,
               phase,outcome_class
           )
           DO UPDATE SET
               event_count=insula.vitals_minute.event_count+1,
               duration_us_sum=insula.vitals_minute.duration_us_sum+EXCLUDED.duration_us_sum,
               duration_us_max=GREATEST(insula.vitals_minute.duration_us_max,EXCLUDED.duration_us_max),
               bytes_in_sum=insula.vitals_minute.bytes_in_sum+EXCLUDED.bytes_in_sum,
               bytes_out_sum=insula.vitals_minute.bytes_out_sum+EXCLUDED.bytes_out_sum,
               tokens_in_sum=insula.vitals_minute.tokens_in_sum+EXCLUDED.tokens_in_sum,
               tokens_out_sum=insula.vitals_minute.tokens_out_sum+EXCLUDED.tokens_out_sum,
               drop_count_sum=insula.vitals_minute.drop_count_sum+EXCLUDED.drop_count_sum,
               source_first_sequence=LEAST(
                   insula.vitals_minute.source_first_sequence,EXCLUDED.source_first_sequence
               ),
               source_last_sequence=GREATEST(
                   insula.vitals_minute.source_last_sequence,EXCLUDED.source_last_sequence
               ),
               source_first_observed_at=LEAST(
                   insula.vitals_minute.source_first_observed_at,EXCLUDED.source_first_observed_at
               ),
               source_last_observed_at=GREATEST(
                   insula.vitals_minute.source_last_observed_at,EXCLUDED.source_last_observed_at
               ),
               updated_at=NOW()"#,
    )
    .bind(e.observed_at)
    .bind(&b.house_id)
    .bind(&b.room)
    .bind(&b.spirit)
    .bind(&e.component)
    .bind(&e.layer)
    .bind(&e.operation)
    .bind(e.phase.as_str())
    .bind(e.outcome_class.as_str())
    .bind(e.duration_us)
    .bind(e.bytes_in)
    .bind(e.bytes_out)
    .bind(e.tokens_in)
    .bind(e.tokens_out)
    .bind(e.drop_count)
    .bind(e.writer_sequence)
    .bind(&e.semantic_hash)
    .execute(&mut **tx)
    .await?;

    // Coverage is a canonical set hash, not an arrival-order hash chain.
    sqlx::query(
        r#"UPDATE insula.vitals_minute AS v
           SET source_coverage_hash=source.coverage_hash,updated_at=NOW()
           FROM (
               SELECT encode(
                   digest(
                       convert_to(
                           string_agg(event_id::text||':'||semantic_hash,E'\n' ORDER BY event_id),
                           'UTF8'
                       ),
                       'sha256'
                   ),
                   'hex'
               ) AS coverage_hash
               FROM insula.log
               WHERE house_id=$2 AND room=$3 AND spirit=$4 AND component=$5 AND layer=$6
                 AND operation=$7 AND phase=$8 AND outcome_class=$9
                 AND date_trunc('minute',observed_at AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
                     = date_trunc('minute',$1::timestamptz AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
           ) AS source
           WHERE v.query_name='insula.vitals.minute' AND v.query_version=1
             AND v.minute=date_trunc('minute',$1::timestamptz AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
             AND v.house_id=$2 AND v.room=$3 AND v.spirit=$4 AND v.component=$5
             AND v.layer=$6 AND v.operation=$7 AND v.phase=$8 AND v.outcome_class=$9"#,
    )
    .bind(e.observed_at)
    .bind(&b.house_id)
    .bind(&b.room)
    .bind(&b.spirit)
    .bind(&e.component)
    .bind(&e.layer)
    .bind(&e.operation)
    .bind(e.phase.as_str())
    .bind(e.outcome_class.as_str())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

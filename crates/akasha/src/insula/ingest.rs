use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use super::binding::{TrustedBinding, binding};
use super::error::{InsulaError, bad};
use super::event::{ObservationEvent, event};
use super::idempotency::{derive_idempotency_key_v1, derive_semantic_hash_v1};
use super::lock::lock;
use super::vitals::vitals;

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

#[derive(Debug)]
struct Existing {
    event_id: String,
    writer_id: String,
    sequence: i64,
    version: i16,
    scope: String,
    key: String,
    semantic: String,
}
fn logical(i: &Existing, e: &ObservationEvent) -> bool {
    i.version == e.idempotency_version
        && i.scope == e.idempotency_scope.as_str()
        && i.key == e.idempotency_key
}
async fn existing(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    b: &TrustedBinding,
    e: &ObservationEvent,
) -> Result<Vec<Existing>, InsulaError> {
    let rs=sqlx::query("SELECT event_id::text event_id,writer_id::text writer_id,writer_sequence,idempotency_version,idempotency_scope,idempotency_key,semantic_hash FROM insula.log WHERE(house_id=$1 AND idempotency_version=$2 AND idempotency_scope=$3 AND idempotency_key=$4)OR event_id=$5::uuid OR(writer_id=$6::uuid AND writer_sequence=$7)ORDER BY event_id FOR UPDATE").bind(&b.house_id).bind(e.idempotency_version).bind(e.idempotency_scope.as_str()).bind(&e.idempotency_key).bind(&e.event_id).bind(&e.writer_id).bind(e.writer_sequence).fetch_all(&mut **tx).await?;
    rs.into_iter()
        .map(|r| {
            Ok(Existing {
                event_id: r.try_get("event_id")?,
                writer_id: r.try_get("writer_id")?,
                sequence: r.try_get("writer_sequence")?,
                version: r.try_get("idempotency_version")?,
                scope: r.try_get("idempotency_scope")?,
                key: r.try_get("idempotency_key")?,
                semantic: r.try_get("semantic_hash")?,
            })
        })
        .collect::<Result<_, sqlx::Error>>()
        .map_err(Into::into)
}
async fn raw(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    b: &TrustedBinding,
    e: &ObservationEvent,
    x: DateTime<Utc>,
) -> Result<bool, InsulaError> {
    Ok(sqlx::query("INSERT INTO insula.log(schema_version,event_id,span_id,trace_id,parent_span_id,writer_id,writer_sequence,house_id,room,spirit,session_id,component,layer,operation,phase,observed_at,duration_us,outcome_class,error_class,bytes_in,bytes_out,tokens_in,tokens_out,tool_call_id,provider_request_id,idempotency_version,idempotency_scope,idempotency_key,receipt_kind,receipt_id,semantic_hash,drop_count,expires_at)VALUES(1,$1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::uuid,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32)ON CONFLICT DO NOTHING").bind(&e.event_id).bind(&e.span_id).bind(&e.trace_id).bind(&e.parent_span_id).bind(&e.writer_id).bind(e.writer_sequence).bind(&b.house_id).bind(&b.room).bind(&b.spirit).bind(&b.session_id).bind(&e.component).bind(&e.layer).bind(&e.operation).bind(e.phase.as_str()).bind(e.observed_at).bind(e.duration_us).bind(e.outcome_class.as_str()).bind(&e.error_class).bind(e.bytes_in).bind(e.bytes_out).bind(e.tokens_in).bind(e.tokens_out).bind(&e.tool_call_id).bind(&e.provider_request_id).bind(e.idempotency_version).bind(e.idempotency_scope.as_str()).bind(&e.idempotency_key).bind(&e.receipt_kind).bind(&e.receipt_id).bind(&e.semantic_hash).bind(e.drop_count).bind(x).execute(&mut **tx).await?.rows_affected()==1)
}

pub async fn ingest_batch(
    pool: &PgPool,
    b: &TrustedBinding,
    batch: &IngestBatch,
) -> Result<IngestReceipt, InsulaError> {
    binding(b)?;
    if batch.events.is_empty() {
        return Err(bad("events", "empty_batch"));
    }
    if batch.events.len() > INSULA_MAX_BATCH_EVENTS {
        return Err(bad("events", "batch_too_large"));
    }
    let prepared = batch
        .events
        .iter()
        .cloned()
        .map(|mut e| {
            let x = event(&e)?;
            e.idempotency_key = derive_idempotency_key_v1(b, &e)?;
            e.semantic_hash = derive_semantic_hash_v1(b, &e)?;
            Ok((e, x))
        })
        .collect::<Result<Vec<_>, InsulaError>>()?;
    let mut tx = pool.begin().await?;
    lock(&mut tx, &b.house_id, false).await?;
    let mut identity_locks = Vec::with_capacity(prepared.len() * 3);
    for (event, _) in &prepared {
        identity_locks.push(format!("insula:event:{}", event.event_id));
        identity_locks.push(format!(
            "insula:writer:{}:{}",
            event.writer_id, event.writer_sequence
        ));
        identity_locks.push(format!(
            "insula:logical:{}:{}:{}:{}",
            b.house_id,
            event.idempotency_version,
            event.idempotency_scope.as_str(),
            event.idempotency_key
        ));
    }
    identity_locks.sort_unstable();
    identity_locks.dedup();
    for identity in identity_locks {
        lock(&mut tx, &identity, true).await?;
    }
    let mut out = IngestReceipt {
        schema_version: 1,
        accepted_count: 0,
        duplicate_count: 0,
        conflicts: vec![],
    };
    for (index, (e, x)) in prepared.into_iter().enumerate() {
        if raw(&mut tx, b, &e, x).await? {
            vitals(&mut tx, b, &e).await?;
            out.accepted_count += 1;
            continue;
        }
        let found = existing(&mut tx, b, &e).await?;
        if let Some(i) = found.iter().find(|i| logical(i, &e))
            && i.semantic == e.semantic_hash
        {
            sqlx::query("UPDATE insula.log SET duplicate_count=duplicate_count+1,last_duplicate_at=NOW() WHERE event_id=$1::uuid")
                .bind(&i.event_id)
                .execute(&mut *tx)
                .await?;
            out.duplicate_count += 1;
            continue;
        }
        let before = out.conflicts.len();
        for i in &found {
            let kind = if logical(i, &e) {
                Some(IngestConflictKind::LogicalKeyReuse)
            } else if i.event_id == e.event_id {
                Some(IngestConflictKind::EventIdReuse)
            } else if i.writer_id == e.writer_id && i.sequence == e.writer_sequence {
                Some(IngestConflictKind::WriterSequenceReuse)
            } else {
                None
            };
            if let Some(kind) = kind {
                out.conflicts.push(IngestConflict {
                    event_index: index as u32,
                    kind,
                    event_id: e.event_id.clone(),
                    incumbent_event_id: i.event_id.clone(),
                })
            }
        }
        if out.conflicts.len() == before {
            return Err(InsulaError::Invariant(
                "conflicting insert had no reloadable incumbent",
            ));
        }
    }
    tx.commit().await?;
    Ok(out)
}

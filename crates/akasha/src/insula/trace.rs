use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use super::binding::{TrustedBinding, is_house, is_room, opaque, uuid};
use super::error::{InsulaError, bad};

pub const INSULA_MAX_TRACE_ROWS: u32 = 1_000;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TraceScope {
    pub house_id: String,
    pub room: Option<String>,
    pub spirit: Option<String>,
    pub session_id: Option<String>,
}
impl From<&TrustedBinding> for TraceScope {
    fn from(v: &TrustedBinding) -> Self {
        Self {
            house_id: v.house_id.clone(),
            room: Some(v.room.clone()),
            spirit: Some(v.spirit.clone()),
            session_id: Some(v.session_id.clone()),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TraceRow {
    pub schema_version: i16,
    pub event_id: String,
    pub span_id: String,
    pub trace_id: String,
    pub parent_span_id: Option<String>,
    pub writer_id: String,
    pub writer_sequence: i64,
    pub house_id: String,
    pub room: String,
    pub spirit: String,
    pub session_id: String,
    pub component: String,
    pub layer: String,
    pub operation: String,
    pub phase: String,
    pub observed_at: DateTime<Utc>,
    pub duration_us: Option<i64>,
    pub outcome_class: String,
    pub error_class: Option<String>,
    pub bytes_in: i64,
    pub bytes_out: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub tool_call_id: Option<String>,
    pub provider_request_id: Option<String>,
    pub idempotency_version: i16,
    pub idempotency_scope: String,
    pub idempotency_key: String,
    pub receipt_kind: Option<String>,
    pub receipt_id: Option<String>,
    pub semantic_hash: String,
    pub drop_count: i64,
    pub expires_at: DateTime<Utc>,
    pub duplicate_count: i64,
    pub last_duplicate_at: Option<DateTime<Utc>>,
    pub ingested_at: DateTime<Utc>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TraceResult {
    pub query_name: String,
    pub query_version: i16,
    pub rows: Vec<TraceRow>,
}

pub async fn query_trace(
    pool: &PgPool,
    s: &TraceScope,
    trace: &str,
    limit: u32,
) -> Result<TraceResult, InsulaError> {
    if !is_house(&s.house_id)
        || s.room.as_deref().is_some_and(|v| !is_room(v))
        || s.spirit.as_deref().is_some_and(|v| !opaque(v, 64))
        || s.session_id.as_deref().is_some_and(|v| !opaque(v, 128))
    {
        return Err(bad("scope", "invalid_scope"));
    }
    uuid("traceId", trace)?;
    if limit == 0 || limit > INSULA_MAX_TRACE_ROWS {
        return Err(bad("limit", "out_of_range"));
    }
    let rs = sqlx::query(
        r#"WITH RECURSIVE scoped AS (
               SELECT *
               FROM insula.log
               WHERE house_id=$1 AND trace_id=$2::uuid
                 AND ($3::text IS NULL OR room=$3)
                 AND ($4::text IS NULL OR spirit=$4)
                 AND ($5::text IS NULL OR session_id=$5)
           ),
           span_edges AS (
               SELECT DISTINCT span_id,parent_span_id
               FROM scoped
           ),
           ancestry(event_id,next_parent,path,causal_depth) AS (
               SELECT event_id,parent_span_id,ARRAY[span_id],0
               FROM scoped
               UNION ALL
               SELECT ancestry.event_id,edge.parent_span_id,ancestry.path||edge.span_id,
                      ancestry.causal_depth+1
               FROM ancestry
               JOIN span_edges AS edge ON edge.span_id=ancestry.next_parent
               WHERE ancestry.next_parent IS NOT NULL
                 AND NOT edge.span_id=ANY(ancestry.path)
                 AND ancestry.causal_depth<255
           ),
           ranked AS (
               SELECT event_id,MAX(causal_depth) AS causal_depth
               FROM ancestry
               GROUP BY event_id
           )
           SELECT scoped.schema_version,scoped.event_id::text event_id,
                  scoped.span_id::text span_id,scoped.trace_id::text trace_id,
                  scoped.parent_span_id::text parent_span_id,scoped.writer_id::text writer_id,
                  scoped.writer_sequence,scoped.house_id,scoped.room,scoped.spirit,
                  scoped.session_id,scoped.component,scoped.layer,scoped.operation,scoped.phase,
                  scoped.observed_at,scoped.duration_us,scoped.outcome_class,scoped.error_class,
                  scoped.bytes_in,scoped.bytes_out,scoped.tokens_in,scoped.tokens_out,
                  scoped.tool_call_id,scoped.provider_request_id,scoped.idempotency_version,
                  scoped.idempotency_scope,scoped.idempotency_key,scoped.receipt_kind,
                  scoped.receipt_id,scoped.semantic_hash,scoped.drop_count,scoped.expires_at,
                  scoped.duplicate_count,scoped.last_duplicate_at,scoped.ingested_at
           FROM scoped
           JOIN ranked USING(event_id)
           ORDER BY ranked.causal_depth,scoped.writer_id,scoped.writer_sequence,scoped.event_id
           LIMIT $6"#,
    )
    .bind(&s.house_id)
    .bind(trace)
    .bind(&s.room)
    .bind(&s.spirit)
    .bind(&s.session_id)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    let rows = rs
        .into_iter()
        .map(|r| {
            Ok(TraceRow {
                schema_version: r.try_get("schema_version")?,
                event_id: r.try_get("event_id")?,
                span_id: r.try_get("span_id")?,
                trace_id: r.try_get("trace_id")?,
                parent_span_id: r.try_get("parent_span_id")?,
                writer_id: r.try_get("writer_id")?,
                writer_sequence: r.try_get("writer_sequence")?,
                house_id: r.try_get("house_id")?,
                room: r.try_get("room")?,
                spirit: r.try_get("spirit")?,
                session_id: r.try_get("session_id")?,
                component: r.try_get("component")?,
                layer: r.try_get("layer")?,
                operation: r.try_get("operation")?,
                phase: r.try_get("phase")?,
                observed_at: r.try_get("observed_at")?,
                duration_us: r.try_get("duration_us")?,
                outcome_class: r.try_get("outcome_class")?,
                error_class: r.try_get("error_class")?,
                bytes_in: r.try_get("bytes_in")?,
                bytes_out: r.try_get("bytes_out")?,
                tokens_in: r.try_get("tokens_in")?,
                tokens_out: r.try_get("tokens_out")?,
                tool_call_id: r.try_get("tool_call_id")?,
                provider_request_id: r.try_get("provider_request_id")?,
                idempotency_version: r.try_get("idempotency_version")?,
                idempotency_scope: r.try_get("idempotency_scope")?,
                idempotency_key: r.try_get("idempotency_key")?,
                receipt_kind: r.try_get("receipt_kind")?,
                receipt_id: r.try_get("receipt_id")?,
                semantic_hash: r.try_get("semantic_hash")?,
                drop_count: r.try_get("drop_count")?,
                expires_at: r.try_get("expires_at")?,
                duplicate_count: r.try_get("duplicate_count")?,
                last_duplicate_at: r.try_get("last_duplicate_at")?,
                ingested_at: r.try_get("ingested_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(TraceResult {
        query_name: "insula.trace".into(),
        query_version: 1,
        rows,
    })
}

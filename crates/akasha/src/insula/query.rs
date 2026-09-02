//! The read side of Insula: vitals rollups, causal traces, and sessions
//! whose restart exit was never verified.

use chrono::{DateTime, Duration, Utc};
use protocol::restart::RestartState;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::event::{ObservationPhase, OutcomeClass, TrustedBinding, atom, is_room, opaque, uuid};
use super::{InsulaError, bad};

pub const INSULA_MAX_VITALS_ROWS: u32 = 5_000;
pub(super) const VITALS: &str = "insula.vitals.minute";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VitalsQuery {
    pub house_id: String,
    pub room: Option<String>,
    pub spirit: Option<String>,
    pub component: Option<String>,
    pub layer: Option<String>,
    pub operation: Option<String>,
    pub phase: Option<ObservationPhase>,
    pub outcome_class: Option<OutcomeClass>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub limit: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct VitalsRow {
    pub minute: DateTime<Utc>,
    pub house_id: String,
    pub room: String,
    pub spirit: String,
    pub component: String,
    pub layer: String,
    pub operation: String,
    pub phase: String,
    pub outcome_class: String,
    pub event_count: i64,
    pub duration_us_sum: i64,
    pub duration_us_max: Option<i64>,
    pub bytes_in_sum: i64,
    pub bytes_out_sum: i64,
    pub tokens_in_sum: i64,
    pub tokens_out_sum: i64,
    pub drop_count_sum: i64,
    pub source_first_sequence: i64,
    pub source_last_sequence: i64,
    pub source_first_observed_at: DateTime<Utc>,
    pub source_last_observed_at: DateTime<Utc>,
    pub source_coverage_hash: String,
    pub updated_at: DateTime<Utc>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VitalsResult {
    pub query_name: String,
    pub query_version: i16,
    pub rows: Vec<VitalsRow>,
}

pub async fn query_vitals(pool: &PgPool, q: &VitalsQuery) -> Result<VitalsResult, InsulaError> {
    if !atom(&q.house_id, 64)
        || q.end <= q.start
        || q.end - q.start > Duration::days(366)
        || q.limit == 0
        || q.limit > INSULA_MAX_VITALS_ROWS
    {
        return Err(bad("query", "out_of_range"));
    }
    let p = q.phase.map(ObservationPhase::as_str);
    let o = q.outcome_class.map(OutcomeClass::as_str);
    let rows=sqlx::query_as::<_, VitalsRow>("SELECT minute,house_id,room,spirit,component,layer,operation,phase,outcome_class,event_count,duration_us_sum,duration_us_max,bytes_in_sum,bytes_out_sum,tokens_in_sum,tokens_out_sum,drop_count_sum,source_first_sequence,source_last_sequence,source_first_observed_at,source_last_observed_at,source_coverage_hash,updated_at FROM insula.vitals_minute WHERE query_name='insula.vitals.minute'AND query_version=1 AND house_id=$1 AND minute>=$2 AND minute<$3 AND($4::text IS NULL OR room=$4)AND($5::text IS NULL OR spirit=$5)AND($6::text IS NULL OR component=$6)AND($7::text IS NULL OR layer=$7)AND($8::text IS NULL OR operation=$8)AND($9::text IS NULL OR phase=$9)AND($10::text IS NULL OR outcome_class=$10)ORDER BY minute,room,spirit LIMIT $11").bind(&q.house_id).bind(q.start).bind(q.end).bind(&q.room).bind(&q.spirit).bind(&q.component).bind(&q.layer).bind(&q.operation).bind(p).bind(o).bind(i64::from(q.limit)).fetch_all(pool).await?;
    Ok(VitalsResult {
        query_name: VITALS.into(),
        query_version: 1,
        rows,
    })
}

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
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, sqlx::FromRow)]
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
    if !atom(&s.house_id, 64)
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
    let rows = sqlx::query_as::<_, TraceRow>(
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
    Ok(TraceResult {
        query_name: "insula.trace".into(),
        query_version: 1,
        rows,
    })
}

// enough: a restart storm is bounded to three exits per workspace per hour, so
// a hundred newest-first rows cover days of unverified exits; upgrade path is a
// keyset cursor on (exiting_at, intent_id), not a bigger cap.
pub const INSULA_MAX_UNVERIFIED_EXIT_ROWS: u32 = 100;
const UNVERIFIED_EXIT: &str = "insula.session.unverified_exit";

/// One session that armed a restart exit and never came back verified. The
/// restart plane owns these columns (`restart.intents`); this family only
/// observes them, which is why the row carries no writer or span identity.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct UnverifiedExitRow {
    pub intent_id: String,
    pub workspace: String,
    pub session_id: Option<String>,
    pub mode: String,
    pub state: String,
    pub failed_stage: Option<String>,
    pub requester_room: String,
    pub requester_spirit: String,
    pub requester_session: String,
    pub exiting_at: DateTime<Utc>,
    pub deadline_at: DateTime<Utc>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UnverifiedExitResult {
    pub query_name: String,
    pub query_version: i16,
    /// The room this read was scoped to. The rows carry a workspace path and a
    /// requester session, so the answer names whose divergence it reports.
    pub room: String,
    pub window_secs: i64,
    pub rows: Vec<UnverifiedExitRow>,
}

/// Sessions whose restart intent reached `exiting` and never reached
/// `verified` inside the stage window, for one room only. The rows carry a
/// workspace path and a requester identity, so the room comes from the Host's
/// own binding and never from the caller (Kintsu's Insula verdict, 2026-08-25).
///
/// The window comes from the restart module's const block — one authority for
/// the deadline, so a policy change here can never drift from the plane that
/// enforces it.
pub async fn query_unverified_exit(
    pool: &PgPool,
    room: &str,
    limit: u32,
) -> Result<UnverifiedExitResult, InsulaError> {
    if !is_room(room) {
        return Err(bad("room", "invalid_room_key"));
    }
    if limit == 0 || limit > INSULA_MAX_UNVERIFIED_EXIT_ROWS {
        return Err(bad("limit", "out_of_range"));
    }
    let window_secs =
        crate::restart::EXITING_DEADLINE_SECS + crate::restart::RELAUNCHING_DEADLINE_SECS;
    // The first exiting event is the one that starts the clock: a retry never
    // buys a session more silence.
    let rows = sqlx::query_as::<_, UnverifiedExitRow>(
        "SELECT intent.intent_id::text intent_id,intent.workspace,intent.session_id,intent.mode,intent.state,intent.failed_stage,intent.requester_room,intent.requester_spirit,intent.requester_session,exit_event.created_at exiting_at,exit_event.created_at+($1*INTERVAL '1 second')deadline_at FROM restart.intents intent JOIN LATERAL(SELECT created_at FROM restart.intent_events WHERE intent_id=intent.intent_id AND event_kind=$2 ORDER BY created_at LIMIT 1)exit_event ON TRUE WHERE intent.requester_room=$3 AND intent.verified_at IS NULL AND exit_event.created_at+($1*INTERVAL '1 second')<=NOW() ORDER BY exit_event.created_at DESC LIMIT $4",
    )
    .bind(window_secs)
    .bind(RestartState::Exiting.as_str())
    .bind(room)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    Ok(UnverifiedExitResult {
        query_name: UNVERIFIED_EXIT.into(),
        query_version: 1,
        room: room.to_owned(),
        window_secs,
        rows,
    })
}

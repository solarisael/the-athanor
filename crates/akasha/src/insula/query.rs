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

// enough: a lane drawer names the newest handful of spans behind one lane, so a
// hundred newest-first rows cover every window this read offers; upgrade path
// is a keyset cursor on (observed_at, span_id), not a bigger cap.
//
// 101, not 100: the Host offers callers a ceiling of 100 and then asks for one
// row beyond it to report truncation honestly, the same probe the vitals and
// trace routes perform. The extra row is that probe, never an offered row.
pub const INSULA_MAX_SPAN_ROWS: u32 = 101;
const SPANS: &str = "insula.spans.recent";

/// The windows a lane drawer may ask for. A closed set rather than a
/// caller-supplied interval: this read exists because `insula.log` holds
/// millions of rows, and only a recent lower bound keeps it an index walk. An
/// open range would restore the scan the query was written to avoid.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum SpanWindow {
    #[serde(rename = "15m")]
    M15,
    #[serde(rename = "1h")]
    H1,
    #[serde(rename = "24h")]
    D24,
}
impl SpanWindow {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::M15 => "15m",
            Self::H1 => "1h",
            Self::D24 => "24h",
        }
    }
    pub fn secs(self) -> i64 {
        match self {
            Self::M15 => 900,
            Self::H1 => 3_600,
            Self::D24 => 86_400,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SpansQuery {
    pub house_id: String,
    pub room: String,
    pub operation: String,
    pub phase: Option<ObservationPhase>,
    pub outcome_class: Option<OutcomeClass>,
    pub window: SpanWindow,
    pub limit: u32,
}

/// One span behind a lane: enough to name it, and the trace identity the
/// operator drills into. The wide row shape belongs to `insula.trace`; this
/// read is the door to that one, never a second copy of it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SpanRow {
    pub trace_id: String,
    pub span_id: String,
    pub observed_at: DateTime<Utc>,
    pub duration_us: Option<i64>,
    pub outcome_class: String,
    pub error_class: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpansResult {
    pub query_name: String,
    pub query_version: i16,
    pub window_secs: i64,
    pub rows: Vec<SpanRow>,
}

/// The shipped statement, named so the plan proof can EXPLAIN exactly this and
/// never a paraphrase of it.
///
/// `ORDER BY log.observed_at, log.span_id` is qualified on purpose. The select
/// list aliases `span_id::text AS span_id`, and an unqualified `span_id` in
/// ORDER BY binds to that text output column instead of the uuid column — which
/// silently costs the index and reintroduces a Sort over the whole window.
pub(super) const SPANS_SQL: &str = r#"SELECT log.trace_id::text trace_id,log.span_id::text span_id,
                  log.observed_at,log.duration_us,log.outcome_class,log.error_class
           FROM insula.log AS log
           WHERE log.house_id=$1 AND log.room=$2 AND log.operation=$3
             AND log.observed_at>=NOW()-($4*INTERVAL '1 second')
             AND ($5::text IS NULL OR log.phase=$5)
             AND ($6::text IS NULL OR log.outcome_class=$6)
           ORDER BY log.observed_at DESC,log.span_id DESC
           LIMIT $7"#;

/// The newest spans for one lane, newest first.
///
/// `room` is required and has no wildcard, unlike `TraceScope::room`. A lane
/// drawer that could read house-wide would let one room enumerate another
/// room's spans and then walk them through `insula.trace`, so an absent or
/// malformed room is a refusal here rather than "every room".
///
/// The ordering and the window bound are the same pair the composite index in
/// `0029_insula_log_lane_spans.sql` was added for; changing either one without
/// that index turns this read back into a full-table scan.
pub async fn query_spans(pool: &PgPool, q: &SpansQuery) -> Result<SpansResult, InsulaError> {
    if !atom(&q.house_id, 64) {
        return Err(bad("houseId", "invalid_house_key"));
    }
    if !is_room(&q.room) {
        return Err(bad("room", "invalid_room_key"));
    }
    if !atom(&q.operation, 64) {
        return Err(bad("operation", "invalid_operation"));
    }
    if q.limit == 0 || q.limit > INSULA_MAX_SPAN_ROWS {
        return Err(bad("limit", "out_of_range"));
    }
    let window_secs = q.window.secs();
    let rows = sqlx::query_as::<_, SpanRow>(SPANS_SQL)
        .bind(&q.house_id)
        .bind(&q.room)
        .bind(&q.operation)
        .bind(window_secs)
        .bind(q.phase.map(ObservationPhase::as_str))
        .bind(q.outcome_class.map(OutcomeClass::as_str))
        .bind(q.limit as i64)
        .fetch_all(pool)
        .await?;
    Ok(SpansResult {
        query_name: SPANS.into(),
        query_version: 1,
        window_secs,
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

#[cfg(test)]
mod tests {
    use super::{
        INSULA_MAX_SPAN_ROWS, ObservationPhase, OutcomeClass, SPANS_SQL, SpanWindow, SpansQuery,
        query_spans,
    };
    use crate::InsulaError;
    use chrono::{DateTime, Duration, Utc};
    use sha2::{Digest, Sha256};
    use sqlx::PgPool;
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

    const INSULA_MIGRATION: &str = include_str!("../../../../substrate/migrations/0022_insula.sql");
    const LANE_SPANS_MIGRATION: &str =
        include_str!("../../../../substrate/migrations/0029_insula_log_lane_spans.sql");

    const HOUSE: &str = "solarisael";

    /// `insula.log` constrains both `idempotency_key` and `semantic_hash` to a
    /// lowercase sha256, so the seed derives them instead of inventing labels
    /// the schema would rightly refuse.
    fn hex64(domain: &str, event_id: Uuid) -> String {
        let mut hash = Sha256::new();
        hash.update(domain);
        hash.update(event_id.as_bytes());
        format!("{:x}", hash.finalize())
    }

    #[test]
    fn span_windows_name_their_own_seconds() {
        assert_eq!(SpanWindow::M15.as_str(), "15m");
        assert_eq!(SpanWindow::M15.secs(), 900);
        assert_eq!(SpanWindow::H1.as_str(), "1h");
        assert_eq!(SpanWindow::H1.secs(), 3_600);
        assert_eq!(SpanWindow::D24.as_str(), "24h");
        assert_eq!(SpanWindow::D24.secs(), 86_400);
    }

    #[test]
    fn span_windows_travel_as_the_operator_wrote_them() -> TestResult {
        assert_eq!(serde_json::to_string(&SpanWindow::M15)?, "\"15m\"");
        assert_eq!(
            serde_json::from_str::<SpanWindow>("\"24h\"")?,
            SpanWindow::D24
        );
        assert!(serde_json::from_str::<SpanWindow>("\"7d\"").is_err());
        Ok(())
    }

    fn isolated_database_url() -> String {
        let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
            .expect("Insula spans proof requires a dedicated PostgreSQL URL");
        let lower = url.to_ascii_lowercase();
        assert!(
            !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
            "refusing a live or production-looking database"
        );
        url
    }

    /// The isolated Insula schema plus the lane-spans index this query was
    /// written against, so the plan proof below measures the shipped shape.
    async fn fresh_insula() -> TestResult<PgPool> {
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&isolated_database_url())
            .await?;
        sqlx::query("DROP SCHEMA IF EXISTS insula CASCADE")
            .execute(&pool)
            .await?;
        sqlx::raw_sql(INSULA_MIGRATION).execute(&pool).await?;
        sqlx::raw_sql(LANE_SPANS_MIGRATION).execute(&pool).await?;
        Ok(pool)
    }

    struct Seed<'a> {
        room: &'a str,
        operation: &'a str,
        phase: &'a str,
        outcome: &'a str,
        error_class: Option<&'a str>,
        age: Duration,
        duration_us: Option<i64>,
    }

    /// One raw observation, inserted directly. `ingest_batch` derives keys and
    /// hashes it does not need here; this proof is about the read.
    async fn seed(pool: &PgPool, row: &Seed<'_>) -> TestResult<(String, DateTime<Utc>)> {
        let event_id = Uuid::new_v4();
        let span_id = Uuid::new_v4();
        let trace_id = Uuid::new_v4();
        let observed_at = Utc::now() - row.age;
        sqlx::query(
            "INSERT INTO insula.log (
                 event_id,span_id,trace_id,writer_id,writer_sequence,
                 house_id,room,spirit,session_id,component,layer,operation,phase,
                 observed_at,duration_us,outcome_class,error_class,
                 idempotency_scope,idempotency_key,semantic_hash,expires_at
             ) VALUES (
                 $1::uuid,$2::uuid,$3::uuid,gen_random_uuid(),$4,
                 $5,$6,'Kodo','service:kodo','omp_adapter','adapter',$7,$8,
                 $9,$10,$11,$12,
                 'trace_span',$13,$14,$9 + INTERVAL '14 days'
             )",
        )
        .bind(event_id.to_string())
        .bind(span_id.to_string())
        .bind(trace_id.to_string())
        .bind(observed_at.timestamp_micros())
        .bind(HOUSE)
        .bind(row.room)
        .bind(row.operation)
        .bind(row.phase)
        .bind(observed_at)
        .bind(row.duration_us)
        .bind(row.outcome)
        .bind(row.error_class)
        .bind(hex64("idempotency", event_id))
        .bind(hex64("semantic", event_id))
        .execute(pool)
        .await?;
        Ok((trace_id.to_string(), observed_at))
    }

    fn lane(room: &str, window: SpanWindow) -> SpansQuery {
        SpansQuery {
            house_id: HOUSE.to_owned(),
            room: room.to_owned(),
            operation: "tool_call".to_owned(),
            phase: Some(ObservationPhase::End),
            outcome_class: None,
            window,
            limit: 10,
        }
    }

    #[tokio::test]
    #[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Insula schema"]
    async fn postgres_spans_honor_window_limit_order_and_room_scope() -> TestResult {
        let pool = fresh_insula().await?;

        // Three settled kodo spans inside 15m, newest last so insertion order
        // cannot be mistaken for the ordering the query proves.
        let mut recent = Vec::new();
        for minutes in [12, 6, 2] {
            recent.push(
                seed(
                    &pool,
                    &Seed {
                        room: "kodo",
                        operation: "tool_call",
                        phase: "end",
                        outcome: "ok",
                        error_class: None,
                        age: Duration::minutes(minutes),
                        duration_us: Some(minutes * 1_000),
                    },
                )
                .await?,
            );
        }
        // Older than 15m, inside 1h.
        let middle = seed(
            &pool,
            &Seed {
                room: "kodo",
                operation: "tool_call",
                phase: "end",
                outcome: "error",
                error_class: Some("tool_error"),
                age: Duration::minutes(40),
                duration_us: Some(9_000),
            },
        )
        .await?;
        // Older than 24h: outside every offered window.
        seed(
            &pool,
            &Seed {
                room: "kodo",
                operation: "tool_call",
                phase: "end",
                outcome: "ok",
                error_class: None,
                age: Duration::hours(30),
                duration_us: Some(1),
            },
        )
        .await?;
        // Same lane name, another room: the scoping case.
        let other_room = seed(
            &pool,
            &Seed {
                room: "kintsu",
                operation: "tool_call",
                phase: "end",
                outcome: "ok",
                error_class: None,
                age: Duration::minutes(1),
                duration_us: Some(5),
            },
        )
        .await?;
        // Another lane in this room: the operation filter's case.
        seed(
            &pool,
            &Seed {
                room: "kodo",
                operation: "provider_request",
                phase: "end",
                outcome: "ok",
                error_class: None,
                age: Duration::minutes(1),
                duration_us: Some(7),
            },
        )
        .await?;
        // A start row in this lane: phase filtering keeps it out.
        seed(
            &pool,
            &Seed {
                room: "kodo",
                operation: "tool_call",
                phase: "start",
                outcome: "unknown",
                error_class: None,
                age: Duration::minutes(1),
                duration_us: None,
            },
        )
        .await?;

        // Window is honored: 15m sees three, 1h sees four, 24h still four.
        let quarter = query_spans(&pool, &lane("kodo", SpanWindow::M15)).await?;
        assert_eq!(quarter.rows.len(), 3);
        assert_eq!(quarter.window_secs, 900);
        assert_eq!(quarter.query_name, "insula.spans.recent");
        assert_eq!(quarter.query_version, 1);

        let hour = query_spans(&pool, &lane("kodo", SpanWindow::H1)).await?;
        assert_eq!(hour.rows.len(), 4);
        assert_eq!(hour.window_secs, 3_600);
        let day = query_spans(&pool, &lane("kodo", SpanWindow::D24)).await?;
        assert_eq!(day.rows.len(), 4, "the 30 h row stays outside 24h");

        // Newest first, and the 40-minute row sorts last inside the hour.
        let observed: Vec<DateTime<Utc>> = hour.rows.iter().map(|row| row.observed_at).collect();
        let mut newest_first = observed.clone();
        newest_first.sort_by(|a, b| b.cmp(a));
        assert_eq!(observed, newest_first);
        assert_eq!(hour.rows[0].trace_id, recent[2].0);
        assert_eq!(hour.rows[3].trace_id, middle.0);
        assert_eq!(hour.rows[3].outcome_class, "error");
        assert_eq!(hour.rows[3].error_class.as_deref(), Some("tool_error"));
        assert_eq!(hour.rows[3].duration_us, Some(9_000));

        // Room scoping: the kintsu row never appears in a kodo read, and the
        // kodo rows never appear in a kintsu read. Neither room can borrow the
        // other's trace identities through this door.
        assert!(
            hour.rows.iter().all(|row| row.trace_id != other_room.0),
            "another room's span reached this room's lane"
        );
        let across = query_spans(&pool, &lane("kintsu", SpanWindow::H1)).await?;
        assert_eq!(across.rows.len(), 1);
        assert_eq!(across.rows[0].trace_id, other_room.0);

        // Limit is honored, and it truncates the newest end of the list.
        let mut capped = lane("kodo", SpanWindow::H1);
        capped.limit = 2;
        let capped = query_spans(&pool, &capped).await?;
        assert_eq!(capped.rows.len(), 2);
        assert_eq!(capped.rows[0].trace_id, recent[2].0);
        assert_eq!(capped.rows[1].trace_id, recent[1].0);

        // The outcome filter narrows without changing the contract.
        let mut only_errors = lane("kodo", SpanWindow::H1);
        only_errors.outcome_class = Some(OutcomeClass::Error);
        let only_errors = query_spans(&pool, &only_errors).await?;
        assert_eq!(only_errors.rows.len(), 1);
        assert_eq!(only_errors.rows[0].trace_id, middle.0);

        // Every returned trace id is a real key into insula.trace.
        for row in &hour.rows {
            let hit: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM insula.log WHERE trace_id=$1::uuid AND room='kodo'",
            )
            .bind(&row.trace_id)
            .fetch_one(&pool)
            .await?;
            assert!(hit > 0, "trace {} names no row", row.trace_id);
        }

        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Insula schema"]
    async fn postgres_spans_refuse_a_malformed_scope_or_limit() -> TestResult {
        let pool = fresh_insula().await?;

        let refusal = |q: &SpansQuery| {
            let q = q.clone();
            let pool = pool.clone();
            async move {
                match query_spans(&pool, &q).await {
                    Err(InsulaError::Validation { field, code }) => (field, code),
                    other => panic!("expected a validation refusal, got {other:?}"),
                }
            }
        };

        let mut empty_room = lane("kodo", SpanWindow::H1);
        empty_room.room = String::new();
        assert_eq!(refusal(&empty_room).await, ("room", "invalid_room_key"));

        // A room key is a lowercase atom, so neither a wildcard nor injected SQL
        // can enter through it.
        let mut wildcard = lane("kodo", SpanWindow::H1);
        wildcard.room = "%".to_owned();
        assert_eq!(refusal(&wildcard).await, ("room", "invalid_room_key"));
        let mut injected = lane("kodo", SpanWindow::H1);
        injected.room = "kodo' OR '1'='1".to_owned();
        assert_eq!(refusal(&injected).await, ("room", "invalid_room_key"));

        let mut house = lane("kodo", SpanWindow::H1);
        house.house_id = "Solarisael".to_owned();
        assert_eq!(refusal(&house).await, ("houseId", "invalid_house_key"));

        let mut operation = lane("kodo", SpanWindow::H1);
        operation.operation = "Tool Call".to_owned();
        assert_eq!(
            refusal(&operation).await,
            ("operation", "invalid_operation")
        );

        let mut zero = lane("kodo", SpanWindow::H1);
        zero.limit = 0;
        assert_eq!(refusal(&zero).await, ("limit", "out_of_range"));
        let mut over = lane("kodo", SpanWindow::H1);
        over.limit = INSULA_MAX_SPAN_ROWS + 1;
        assert_eq!(refusal(&over).await, ("limit", "out_of_range"));

        Ok(())
    }

    /// The plan proof: this read must be an index walk on the lane-spans index,
    /// not a scan of `insula.log`. It EXPLAINs `SPANS_SQL` itself, so the proof
    /// cannot drift from the statement `query_spans` runs.
    ///
    /// `enable_seqscan` and `enable_bitmapscan` are off for the EXPLAIN only. A
    /// seeded test table holds tens of rows, where a sequential or bitmap plan
    /// is genuinely cheaper and says nothing about the 3 M-row case this index
    /// exists for. Both of those plans also throw the index's ordering away — a
    /// bitmap scan collects row locations and then must Sort — so at this size
    /// the planner would hide the very property under test.
    ///
    /// Denying them asks the question that actually matters: *can* the index
    /// answer this predicate in this order, or must the planner still sort? The
    /// absent Sort node is the real assertion, and it is row-count independent,
    /// because it proves the requested ordering is the index's own. At real size
    /// the ordered index scan is also what the planner picks unaided: with
    /// `LIMIT 10` it can stop after ten rows instead of materializing the
    /// window.
    #[tokio::test]
    #[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Insula schema"]
    async fn postgres_spans_walk_the_lane_index_rather_than_scanning() -> TestResult {
        let pool = fresh_insula().await?;
        for minutes in 0..40 {
            seed(
                &pool,
                &Seed {
                    room: if minutes % 3 == 0 { "kintsu" } else { "kodo" },
                    operation: if minutes % 2 == 0 {
                        "tool_call"
                    } else {
                        "provider_request"
                    },
                    phase: "end",
                    outcome: "ok",
                    error_class: None,
                    age: Duration::minutes(minutes),
                    duration_us: Some(minutes),
                },
            )
            .await?;
        }
        sqlx::query("ANALYZE insula.log").execute(&pool).await?;

        let mut tx = pool.begin().await?;
        sqlx::query("SET LOCAL enable_seqscan = off")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET LOCAL enable_bitmapscan = off")
            .execute(&mut *tx)
            .await?;
        let plan: Vec<String> = sqlx::query_scalar(&format!("EXPLAIN {SPANS_SQL}"))
            .bind(HOUSE)
            .bind("kodo")
            .bind("tool_call")
            .bind(SpanWindow::H1.secs())
            .bind(Some("end"))
            .bind(None::<&str>)
            .bind(10_i64)
            .fetch_all(&mut *tx)
            .await?;
        tx.rollback().await?;
        let plan = plan.join("\n");
        // The shape this index was added to produce, verbatim from EXPLAIN:
        //   Limit
        //     ->  Index Scan using idx_insula_log_lane_spans on log
        //           Index Cond: house_id, room, operation, observed_at >= ...
        //           Filter: phase = 'end'
        assert!(
            plan.contains("Index Scan using idx_insula_log_lane_spans"),
            "the lane read did not walk its index in order:\n{plan}"
        );

        assert!(
            plan.contains("idx_insula_log_lane_spans"),
            "the lane read left its index:\n{plan}"
        );
        assert!(
            !plan.contains("Seq Scan"),
            "the lane read scanned insula.log:\n{plan}"
        );
        // The index supplies the ordering, so no Sort node may appear: a Sort
        // here would mean the whole matched window is materialized before LIMIT.
        assert!(
            !plan.contains("Sort"),
            "the lane read sorted instead of walking in index order:\n{plan}"
        );
        // And the window bound really is a seek, not a filter applied after the
        // walk: the equality prefix plus the range must ride the index condition.
        assert!(
            plan.contains("Index Cond") && plan.contains("observed_at"),
            "the window bound never reached the index condition:\n{plan}"
        );

        Ok(())
    }
}

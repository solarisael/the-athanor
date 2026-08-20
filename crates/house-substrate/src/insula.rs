use chrono::{DateTime, Duration, SecondsFormat, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

pub const INSULA_SCHEMA_VERSION: i16 = 1;
pub const INSULA_QUERY_VERSION: i16 = 1;
pub const INSULA_DEFAULT_RETENTION_DAYS: i16 = 14;
pub const INSULA_MAX_BATCH_EVENTS: usize = 512;
pub const INSULA_MAX_TRACE_ROWS: u32 = 1_000;
pub const INSULA_MAX_VITALS_ROWS: u32 = 5_000;
const VITALS: &str = "insula.vitals.minute";
const RETENTION: &str = "insula.retention.raw_delete";

#[derive(Debug, Error)]
pub enum InsulaError {
    #[error("invalid Insula field {field}: {code}")]
    Validation {
        field: &'static str,
        code: &'static str,
    },
    #[error("Insula database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Insula persistence invariant failed: {0}")]
    Invariant(&'static str),
}
fn bad(field: &'static str, code: &'static str) -> InsulaError {
    InsulaError::Validation { field, code }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TrustedBinding {
    pub house_id: String,
    pub room: String,
    pub spirit: String,
    pub session_id: String,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservationPhase {
    Start,
    End,
    Point,
    Drop,
}
impl ObservationPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::Point => "point",
            Self::Drop => "drop",
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeClass {
    Ok,
    Refused,
    Error,
    Timeout,
    Cancelled,
    Degraded,
    Unknown,
}
impl OutcomeClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Refused => "refused",
            Self::Error => "error",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::Degraded => "degraded",
            Self::Unknown => "unknown",
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdempotencyScope {
    WriterSequence,
    ToolCall,
    ProviderRequest,
    TraceSpan,
    RoomOperation,
}
impl IdempotencyScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WriterSequence => "writer_sequence",
            Self::ToolCall => "tool_call",
            Self::ProviderRequest => "provider_request",
            Self::TraceSpan => "trace_span",
            Self::RoomOperation => "room_operation",
        }
    }
}
fn one() -> i16 {
    1
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ObservationEvent {
    pub event_id: String,
    pub span_id: String,
    pub trace_id: String,
    pub parent_span_id: Option<String>,
    pub writer_id: String,
    pub writer_sequence: i64,
    pub component: String,
    pub layer: String,
    pub operation: String,
    pub phase: ObservationPhase,
    pub observed_at: DateTime<Utc>,
    pub duration_us: Option<i64>,
    pub outcome_class: OutcomeClass,
    pub error_class: Option<String>,
    #[serde(default)]
    pub bytes_in: i64,
    #[serde(default)]
    pub bytes_out: i64,
    #[serde(default)]
    pub tokens_in: i64,
    #[serde(default)]
    pub tokens_out: i64,
    pub tool_call_id: Option<String>,
    pub provider_request_id: Option<String>,
    #[serde(default = "one")]
    pub idempotency_version: i16,
    pub idempotency_scope: IdempotencyScope,
    #[serde(skip)]
    pub idempotency_key: String,
    pub receipt_kind: Option<String>,
    pub receipt_id: Option<String>,
    #[serde(skip)]
    pub semantic_hash: String,
    #[serde(default)]
    pub drop_count: i64,
}
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
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetentionStatus {
    Deleted,
    Replayed,
    Noop,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RetentionReceipt {
    pub receipt_id: Option<String>,
    pub receipt_kind: String,
    pub receipt_version: i16,
    pub status: RetentionStatus,
    pub house_id: String,
    pub sweep_version: i16,
    pub sweep_key: String,
    pub retention_days: i16,
    pub swept_through: DateTime<Utc>,
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: Option<DateTime<Utc>>,
    pub event_count: i64,
    pub writer_count: i64,
    pub duplicate_count_sum: i64,
    pub drop_count_sum: i64,
    pub coverage_version: Option<i16>,
    pub coverage_hash: Option<String>,
    pub rollup_query_name: String,
    pub rollup_query_version: i16,
    pub rollup_watermark: Option<DateTime<Utc>>,
}

fn is_house(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.bytes().enumerate().all(|(i, b)| {
            b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || (i > 0 && matches!(b, b'_' | b'.' | b':' | b'-'))
        })
}
fn is_room(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && !v.starts_with('-')
        && !v.ends_with('-')
        && !v.contains("--")
        && v.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn atom(v: &str, n: usize) -> bool {
    !v.is_empty()
        && v.len() <= n
        && v.bytes().enumerate().all(|(i, b)| {
            b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || (i > 0 && matches!(b, b'_' | b'.' | b':' | b'-'))
        })
}
fn opaque(v: &str, n: usize) -> bool {
    !v.is_empty()
        && v.len() <= n
        && v.is_ascii()
        && v.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'/' | b'-' | b'@')
        })
}
fn uuid(f: &'static str, v: &str) -> Result<(), InsulaError> {
    let u = Uuid::parse_str(v).map_err(|_| bad(f, "malformed_uuid"))?;
    if u.to_string() != v {
        return Err(bad(f, "noncanonical_uuid"));
    }
    Ok(())
}
fn binding(v: &TrustedBinding) -> Result<(), InsulaError> {
    if !is_house(&v.house_id) {
        return Err(bad("houseId", "invalid_house_key"));
    }
    if !is_room(&v.room) {
        return Err(bad("room", "invalid_room_key"));
    }
    if !opaque(&v.spirit, 64) {
        return Err(bad("spirit", "invalid_identity_atom"));
    }
    if !opaque(&v.session_id, 128) {
        return Err(bad("sessionId", "invalid_session_id"));
    }
    Ok(())
}
pub fn validate_trusted_binding(value: &TrustedBinding) -> Result<(), InsulaError> {
    binding(value)
}
fn expires(e: &ObservationEvent) -> Result<DateTime<Utc>, InsulaError> {
    e.observed_at
        .checked_add_signed(Duration::days(14))
        .ok_or_else(|| bad("observedAt", "expiry_overflow"))
}
fn hp(h: &mut Sha256, n: &str, v: &str) {
    h.update((n.len() as u64).to_be_bytes());
    h.update(n);
    h.update((v.len() as u64).to_be_bytes());
    h.update(v)
}
fn hs(domain: &str) -> Sha256 {
    let mut h = Sha256::new();
    hp(&mut h, "domain", domain);
    h
}
fn hf(h: Sha256) -> String {
    format!("{:x}", h.finalize())
}
fn ho(h: &mut Sha256, n: &str, v: Option<&str>) {
    hp(h, n, v.unwrap_or("<absent>"))
}
pub fn derive_idempotency_key_v1(
    b: &TrustedBinding,
    e: &ObservationEvent,
) -> Result<String, InsulaError> {
    binding(b)?;
    if e.idempotency_version != 1 {
        return Err(bad("idempotencyVersion", "unsupported_recipe"));
    }
    let mut h = hs("insula.idempotency.v1");
    hp(&mut h, "scope", e.idempotency_scope.as_str());
    hp(&mut h, "house", &b.house_id);
    match e.idempotency_scope {
        IdempotencyScope::WriterSequence => {
            hp(&mut h, "writer", &e.writer_id);
            hp(&mut h, "sequence", &e.writer_sequence.to_string())
        }
        IdempotencyScope::ToolCall => {
            hp(&mut h, "room", &b.room);
            hp(&mut h, "component", &e.component);
            hp(
                &mut h,
                "tool",
                e.tool_call_id
                    .as_deref()
                    .ok_or_else(|| bad("toolCallId", "required_by_scope"))?,
            )
        }
        IdempotencyScope::ProviderRequest => {
            hp(&mut h, "room", &b.room);
            hp(&mut h, "component", &e.component);
            hp(
                &mut h,
                "provider",
                e.provider_request_id
                    .as_deref()
                    .ok_or_else(|| bad("providerRequestId", "required_by_scope"))?,
            )
        }
        IdempotencyScope::TraceSpan => {
            hp(&mut h, "trace", &e.trace_id);
            hp(&mut h, "span", &e.span_id);
            hp(&mut h, "phase", e.phase.as_str())
        }
        IdempotencyScope::RoomOperation => {
            hp(&mut h, "room", &b.room);
            hp(&mut h, "component", &e.component);
            hp(&mut h, "layer", &e.layer);
            hp(&mut h, "operation", &e.operation);
            hp(&mut h, "phase", e.phase.as_str());
            hp(
                &mut h,
                "receiptKind",
                e.receipt_kind
                    .as_deref()
                    .ok_or_else(|| bad("receiptKind", "required_by_scope"))?,
            );
            hp(
                &mut h,
                "receiptId",
                e.receipt_id
                    .as_deref()
                    .ok_or_else(|| bad("receiptId", "required_by_scope"))?,
            )
        }
    }
    Ok(hf(h))
}
/// Semantic v1 hashes only the mechanical observation content. It includes the
/// permanent House/room/spirit dimensions, component/layer/operation,
/// phase/outcome/error, mechanical counts, optional correlation/receipt
/// pointers, drop count, and idempotency recipe version/scope. It deliberately
/// excludes event/span/trace/parent identity, writer identity and sequence,
/// session, observed/expiry/ingest timestamps, and the derived logical key:
/// those are transport envelope and may change during an identical failover
/// redelivery.
pub fn derive_semantic_hash_v1(
    b: &TrustedBinding,
    e: &ObservationEvent,
) -> Result<String, InsulaError> {
    binding(b)?;
    let mut h = hs("insula.semantic.v1");
    for (n, v) in [
        ("house", b.house_id.as_str()),
        ("room", b.room.as_str()),
        ("spirit", b.spirit.as_str()),
        ("component", e.component.as_str()),
        ("layer", e.layer.as_str()),
        ("operation", e.operation.as_str()),
        ("phase", e.phase.as_str()),
        ("outcome", e.outcome_class.as_str()),
        ("scope", e.idempotency_scope.as_str()),
    ] {
        hp(&mut h, n, v)
    }
    for (n, v) in [
        ("error", e.error_class.as_deref()),
        ("tool", e.tool_call_id.as_deref()),
        ("provider", e.provider_request_id.as_deref()),
        ("receiptKind", e.receipt_kind.as_deref()),
        ("receiptId", e.receipt_id.as_deref()),
    ] {
        ho(&mut h, n, v)
    }
    for (n, v) in [
        (
            "duration",
            e.duration_us
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<absent>".into()),
        ),
        ("bytesIn", e.bytes_in.to_string()),
        ("bytesOut", e.bytes_out.to_string()),
        ("tokensIn", e.tokens_in.to_string()),
        ("tokensOut", e.tokens_out.to_string()),
        ("version", e.idempotency_version.to_string()),
        ("drops", e.drop_count.to_string()),
    ] {
        hp(&mut h, n, &v)
    }
    Ok(hf(h))
}
fn event(e: &ObservationEvent) -> Result<DateTime<Utc>, InsulaError> {
    for (f, v) in [
        ("eventId", e.event_id.as_str()),
        ("spanId", e.span_id.as_str()),
        ("traceId", e.trace_id.as_str()),
        ("writerId", e.writer_id.as_str()),
    ] {
        uuid(f, v)?
    }
    if let Some(v) = e.parent_span_id.as_deref() {
        uuid("parentSpanId", v)?;
        if v == e.span_id {
            return Err(bad("parentSpanId", "self_parent"));
        }
    }
    if e.observed_at > Utc::now() + Duration::minutes(5) {
        return Err(bad("observedAt", "future_clock_skew"));
    }
    if e.writer_sequence <= 0 {
        return Err(bad("writerSequence", "must_be_positive"));
    }
    for (f, v) in [
        ("component", e.component.as_str()),
        ("layer", e.layer.as_str()),
        ("operation", e.operation.as_str()),
    ] {
        if !atom(v, 64) {
            return Err(bad(f, "invalid_mechanical_name"));
        }
    }
    if e.phase == ObservationPhase::Start && e.duration_us.is_some() {
        return Err(bad("durationUs", "start_has_no_duration"));
    }
    if e.duration_us
        .is_some_and(|v| !(0..=86_400_000_000).contains(&v))
    {
        return Err(bad("durationUs", "out_of_range"));
    }
    if e.outcome_class == OutcomeClass::Ok && e.error_class.is_some() {
        return Err(bad("errorClass", "ok_has_no_error"));
    }
    if e.error_class.as_deref().is_some_and(|v| !atom(v, 64)) {
        return Err(bad("errorClass", "invalid_mechanical_name"));
    }
    for (f, v) in [
        ("bytesIn", e.bytes_in),
        ("bytesOut", e.bytes_out),
        ("tokensIn", e.tokens_in),
        ("tokensOut", e.tokens_out),
    ] {
        if !(0..=1_099_511_627_776).contains(&v) {
            return Err(bad(f, "out_of_range"));
        }
    }
    for (f, v) in [
        ("toolCallId", e.tool_call_id.as_deref()),
        ("providerRequestId", e.provider_request_id.as_deref()),
        ("receiptId", e.receipt_id.as_deref()),
    ] {
        if v.is_some_and(|v| !opaque(v, 256)) {
            return Err(bad(f, "invalid_identifier"));
        }
    }
    if e.receipt_kind.as_deref().is_some_and(|v| !atom(v, 64)) {
        return Err(bad("receiptKind", "invalid_mechanical_name"));
    }
    if e.receipt_kind.is_some() != e.receipt_id.is_some() {
        return Err(bad("receiptKind", "receipt_pair_required"));
    }
    if !(0..=1_000_000_000).contains(&e.drop_count)
        || e.drop_count > 0 && e.phase != ObservationPhase::Drop
    {
        return Err(bad("dropCount", "invalid_drop_shape"));
    }
    expires(e)
}

async fn lock(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    h: &str,
    exclusive: bool,
) -> Result<(), InsulaError> {
    let q = if exclusive {
        "SELECT pg_advisory_xact_lock(hashtextextended($1,723684291))"
    } else {
        "SELECT pg_advisory_xact_lock_shared(hashtextextended($1,723684291))"
    };
    sqlx::query(q).bind(h).execute(&mut **tx).await?;
    Ok(())
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
async fn vitals(
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
pub async fn query_vitals(pool: &PgPool, q: &VitalsQuery) -> Result<VitalsResult, InsulaError> {
    if !is_house(&q.house_id)
        || q.end <= q.start
        || q.end - q.start > Duration::days(366)
        || q.limit == 0
        || q.limit > INSULA_MAX_VITALS_ROWS
    {
        return Err(bad("query", "out_of_range"));
    }
    let p = q.phase.map(ObservationPhase::as_str);
    let o = q.outcome_class.map(OutcomeClass::as_str);
    let rs=sqlx::query("SELECT minute,house_id,room,spirit,component,layer,operation,phase,outcome_class,event_count,duration_us_sum,duration_us_max,bytes_in_sum,bytes_out_sum,tokens_in_sum,tokens_out_sum,drop_count_sum,source_first_sequence,source_last_sequence,source_first_observed_at,source_last_observed_at,source_coverage_hash,updated_at FROM insula.vitals_minute WHERE query_name='insula.vitals.minute'AND query_version=1 AND house_id=$1 AND minute>=$2 AND minute<$3 AND($4::text IS NULL OR room=$4)AND($5::text IS NULL OR spirit=$5)AND($6::text IS NULL OR component=$6)AND($7::text IS NULL OR layer=$7)AND($8::text IS NULL OR operation=$8)AND($9::text IS NULL OR phase=$9)AND($10::text IS NULL OR outcome_class=$10)ORDER BY minute,room,spirit LIMIT $11").bind(&q.house_id).bind(q.start).bind(q.end).bind(&q.room).bind(&q.spirit).bind(&q.component).bind(&q.layer).bind(&q.operation).bind(p).bind(o).bind(i64::from(q.limit)).fetch_all(pool).await?;
    let rows = rs
        .into_iter()
        .map(|r| {
            Ok(VitalsRow {
                minute: r.try_get("minute")?,
                house_id: r.try_get("house_id")?,
                room: r.try_get("room")?,
                spirit: r.try_get("spirit")?,
                component: r.try_get("component")?,
                layer: r.try_get("layer")?,
                operation: r.try_get("operation")?,
                phase: r.try_get("phase")?,
                outcome_class: r.try_get("outcome_class")?,
                event_count: r.try_get("event_count")?,
                duration_us_sum: r.try_get("duration_us_sum")?,
                duration_us_max: r.try_get("duration_us_max")?,
                bytes_in_sum: r.try_get("bytes_in_sum")?,
                bytes_out_sum: r.try_get("bytes_out_sum")?,
                tokens_in_sum: r.try_get("tokens_in_sum")?,
                tokens_out_sum: r.try_get("tokens_out_sum")?,
                drop_count_sum: r.try_get("drop_count_sum")?,
                source_first_sequence: r.try_get("source_first_sequence")?,
                source_last_sequence: r.try_get("source_last_sequence")?,
                source_first_observed_at: r.try_get("source_first_observed_at")?,
                source_last_observed_at: r.try_get("source_last_observed_at")?,
                source_coverage_hash: r.try_get("source_coverage_hash")?,
                updated_at: r.try_get("updated_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(VitalsResult {
        query_name: VITALS.into(),
        query_version: 1,
        rows,
    })
}

fn sweep(h: &str, c: DateTime<Utc>, d: i16) -> String {
    let mut x = hs("insula.retention.sweep.v1");
    hp(&mut x, "house", h);
    hp(
        &mut x,
        "cutoff",
        &c.to_rfc3339_opts(SecondsFormat::Nanos, true),
    );
    hp(&mut x, "days", &d.to_string());
    hf(x)
}
fn rid(k: &str) -> String {
    let d = Sha256::digest(k);
    let mut b = [0; 16];
    b.copy_from_slice(&d[..16]);
    b[6] = (b[6] & 15) | 80;
    b[8] = (b[8] & 63) | 128;
    Uuid::from_bytes(b).to_string()
}
pub async fn run_retention(
    pool: &PgPool,
    house_id: &str,
    cutoff: DateTime<Utc>,
    days: i16,
) -> Result<RetentionReceipt, InsulaError> {
    if !is_house(house_id) || days != INSULA_DEFAULT_RETENTION_DAYS || cutoff > Utc::now() {
        return Err(bad("retention", "invalid_request"));
    }
    let cutoff = cutoff
        .with_second(0)
        .and_then(|value| value.with_nanosecond(0))
        .ok_or_else(|| bad("retention", "invalid_cutoff"))?;
    let key = sweep(house_id, cutoff, days);
    let id = rid(&key);
    let mut tx = pool.begin().await?;
    lock(&mut tx, house_id, true).await?;
    let select = "SELECT receipt_id::text receipt_id,receipt_kind,receipt_version,house_id,sweep_version,sweep_key,retention_days,swept_through,window_start,window_end,event_count,writer_count,duplicate_count_sum,drop_count_sum,coverage_version,coverage_hash,rollup_query_name,rollup_query_version,rollup_watermark FROM insula.retention_receipts WHERE house_id=$1 AND sweep_version=1 AND sweep_key=$2 FOR UPDATE";
    if let Some(r) = sqlx::query(select)
        .bind(house_id)
        .bind(&key)
        .fetch_optional(&mut *tx)
        .await?
    {
        let out = RetentionReceipt {
            receipt_id: Some(r.try_get("receipt_id")?),
            receipt_kind: r.try_get("receipt_kind")?,
            receipt_version: r.try_get("receipt_version")?,
            status: RetentionStatus::Replayed,
            house_id: r.try_get("house_id")?,
            sweep_version: r.try_get("sweep_version")?,
            sweep_key: r.try_get("sweep_key")?,
            retention_days: r.try_get("retention_days")?,
            swept_through: r.try_get("swept_through")?,
            window_start: Some(r.try_get("window_start")?),
            window_end: Some(r.try_get("window_end")?),
            event_count: r.try_get("event_count")?,
            writer_count: r.try_get("writer_count")?,
            duplicate_count_sum: r.try_get("duplicate_count_sum")?,
            drop_count_sum: r.try_get("drop_count_sum")?,
            coverage_version: Some(r.try_get("coverage_version")?),
            coverage_hash: Some(r.try_get("coverage_hash")?),
            rollup_query_name: r.try_get("rollup_query_name")?,
            rollup_query_version: r.try_get("rollup_query_version")?,
            rollup_watermark: Some(r.try_get("rollup_watermark")?),
        };
        tx.commit().await?;
        return Ok(out);
    }
    let a=sqlx::query("SELECT MIN(observed_at) ws,MAX(observed_at) we,COUNT(*)::bigint n,COUNT(DISTINCT writer_id)::bigint writers,COALESCE(SUM(duplicate_count),0)::bigint duplicates,COALESCE(SUM(drop_count),0)::bigint drops,MAX(ingested_at) watermark,encode(digest(convert_to(string_agg(event_id::text||':'||semantic_hash,E'\\n' ORDER BY event_id),'UTF8'),'sha256'),'hex') coverage FROM insula.log WHERE house_id=$1 AND expires_at<$2").bind(house_id).bind(cutoff).fetch_one(&mut *tx).await?;
    let n: i64 = a.try_get("n")?;
    if n == 0 {
        tx.commit().await?;
        return Ok(RetentionReceipt {
            receipt_id: None,
            receipt_kind: RETENTION.into(),
            receipt_version: 1,
            status: RetentionStatus::Noop,
            house_id: house_id.into(),
            sweep_version: 1,
            sweep_key: key,
            retention_days: days,
            swept_through: cutoff,
            window_start: None,
            window_end: None,
            event_count: 0,
            writer_count: 0,
            duplicate_count_sum: 0,
            drop_count_sum: 0,
            coverage_version: None,
            coverage_hash: None,
            rollup_query_name: VITALS.into(),
            rollup_query_version: 1,
            rollup_watermark: None,
        });
    }
    let ws: DateTime<Utc> = a.try_get("ws")?;
    let we: DateTime<Utc> = a.try_get("we")?;
    let writers: i64 = a.try_get("writers")?;
    let duplicates: i64 = a.try_get("duplicates")?;
    let drops: i64 = a.try_get("drops")?;
    let watermark: DateTime<Utc> = a.try_get("watermark")?;
    let coverage: String = a.try_get("coverage")?;
    // Exact observed_at + 14-day expiry, a minute-truncated cutoff, and a
    // strict `< cutoff` predicate make every selected source minute complete:
    // the boundary minute is wholly retained and every prior minute is wholly
    // eligible. Comparing the selected source groups to whole Vitals rows is
    // therefore exact rather than a subset comparison.
    let missing:i64=sqlx::query_scalar("WITH s AS(SELECT date_trunc('minute',observed_at AT TIME ZONE 'UTC')AT TIME ZONE 'UTC' minute,house_id,room,spirit,component,layer,operation,phase,outcome_class,COUNT(*)::bigint n,COALESCE(SUM(duration_us),0)::bigint duration_sum,MAX(duration_us) duration_max,SUM(bytes_in)::bigint bytes_in_sum,SUM(bytes_out)::bigint bytes_out_sum,SUM(tokens_in)::bigint tokens_in_sum,SUM(tokens_out)::bigint tokens_out_sum,SUM(drop_count)::bigint drops,MIN(writer_sequence) first_sequence,MAX(writer_sequence) last_sequence,MIN(observed_at) first_observed,MAX(observed_at) last_observed,encode(digest(convert_to(string_agg(event_id::text||':'||semantic_hash,E'\\n' ORDER BY event_id),'UTF8'),'sha256'),'hex') coverage FROM insula.log WHERE house_id=$1 AND expires_at<$2 GROUP BY 1,2,3,4,5,6,7,8,9)SELECT COUNT(*)::bigint FROM s LEFT JOIN insula.vitals_minute v ON v.query_name='insula.vitals.minute'AND v.query_version=1 AND v.minute=s.minute AND v.house_id=s.house_id AND v.room=s.room AND v.spirit=s.spirit AND v.component=s.component AND v.layer=s.layer AND v.operation=s.operation AND v.phase=s.phase AND v.outcome_class=s.outcome_class WHERE v.event_count IS NULL OR v.event_count<>s.n OR v.duration_us_sum<>s.duration_sum OR v.duration_us_max IS DISTINCT FROM s.duration_max OR v.bytes_in_sum<>s.bytes_in_sum OR v.bytes_out_sum<>s.bytes_out_sum OR v.tokens_in_sum<>s.tokens_in_sum OR v.tokens_out_sum<>s.tokens_out_sum OR v.drop_count_sum<>s.drops OR v.source_first_sequence<>s.first_sequence OR v.source_last_sequence<>s.last_sequence OR v.source_first_observed_at<>s.first_observed OR v.source_last_observed_at<>s.last_observed OR v.source_coverage_hash<>s.coverage").bind(house_id).bind(cutoff).fetch_one(&mut *tx).await?;
    if missing != 0 {
        return Err(InsulaError::Invariant(
            "retention refused: Vitals coverage incomplete",
        ));
    }
    sqlx::query("INSERT INTO insula.retention_receipts(receipt_id,receipt_kind,receipt_version,house_id,sweep_version,sweep_key,retention_days,swept_through,window_start,window_end,event_count,writer_count,duplicate_count_sum,drop_count_sum,coverage_version,coverage_hash,rollup_query_name,rollup_query_version,rollup_watermark)VALUES($1::uuid,'insula.retention.raw_delete',1,$2,1,$3,$4,$5,$6,$7,$8,$9,$10,$11,1,$12,'insula.vitals.minute',1,$13)").bind(&id).bind(house_id).bind(&key).bind(days).bind(cutoff).bind(ws).bind(we).bind(n).bind(writers).bind(duplicates).bind(drops).bind(&coverage).bind(watermark).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO insula.log_tombstones(tombstone_id,receipt_id,receipt_kind,house_id,writer_id,first_writer_sequence,last_writer_sequence,first_observed_at,last_observed_at,event_count,room_count,spirit_count,session_count,duplicate_count_sum,drop_count_sum,coverage_version,coverage_hash)SELECT gen_random_uuid(),$1::uuid,'insula.retention.raw_delete',house_id,writer_id,MIN(writer_sequence),MAX(writer_sequence),MIN(observed_at),MAX(observed_at),COUNT(*)::bigint,COUNT(DISTINCT room)::bigint,COUNT(DISTINCT spirit)::bigint,COUNT(DISTINCT session_id)::bigint,COALESCE(SUM(duplicate_count),0)::bigint,COALESCE(SUM(drop_count),0)::bigint,1,encode(digest(convert_to(string_agg(event_id::text||':'||semantic_hash,E'\\n' ORDER BY event_id),'UTF8'),'sha256'),'hex')FROM insula.log WHERE house_id=$2 AND expires_at<$3 GROUP BY house_id,writer_id").bind(&id).bind(house_id).bind(cutoff).execute(&mut *tx).await?;
    let proof:i64=sqlx::query_scalar("SELECT COALESCE(SUM(event_count),0)::bigint FROM insula.log_tombstones WHERE receipt_id=$1::uuid AND house_id=$2").bind(&id).bind(house_id).fetch_one(&mut *tx).await?;
    if proof != n {
        return Err(InsulaError::Invariant("tombstone coverage mismatch"));
    }
    let deleted = sqlx::query("DELETE FROM insula.log WHERE house_id=$1 AND expires_at<$2")
        .bind(house_id)
        .bind(cutoff)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted != n as u64 {
        return Err(InsulaError::Invariant("retention delete count changed"));
    }
    tx.commit().await?;
    Ok(RetentionReceipt {
        receipt_id: Some(id),
        receipt_kind: RETENTION.into(),
        receipt_version: 1,
        status: RetentionStatus::Deleted,
        house_id: house_id.into(),
        sweep_version: 1,
        sweep_key: key,
        retention_days: days,
        swept_through: cutoff,
        window_start: Some(ws),
        window_end: Some(we),
        event_count: n,
        writer_count: writers,
        duplicate_count_sum: duplicates,
        drop_count_sum: drops,
        coverage_version: Some(1),
        coverage_hash: Some(coverage),
        rollup_query_name: VITALS.into(),
        rollup_query_version: 1,
        rollup_watermark: Some(watermark),
    })
}

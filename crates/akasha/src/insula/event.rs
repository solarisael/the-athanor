use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use uuid::Uuid;

use super::{InsulaError, bad, hf, ho, hp, hs};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TrustedBinding {
    pub house_id: String,
    pub room: String,
    pub spirit: String,
    pub session_id: String,
}

pub(super) fn is_room(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && !v.starts_with('-')
        && !v.ends_with('-')
        && !v.contains("--")
        && v.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
pub(super) fn atom(v: &str, n: usize) -> bool {
    !v.is_empty()
        && v.len() <= n
        && v.bytes().enumerate().all(|(i, b)| {
            b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || (i > 0 && matches!(b, b'_' | b'.' | b':' | b'-'))
        })
}
pub(super) fn opaque(v: &str, n: usize) -> bool {
    !v.is_empty()
        && v.len() <= n
        && v.is_ascii()
        && v.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'/' | b'-' | b'@')
        })
}
pub(super) fn uuid(f: &'static str, v: &str) -> Result<(), InsulaError> {
    let u = Uuid::parse_str(v).map_err(|_| bad(f, "malformed_uuid"))?;
    if u.to_string() != v {
        return Err(bad(f, "noncanonical_uuid"));
    }
    Ok(())
}
pub fn validate_trusted_binding(v: &TrustedBinding) -> Result<(), InsulaError> {
    if !atom(&v.house_id, 64) {
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

fn expires(e: &ObservationEvent) -> Result<DateTime<Utc>, InsulaError> {
    e.observed_at
        .checked_add_signed(Duration::days(14))
        .ok_or_else(|| bad("observedAt", "expiry_overflow"))
}

pub(super) fn validate_event(e: &ObservationEvent) -> Result<DateTime<Utc>, InsulaError> {
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

pub fn derive_idempotency_key_v1(
    b: &TrustedBinding,
    e: &ObservationEvent,
) -> Result<String, InsulaError> {
    validate_trusted_binding(b)?;
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
    validate_trusted_binding(b)?;
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

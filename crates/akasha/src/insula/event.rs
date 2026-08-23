use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::binding::{atom, opaque, uuid};
use super::error::{InsulaError, bad};

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

pub(super) fn event(e: &ObservationEvent) -> Result<DateTime<Utc>, InsulaError> {
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

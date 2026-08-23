use super::binding::{TrustedBinding, binding};
use super::error::{InsulaError, bad};
use super::event::{IdempotencyScope, ObservationEvent};
use super::hash::{hf, ho, hp, hs};

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

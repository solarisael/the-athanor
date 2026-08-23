//! The Sea — the shared spine every shape obeys. Digests, idempotency,
//! and subject ownership.
//!
//! Skeleton only. Absorb targets from the 2026-08-23 census.
//! Standing law (coding#368): JetStream dedup ends at the stream
//! boundary; every business effect claims a domain idempotency key in
//! the same durable transaction, commits, then acknowledges.

/// Digest raw payload bytes for dead-letter and integrity rows.
/// Absorbs house-delivery/src/model.rs:240 `payload_sha256`.
pub fn payload_digest() {
    todo!("extraction: absorbs model.rs:240")
}

/// Derive a length-prefixed idempotency digest for message-family writes.
/// Absorbs house-substrate/src/hallway.rs:24 `digest`.
pub fn idempotency_digest() {
    todo!("extraction: absorbs hallway.rs:24")
}

/// Test exact and `>`-suffix NATS subject ownership.
/// Absorbs house-delivery/src/broker.rs:307 `subject_matches_filter`.
pub fn subject_owns() {
    todo!("extraction: absorbs broker.rs:307")
}

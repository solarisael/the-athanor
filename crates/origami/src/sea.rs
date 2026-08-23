//! The Sea — the shared spine every shape obeys. Digests, idempotency,
//! and subject ownership.
//!
//! Extraction in progress: a function that still carries `todo!` names
//! the absorb target it is waiting on, from the 2026-08-23 census.
//! Standing law (coding#368): JetStream dedup ends at the stream
//! boundary; every business effect claims a domain idempotency key in
//! the same durable transaction, commits, then acknowledges.

use sha2::{Digest, Sha256};

/// Digest raw payload bytes for dead-letter and integrity rows.
///
/// The one digest door for crane payloads and boat bodies: lowercase hex
/// SHA-256, matching the 64-character check every `*_sha256` column and
/// the crane envelope validator apply.
/// Absorbed from house-delivery/src/model.rs:240 `payload_sha256`.
pub fn payload_digest(payload: &[u8]) -> String {
    hex::encode(Sha256::digest(payload))
}

/// Derive a length-prefixed idempotency digest for message-family writes.
/// Absorbs house-substrate/src/hallway.rs:24 `digest`.
pub fn idempotency_digest() {
    todo!("extraction: absorbs hallway.rs:24")
}

/// Test exact and `>`-suffix NATS subject ownership.
///
/// A `.>` filter owns every subject strictly below its prefix; any other
/// filter owns exactly itself and nothing that merely starts with it.
/// Absorbed from house-delivery/src/broker.rs:307 `subject_matches_filter`.
pub fn subject_owns(subject: &str, filter: &str) -> bool {
    match filter.strip_suffix(".>") {
        Some(prefix) => subject.len() > prefix.len() + 1 && subject.starts_with(prefix),
        None => subject == filter,
    }
}

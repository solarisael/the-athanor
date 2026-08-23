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
///
/// Each part is hashed as its big-endian u64 byte length followed by its
/// bytes, so no rearrangement of the same characters across parts can
/// collide: `["ab","c"]` and `["a","bc"]` are different digests. Callers
/// pass the request fields that define sameness, in a fixed order; the
/// hex string is persisted and later compared byte-for-byte to decide
/// whether an idempotency key was reused with a different command.
///
/// Absorbs house-substrate/src/hallway.rs:24 `digest`.
pub fn idempotency_digest(parts: &[&str]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    format!("{:x}", hasher.finalize())
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

#[cfg(test)]
mod tests {
    use super::idempotency_digest;

    /// These hex strings are already written into `hallway_channels`,
    /// `hallway_presences`, `hallway_messages`, `hallway_knock_policies`,
    /// and `hallway_knocks` in the live House, and every duplicate-versus-
    /// reuse decision compares against them. The extraction from
    /// house-substrate must not have moved a single byte. Expected values
    /// were computed independently (python hashlib, big-endian u64 length
    /// prefix), not by running this function.
    #[test]
    fn digests_stay_byte_identical_to_the_rows_already_persisted() {
        assert_eq!(
            idempotency_digest(&["family-hallway", "kodo", "Kodo", "session-1"]),
            "0f9ad9a3caec4ebd10afe3c15dab4718663afd3ae9482fab17c2bec9a3c621fe"
        );
        assert_eq!(
            idempotency_digest(&[]),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// The length prefix is the whole point: without it, a body ending in
    /// one character and a spirit beginning with it would digest the same
    /// as the pair shifted by one, and an idempotency key reused with
    /// different content would read as a duplicate success.
    #[test]
    fn parts_cannot_be_rearranged_into_the_same_digest() {
        assert_eq!(
            idempotency_digest(&["ab", "c"]),
            "601d5476e2ccfe2c87a2bba7a322659734a05749d5b5aa781f513e4912db0d5f"
        );
        assert_eq!(
            idempotency_digest(&["a", "bc"]),
            "3fafa1cf2f19a7c1129beb20cf0983f73a489a221fc0dd2f16d1be292d089205"
        );
        assert_ne!(idempotency_digest(&["ab", "c"]), idempotency_digest(&["a", "bc"]));
    }
}

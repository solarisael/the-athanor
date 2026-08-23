//! The crane outbox: PostgreSQL-authoritative claim, publish, receipt,
//! and dead-letter ledger. The database is the authority side; the
//! broker is the delivery side.
//!
//! Skeleton only. Absorb targets from the 2026-08-23 census.

/// Lease the next due outbox row with a skip-locked claim.
/// Absorbs house-delivery/src/store.rs:114 `Store::claim_next`.
pub fn claim_next() {
    todo!("extraction: absorbs store.rs:114")
}

/// Record the acknowledged JetStream publish under the lease.
/// Absorbs house-delivery/src/store.rs:164 `Store::mark_published`.
pub fn mark_published() {
    todo!("extraction: absorbs store.rs:164")
}

/// Retry or exhaust a failed publish; exhausted rows dead-letter.
/// Absorbs house-delivery/src/store.rs:189 `Store::mark_publish_failure`.
pub fn mark_publish_failure() {
    todo!("extraction: absorbs store.rs:189")
}

/// Refuse a claimed row outright and record why.
/// Absorbs house-delivery/src/store.rs:248 `Store::dead_letter_claim`.
pub fn dead_letter_claim() {
    todo!("extraction: absorbs store.rs:248")
}

/// Write exactly one receipt row per consumed event; replays detected
/// under lock.
/// Absorbs house-delivery/src/store.rs:288 `Store::record_receipt`.
pub fn record_receipt() {
    todo!("extraction: absorbs store.rs:288")
}

/// Report schema version plus outbox, receipt, and dead-letter counts.
/// Absorbs house-delivery/src/store.rs:453 `Store::health`.
pub fn health() {
    todo!("extraction: absorbs store.rs:453")
}

//! The crane envelope: a strict deny-unknown-fields pointer both lanes
//! carry. Version, lane token, record id, room, digest, addressing,
//! expiry, lineage.
//!
//! Skeleton only. Absorb targets from the 2026-08-23 census.

/// Deserialize and validate a payload into an envelope.
/// Absorbs house-delivery/src/model.rs:123 `CraneEvent::parse`.
pub fn parse() {
    todo!("extraction: absorbs model.rs:123")
}

/// Validate version, lane token, record id, room, digest, addressing,
/// and expiry rules.
/// Absorbs house-delivery/src/model.rs:129 `CraneEvent::validate`.
pub fn validate() {
    todo!("extraction: absorbs model.rs:129")
}

/// Apply the deadline test before the ledger.
/// Absorbs house-delivery/src/model.rs:188 `CraneEvent::is_expired`.
pub fn is_expired() {
    todo!("extraction: absorbs model.rs:188")
}

/// Refuse body, title, or message-shaped keys crossing the broker.
/// Absorbs house-delivery/src/model.rs:218 `contains_private_key`.
pub fn refuse_private_keys() {
    todo!("extraction: absorbs model.rs:218")
}

/// Name the dead-letter reason for an unparseable or forbidden payload.
/// Absorbs house-delivery/src/model.rs:199 `classify_invalid_payload`.
pub fn classify_invalid() {
    todo!("extraction: absorbs model.rs:199")
}

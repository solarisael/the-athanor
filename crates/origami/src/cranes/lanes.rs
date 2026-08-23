//! Lanes: which current a crane flies. `boat.ready` predates addressing
//! and carries none; addressed lanes name a recipient kind and key.
//!
//! Skeleton only. Absorb targets from the 2026-08-23 census.

/// Map a lane to the NATS subject it owns.
/// Absorbs house-delivery/src/model.rs:68 `Lane::subject`.
pub fn subject_for() {
    todo!("extraction: absorbs model.rs:68")
}

/// Map a NATS subject back to its owning lane, or refuse.
/// Absorbs house-delivery/src/model.rs:78 `Lane::from_subject`.
pub fn lane_from_subject() {
    todo!("extraction: absorbs model.rs:78")
}

/// Format and parse the recipient kind token (worker/familiar/room/reviewer).
/// Absorbs house-delivery/src/model.rs:26 `RecipientKind::as_str` and :45 `from_str`.
pub fn recipient_token() {
    todo!("extraction: absorbs model.rs:26,45")
}

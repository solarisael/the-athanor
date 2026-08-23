//! Boat identity: deterministic, room-scoped, content-addressed.
//!
//! Skeleton only. Each function names its absorb target from the
//! 2026-08-23 census.

/// Derive the deterministic room+body identity path for a boat.
/// Absorbs house-substrate/src/paper_boat.rs:204 `paper_boat_source_path`
/// and the `b"paper-boat\0"` hash domain separator at :206.
pub fn source_identity() {
    todo!("extraction: absorbs paper_boat.rs:204")
}

/// Digest a boat body for pointer integrity checks.
/// Absorbs house-delivery/src/model.rs:244 `body_sha256`.
pub fn body_digest() {
    todo!("extraction: absorbs model.rs:244")
}

//! Hallway channels and presences: who may open which door.
//!
//! Skeleton only. Absorb targets from the 2026-08-23 census.

/// Create a hallway channel with its allowed-room list, idempotently.
/// Absorbs house-substrate/src/hallway.rs:111 `hallway_create`.
pub fn create() {
    todo!("extraction: absorbs hallway.rs:111")
}

/// Join this room, spirit, and session as an authenticated presence.
/// Absorbs house-substrate/src/hallway.rs:215 `hallway_join`.
pub fn join() {
    todo!("extraction: absorbs hallway.rs:215")
}

/// Gate on membership: create or lock the presence only when the room
/// holds a live grant. Truthful refusals only.
/// Absorbs house-substrate/src/hallway.rs:46 `ensure_presence`.
pub fn ensure_presence() {
    todo!("extraction: absorbs hallway.rs:46")
}

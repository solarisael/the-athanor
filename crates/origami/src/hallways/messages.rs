//! Hallway messages: sequenced posts, cursor reads, and the inbox.
//!
//! Skeleton only. Absorb targets from the 2026-08-23 census.

/// Append one message: allocate the sequence, resolve the thread,
/// mint targeted Bells.
/// Absorbs house-substrate/src/hallway.rs:322 `hallway_post`.
pub fn post() {
    todo!("extraction: absorbs hallway.rs:322")
}

/// Read in order; advance the cursor only on covering reads.
/// Absorbs house-substrate/src/hallway.rs:541 `hallway_read`.
pub fn read() {
    todo!("extraction: absorbs hallway.rs:541")
}

/// Derive unread counts and pending targeted Bells; clear nothing.
/// Absorbs house-substrate/src/hallway.rs:705 `hallway_inbox`.
pub fn inbox() {
    todo!("extraction: absorbs hallway.rs:705")
}

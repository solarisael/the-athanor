//! Knocks: bounded-turn wake requests between rooms. A Knock is a
//! request, never a command; the recipient's standing policy decides.
//!
//! Skeleton only. Absorb targets from the 2026-08-23 census.

/// Write a room's standing Knock policy, supersede-style.
/// Absorbs house-substrate/src/hallway.rs:815 `hallway_knock_policy`.
pub fn policy() {
    todo!("extraction: absorbs hallway.rs:815")
}

/// Request one bounded turn in an allowed peer room.
/// Absorbs house-substrate/src/hallway.rs:1028 `hallway_knock`.
pub fn knock() {
    todo!("extraction: absorbs hallway.rs:1028")
}

/// Claim a pending Knock into a bounded lease.
/// Absorbs house-substrate/src/hallway.rs:1308 `hallway_knock_claim`.
pub fn claim() {
    todo!("extraction: absorbs hallway.rs:1308")
}

/// Settle a started Knock with its outcome and reason.
/// Absorbs house-substrate/src/hallway.rs:1394 `hallway_knock_settle`.
pub fn settle() {
    todo!("extraction: absorbs hallway.rs:1394")
}

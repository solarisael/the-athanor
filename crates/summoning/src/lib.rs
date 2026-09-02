//! Domain vocabulary for the Summoning session cycle.
//!
//! Anamnesis, Presence, Paper Boat. Summoning owns the cycle; the `presence`
//! crate owns the Presence domain inside it, and this crate is the boundary
//! every consumer reaches it through. Host and protocol say
//! `summoning::presence::…`, so the cycle names its own middle even though the
//! branch weight of frame and turn assembly lives with its own owner.

pub mod anamnesis;

pub use anamnesis::*;
/// The boat is a fold of origami; Summoning is the cycle that sails it.
pub use origami::boats::paper_boat::*;

/// Presence, reached through the cycle that owns it.
pub use ::presence;

/// Close material becomes a paper boat body, so the two bounds are one bound.
///
/// Presence cannot depend on Summoning without a cycle, so it declares the
/// number and Summoning pins it here. Change either constant alone and the
/// build stops, which is the point: a letter that survives Presence must
/// survive the boat it becomes.
const _: () = assert!(
    presence::PRESENCE_MAX_CLOSE_BODY_BYTES == PAPER_BOAT_MAX_BODY_BYTES,
    "Presence close material becomes a paper boat body; the two bounds must agree"
);

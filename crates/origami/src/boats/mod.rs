//! Boats — the sleep shape. Stasis in the Sea, with a return point.
//!
//! A boat is a memory a spirit writes at sleep. The boat rests where it
//! was cast. The wake path returns the newest boat when the spirit is
//! called. A crane is different: a crane is movement with a destination
//! point. The two shapes touch at one seam: when a boat lands in
//! PostgreSQL, the outbox enqueues one `boat.ready` pointer event.
//!
//! These constants are the single declaration of the boat vocabulary.
//! Census 2026-08-23: 32 bare duplicates of this vocabulary remain
//! outside this crate (18 Rust, the rest SQL), including roughly ten
//! `type <> 'paper-boat'` exclusions in recall.rs and the sanctioned
//! 0016/0017 trigger literals. Quest A1 replaces them with the
//! memory-kind registry; the extraction quests replace the rest.

// enough: bare string constants; quest A1 moves this vocabulary into the
// memory_kinds registry with behavior flags, and consumers route on flags.

pub mod error;
pub mod identity;
pub mod record;
pub mod sleep;
pub mod wake;

/// The memory kind that marks a boat in `memories.type`.
pub const MEMORY_KIND: &str = "paper-boat";

/// The outbox event kind for the boat-ready pointer.
pub const EVENT_KIND: &str = "boat.ready";

/// The versioned crease pattern for the boat-ready payload contract.
pub const CREASE_PATTERN: &str = "boat.ready.v1";

/// The thread key every boat is filed under.
/// Absorbs house-substrate/src/paper_boat.rs:13 `PAPER_BOAT_THREAD`.
pub const THREAD_KEY: &str = "paper boat / sleep / for tomorrow";

/// The metadata origin marker a boat carries when the sleep path wrote it.
/// Absorbs house-substrate/src/paper_boat.rs:36.
pub const SLEEP_ORIGIN: &str = "paper-boat-sleep";

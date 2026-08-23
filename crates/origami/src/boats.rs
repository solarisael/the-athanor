//! Boats — the sleep shape. Stasis in the Sea, with a return point.
//!
//! A boat is a memory a spirit writes at sleep. The boat rests where it
//! was cast. The wake path returns the newest boat when the spirit is
//! called. This is stasis with a return point. A crane is different: a
//! crane is movement with a destination point.
//!
//! The two shapes touch at one seam. When a boat lands in PostgreSQL,
//! the outbox enqueues one `boat.ready` pointer event. The crane runtime
//! carries that pointer. The body never leaves PostgreSQL.
//!
//! These constants are the single declaration of the boat vocabulary.
//! The SQL trigger in migration 0016 repeats `paper-boat` and stays the
//! one sanctioned duplicate until quest A1 replaces it with the
//! memory-kind registry.

// enough: bare string constants; quest A1 moves this vocabulary into the
// memory_kinds registry with behavior flags, and consumers route on flags.

/// The memory kind that marks a boat in `memories.type`.
pub const MEMORY_KIND: &str = "paper-boat";

/// The outbox event kind for the boat-ready pointer.
pub const EVENT_KIND: &str = "boat.ready";

/// The versioned crease pattern for the boat-ready payload contract.
pub const CREASE_PATTERN: &str = "boat.ready.v1";

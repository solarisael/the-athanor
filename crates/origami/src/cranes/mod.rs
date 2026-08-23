//! Cranes — the message shape. Movement in the Sea, with a destination
//! point.
//!
//! A crane carries a pointer event from PostgreSQL through NATS to a
//! recipient: worker, familiar, room, or reviewer. The body never
//! leaves PostgreSQL; the crane carries identity and integrity only.
//!
//! Census 2026-08-23: the whole crane runtime lives in house-delivery.
//! Its vocabulary to absorb here: stream, subject, and consumer names
//! (broker.rs:13-19), envelope schema and crease bounds (model.rs:10-14),
//! outbox lease and retry bounds (store.rs:11-13). FaroEdges finding:
//! this module was declared by the family doc and stood empty — the
//! declared mouth is now growing teeth.

// enough: skeleton module; the extraction quests move vocabulary values
// here with re-exports from house-delivery, then the logic.

pub mod broker;
pub mod envelope;
pub mod lanes;
pub mod outbox;

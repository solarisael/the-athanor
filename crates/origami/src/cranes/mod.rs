//! Cranes — the message shape. Movement in the Sea, with a destination
//! point.
//!
//! A crane carries a pointer event from PostgreSQL through NATS to a
//! recipient: worker, familiar, room, or reviewer. The body never
//! leaves PostgreSQL; the crane carries identity and integrity only.
//!
//! The four concerns, each its own module with one door:
//! - [`lanes`]: which current a crane flies — recipient to subject.
//! - [`envelope`]: what a crane payload may say, and what it may never
//!   carry across the broker.
//! - [`outbox`]: the PostgreSQL authority side — claim, publish record,
//!   retry bounds, dead letters, and the commit-before-ack receipt
//!   ledger that coding#368 requires.
//! - [`broker`]: the NATS JetStream delivery side, and the single
//!   declaration of the crane wire vocabulary.
//!

// enough: the lane subject vocabulary sits in `broker` because it is wire
// naming, so `lanes` reads its constants from the delivery side. If a
// third reader appears, lift the subject vocabulary into its own module
// here and let both sides import it.

pub mod broker;
pub mod envelope;
pub mod lanes;
pub mod outbox;

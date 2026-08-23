//! The crane broker: the NATS JetStream delivery side. Streams,
//! durable consumers, publish with dedup headers, health.
//!
//! Skeleton only. Absorb targets from the 2026-08-23 census.

/// Open the NATS delivery endpoint.
/// Absorbs house-delivery/src/broker.rs:45 `Broker::connect`.
pub fn connect() {
    todo!("extraction: absorbs broker.rs:45")
}

/// Create or verify both lane streams, the receipt stream, and both
/// durable consumers; refuse drifted configurations.
/// Absorbs house-delivery/src/broker.rs:168 `Broker::configure`,
/// with the refusals at :314 and :341.
pub fn configure() {
    todo!("extraction: absorbs broker.rs:168,314,341")
}

/// Publish one event to its lane subject with a Nats-Msg-Id dedup header.
/// Absorbs house-delivery/src/broker.rs:220 `Broker::publish`.
pub fn publish() {
    todo!("extraction: absorbs broker.rs:220")
}

/// Publish the sanitized boat-receipt projection on the receipt subject.
/// Absorbs house-delivery/src/broker.rs:232 `Broker::publish_receipt`.
pub fn publish_receipt() {
    todo!("extraction: absorbs broker.rs:232")
}

/// Snapshot per-lane stream and consumer health.
/// Absorbs house-delivery/src/broker.rs:245 `Broker::health`.
pub fn health() {
    todo!("extraction: absorbs broker.rs:245")
}

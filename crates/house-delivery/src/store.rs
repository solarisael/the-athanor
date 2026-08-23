//! Re-export shim. The PostgreSQL authority side — claim, publish
//! record, retry bounds, dead letters, and the commit-before-ack receipt
//! ledger — now lives in [`origami::cranes::outbox`].

// enough: this shim exists only so existing callers keep compiling —
// tests/crane_delivery_integration.rs still imports
// `athanor_house_delivery::store::Store`. Way up: point that test at
// `origami::cranes::outbox` and delete this file.

pub use origami::cranes::outbox::*;

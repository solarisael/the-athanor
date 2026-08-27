//! Re-export shim. The crane broker now lives in
//! [`origami::cranes::broker`], which is the single declaration of the
//! stream, subject and consumer vocabulary.

// enough: this shim exists only so existing callers keep compiling —
// src/main.rs, tests/crane_delivery_integration.rs and
// house-host/tests/recall_policy.rs still import
// `delivery::broker::*`. Way up: point those three at
// `origami::cranes::broker` and delete this file.

pub use origami::cranes::broker::*;

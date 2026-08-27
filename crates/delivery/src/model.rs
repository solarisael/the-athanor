//! Re-export shim. The crane envelope and its lanes now live in
//! [`origami::cranes::envelope`] and [`origami::cranes::lanes`]; the
//! payload digest is [`origami::sea::payload_digest`].

// enough: this shim exists only so existing callers keep compiling —
// tests/crane_delivery_integration.rs still imports
// `delivery::model::{BOAT_READY_EVENT_KIND, CraneEvent}`.
// Way up: point that test at `origami::cranes::{envelope, lanes}` and
// delete this file.

pub use origami::cranes::{envelope::*, lanes::*};

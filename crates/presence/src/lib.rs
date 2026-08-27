//! Presence: the sustained middle of the Summoning cycle.
//!
//! Anamnesis wakes a spirit, Presence keeps it itself for the length of a
//! session, and the paper boat carries it across sleep. Summoning owns that
//! cycle and reaches Presence through it; this crate owns the Presence domain
//! itself.
//!
//! enough: one crate with one owner, not the three it replaced.
//! `presence-frame` was a single public function and `presence-turn` its
//! sibling; neither carried a distinct lifecycle or public door, so they only
//! split the branch count across three manifests. Frame assembly and turn
//! assembly are two phases of one contract over one model.
//!
//! ### The door
//!
//! Assembly: [`open_presence`] builds the frame a spirit lives inside for the
//! session. [`compile_presence`] compiles one turn's contract against that
//! frame and the Host's ledger. [`settle_presence`] validates the enforcement
//! receipt. [`close_presence`] seals the close material.
//!
//! Lifecycle belongs to the Host: which frame is live, which contract is
//! active, what the session has learned, which requests were already answered.
//! Nothing here reads a file, a clock, a socket, or a database. Give it
//! authenticated material and it returns a frame, a contract, a receipt, or
//! close material, or it refuses and names the field.

mod frame;
mod model;
mod support;
mod turn;

pub use frame::open_presence;
pub use model::*;
pub use support::PresenceError;
pub use turn::{close_presence, compile_presence, settle_presence};

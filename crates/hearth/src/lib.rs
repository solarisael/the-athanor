//! Pure domain rules shared by Athanor concerns.

/// Head of `substrate/migrations/`. The installer pins a release to exactly
/// this number; origami's outbox treats it as a floor on a live database.
pub const SUBSTRATE_SCHEMA_VERSION: u32 = 28;

pub mod authority;
pub mod canon;
pub mod cluster;
pub mod context;
pub mod conversation;
pub mod error;
pub mod giga;
pub mod hallway;
pub mod lesson_triggers;
pub mod lineage;
pub mod recall;
pub mod remember;
pub mod room;
pub mod routing;
pub mod triggers;

pub use authority::*;
pub use canon::*;
pub use cluster::*;
pub use error::*;
pub use giga::*;
pub use recall::*;
pub use remember::*;
pub use room::*;

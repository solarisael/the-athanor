mod model;
mod support;

pub use model::*;
pub use support::PresenceError;

#[doc(hidden)]
pub mod internal {
    pub use crate::support::*;
}

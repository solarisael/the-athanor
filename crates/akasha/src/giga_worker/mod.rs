mod bounds;
mod classify;
mod context;
mod enablement;
mod failure;
mod health;
mod identity;
mod ledger;
mod process;
mod promotion_sources;
mod resonance;
mod typesafe;
mod worker;

pub use enablement::GigaEnablement;
pub(crate) use health::giga_classifier_health;
#[allow(unused_imports)]
pub use identity::{GIGA_MODEL_TAG, GIGA_PROMPT_VERSION};
pub use process::giga_process;
pub(crate) use promotion_sources::verify_promotion_sources;
pub use typesafe::GigaClassifierConfig;
pub use worker::{GigaWorkerHandle, spawn_giga_worker, spawn_room_giga_worker};

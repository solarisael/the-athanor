mod bounds;
mod classify;
mod enablement;
mod failure;
mod health;
mod identity;
mod ledger;
mod ollama;
mod process;
mod promotion_sources;
mod prompts;
mod schema;
mod validation;
mod worker;

#[cfg(test)]
mod tests;

pub(crate) use enablement::giga_capability_state;
pub(crate) use health::giga_classifier_health;
pub(crate) use promotion_sources::verify_promotion_sources;
// The classifier identity and the two prompt texts stay reachable as
// `giga_worker::GIGA_*`; nothing inside this crate walks through that door.
#[allow(unused_imports)]
pub use identity::{GIGA_MODEL_MANIFEST_DIGEST, GIGA_MODEL_TAG, GIGA_PROMPT_VERSION};
pub use process::giga_process;
#[allow(unused_imports)]
pub use prompts::{GIGA_EXTRACTION_PROMPT, GIGA_GATE_PROMPT};
pub use worker::{GigaWorkerHandle, spawn_giga_worker};

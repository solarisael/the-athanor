use serde::{Deserialize, Serialize};
use std::env;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GigaEnablement {
    pub giga_enabled: bool,
    pub hippocampus_enabled: bool,
    pub replay_mode: bool,
}

impl GigaEnablement {
    pub fn from_env() -> Self {
        Self {
            giga_enabled: env::var("ATHANOR_GIGA_ENABLED").ok().as_deref() == Some("1"),
            hippocampus_enabled: env::var("ATHANOR_HIPPOCAMPUS_ENABLED").ok().as_deref()
                == Some("1"),
            replay_mode: env::var("ATHANOR_REPLAY_MODE").ok().as_deref() == Some("1"),
        }
    }

    pub fn capture_enabled(self) -> bool {
        self.giga_enabled
    }

    pub fn classifier_enabled(self) -> bool {
        self.giga_enabled && self.hippocampus_enabled && !self.replay_mode
    }
}

pub(super) fn claim_owner_enabled() -> bool {
    GigaEnablement::from_env().classifier_enabled()
        && env::var("ATHANOR_GIGA_CLAIM_OWNER").ok().as_deref() == Some("1")
}

use std::env;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GigaCapabilityState {
    pub capture_enabled: bool,
    pub classifier_enabled: bool,
}

pub(crate) fn giga_capability_state_from_flags(
    giga_enabled: Option<&str>,
    hippocampus_enabled: Option<&str>,
    replay_mode: Option<&str>,
) -> GigaCapabilityState {
    let capture_enabled = giga_enabled == Some("1");
    GigaCapabilityState {
        capture_enabled,
        classifier_enabled: capture_enabled
            && hippocampus_enabled == Some("1")
            && replay_mode != Some("1"),
    }
}

pub(crate) fn giga_capability_state() -> GigaCapabilityState {
    let giga_enabled = env::var("SOLARISAEL_GIGA_ENABLED").ok();
    let hippocampus_enabled = env::var("SOLARISAEL_HIPPOCAMPUS_ENABLED").ok();
    let replay_mode = env::var("SOLARISAEL_REPLAY_MODE").ok();
    giga_capability_state_from_flags(
        giga_enabled.as_deref(),
        hippocampus_enabled.as_deref(),
        replay_mode.as_deref(),
    )
}

pub(super) fn classifier_enabled() -> bool {
    giga_capability_state().classifier_enabled
}

pub(super) fn claim_owner_enabled() -> bool {
    classifier_enabled() && env::var("SOLARISAEL_GIGA_CLAIM_OWNER").ok().as_deref() == Some("1")
}

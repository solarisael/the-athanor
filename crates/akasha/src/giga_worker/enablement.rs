use std::env;

pub(super) fn classifier_enabled() -> bool {
    env::var("SOLARISAEL_GIGA_ENABLED").ok().as_deref() == Some("1")
        && env::var("SOLARISAEL_HIPPOCAMPUS_ENABLED").ok().as_deref() == Some("1")
        && env::var("SOLARISAEL_REPLAY_MODE").ok().as_deref() != Some("1")
}

pub(super) fn claim_owner_enabled() -> bool {
    classifier_enabled() && env::var("SOLARISAEL_GIGA_CLAIM_OWNER").ok().as_deref() == Some("1")
}

pub(crate) fn giga_classifier_enabled() -> bool {
    classifier_enabled()
}

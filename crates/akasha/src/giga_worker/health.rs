use protocol::{GigaClassifierHealthResult, RequiredNullable};
use reqwest::Url;
use std::env;
use super::failure::safe_error_class;
use super::identity::{GIGA_MODEL_MANIFEST_DIGEST, GIGA_MODEL_TAG, GIGA_PROMPT_VERSION};
use super::ollama::{GIGA_DEFAULT_OLLAMA_ENDPOINT, is_loopback};

pub(crate) fn giga_classifier_health(
    last_error_class: Option<String>,
    last_error_at: Option<String>,
    consecutive_failures: u64,
) -> GigaClassifierHealthResult {
    let raw = env::var("ATHANOR_HIPPOCAMPUS_OLLAMA_ENDPOINT")
        .unwrap_or_else(|_| GIGA_DEFAULT_OLLAMA_ENDPOINT.into());
    let endpoint_scope = Url::parse(&raw)
        .ok()
        .map(|endpoint| {
            if is_loopback(&endpoint) {
                "loopback"
            } else {
                "remote"
            }
        })
        .unwrap_or("invalid");
    GigaClassifierHealthResult {
        provider_type: "ollama".into(),
        model: GIGA_MODEL_TAG.into(),
        model_digest: GIGA_MODEL_MANIFEST_DIGEST.into(),
        prompt_version: GIGA_PROMPT_VERSION.into(),
        endpoint_scope: endpoint_scope.into(),
        // This is historical sticky context; consecutive_failures is the live signal.
        last_error_class: RequiredNullable(safe_error_class(last_error_class)),
        last_error_at: RequiredNullable(last_error_at),
        consecutive_failures,
    }
}

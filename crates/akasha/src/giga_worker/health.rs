use super::failure::safe_error_class;
use super::identity::{GIGA_MODEL_TAG, GIGA_PROMPT_VERSION};
use protocol::{GigaClassifierHealthResult, RequiredNullable};

pub(crate) fn giga_classifier_health(
    last_error_class: Option<String>,
    last_error_at: Option<String>,
    consecutive_failures: u64,
) -> GigaClassifierHealthResult {
    GigaClassifierHealthResult {
        provider_type: "typesafe".into(),
        model: GIGA_MODEL_TAG.into(),
        model_digest: RequiredNullable(None),
        prompt_version: GIGA_PROMPT_VERSION.into(),
        endpoint_scope: "remote".into(),
        last_error_class: RequiredNullable(safe_error_class(last_error_class)),
        last_error_at: RequiredNullable(last_error_at),
        consecutive_failures,
    }
}

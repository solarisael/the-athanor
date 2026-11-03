use super::ledger::resolve_candidate_sources;
use crate::{AppError, Config};
use hearth::{GigaEvent, GigaSourceRef};

pub(crate) async fn verify_promotion_sources(
    config: &Config,
    event: &GigaEvent,
    source_refs: &[GigaSourceRef],
) -> Result<(), AppError> {
    resolve_candidate_sources(config, event, source_refs)
        .await
        .map(|_| ())
        .map_err(|failure| AppError::Invalid(failure.class().into()))
}

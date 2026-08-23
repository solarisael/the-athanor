use crate::{AppError, Config};
use hearth::GigaEvent;
use super::ledger::resolve_sources_from_ledger;

pub(crate) async fn verify_promotion_sources(
    config: &Config,
    event: &GigaEvent,
) -> Result<(), AppError> {
    resolve_sources_from_ledger(config, event)
        .await
        .map(|_| ())
        .map_err(|failure| AppError::Invalid(failure.class().into()))
}

use super::classify::classify_event;
use super::identity::{
    GIGA_MODEL_MANIFEST_DIGEST, GIGA_MODEL_TAG, GIGA_PROMPT_VERSION, sha256_bytes, source_digest,
};
use super::ledger::resolve_sources_from_ledger;
use crate::{
    AppError, Config,
    giga::{database_now, event_from_store, giga_candidate_store_and_finish, giga_event_finish},
};
use chrono::{DateTime, Utc};
use hearth::{
    GIGA_MAX_EVENT_ATTEMPTS, GigaCandidate, GigaEvent, GigaEventClaimReceipt,
    GigaEventFinishOutcome, GigaEventFinishRequest,
};
use protocol::{GigaProcessResult, RequiredNullable};
use sqlx::{PgPool, Row};

const GIGA_RETRY_DELAY_SECONDS: u32 = 30;

async fn validate_claim(
    pool: &PgPool,
    claim: &GigaEventClaimReceipt,
) -> Result<(GigaEvent, u32), AppError> {
    let claimed_event = claim
        .event()
        .ok_or_else(|| AppError::Invalid("GIGA process requires a claimed event".into()))?;
    let attempt_count = claim
        .attempt_count()
        .ok_or_else(|| AppError::Invalid("GIGA process claim has no attempt".into()))?;
    let claimed_at = DateTime::parse_from_rfc3339(claim.claimed_at())
        .map_err(|_| AppError::Invalid("GIGA process claim time is invalid".into()))?
        .with_timezone(&Utc);
    let claimed_lease_expires_at = DateTime::parse_from_rfc3339(
        claim
            .lease_expires_at()
            .ok_or_else(|| AppError::Invalid("GIGA process claim has no lease expiry".into()))?,
    )
    .map_err(|_| AppError::Invalid("GIGA process lease expiry is invalid".into()))?
    .with_timezone(&Utc);
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "SELECT room,queue_state,locked_by,locked_at,lease_expires_at,attempt_count,replay_count
         FROM giga_events WHERE event_id=$1 FOR SHARE",
    )
    .bind(claimed_event.event_id())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Invalid("GIGA process event does not exist".into()))?;
    let room: String = row
        .try_get::<Option<String>, _>("room")?
        .ok_or_else(|| AppError::Invalid("GIGA process event has no room".into()))?;
    let queue_state: String = row.try_get("queue_state")?;
    let locked_by: Option<String> = row.try_get("locked_by")?;
    let locked_at: Option<DateTime<Utc>> = row.try_get("locked_at")?;
    let lease_expires_at: Option<DateTime<Utc>> = row.try_get("lease_expires_at")?;
    let stored_attempt_count: i32 = row.try_get("attempt_count")?;
    if room != claim.room().as_str()
        || claimed_event.room() != claim.room()
        || queue_state != "running"
        || locked_by.as_deref() != Some(claim.worker_id())
        || locked_at != Some(claimed_at)
        || lease_expires_at != Some(claimed_lease_expires_at)
        || stored_attempt_count != i32::try_from(attempt_count).unwrap_or(i32::MAX)
    {
        return Err(AppError::Invalid(
            "GIGA process claim is not the active event lease".into(),
        ));
    }
    let now = database_now(&mut tx).await?;
    if now >= claimed_lease_expires_at {
        return Err(AppError::Invalid("GIGA process lease has expired".into()));
    }
    let replay_count: i32 = row.try_get("replay_count")?;
    let active_attempts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM giga_event_attempts
         WHERE event_id=$1 AND replay_count=$2 AND attempt_count=$3
           AND worker_id=$4 AND claimed_at=$5 AND lease_expires_at=$6 AND finished_at IS NULL",
    )
    .bind(claimed_event.event_id())
    .bind(replay_count)
    .bind(stored_attempt_count)
    .bind(claim.worker_id())
    .bind(claimed_at)
    .bind(claimed_lease_expires_at)
    .fetch_one(&mut *tx)
    .await?;
    if active_attempts != 1 {
        return Err(AppError::Invalid(
            "GIGA process claim has no unique active attempt".into(),
        ));
    }
    let stored_event = event_from_store(&mut tx, claimed_event.event_id()).await?;
    if &stored_event != claimed_event {
        return Err(AppError::Invalid(
            "GIGA process claim event does not match durable event".into(),
        ));
    }
    tx.commit().await?;
    Ok((stored_event, attempt_count))
}

fn finish_request(
    event: &GigaEvent,
    worker_id: &str,
    outcome: GigaEventFinishOutcome,
    candidate_count: u32,
    error_class: Option<&str>,
) -> Result<GigaEventFinishRequest, AppError> {
    GigaEventFinishRequest::new(
        event.room().clone(),
        event.event_id().into(),
        worker_id.into(),
        outcome,
        candidate_count,
        error_class.map(str::to_owned),
        (outcome == GigaEventFinishOutcome::Retry).then_some(GIGA_RETRY_DELAY_SECONDS),
    )
    .map_err(|error| AppError::Invalid(error.to_string()))
}

fn process_result(
    event: &GigaEvent,
    attempt_count: u32,
    outcome: GigaEventFinishOutcome,
    candidate_count: u32,
    error_class: Option<&str>,
) -> GigaProcessResult {
    GigaProcessResult {
        event_id: event.event_id().into(),
        outcome: outcome.as_str().into(),
        candidate_count,
        attempt_count,
        error_class: RequiredNullable(error_class.map(str::to_owned)),
    }
}

async fn finish_attempt(
    pool: &PgPool,
    event: &GigaEvent,
    worker_id: &str,
    attempt_count: u32,
    outcome: GigaEventFinishOutcome,
    candidate_count: u32,
    error_class: Option<&str>,
) -> Result<GigaProcessResult, AppError> {
    let request = finish_request(event, worker_id, outcome, candidate_count, error_class)?;
    giga_event_finish(pool, request).await?;
    Ok(process_result(
        event,
        attempt_count,
        outcome,
        candidate_count,
        error_class,
    ))
}

async fn store_candidate_and_finish(
    pool: &PgPool,
    event: &GigaEvent,
    worker_id: &str,
    attempt_count: u32,
    candidate: GigaCandidate,
) -> Result<GigaProcessResult, AppError> {
    let request = finish_request(event, worker_id, GigaEventFinishOutcome::Succeeded, 1, None)?;
    giga_candidate_store_and_finish(pool, candidate, request).await?;
    Ok(process_result(
        event,
        attempt_count,
        GigaEventFinishOutcome::Succeeded,
        1,
        None,
    ))
}

pub async fn giga_process(
    pool: &PgPool,
    config: &Config,
    claim: &GigaEventClaimReceipt,
) -> Result<GigaProcessResult, AppError> {
    let (event, attempt_count) = validate_claim(pool, claim).await?;
    let source_hash = source_digest(&event);
    let event_hash = sha256_bytes(event.event_id().as_bytes());
    let result = resolve_sources_from_ledger(config, &event).await;
    let classified = match result {
        Ok(sources) => {
            let existing_candidates: i64 = sqlx::query_scalar(
                "SELECT COUNT(*)::bigint FROM giga_candidates WHERE event_id=$1",
            )
            .bind(event.event_id())
            .fetch_one(pool)
            .await?;
            match existing_candidates {
                0 => classify_event(&event, &sources).await,
                1 => {
                    tracing::info!(
                        operation = "giga_process",
                        event_hash = %event_hash,
                        source_hash = %source_hash,
                        source_count = event.source_refs().len(),
                        candidate_count = 1,
                        outcome = "succeeded",
                        recovery = "existing_candidate",
                        model = GIGA_MODEL_TAG,
                        model_digest = GIGA_MODEL_MANIFEST_DIGEST,
                        prompt_version = GIGA_PROMPT_VERSION,
                    );
                    return finish_attempt(
                        pool,
                        &event,
                        claim.worker_id(),
                        attempt_count,
                        GigaEventFinishOutcome::Succeeded,
                        1,
                        None,
                    )
                    .await;
                }
                _ => {
                    return Err(AppError::Invalid(
                        "GIGA event has more than one durable candidate".into(),
                    ));
                }
            }
        }
        Err(failure) => Err(failure),
    };
    match classified {
        Ok(None) => {
            let result = finish_attempt(
                pool,
                &event,
                claim.worker_id(),
                attempt_count,
                GigaEventFinishOutcome::Succeeded,
                0,
                None,
            )
            .await?;
            tracing::info!(
                operation = "giga_process",
                event_hash = %event_hash,
                source_hash = %source_hash,
                source_count = event.source_refs().len(),
                candidate_count = 0,
                outcome = "succeeded",
                model = GIGA_MODEL_TAG,
                model_digest = GIGA_MODEL_MANIFEST_DIGEST,
                prompt_version = GIGA_PROMPT_VERSION,
            );
            Ok(result)
        }
        Ok(Some(candidate)) => {
            let result = store_candidate_and_finish(
                pool,
                &event,
                claim.worker_id(),
                attempt_count,
                candidate,
            )
            .await?;
            tracing::info!(
                operation = "giga_process",
                event_hash = %event_hash,
                source_hash = %source_hash,
                source_count = event.source_refs().len(),
                candidate_count = 1,
                outcome = "succeeded",
                model = GIGA_MODEL_TAG,
                model_digest = GIGA_MODEL_MANIFEST_DIGEST,
                prompt_version = GIGA_PROMPT_VERSION,
            );
            Ok(result)
        }
        Err(failure) => {
            let retry = failure.retryable() && attempt_count < GIGA_MAX_EVENT_ATTEMPTS;
            let outcome = if retry {
                GigaEventFinishOutcome::Retry
            } else {
                GigaEventFinishOutcome::Failed
            };
            tracing::warn!(
                operation = "giga_process",
                event_hash = %event_hash,
                source_hash = %source_hash,
                source_count = event.source_refs().len(),
                candidate_count = 0,
                outcome = outcome.as_str(),
                error_class = failure.class(),
                model = GIGA_MODEL_TAG,
                model_digest = GIGA_MODEL_MANIFEST_DIGEST,
                prompt_version = GIGA_PROMPT_VERSION,
            );
            finish_attempt(
                pool,
                &event,
                claim.worker_id(),
                attempt_count,
                outcome,
                0,
                Some(failure.class()),
            )
            .await
        }
    }
}

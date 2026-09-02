use super::clock::database_now;
use super::error::domain_error;
use super::review::giga_review;
use super::sources::fresh_candidate_sources;
use crate::AppError;
use chrono::{DateTime, Utc};
use hearth::{GigaReviewAction, GigaReviewState};
use protocol::{GigaReviewResult, GigaToolReviewParams};
use sqlx::{PgPool, Row};

pub async fn giga_tool_review(
    pool: &PgPool,
    request: GigaToolReviewParams,
) -> Result<GigaReviewResult, AppError> {
    let mut tx = pool.begin().await?;
    let candidate = sqlx::query(
        "SELECT c.room,c.review_state
         FROM giga_candidates c JOIN giga_events e ON e.event_id=c.event_id
         WHERE c.candidate_id=$1 AND c.room=$2 AND e.room=$2
         FOR UPDATE OF c",
    )
    .bind(&request.candidate_id)
    .bind(&request.room)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Invalid("GIGA candidate does not exist in the trusted room".into()))?;
    let previous_state = GigaReviewState::parse(&candidate.try_get::<String, _>("review_state")?)
        .map_err(domain_error)?;
    let new_state = GigaReviewState::parse(&request.new_state).map_err(domain_error)?;
    if matches!(
        new_state,
        GigaReviewState::Promoted
            | GigaReviewState::Merged
            | GigaReviewState::Corrected
            | GigaReviewState::Superseded
    ) {
        return Err(AppError::Invalid(
            "GIGA review tool cannot commit an authority transition".into(),
        ));
    }
    let source_refs = fresh_candidate_sources(&mut tx, &request.candidate_id).await?;
    let reviewed_at: DateTime<Utc> = database_now(&mut tx).await?;
    tx.commit().await?;
    let review = GigaReviewAction::new(
        request.candidate_id,
        request.reviewer_id,
        previous_state,
        new_state,
        request.reason,
        request.authorization_basis,
        source_refs,
        None,
        None,
        Vec::new(),
        None,
        reviewed_at.to_rfc3339(),
    )
    .map_err(domain_error)?;
    giga_review(pool, review).await
}

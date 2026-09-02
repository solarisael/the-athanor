use super::clock::database_now;
use super::error::domain_error;
use super::event_store::event_from_store;
use super::promotion::giga_promote;
use super::sources::fresh_candidate_sources;
use crate::{AppError, config::Config, giga_worker::verify_promotion_sources};
use chrono::{DateTime, Utc};
use hearth::{
    GigaCandidateKind, GigaCodingLessonPromotionPayload, GigaMemoryPromotionPayload,
    GigaProjectLessonPromotionPayload, GigaPromotionPayload, GigaPromotionReceipt,
    GigaPromotionRequest, GigaPublicationConsent, GigaReviewState, RoomKey,
};
use protocol::{GigaToolPromoteParams, GigaToolPromotionTargetParams};
use sqlx::{PgPool, Row};

pub async fn giga_tool_promote(
    pool: &PgPool,
    cfg: &Config,
    request: GigaToolPromoteParams,
) -> Result<GigaPromotionReceipt, AppError> {
    let mut tx = pool.begin().await?;
    let candidate = sqlx::query(
        "SELECT c.*,e.room AS event_room
         FROM giga_candidates c JOIN giga_events e ON e.event_id=c.event_id
         WHERE c.candidate_id=$1 AND c.room=$2 AND e.room=$2
         FOR UPDATE OF c",
    )
    .bind(&request.candidate_id)
    .bind(&request.room)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Invalid("GIGA candidate does not exist in the trusted room".into()))?;
    let kind =
        GigaCandidateKind::parse(&candidate.try_get::<String, _>("kind")?).map_err(domain_error)?;
    let review_state = GigaReviewState::parse(&candidate.try_get::<String, _>("review_state")?)
        .map_err(domain_error)?;
    if review_state != GigaReviewState::InReview {
        return Err(AppError::Invalid(
            "GIGA promotion requires an in_review candidate".into(),
        ));
    }
    let project_keys: Vec<String> = candidate.try_get("project_keys")?;
    let thread_keys: Vec<String> = candidate.try_get("thread_keys")?;
    let source_refs = fresh_candidate_sources(&mut tx, &request.candidate_id).await?;
    let event_id: String = candidate.try_get("event_id")?;
    let event = event_from_store(&mut tx, &event_id).await?;
    verify_promotion_sources(cfg, &event).await?;
    let reviewed_at: DateTime<Utc> = database_now(&mut tx).await?;
    let payload = match request.target {
        GigaToolPromotionTargetParams::Memory {
            title,
            body,
            threads,
        } => {
            if kind != GigaCandidateKind::Memory {
                return Err(AppError::Invalid(
                    "memory promotion tool requires a memory candidate".into(),
                ));
            }
            GigaPromotionPayload::Memory(
                GigaMemoryPromotionPayload::new(title, body, threads).map_err(domain_error)?,
            )
        }
        GigaToolPromotionTargetParams::CodingLesson {
            title,
            body,
            shape,
            proof_pattern,
            trigger_context,
            language_keys,
            technology_keys,
            tags,
        } => {
            if kind != GigaCandidateKind::CodingLesson || !project_keys.is_empty() {
                return Err(AppError::Invalid(
                    "coding lesson promotion requires a global coding lesson candidate".into(),
                ));
            }
            GigaPromotionPayload::CodingLesson(
                GigaCodingLessonPromotionPayload::new(
                    title,
                    body,
                    shape,
                    proof_pattern.unwrap_or_default(),
                    trigger_context.unwrap_or_default(),
                    language_keys,
                    technology_keys,
                    thread_keys,
                    tags,
                )
                .map_err(domain_error)?,
            )
        }
        GigaToolPromotionTargetParams::ProjectLesson {
            title,
            body,
            proof_pattern,
            trigger_context,
            language_keys,
            technology_keys,
            tags,
            publication_approved,
        } => {
            if kind != GigaCandidateKind::ProjectLesson || project_keys.len() != 1 {
                return Err(AppError::Invalid(
                    "project lesson promotion requires one stored candidate project".into(),
                ));
            }
            GigaPromotionPayload::ProjectLesson {
                payload: GigaProjectLessonPromotionPayload::new(
                    title,
                    body,
                    project_keys[0].clone(),
                    proof_pattern.unwrap_or_default(),
                    trigger_context.unwrap_or_default(),
                    language_keys,
                    technology_keys,
                    thread_keys,
                    tags,
                )
                .map_err(domain_error)?,
                publication_consent: GigaPublicationConsent::new(publication_approved)
                    .map_err(domain_error)?,
            }
        }
    };
    tx.commit().await?;
    let promotion = GigaPromotionRequest::new(
        request.candidate_id,
        RoomKey::new(request.room).map_err(domain_error)?,
        request.reviewer_id,
        request.operator_identity,
        request.authorization_basis,
        source_refs,
        payload,
        reviewed_at.to_rfc3339(),
    )
    .map_err(domain_error)?;
    giga_promote(pool, cfg, promotion).await
}

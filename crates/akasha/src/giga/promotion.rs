use super::clock::{database_now, timestamp};
use super::error::domain_error;
use super::promotion_payload::{
    normalize_promotion_values, promotion_digest, promotion_sources_json, publication_consent_json,
};
use super::promotion_receipt::{idempotent_promotion_receipt, typed_promotion_receipt};
use super::sources::{candidate_scope, fresh_candidate_sources};
use crate::{
    AppError,
    config::Config,
    remember::{
        prepare_memory_write, write_coding_lesson_tx, write_memory_tx, write_project_lesson_tx,
    },
    settings::RoomSettings,
};
use chrono::{DateTime, Utc};
use hearth::{
    GigaCandidateKind, GigaPromotionAuthority, GigaPromotionKind, GigaPromotionPayload,
    GigaPromotionReceipt, GigaPromotionRequest, GigaReviewState, GigaVisibility,
    lesson_triggers::LessonTriggerSpec,
};
use serde_json::json;
use sqlx::{PgPool, Row, types::Json};

pub async fn giga_promote(
    pool: &PgPool,
    cfg: &Config,
    request: GigaPromotionRequest,
) -> Result<GigaPromotionReceipt, AppError> {
    let settings = RoomSettings::load(pool, request.room().as_str()).await?;
    let request_digest = promotion_digest(&request)?;
    let reviewed_at = timestamp(request.reviewed_at())?;
    let prepared_memory = if let GigaPromotionPayload::Memory(payload) = request.payload() {
        let candidate =
            sqlx::query("SELECT event_id,review_state FROM giga_candidates WHERE candidate_id=$1")
                .bind(request.candidate_id())
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| AppError::Invalid("GIGA candidate does not exist".into()))?;
        if candidate.try_get::<String, _>("review_state")? == GigaReviewState::Promoted.as_str() {
            None
        } else {
            let event_id: String = candidate.try_get("event_id")?;
            let source_path = format!(
                "giga/{}/{}/{}",
                request.room(),
                event_id,
                request.candidate_id()
            );
            let prepared = prepare_memory_write(
                cfg,
                &settings,
                &source_path,
                payload.body(),
                payload.threads(),
                reviewed_at.date_naive(),
            )
            .await?;
            Some((event_id, source_path, prepared))
        }
    } else {
        None
    };
    let mut tx = pool.begin().await?;
    let candidate = sqlx::query(
        "SELECT c.*,e.room AS event_room
         FROM giga_candidates c
         JOIN giga_events e ON e.event_id=c.event_id
         WHERE c.candidate_id=$1
         FOR UPDATE OF c",
    )
    .bind(request.candidate_id())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Invalid("GIGA candidate does not exist".into()))?;
    let candidate_room: String = candidate.try_get("room")?;
    let event_room: Option<String> = candidate.try_get("event_room")?;
    if candidate_room != request.room().to_string()
        || event_room.as_deref() != Some(candidate_room.as_str())
    {
        return Err(AppError::Invalid(
            "GIGA promotion crosses the candidate or event room boundary".into(),
        ));
    }
    let stored_state: String = candidate.try_get("review_state")?;
    if stored_state == GigaReviewState::Promoted.as_str() {
        let review = sqlx::query(
            "SELECT promotion_target,committed_at FROM giga_reviews
             WHERE candidate_id=$1 AND action='promote' AND promotion_request_digest=$2
               AND reviewer_principal=$3 AND operator_identity=$4
               AND authorization_basis=$5 AND reviewed_at=$6",
        )
        .bind(request.candidate_id())
        .bind(&request_digest)
        .bind(request.reviewer_id())
        .bind(request.operator_identity())
        .bind(request.authorization_basis())
        .bind(reviewed_at)
        .fetch_optional(&mut *tx)
        .await?;
        let review = review.ok_or_else(|| {
            AppError::Invalid(
                "GIGA candidate was promoted by a different authorized request".into(),
            )
        })?;
        let receipt = idempotent_promotion_receipt(&request, &review)?;
        let expected_ref = format!(
            "{}:{}",
            receipt.durable_kind().as_str(),
            receipt.durable_id()
        );
        let promotion_refs: Json<Vec<String>> = candidate.try_get("promotion_refs")?;
        if promotion_refs.0 != vec![expected_ref] {
            return Err(AppError::Invalid(
                "stored GIGA promotion references are inconsistent".into(),
            ));
        }
        let durable_id = i64::try_from(receipt.durable_id())
            .map_err(|_| AppError::Invalid("stored GIGA durable ID is invalid".into()))?;
        let durable_exists: bool = match receipt.durable_kind() {
            GigaPromotionKind::Memory => {
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM memories WHERE id=$1)")
                    .bind(durable_id)
                    .fetch_one(&mut *tx)
                    .await?
            }
            GigaPromotionKind::CodingLesson => {
                sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM lessons WHERE lesson_key='coding' AND id=$1)",
                )
                .bind(durable_id)
                .fetch_one(&mut *tx)
                .await?
            }
            GigaPromotionKind::ProjectLesson => {
                sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM lessons WHERE lesson_key='project' AND id=$1)",
                )
                .bind(durable_id)
                .fetch_one(&mut *tx)
                .await?
            }
        };
        if !durable_exists {
            return Err(AppError::Invalid(
                "stored GIGA durable promotion target no longer exists".into(),
            ));
        }
        tx.commit().await?;
        return Ok(receipt);
    }

    let kind =
        GigaCandidateKind::parse(&candidate.try_get::<String, _>("kind")?).map_err(domain_error)?;
    let review_state = GigaReviewState::parse(&stored_state).map_err(domain_error)?;
    let scope = candidate_scope(&candidate)?;
    let project_keys: Vec<String> = candidate.try_get("project_keys")?;
    let source_refs = fresh_candidate_sources(&mut tx, request.candidate_id()).await?;
    request
        .validate_candidate(
            request.candidate_id(),
            request.room(),
            kind,
            review_state,
            &source_refs,
            &project_keys,
            &scope,
        )
        .map_err(domain_error)?;
    if let GigaPromotionPayload::ProjectLesson { payload, .. } = request.payload() {
        let reviewer_principal: Option<String> = sqlx::query_scalar(
            "SELECT reviewer_principal
             FROM giga_reviews
             WHERE candidate_id=$1 AND action='start_review' AND new_state='in_review'
             ORDER BY id DESC
             LIMIT 1",
        )
        .bind(request.candidate_id())
        .fetch_optional(&mut *tx)
        .await?;
        if reviewer_principal.as_deref() != Some(request.reviewer_id())
            || scope.visibility() != GigaVisibility::Private
            || scope.room() != Some(request.room())
            || !scope.publication_review_required()
            || project_keys.len() != 1
            || project_keys[0] != payload.project()
        {
            return Err(AppError::Invalid(
                "project lesson publication authority or scope is invalid".into(),
            ));
        }
    }

    let event_id: String = candidate.try_get("event_id")?;
    let source_path = format!(
        "giga/{}/{}/{}",
        request.room(),
        event_id,
        request.candidate_id()
    );
    let source_provenance = promotion_sources_json(request.source_refs());
    let mut metadata = json!({
        "origin": "giga-promotion",
        "authority": GigaPromotionAuthority::Full.as_str(),
        "origin_room": request.room().to_string(),
        "candidate_id": request.candidate_id(),
        "event_id": event_id,
        "source_refs": source_provenance,
        "promotion_request_digest": request_digest,
        "reviewer_id": request.reviewer_id(),
        "operator_identity": request.operator_identity(),
        "authorization_basis": request.authorization_basis(),
        "publication_consent": publication_consent_json(request.payload()),
        "scope": {
            "room": scope.room().map(ToString::to_string),
            "project": scope.project(),
            "visibility": scope.visibility().as_str(),
            "publication_review_required": scope.publication_review_required(),
        },
        "reviewed_at": request.reviewed_at(),
        "classifier": {
            "model": candidate.try_get::<String, _>("classifier_model")?,
            "provider_type": candidate.try_get::<String, _>("classifier_provider_type")?,
            "model_version": candidate.try_get::<String, _>("classifier_model_version")?,
            "prompt_version": candidate.try_get::<String, _>("classifier_prompt_version")?,
            "configuration_digest": candidate.try_get::<String, _>("classifier_configuration_digest")?,
            "run_id": candidate.try_get::<String, _>("classifier_run_id")?,
        },
    });
    if matches!(request.payload(), GigaPromotionPayload::Memory(_)) {
        let durability: f64 = candidate.try_get("durability")?;
        let candidate_created_at: DateTime<Utc> = candidate.try_get("created_at")?;
        metadata["giga"] = json!({
            "durability": durability,
            "decay_anchor": "candidate_created_at",
            "decay_anchor_at": candidate_created_at.to_rfc3339(),
        });
    }
    let (durable_kind, durable_id) = match request.payload() {
        GigaPromotionPayload::Memory(payload) => {
            let (prepared_event_id, prepared_source_path, prepared) =
                prepared_memory.as_ref().ok_or_else(|| {
                    AppError::Invalid("GIGA memory promotion preparation is missing".into())
                })?;
            if prepared_event_id != &event_id || prepared_source_path != &source_path {
                return Err(AppError::Invalid(
                    "GIGA memory promotion source changed during preparation".into(),
                ));
            }
            let (memory_id, _) = write_memory_tx(
                &mut tx,
                &candidate_room,
                "memory",
                payload.title(),
                &source_path,
                payload.body(),
                &[],
                metadata.clone(),
                prepared,
            )
            .await?;
            (
                GigaPromotionKind::Memory,
                u64::try_from(memory_id)
                    .map_err(|_| AppError::Invalid("GIGA memory ID is invalid".into()))?,
            )
        }
        GigaPromotionPayload::CodingLesson(payload) => {
            let tags = normalize_promotion_values(payload.tags());
            let lesson_id = write_coding_lesson_tx(
                &mut tx,
                &candidate_room,
                None,
                None,
                payload.shape(),
                payload.title(),
                payload.body(),
                Some(payload.trigger_context()),
                Some(payload.proof_pattern()),
                payload.language_keys(),
                payload.technology_keys(),
                payload.thread_keys(),
                &tags,
                Some(source_path.as_str()),
                // A promoted candidate carries no triggers: Stage 1 never
                // proposes them, and a trigger is an explicit lesson edit.
                &LessonTriggerSpec::default(),
                metadata.clone(),
            )
            .await?;
            (
                GigaPromotionKind::CodingLesson,
                u64::try_from(lesson_id)
                    .map_err(|_| AppError::Invalid("GIGA coding lesson ID is invalid".into()))?,
            )
        }
        GigaPromotionPayload::ProjectLesson { payload, .. } => {
            let tags = normalize_promotion_values(payload.tags());
            let lesson_id = write_project_lesson_tx(
                &mut tx,
                payload.project(),
                payload.title(),
                payload.body(),
                Some(payload.trigger_context()),
                Some(payload.proof_pattern()),
                payload.language_keys(),
                payload.technology_keys(),
                payload.thread_keys(),
                &tags,
                Some(source_path.as_str()),
                &LessonTriggerSpec::default(),
                metadata.clone(),
            )
            .await?;
            (
                GigaPromotionKind::ProjectLesson,
                u64::try_from(lesson_id)
                    .map_err(|_| AppError::Invalid("GIGA project lesson ID is invalid".into()))?,
            )
        }
    };
    let committed_at = database_now(&mut tx).await?;
    let target_ref = format!("{}:{durable_id}", durable_kind.as_str());
    let promotion_target = json!({
        "kind": durable_kind.as_str(),
        "id": durable_id,
        "ref": target_ref,
    });
    sqlx::query(
        "INSERT INTO giga_reviews
         (candidate_id,action,reviewer_principal,operator_identity,authorization_basis,
          previous_state,new_state,reason,promotion_target,target_refs,reviewed_at,
          promotion_request_digest,publication_consent,committed_at)
         VALUES ($1,'promote',$2,$3,$4,'in_review','promoted',$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(request.candidate_id())
    .bind(request.reviewer_id())
    .bind(request.operator_identity())
    .bind(request.authorization_basis())
    .bind("authorized deliberate GIGA promotion")
    .bind(promotion_target)
    .bind(Json(vec![target_ref.clone()]))
    .bind(reviewed_at)
    .bind(&request_digest)
    .bind(publication_consent_json(request.payload()))
    .bind(committed_at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE giga_candidates
         SET review_state='promoted',promotion_refs=$2
         WHERE candidate_id=$1 AND review_state='in_review'",
    )
    .bind(request.candidate_id())
    .bind(Json(vec![target_ref]))
    .execute(&mut *tx)
    .await?;
    let receipt = typed_promotion_receipt(&request, durable_id, committed_at.to_rfc3339())?;
    tx.commit().await?;
    Ok(receipt)
}

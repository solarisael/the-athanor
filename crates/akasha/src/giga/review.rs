use super::clock::timestamp;
use super::promotion_payload::promotion_sources_json;
use super::sources::{source_ref_params, stored_source_matches, verify_event_source};
use crate::AppError;
use hearth::{GigaResonance, GigaReviewAction, GigaReviewState, GigaVisibility, RoomKey};
use protocol::{GigaClassifierParams, GigaResonanceParams, GigaReviewResult, RequiredNullable};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction, types::Json};
use std::collections::HashSet;

fn review_action(state: GigaReviewState) -> &'static str {
    match state {
        GigaReviewState::InReview => "start_review",
        GigaReviewState::Promoted => "promote",
        GigaReviewState::Merged => "merge",
        GigaReviewState::Corrected => "correct",
        GigaReviewState::Dismissed => "dismiss",
        GigaReviewState::Unresolved => "resolve",
        GigaReviewState::Curio => "curio",
        GigaReviewState::Expired => "expire",
        GigaReviewState::Superseded => "supersede",
        GigaReviewState::Unreviewed => "start_review",
    }
}

fn resonance_params(resonance: &GigaResonance) -> GigaResonanceParams {
    let classifier = resonance.classifier();
    GigaResonanceParams {
        event_id: resonance.event_id().into(),
        score: resonance.score(),
        classifier: GigaClassifierParams {
            model: classifier.model().into(),
            provider_type: classifier.provider_type().into(),
            model_version: classifier.model_version().into(),
            prompt_version: classifier.prompt_version().into(),
            configuration_digest: classifier.configuration_digest().into(),
            run_id: classifier.run_id().into(),
            completed_at: classifier.completed_at().into(),
        },
        source_refs: resonance
            .source_refs()
            .iter()
            .map(source_ref_params)
            .collect(),
    }
}

async fn verify_resonance(
    tx: &mut Transaction<'_, Postgres>,
    room: &str,
    resonance: &GigaResonance,
) -> Result<(), AppError> {
    let event = sqlx::query("SELECT room FROM giga_events WHERE event_id=$1")
        .bind(resonance.event_id())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| AppError::Invalid("GIGA resonance event does not exist".into()))?;
    if event.try_get::<Option<String>, _>("room")?.as_deref() != Some(room) {
        return Err(AppError::Invalid(
            "GIGA resonance event crosses the candidate room boundary".into(),
        ));
    }
    let mut identities = HashSet::with_capacity(resonance.source_refs().len());
    for source in resonance.source_refs() {
        if source.scope().visibility() == GigaVisibility::Private
            && source.scope().room().map(RoomKey::as_str) != Some(room)
        {
            return Err(AppError::Invalid(
                "GIGA resonance source crosses the candidate room boundary".into(),
            ));
        }
        if !identities.insert((
            source.source_type().as_str().to_owned(),
            source.source_id().to_owned(),
        )) {
            return Err(AppError::Invalid(
                "GIGA resonance contains duplicate source identities".into(),
            ));
        }
        verify_event_source(tx, resonance.event_id(), source).await?;
    }
    Ok(())
}

// The substrate review write set, named once and spent twice by the INSERT in
// giga_review. The list is explicit because id is BIGSERIAL and must keep its
// sequence default (0003_giga.sql:110) — a record `SELECT *` would write NULL
// over it — and because the four columns 0004 added stay NULL for a substrate
// review (0004_giga_runtime.sql:74-78).
const REVIEW_INSERT_COLUMNS: &str = "candidate_id,action,reviewer_principal,authorization_basis,previous_state,new_state,reason,promotion_target,merge_targets,target_refs,reviewed_at";

// Keys are giga_reviews column names; the table is the only schema
// (0003_giga.sql:109-131). jsonb_populate_record matches by NAME, so a wrong
// key dies as a NOT NULL or CHECK refusal instead of a silent positional swap.
fn review_row(review: &GigaReviewAction) -> Result<Value, AppError> {
    let merge_targets =
        if review.merge_target().is_some() || !review.merge_source_candidates().is_empty() {
            Some(json!([{
                "target": review.merge_target(),
                "sources": review.merge_source_candidates(),
            }]))
        } else {
            None
        };
    Ok(json!({
        "candidate_id": review.candidate_id(),
        "action": review_action(review.new_state()),
        "reviewer_principal": review.reviewer_id(),
        "authorization_basis": review.authorization_basis(),
        "previous_state": review.previous_state().as_str(),
        "new_state": review.new_state().as_str(),
        "reason": review.reason(),
        "promotion_target": review.promotion_target().map(|target| json!({"ref": target})),
        "merge_targets": merge_targets,
        "target_refs": review
            .source_refs()
            .iter()
            .map(|source| source.content_hash())
            .collect::<Vec<_>>(),
        "reviewed_at": timestamp(review.reviewed_at())?,
    }))
}

// Keys are giga_review_resonances column names; the table is the only schema
// (0005_giga_resonance.sql:29-48). All twelve columns are NOT NULL with no
// DEFAULT and the table has no generated columns, so `SELECT *` is safe: a
// missing key refuses loudly at the first write.
fn resonance_row(
    review_id: i64,
    candidate_id: &str,
    resonance: &GigaResonance,
) -> Result<Value, AppError> {
    let classifier = resonance.classifier();
    Ok(json!({
        "review_id": review_id,
        "candidate_id": candidate_id,
        "event_id": resonance.event_id(),
        "score": resonance.score(),
        "classifier_model": classifier.model(),
        "classifier_provider_type": classifier.provider_type(),
        "classifier_model_version": classifier.model_version(),
        "classifier_prompt_version": classifier.prompt_version(),
        "classifier_configuration_digest": classifier.configuration_digest(),
        "classifier_run_id": classifier.run_id(),
        "classifier_completed_at": timestamp(classifier.completed_at())?,
        "source_refs": promotion_sources_json(resonance.source_refs()),
    }))
}

pub async fn giga_review(
    pool: &PgPool,
    review: GigaReviewAction,
) -> Result<GigaReviewResult, AppError> {
    let mut tx = pool.begin().await?;
    let candidate = sqlx::query(
        "SELECT c.room,c.review_state,e.room AS event_room
         FROM giga_candidates c JOIN giga_events e ON e.event_id=c.event_id
         WHERE c.candidate_id=$1 FOR UPDATE OF c",
    )
    .bind(review.candidate_id())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Invalid("GIGA candidate does not exist".into()))?;
    let room: String = candidate.try_get("room")?;
    let stored_state: String = candidate.try_get("review_state")?;
    let event_room: Option<String> = candidate.try_get("event_room")?;
    if event_room.as_deref() != Some(room.as_str()) {
        return Err(AppError::Invalid(
            "GIGA candidate crosses its parent event room".into(),
        ));
    }
    if stored_state != review.previous_state().as_str()
        || !review.previous_state().can_transition(review.new_state())
    {
        return Err(AppError::Invalid(
            "GIGA candidate review state changed or transition is invalid".into(),
        ));
    }
    if matches!(
        review.new_state(),
        GigaReviewState::Promoted
            | GigaReviewState::Merged
            | GigaReviewState::Corrected
            | GigaReviewState::Superseded
    ) {
        return Err(AppError::Invalid(
            "GIGA substrate has no authority to commit this durable transition".into(),
        ));
    }
    let stored_sources = sqlx::query(
        "SELECT cs.source_type,cs.source_id,cs.source_role,cs.content_hash,cs.scope_room,
                cs.scope_project,cs.scope_visibility,cs.publication_review_required,
                cs.range_start,cs.range_end,es.created_at AS source_created_at
         FROM giga_candidate_sources cs
         JOIN giga_event_sources es
           ON es.event_id=cs.event_id
          AND es.source_type=cs.source_type
          AND es.source_id=cs.source_id
         WHERE cs.candidate_id=$1",
    )
    .bind(review.candidate_id())
    .fetch_all(&mut *tx)
    .await?;
    if stored_sources.len() != review.source_refs().len() {
        return Err(AppError::Invalid(
            "GIGA review must retain the candidate source set".into(),
        ));
    }
    let mut matched_keys = HashSet::with_capacity(review.source_refs().len());
    for source in review.source_refs() {
        if source.scope().visibility() == GigaVisibility::Private
            && source.scope().room().map(RoomKey::as_str) != Some(room.as_str())
        {
            return Err(AppError::Invalid(
                "GIGA review crosses the candidate room boundary".into(),
            ));
        }
        if !matched_keys.insert((
            source.source_type().as_str().to_owned(),
            source.source_id().to_owned(),
        )) {
            return Err(AppError::Invalid(
                "GIGA review contains duplicate source identities".into(),
            ));
        }
        let mut matched = false;
        for row in &stored_sources {
            if stored_source_matches(row, source)? {
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(AppError::Invalid(
                "GIGA review source identity or hash does not match".into(),
            ));
        }
    }
    if let Some(resonance) = review.resonance() {
        verify_resonance(&mut tx, &room, resonance).await?;
    }

    let sql = format!(
        "INSERT INTO giga_reviews ({REVIEW_INSERT_COLUMNS})
         SELECT {REVIEW_INSERT_COLUMNS} FROM jsonb_populate_record(NULL::giga_reviews,$1)
         RETURNING id"
    );
    let review_id: i64 = sqlx::query_scalar(&sql)
        .bind(review_row(&review)?)
        .fetch_one(&mut *tx)
        .await?;
    if let Some(resonance) = review.resonance() {
        sqlx::query(
            "INSERT INTO giga_review_resonances
             SELECT * FROM jsonb_populate_record(NULL::giga_review_resonances,$1)",
        )
        .bind(resonance_row(review_id, review.candidate_id(), resonance)?)
        .execute(&mut *tx)
        .await?;
    }
    let promotion_refs = review
        .promotion_target()
        .into_iter()
        .chain(review.merge_target())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let updated = sqlx::query(
        "UPDATE giga_candidates SET review_state=$2,promotion_refs=$3
         WHERE candidate_id=$1 AND review_state=$4",
    )
    .bind(review.candidate_id())
    .bind(review.new_state().as_str())
    .bind(Json(promotion_refs))
    .bind(review.previous_state().as_str())
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(AppError::Invalid(
            "GIGA candidate review state changed before commit".into(),
        ));
    }
    let resonance = RequiredNullable(review.resonance().map(resonance_params));
    tx.commit().await?;
    Ok(GigaReviewResult {
        candidate_id: review.candidate_id().into(),
        previous_state: review.previous_state().as_str().into(),
        new_state: review.new_state().as_str().into(),
        reviewed_at: review.reviewed_at().into(),
        resonance,
    })
}

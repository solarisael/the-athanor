use super::clock::timestamp;
use super::sources::{range_parts, scope_parts, verify_event_source};
use crate::AppError;
use hearth::GigaCandidate;
use protocol::{GigaCandidateStoreDisposition, GigaCandidateStoreResult};
use sqlx::{PgPool, Postgres, Row, Transaction, types::Json};

async fn verify_parent_event(
    tx: &mut Transaction<'_, Postgres>,
    candidate: &GigaCandidate,
) -> Result<(), AppError> {
    let row = sqlx::query("SELECT room, session_id FROM giga_events WHERE event_id=$1")
        .bind(candidate.event_id())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| AppError::Invalid("GIGA candidate parent event does not exist".into()))?;
    let room: Option<String> = row.try_get("room")?;
    let session: String = row.try_get("session_id")?;
    if room.as_deref() != Some(candidate.room().as_str()) || session != candidate.session_id() {
        return Err(AppError::Invalid(
            "GIGA candidate crosses its parent event boundary".into(),
        ));
    }
    Ok(())
}

pub(super) async fn giga_candidate_store_tx(
    tx: &mut Transaction<'_, Postgres>,
    candidate: &GigaCandidate,
) -> Result<GigaCandidateStoreResult, AppError> {
    if sqlx::query("SELECT 1 FROM giga_candidates WHERE candidate_id=$1")
        .bind(candidate.candidate_id())
        .fetch_optional(&mut **tx)
        .await?
        .is_some()
    {
        return Ok(GigaCandidateStoreResult {
            candidate_id: candidate.candidate_id().into(),
            disposition: GigaCandidateStoreDisposition::Duplicate,
        });
    }
    let scores = candidate.scores();
    let classifier = candidate.classifier();
    let scope = candidate.scope();
    let inserted = sqlx::query(
        "INSERT INTO giga_candidates
         (candidate_schema_version,candidate_id,event_id,room,session_id,kind,priority,novelty,
          durability,confidence,project_keys,thread_keys,entity_hints,retrieval_terms,proposed_title,
          gist,rationale,proof_refs,scope_room,scope_project,scope_visibility,
          publication_review_required,authority,review_state,classifier_model,
          classifier_provider_type,classifier_model_version,classifier_prompt_version,
          classifier_configuration_digest,classifier_run_id,classifier_completed_at,created_at,
          expires_at,promotion_refs)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,
                 $21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33,$34)
         ON CONFLICT (candidate_id) DO NOTHING",
    )
    .bind(i32::from(candidate.candidate_schema_version()))
    .bind(candidate.candidate_id()).bind(candidate.event_id()).bind(candidate.room().to_string())
    .bind(candidate.session_id()).bind(candidate.kind().as_str())
    .bind(scores.priority()).bind(scores.novelty()).bind(scores.durability()).bind(scores.confidence())
    .bind(candidate.project_keys().to_vec()).bind(candidate.thread_keys().to_vec())
    .bind(candidate.entity_hints().to_vec()).bind(candidate.retrieval_terms().to_vec())
    .bind(candidate.proposed_title()).bind(candidate.gist())
    .bind(candidate.rationale()).bind(Json(candidate.proof_refs().to_vec()))
    .bind(scope.room().map(ToString::to_string)).bind(scope.project()).bind(scope.visibility().as_str())
    .bind(scope.publication_review_required()).bind(candidate.authority().as_str().replace('-', "_"))
    .bind(candidate.review_state().as_str()).bind(classifier.model()).bind(classifier.provider_type())
    .bind(classifier.model_version()).bind(classifier.prompt_version())
    .bind(classifier.configuration_digest()).bind(classifier.run_id())
    .bind(timestamp(classifier.completed_at())?).bind(timestamp(candidate.created_at())?)
    .bind(candidate.expires_at().map(timestamp).transpose()?)
    .bind(Json(candidate.promotion_refs().to_vec()))
    .execute(&mut **tx).await?;
    if inserted.rows_affected() == 0 {
        return Ok(GigaCandidateStoreResult {
            candidate_id: candidate.candidate_id().into(),
            disposition: GigaCandidateStoreDisposition::Duplicate,
        });
    }
    verify_parent_event(tx, candidate).await?;
    for source in candidate.source_refs() {
        verify_event_source(tx, candidate.event_id(), source).await?;
    }
    for source in candidate.source_refs() {
        let (room, project, visibility, review_required) = scope_parts(source);
        let (range_start, range_end) = range_parts(source)?;
        sqlx::query(
            "INSERT INTO giga_candidate_sources
             (candidate_id,event_id,source_type,source_id,source_role,content_hash,scope_room,scope_project,
              scope_visibility,publication_review_required,range_start,range_end,is_proof)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
        )
        .bind(candidate.candidate_id()).bind(candidate.event_id()).bind(source.source_type().as_str())
        .bind(source.source_id()).bind(source.role()).bind(source.content_hash()).bind(room).bind(project).bind(visibility)
        .bind(review_required).bind(range_start).bind(range_end)
        .bind(candidate.proof_refs().iter().any(|proof| proof == source.source_id()))
        .execute(&mut **tx).await?;
    }
    Ok(GigaCandidateStoreResult {
        candidate_id: candidate.candidate_id().into(),
        disposition: GigaCandidateStoreDisposition::Stored,
    })
}

pub async fn giga_candidate_store(
    pool: &PgPool,
    candidate: GigaCandidate,
) -> Result<GigaCandidateStoreResult, AppError> {
    let mut tx = pool.begin().await?;
    let result = giga_candidate_store_tx(&mut tx, &candidate).await?;
    tx.commit().await?;
    Ok(result)
}

use super::sources::source_params;
use crate::AppError;
use chrono::{DateTime, Utc};
use hearth::GigaAuthority;
use protocol::{
    GigaCandidateListRequest, GigaCandidateListResult, GigaCandidateParams, GigaClassifierParams,
    GigaScopeParams, GigaSourceRefParams, RequiredNullable,
};
use sqlx::{PgPool, Row, types::Json};

async fn candidate_sources(
    pool: &PgPool,
    candidate_id: &str,
) -> Result<Vec<GigaSourceRefParams>, AppError> {
    let rows = sqlx::query(
        "SELECT cs.source_type,cs.source_id,cs.source_role,cs.content_hash,cs.scope_room,cs.scope_project,
                cs.scope_visibility,cs.publication_review_required,cs.range_start,cs.range_end,
                es.created_at AS source_created_at
         FROM giga_candidate_sources cs JOIN giga_event_sources es
           ON es.event_id=cs.event_id AND es.source_type=cs.source_type AND es.source_id=cs.source_id
         WHERE cs.candidate_id=$1 ORDER BY cs.source_type,cs.source_id",
    ).bind(candidate_id).fetch_all(pool).await?;
    rows.iter().map(source_params).collect()
}

pub async fn giga_candidate_list(
    pool: &PgPool,
    request: GigaCandidateListRequest,
) -> Result<GigaCandidateListResult, AppError> {
    let state = request.review_state().map(|value| value.as_str());
    let rows = sqlx::query(
        "SELECT * FROM giga_candidates
         WHERE room=$1 AND ($2::text IS NULL OR review_state=$2)
         ORDER BY created_at DESC,candidate_id LIMIT $3",
    )
    .bind(request.room().to_string())
    .bind(state)
    .bind(i64::from(request.limit().min(200)))
    .fetch_all(pool)
    .await?;
    let mut candidates = Vec::with_capacity(rows.len());
    for row in rows {
        let candidate_id: String = row.try_get("candidate_id")?;
        let completed_at: DateTime<Utc> = row.try_get("classifier_completed_at")?;
        let created_at: DateTime<Utc> = row.try_get("created_at")?;
        let expires_at: Option<DateTime<Utc>> = row.try_get("expires_at")?;
        candidates.push(GigaCandidateParams {
            candidate_schema_version: row.try_get::<i32, _>("candidate_schema_version")? as u8,
            candidate_id: candidate_id.clone(),
            event_id: row.try_get("event_id")?,
            room: row.try_get("room")?,
            session_id: row.try_get("session_id")?,
            kind: row.try_get("kind")?,
            source_refs: candidate_sources(pool, &candidate_id).await?,
            priority: row.try_get("priority")?,
            novelty: row.try_get("novelty")?,
            durability: row.try_get("durability")?,
            confidence: row.try_get("confidence")?,
            project_keys: row.try_get("project_keys")?,
            thread_keys: row.try_get("thread_keys")?,
            entity_hints: row.try_get("entity_hints")?,
            retrieval_terms: row.try_get("retrieval_terms")?,
            proposed_title: row.try_get("proposed_title")?,
            gist: row.try_get("gist")?,
            rationale: row.try_get("rationale")?,
            proof_refs: row.try_get::<Json<Vec<String>>, _>("proof_refs")?.0,
            scope: GigaScopeParams {
                room: RequiredNullable(row.try_get("scope_room")?),
                project: RequiredNullable(row.try_get("scope_project")?),
                visibility: row.try_get("scope_visibility")?,
                publication_review_required: row.try_get("publication_review_required")?,
            },
            authority: {
                let stored = row.try_get::<String, _>("authority")?.replace('_', "-");
                GigaAuthority::parse(&stored)
                    .map_err(|_| {
                        AppError::Invalid(format!("GIGA candidate authority is invalid: {stored}"))
                    })?
                    .as_str()
                    .to_string()
            },
            review_state: row.try_get("review_state")?,
            classifier: GigaClassifierParams {
                model: row.try_get("classifier_model")?,
                provider_type: row.try_get("classifier_provider_type")?,
                model_version: row.try_get("classifier_model_version")?,
                prompt_version: row.try_get("classifier_prompt_version")?,
                configuration_digest: row.try_get("classifier_configuration_digest")?,
                run_id: row.try_get("classifier_run_id")?,
                completed_at: completed_at.to_rfc3339(),
            },
            created_at: created_at.to_rfc3339(),
            expires_at: RequiredNullable(expires_at.map(|value| value.to_rfc3339())),
            promotion_refs: row.try_get::<Json<Vec<String>>, _>("promotion_refs")?.0,
        });
    }
    Ok(GigaCandidateListResult { candidates })
}

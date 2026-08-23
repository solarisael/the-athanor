use crate::{
    AppError,
    config::Config,
    giga_worker::{giga_classifier_enabled, giga_classifier_health, verify_promotion_sources},
    remember::{
        prepare_memory_write, write_coding_lesson_tx, write_memory_tx, write_project_lesson_tx,
    },
};
use chrono::{DateTime, Duration, Utc};
use hearth::{
    GIGA_MAX_EVENT_ATTEMPTS, GigaAuthority, GigaCandidate, GigaCandidateKind,
    GigaCodingLessonPromotionPayload, GigaEvent, GigaEventClaimReceipt, GigaEventClaimRequest,
    GigaEventFinishOutcome, GigaEventFinishReceipt, GigaEventFinishRequest, GigaEventReplayReceipt,
    GigaEventReplayRequest, GigaEventType, GigaLifecycle, GigaMemoryPromotionPayload,
    GigaProjectLessonPromotionPayload, GigaPromotionAuthority, GigaPromotionKind,
    GigaPromotionPayload, GigaPromotionReceipt, GigaPromotionRequest, GigaPublicationConsent,
    GigaQueueMaintenanceOperation, GigaQueueMaintenanceRequest, GigaQueueMaintenanceScope,
    GigaQueueState, GigaResonance, GigaReviewAction, GigaReviewState, GigaRisk, GigaScope,
    GigaSourceRange, GigaSourceRef, GigaSourceType, GigaVisibility, RoomKey,
    lesson_triggers::LessonTriggerSpec,
};
use protocol::{
    GigaCandidateListRequest, GigaCandidateListResult, GigaCandidateParams,
    GigaCandidateStoreDisposition, GigaCandidateStoreResult, GigaClassifierParams,
    GigaConversationIngestParams, GigaEventIngestDisposition, GigaEventIngestResult,
    GigaHealthCount, GigaHealthRequest, GigaHealthResult, GigaQueueMaintenanceResult,
    GigaQueueStateCount, GigaResonanceParams, GigaReviewResult, GigaScopeParams,
    GigaSourceRangeParams, GigaSourceRefParams, GigaToolPromoteParams,
    GigaToolPromotionTargetParams, GigaToolReviewParams, RequiredNullable,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow, types::Json};
use std::collections::{BTreeSet, HashSet};

fn timestamp(value: &str) -> Result<DateTime<Utc>, AppError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| AppError::Invalid("GIGA timestamp is invalid".into()))
}

pub(crate) async fn database_now(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<DateTime<Utc>, AppError> {
    sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
}

fn scope_parts(source: &GigaSourceRef) -> (Option<String>, Option<String>, &'static str, bool) {
    (
        source.scope().room().map(ToString::to_string),
        source.scope().project().map(str::to_owned),
        source.scope().visibility().as_str(),
        source.scope().publication_review_required(),
    )
}
fn range_parts(source: &GigaSourceRef) -> Result<(Option<i64>, Option<i64>), AppError> {
    source.range().map_or(Ok((None, None)), |range| {
        let start = i64::try_from(range.start())
            .map_err(|_| AppError::Invalid("GIGA source range exceeds database bounds".into()))?;
        let end = i64::try_from(range.end())
            .map_err(|_| AppError::Invalid("GIGA source range exceeds database bounds".into()))?;
        Ok((Some(start), Some(end)))
    })
}

async fn insert_event_source(
    tx: &mut Transaction<'_, Postgres>,
    event_id: &str,
    source_ordinal: usize,
    source: &GigaSourceRef,
) -> Result<(), AppError> {
    let (room, project, visibility, review_required) = scope_parts(source);
    let (range_start, range_end) = range_parts(source)?;
    sqlx::query(
        "INSERT INTO giga_event_sources
         (event_id, source_ordinal, source_type, source_id, source_role, content_hash,
          scope_room, scope_project, scope_visibility, publication_review_required,
          range_start, range_end, created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
    )
    .bind(event_id)
    .bind(
        i32::try_from(source_ordinal)
            .map_err(|_| AppError::Invalid("GIGA source ordinal exceeds database bounds".into()))?,
    )
    .bind(source.source_type().as_str())
    .bind(source.source_id())
    .bind(source.role())
    .bind(source.content_hash())
    .bind(room)
    .bind(project)
    .bind(visibility)
    .bind(review_required)
    .bind(range_start)
    .bind(range_end)
    .bind(timestamp(source.timestamp())?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn lifecycle_json(event: &GigaEvent) -> Value {
    let lifecycle = event.lifecycle();
    let mut value = serde_json::Map::new();
    for field in [
        "task_reference",
        "worker_id",
        "worker_role",
        "phase",
        "project_key",
        "task_kind",
        "target",
        "change",
        "outcome",
        "verification_result",
        "subagent_reference",
        "parent_task",
        "role",
        "todo_reference",
        "previous_state",
        "new_state",
        "tool_name",
        "status",
        "sanitized_outcome",
        "reason",
        "operator_identity",
    ] {
        if let Some(field_value) = lifecycle.field(field) {
            value.insert(field.into(), json!(field_value));
        }
    }
    if !lifecycle.proof_contract().is_empty() {
        let field = if event.event_type() == GigaEventType::SubagentDispatched {
            "acceptance"
        } else {
            "proof_contract"
        };
        value.insert(field.into(), json!(lifecycle.proof_contract()));
    }
    if let Some(range) = lifecycle.source_range() {
        value.insert(
            "source_range".into(),
            json!({"start": range.start(), "end": range.end()}),
        );
    }
    if let Some(risk) = lifecycle.risk() {
        value.insert("risk".into(), json!(risk.as_str()));
    }
    Value::Object(value)
}

pub async fn giga_event_ingest(
    pool: &PgPool,
    event: GigaEvent,
) -> Result<GigaEventIngestResult, AppError> {
    let mut tx = pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO giga_events
         (event_schema_version,event_id,event_type,room,session_id,project_keys,lifecycle,created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(i32::from(event.event_schema_version()))
    .bind(event.event_id())
    .bind(event.event_type().as_str())
    .bind(event.room().to_string())
    .bind(event.session_id())
    .bind(event.project_keys().to_vec())
    .bind(Json(lifecycle_json(&event)))
    .bind(timestamp(event.created_at())?)
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() == 0 {
        tx.commit().await?;
        return Ok(GigaEventIngestResult {
            event_id: event.event_id().into(),
            disposition: GigaEventIngestDisposition::Duplicate,
        });
    }
    for (source_ordinal, source) in event.source_refs().iter().enumerate() {
        insert_event_source(&mut tx, event.event_id(), source_ordinal, source).await?;
    }
    tx.commit().await?;
    Ok(GigaEventIngestResult {
        event_id: event.event_id().into(),
        disposition: GigaEventIngestDisposition::Accepted,
    })
}
pub async fn giga_conversation_ingest(
    pool: &PgPool,
    request: GigaConversationIngestParams,
) -> Result<GigaEventIngestResult, AppError> {
    if request.turns.is_empty() || request.turns.len() > hearth::GIGA_MAX_PROCESS_SOURCES {
        return Err(AppError::Invalid(
            "GIGA conversation window must contain between one and eight turns".into(),
        ));
    }
    if request.project_keys.len() > 1 {
        return Err(AppError::Invalid(
            "GIGA conversation window accepts at most one project key".into(),
        ));
    }
    let room = RoomKey::new(request.room).map_err(domain_error)?;
    let session_id = request.turns[0].session_id.clone();
    let project = request.project_keys.first().cloned();
    let mut seen = HashSet::with_capacity(request.turns.len());
    let mut source_refs = Vec::with_capacity(request.turns.len());
    for turn in &request.turns {
        if !turn.has_stable_id
            || turn.session_id != session_id
            || !seen.insert(turn.source_id.as_str())
            || !matches!(turn.role.as_str(), "user" | "assistant")
        {
            return Err(AppError::Invalid(
                "GIGA conversation turns require stable unique identities, one session, and user/assistant roles".into(),
            ));
        }
        let scope = GigaScope::new(
            Some(room.to_string()),
            project.clone(),
            GigaVisibility::Private,
            true,
        )
        .map_err(domain_error)?;
        source_refs.push(
            GigaSourceRef::new(
                GigaSourceType::Turn,
                turn.source_id.clone(),
                turn.role.clone(),
                turn.timestamp.clone(),
                turn.content_hash.to_ascii_lowercase(),
                scope,
                None,
            )
            .map_err(domain_error)?,
        );
    }
    let identity = serde_json::to_string(&(
        1_u8,
        room.as_str(),
        &session_id,
        request
            .turns
            .iter()
            .map(|turn| (&turn.source_id, turn.content_hash.to_ascii_lowercase()))
            .collect::<Vec<_>>(),
    ))
    .map_err(|error| AppError::Invalid(error.to_string()))?;
    let event_id = format!("{:x}", Sha256::digest(identity.as_bytes()));
    let created_at = request
        .turns
        .last()
        .map(|turn| turn.timestamp.clone())
        .expect("non-empty turns");
    let event = GigaEvent::new(
        event_id,
        GigaEventType::ConversationWindow,
        room,
        session_id,
        request.project_keys,
        source_refs,
        GigaLifecycle::conversation_window(),
        created_at,
    )
    .map_err(domain_error)?;
    giga_event_ingest(pool, event).await
}

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

fn stored_source_matches(row: &PgRow, source: &GigaSourceRef) -> Result<bool, AppError> {
    let (room, project, visibility, review_required) = scope_parts(source);
    let (range_start, range_end) = range_parts(source)?;
    Ok(
        row.try_get::<String, _>("source_type")? == source.source_type().as_str()
            && row.try_get::<String, _>("source_id")? == source.source_id()
            && row.try_get::<String, _>("source_role")? == source.role()
            && row.try_get::<String, _>("content_hash")? == source.content_hash()
            && row.try_get::<Option<String>, _>("scope_room")? == room
            && row.try_get::<Option<String>, _>("scope_project")? == project
            && row.try_get::<String, _>("scope_visibility")? == visibility
            && row.try_get::<bool, _>("publication_review_required")? == review_required
            && row.try_get::<Option<i64>, _>("range_start")? == range_start
            && row.try_get::<Option<i64>, _>("range_end")? == range_end
            && row.try_get::<DateTime<Utc>, _>("source_created_at")?
                == timestamp(source.timestamp())?,
    )
}

async fn verify_event_source(
    tx: &mut Transaction<'_, Postgres>,
    event_id: &str,
    source: &GigaSourceRef,
) -> Result<(), AppError> {
    let row = sqlx::query(
        "SELECT source_type,source_id,source_role,content_hash,scope_room,scope_project,
                scope_visibility,publication_review_required,range_start,range_end,
                created_at AS source_created_at
         FROM giga_event_sources WHERE event_id=$1 AND source_type=$2 AND source_id=$3",
    )
    .bind(event_id)
    .bind(source.source_type().as_str())
    .bind(source.source_id())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::Invalid("GIGA source is not part of its named event".into()))?;
    if !stored_source_matches(&row, source)? {
        return Err(AppError::Invalid(
            "GIGA source identity does not match its named event".into(),
        ));
    }
    Ok(())
}

async fn giga_candidate_store_tx(
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

fn source_params(row: &sqlx::postgres::PgRow) -> Result<GigaSourceRefParams, AppError> {
    let start: Option<i64> = row.try_get("range_start")?;
    let end: Option<i64> = row.try_get("range_end")?;
    Ok(GigaSourceRefParams {
        source_type: row.try_get("source_type")?,
        source_id: row.try_get("source_id")?,
        role: row
            .try_get::<Option<String>, _>("source_role")?
            .unwrap_or_else(|| "source".into()),
        timestamp: row
            .try_get::<DateTime<Utc>, _>("source_created_at")?
            .to_rfc3339(),
        content_hash: row.try_get("content_hash")?,
        scope: GigaScopeParams {
            room: RequiredNullable(row.try_get("scope_room")?),
            project: RequiredNullable(row.try_get("scope_project")?),
            visibility: row.try_get("scope_visibility")?,
            publication_review_required: row.try_get("publication_review_required")?,
        },
        range: RequiredNullable(match (start, end) {
            (Some(start), Some(end)) => Some(GigaSourceRangeParams {
                start: start as u64,
                end: end as u64,
            }),
            _ => None,
        }),
    })
}

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

fn source_ref_params(source: &GigaSourceRef) -> GigaSourceRefParams {
    GigaSourceRefParams {
        source_type: source.source_type().as_str().into(),
        source_id: source.source_id().into(),
        role: source.role().into(),
        timestamp: source.timestamp().into(),
        content_hash: source.content_hash().into(),
        scope: GigaScopeParams {
            room: RequiredNullable(source.scope().room().map(ToString::to_string)),
            project: RequiredNullable(source.scope().project().map(str::to_owned)),
            visibility: source.scope().visibility().as_str().into(),
            publication_review_required: source.scope().publication_review_required(),
        },
        range: RequiredNullable(source.range().map(|range| GigaSourceRangeParams {
            start: range.start(),
            end: range.end(),
        })),
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

    let reviewed_at = timestamp(review.reviewed_at())?;
    let promotion_target = review
        .promotion_target()
        .map(|target| json!({"ref": target}));
    let merge_targets =
        if review.merge_target().is_some() || !review.merge_source_candidates().is_empty() {
            Some(json!([{
                "target": review.merge_target(),
                "sources": review.merge_source_candidates(),
            }]))
        } else {
            None
        };
    let target_refs = review
        .source_refs()
        .iter()
        .map(|source| source.content_hash().to_owned())
        .collect::<Vec<_>>();
    let review_id: i64 = sqlx::query_scalar(
        "INSERT INTO giga_reviews
         (candidate_id,action,reviewer_principal,authorization_basis,previous_state,new_state,
          reason,promotion_target,merge_targets,target_refs,reviewed_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
         RETURNING id",
    )
    .bind(review.candidate_id())
    .bind(review_action(review.new_state()))
    .bind(review.reviewer_id())
    .bind(review.authorization_basis())
    .bind(review.previous_state().as_str())
    .bind(review.new_state().as_str())
    .bind(review.reason())
    .bind(promotion_target)
    .bind(merge_targets)
    .bind(Json(target_refs))
    .bind(reviewed_at)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(resonance) = review.resonance() {
        let classifier = resonance.classifier();
        sqlx::query(
            "INSERT INTO giga_review_resonances
             (review_id,candidate_id,event_id,score,classifier_model,classifier_provider_type,
              classifier_model_version,classifier_prompt_version,
              classifier_configuration_digest,classifier_run_id,classifier_completed_at,
              source_refs)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(review_id)
        .bind(review.candidate_id())
        .bind(resonance.event_id())
        .bind(resonance.score())
        .bind(classifier.model())
        .bind(classifier.provider_type())
        .bind(classifier.model_version())
        .bind(classifier.prompt_version())
        .bind(classifier.configuration_digest())
        .bind(classifier.run_id())
        .bind(timestamp(classifier.completed_at())?)
        .bind(Json(promotion_sources_json(resonance.source_refs())))
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

const GIGA_ELIGIBLE_ALL_SQL: &str = "SELECT COUNT(*)::bigint FROM giga_events e
     WHERE (e.queue_state IN ('pending','failed')
            OR (e.queue_state='running' AND e.lease_expires_at <= NOW()))
       AND NOT EXISTS (SELECT 1 FROM giga_candidates c WHERE c.event_id=e.event_id)
       AND NOT EXISTS (SELECT 1 FROM giga_review_resonances r WHERE r.event_id=e.event_id)";
const GIGA_ELIGIBLE_ROOM_SQL: &str = "SELECT COUNT(*)::bigint FROM giga_events e
     WHERE e.room=$1
       AND (e.queue_state IN ('pending','failed')
            OR (e.queue_state='running' AND e.lease_expires_at <= NOW()))
       AND NOT EXISTS (SELECT 1 FROM giga_candidates c WHERE c.event_id=e.event_id)
       AND NOT EXISTS (SELECT 1 FROM giga_review_resonances r WHERE r.event_id=e.event_id)";
const GIGA_ATTEMPTS_ALL_SQL: &str = "SELECT COUNT(*)::bigint FROM giga_event_attempts a
     JOIN giga_events e ON e.event_id=a.event_id
     WHERE (e.queue_state IN ('pending','failed')
            OR (e.queue_state='running' AND e.lease_expires_at <= NOW()))
       AND NOT EXISTS (SELECT 1 FROM giga_candidates c WHERE c.event_id=e.event_id)
       AND NOT EXISTS (SELECT 1 FROM giga_review_resonances r WHERE r.event_id=e.event_id)";
const GIGA_ATTEMPTS_ROOM_SQL: &str = "SELECT COUNT(*)::bigint FROM giga_event_attempts a
     JOIN giga_events e ON e.event_id=a.event_id
     WHERE e.room=$1
       AND (e.queue_state IN ('pending','failed')
            OR (e.queue_state='running' AND e.lease_expires_at <= NOW()))
       AND NOT EXISTS (SELECT 1 FROM giga_candidates c WHERE c.event_id=e.event_id)
       AND NOT EXISTS (SELECT 1 FROM giga_review_resonances r WHERE r.event_id=e.event_id)";

async fn giga_queue_state_counts(
    tx: &mut Transaction<'_, Postgres>,
    room: Option<&str>,
) -> Result<Vec<GigaQueueStateCount>, AppError> {
    let rows = match room {
        Some(room) => {
            sqlx::query(
                "SELECT queue_state,COUNT(*)::bigint AS count
                 FROM giga_events WHERE room=$1 GROUP BY queue_state ORDER BY queue_state",
            )
            .bind(room)
            .fetch_all(&mut **tx)
            .await?
        }
        None => {
            sqlx::query(
                "SELECT queue_state,COUNT(*)::bigint AS count
                 FROM giga_events GROUP BY queue_state ORDER BY queue_state",
            )
            .fetch_all(&mut **tx)
            .await?
        }
    };

    rows.into_iter()
        .map(|row| {
            Ok(GigaQueueStateCount {
                queue_state: row.try_get("queue_state")?,
                count: row.try_get::<i64, _>("count")? as u64,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(AppError::from)
}

async fn giga_queue_count(
    tx: &mut Transaction<'_, Postgres>,
    room: Option<&str>,
    all_sql: &str,
    room_sql: &str,
) -> Result<u64, AppError> {
    let count = match room {
        Some(room) => {
            sqlx::query_scalar::<_, i64>(room_sql)
                .bind(room)
                .fetch_one(&mut **tx)
                .await?
        }
        None => {
            sqlx::query_scalar::<_, i64>(all_sql)
                .fetch_one(&mut **tx)
                .await?
        }
    };

    u64::try_from(count).map_err(|_| AppError::Invalid("GIGA queue count is invalid".into()))
}

pub async fn giga_queue_maintenance(
    pool: &PgPool,
    request: GigaQueueMaintenanceRequest,
) -> Result<GigaQueueMaintenanceResult, AppError> {
    let room = request.room().to_string();
    let scoped_room = match request.scope() {
        GigaQueueMaintenanceScope::Room => Some(room.as_str()),
        GigaQueueMaintenanceScope::All => None,
    };
    let mut tx = pool.begin().await?;

    if request.operation() == GigaQueueMaintenanceOperation::PurgeStuck {
        sqlx::query(
            "SELECT pg_advisory_xact_lock(hashtextextended('solarisael.giga_queue_maintenance', 42))",
        )
        .execute(&mut *tx)
        .await?;
    }

    let before = giga_queue_state_counts(&mut tx, scoped_room).await?;
    let eligible_events = giga_queue_count(
        &mut tx,
        scoped_room,
        GIGA_ELIGIBLE_ALL_SQL,
        GIGA_ELIGIBLE_ROOM_SQL,
    )
    .await?;
    let non_succeeded = giga_queue_count(
        &mut tx,
        scoped_room,
        "SELECT COUNT(*)::bigint FROM giga_events WHERE queue_state <> 'succeeded'",
        "SELECT COUNT(*)::bigint FROM giga_events
         WHERE room=$1 AND queue_state <> 'succeeded'",
    )
    .await?;
    let blocked_events = non_succeeded.saturating_sub(eligible_events);
    let deleted_attempts = if request.operation() == GigaQueueMaintenanceOperation::PurgeStuck {
        giga_queue_count(
            &mut tx,
            scoped_room,
            GIGA_ATTEMPTS_ALL_SQL,
            GIGA_ATTEMPTS_ROOM_SQL,
        )
        .await?
    } else {
        0
    };
    let preserved_candidates = giga_queue_count(
        &mut tx,
        scoped_room,
        "SELECT COUNT(*)::bigint FROM giga_candidates",
        "SELECT COUNT(*)::bigint FROM giga_candidates WHERE room=$1",
    )
    .await?;
    let deleted_events = if request.operation() == GigaQueueMaintenanceOperation::PurgeStuck {
        match scoped_room {
            Some(room) => sqlx::query(
                "DELETE FROM giga_events e
                     WHERE e.room=$1
                       AND (e.queue_state IN ('pending','failed')
                            OR (e.queue_state='running' AND e.lease_expires_at <= NOW()))
                       AND NOT EXISTS (
                           SELECT 1 FROM giga_candidates c WHERE c.event_id=e.event_id
                       )
                       AND NOT EXISTS (
                           SELECT 1 FROM giga_review_resonances r WHERE r.event_id=e.event_id
                       )",
            )
            .bind(room)
            .execute(&mut *tx)
            .await?
            .rows_affected(),
            None => sqlx::query(
                "DELETE FROM giga_events e
                     WHERE (e.queue_state IN ('pending','failed')
                            OR (e.queue_state='running' AND e.lease_expires_at <= NOW()))
                       AND NOT EXISTS (
                           SELECT 1 FROM giga_candidates c WHERE c.event_id=e.event_id
                       )
                       AND NOT EXISTS (
                           SELECT 1 FROM giga_review_resonances r WHERE r.event_id=e.event_id
                       )",
            )
            .execute(&mut *tx)
            .await?
            .rows_affected(),
        }
    } else {
        0
    };
    let after = giga_queue_state_counts(&mut tx, scoped_room).await?;

    tx.commit().await?;

    Ok(GigaQueueMaintenanceResult {
        ok: true,
        operation: request.operation().as_str().into(),
        scope: request.scope().as_str().into(),
        room,
        eligible_events,
        blocked_events,
        deleted_events,
        deleted_attempts,
        preserved_candidates,
        before,
        after,
    })
}

pub async fn giga_health(
    pool: &PgPool,
    request: GigaHealthRequest,
) -> Result<GigaHealthResult, AppError> {
    let room = request.room().to_string();
    let event = sqlx::query(
        "SELECT COUNT(*) FILTER (WHERE queue_state IN ('pending','running','failed'))::bigint AS queue_depth,
                EXTRACT(EPOCH FROM (NOW()-MIN(created_at) FILTER (WHERE queue_state IN ('pending','running','failed'))))::bigint AS oldest_age,
                COUNT(*) FILTER (WHERE queue_state='succeeded')::bigint AS processed_count,
                COUNT(*) FILTER (WHERE queue_state='failed')::bigint AS failed_count,
                (SELECT latest.last_error FROM giga_events latest
                 WHERE latest.room=$1 AND latest.last_error IS NOT NULL
                 ORDER BY latest.updated_at DESC,latest.event_id LIMIT 1) AS last_error,
                (SELECT latest.updated_at FROM giga_events latest
                 WHERE latest.room=$1 AND latest.last_error IS NOT NULL
                 ORDER BY latest.updated_at DESC,latest.event_id LIMIT 1) AS last_error_at,
                COALESCE((
                    SELECT COUNT(*)::bigint
                    FROM (
                        SELECT outcome,
                               SUM(CASE WHEN outcome='succeeded' THEN 1 ELSE 0 END)
                                   OVER (ORDER BY finished_at DESC,event_id,replay_count,attempt_count
                                         ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS successes
                        FROM giga_event_attempts
                        WHERE room=$1 AND finished_at IS NOT NULL
                    ) recent
                    WHERE recent.successes=0 AND recent.outcome <> 'succeeded'
                ),0)::bigint AS consecutive_failures
         FROM giga_events WHERE room=$1",
    )
    .bind(&room)
    .fetch_one(pool)
    .await?;
    let rows = sqlx::query(
        "SELECT kind,review_state,COUNT(*)::bigint AS count FROM giga_candidates
         WHERE room=$1 GROUP BY kind,review_state ORDER BY kind,review_state",
    )
    .bind(&room)
    .fetch_all(pool)
    .await?;
    let candidates_by_kind_state = rows
        .into_iter()
        .map(|row| {
            Ok(GigaHealthCount {
                kind: row.try_get("kind")?,
                review_state: row.try_get("review_state")?,
                count: row.try_get::<i64, _>("count")? as u64,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let oldest: Option<i64> = event.try_get("oldest_age")?;
    let last_error: Option<String> = event.try_get("last_error")?;
    let last_error_at: Option<DateTime<Utc>> = event.try_get("last_error_at")?;
    let consecutive_failures: u64 = event.try_get::<i64, _>("consecutive_failures")? as u64;
    Ok(GigaHealthResult {
        enabled: giga_classifier_enabled(),
        store_healthy: true,
        queue_depth: event.try_get::<i64, _>("queue_depth")? as u64,
        oldest_queue_age_seconds: oldest.map(|age| age.max(0) as u64),
        processed_count: event.try_get::<i64, _>("processed_count")? as u64,
        failed_count: event.try_get::<i64, _>("failed_count")? as u64,
        candidates_by_kind_state,
        classifier: giga_classifier_health(
            last_error,
            last_error_at.map(|value| value.to_rfc3339()),
            consecutive_failures,
        ),
    })
}

fn domain_error(error: impl std::fmt::Display) -> AppError {
    AppError::Invalid(error.to_string())
}

fn lifecycle_text(value: &Value, field: &str) -> Result<String, AppError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AppError::Invalid(format!("stored GIGA lifecycle is missing {field}")))
}

fn lifecycle_strings(value: &Value, field: &str) -> Result<Vec<String>, AppError> {
    value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Invalid(format!("stored GIGA lifecycle is missing {field}")))?
        .iter()
        .map(|entry| {
            entry.as_str().map(str::to_owned).ok_or_else(|| {
                AppError::Invalid(format!("stored GIGA lifecycle {field} is invalid"))
            })
        })
        .collect()
}

fn lifecycle_from_json(
    event_type: GigaEventType,
    value: &Value,
) -> Result<GigaLifecycle, AppError> {
    match event_type {
        GigaEventType::ConversationWindow => Ok(GigaLifecycle::conversation_window()),
        GigaEventType::TaskStarted => GigaLifecycle::task_started(
            lifecycle_text(value, "task_reference")?,
            lifecycle_text(value, "worker_id")?,
            lifecycle_text(value, "worker_role")?,
            lifecycle_text(value, "phase")?,
            lifecycle_text(value, "project_key")?,
            lifecycle_text(value, "task_kind")?,
            GigaRisk::parse(&lifecycle_text(value, "risk")?).map_err(domain_error)?,
            lifecycle_text(value, "target")?,
            lifecycle_text(value, "change")?,
            lifecycle_strings(value, "proof_contract")?,
        )
        .map_err(domain_error),
        GigaEventType::TaskCompleted => GigaLifecycle::task_completed(
            lifecycle_text(value, "task_reference")?,
            lifecycle_text(value, "outcome")?,
            lifecycle_text(value, "verification_result")?,
        )
        .map_err(domain_error),
        GigaEventType::SubagentDispatched => {
            let acceptance = if value.get("acceptance").is_some() {
                lifecycle_strings(value, "acceptance")?
            } else {
                lifecycle_strings(value, "proof_contract")?
            };
            GigaLifecycle::subagent_dispatched(
                lifecycle_text(value, "subagent_reference")?,
                lifecycle_text(value, "parent_task")?,
                lifecycle_text(value, "role")?,
                lifecycle_text(value, "target")?,
                lifecycle_text(value, "change")?,
                acceptance,
            )
            .map_err(domain_error)
        }
        GigaEventType::SubagentCompleted => GigaLifecycle::subagent_completed(
            lifecycle_text(value, "subagent_reference")?,
            lifecycle_text(value, "parent_task")?,
            lifecycle_text(value, "outcome")?,
        )
        .map_err(domain_error),
        GigaEventType::TodoTransition => GigaLifecycle::todo_transition(
            lifecycle_text(value, "todo_reference")?,
            lifecycle_text(value, "previous_state")?,
            lifecycle_text(value, "new_state")?,
        )
        .map_err(domain_error),
        GigaEventType::ToolOutcome => GigaLifecycle::tool_outcome(
            lifecycle_text(value, "tool_name")?,
            lifecycle_text(value, "status")?,
            lifecycle_text(value, "sanitized_outcome")?,
        )
        .map_err(domain_error),
        GigaEventType::ManualReprocess => {
            let range = value
                .get("source_range")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    AppError::Invalid("stored GIGA lifecycle is missing source_range".into())
                })?;
            let start = range.get("start").and_then(Value::as_u64).ok_or_else(|| {
                AppError::Invalid("stored GIGA lifecycle source_range is invalid".into())
            })?;
            let end = range.get("end").and_then(Value::as_u64).ok_or_else(|| {
                AppError::Invalid("stored GIGA lifecycle source_range is invalid".into())
            })?;
            GigaLifecycle::manual_reprocess(
                GigaSourceRange::new(start, end).map_err(domain_error)?,
                lifecycle_text(value, "reason")?,
                lifecycle_text(value, "operator_identity")?,
            )
            .map_err(domain_error)
        }
    }
}

fn source_from_row(row: &PgRow) -> Result<GigaSourceRef, AppError> {
    let range_start: Option<i64> = row.try_get("range_start")?;
    let range_end: Option<i64> = row.try_get("range_end")?;
    let range = match (range_start, range_end) {
        (None, None) => None,
        (Some(start), Some(end)) => Some(
            GigaSourceRange::new(
                u64::try_from(start)
                    .map_err(|_| AppError::Invalid("stored GIGA source range is invalid".into()))?,
                u64::try_from(end)
                    .map_err(|_| AppError::Invalid("stored GIGA source range is invalid".into()))?,
            )
            .map_err(domain_error)?,
        ),
        _ => {
            return Err(AppError::Invalid(
                "stored GIGA source range is incomplete".into(),
            ));
        }
    };
    let visibility = GigaVisibility::parse(&row.try_get::<String, _>("scope_visibility")?)
        .map_err(domain_error)?;
    let scope = GigaScope::new(
        row.try_get("scope_room")?,
        row.try_get("scope_project")?,
        visibility,
        row.try_get("publication_review_required")?,
    )
    .map_err(domain_error)?;
    GigaSourceRef::new(
        GigaSourceType::parse(&row.try_get::<String, _>("source_type")?).map_err(domain_error)?,
        row.try_get("source_id")?,
        row.try_get("source_role")?,
        row.try_get::<DateTime<Utc>, _>("source_created_at")?
            .to_rfc3339(),
        row.try_get("content_hash")?,
        scope,
        range,
    )
    .map_err(domain_error)
}

pub(crate) async fn event_from_store(
    tx: &mut Transaction<'_, Postgres>,
    event_id: &str,
) -> Result<GigaEvent, AppError> {
    let event = sqlx::query(
        "SELECT event_schema_version,event_id,event_type,room,session_id,project_keys,lifecycle,created_at
         FROM giga_events WHERE event_id=$1",
    )
    .bind(event_id)
    .fetch_one(&mut **tx)
    .await?;
    if event.try_get::<i32, _>("event_schema_version")? != 1 {
        return Err(AppError::Invalid(
            "stored GIGA event schema version is unsupported".into(),
        ));
    }
    let event_type =
        GigaEventType::parse(&event.try_get::<String, _>("event_type")?).map_err(domain_error)?;
    let source_rows = sqlx::query(
        "SELECT source_type,source_id,source_role,content_hash,scope_room,scope_project,
                scope_visibility,publication_review_required,range_start,range_end,
                created_at AS source_created_at
         FROM giga_event_sources WHERE event_id=$1 ORDER BY source_ordinal",
    )
    .bind(event_id)
    .fetch_all(&mut **tx)
    .await?;
    let sources = source_rows
        .iter()
        .map(source_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let lifecycle: Json<Value> = event.try_get("lifecycle")?;
    GigaEvent::new(
        event.try_get("event_id")?,
        event_type,
        RoomKey::new(
            event
                .try_get::<Option<String>, _>("room")?
                .ok_or_else(|| AppError::Invalid("stored GIGA event has no room".into()))?,
        )
        .map_err(domain_error)?,
        event.try_get("session_id")?,
        event.try_get("project_keys")?,
        sources,
        lifecycle_from_json(event_type, &lifecycle.0)?,
        event
            .try_get::<DateTime<Utc>, _>("created_at")?
            .to_rfc3339(),
    )
    .map_err(domain_error)
}

pub async fn giga_event_claim(
    pool: &PgPool,
    request: GigaEventClaimRequest,
) -> Result<GigaEventClaimReceipt, AppError> {
    let room = request.room().to_string();
    let mut tx = pool.begin().await?;
    let claimed_at = database_now(&mut tx).await?;
    let lease_expires_at = claimed_at + Duration::seconds(i64::from(request.lease_seconds()));

    let exhausted = sqlx::query(
        "SELECT event_id,replay_count,attempt_count FROM giga_events
         WHERE room=$1 AND queue_state='running' AND lease_expires_at<=$2
           AND attempt_count>=$3
         ORDER BY lease_expires_at,created_at,event_id
         FOR UPDATE SKIP LOCKED",
    )
    .bind(&room)
    .bind(claimed_at)
    .bind(i32::try_from(GIGA_MAX_EVENT_ATTEMPTS).unwrap_or(i32::MAX))
    .fetch_all(&mut *tx)
    .await?;
    for row in exhausted {
        let event_id: String = row.try_get("event_id")?;
        let attempt_count: i32 = row.try_get("attempt_count")?;
        let replay_count: i32 = row.try_get("replay_count")?;
        sqlx::query(
            "UPDATE giga_event_attempts
             SET outcome='lease_expired',error_class='lease_expired_retry_exhausted',
                 finished_at=$4
             WHERE event_id=$1 AND replay_count=$2 AND attempt_count=$3
               AND finished_at IS NULL",
        )
        .bind(&event_id)
        .bind(replay_count)
        .bind(attempt_count)
        .bind(claimed_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE giga_events
             SET queue_state='failed',locked_by=NULL,locked_at=NULL,lease_expires_at=NULL,
                 last_error='lease_expired_retry_exhausted',processed_at=$2,last_finished_at=$2,
                 updated_at=$2
             WHERE event_id=$1",
        )
        .bind(&event_id)
        .bind(claimed_at)
        .execute(&mut *tx)
        .await?;
    }

    let selected = sqlx::query(
        "SELECT event_id,queue_state,replay_count,attempt_count FROM giga_events
         WHERE room=$1 AND attempt_count<$3 AND (
           (queue_state='pending' AND available_at<=$2)
           OR (queue_state='running' AND lease_expires_at<=$2)
         )
         ORDER BY
           CASE WHEN queue_state='pending' THEN 0 ELSE 1 END,
           CASE WHEN queue_state='pending' THEN available_at ELSE lease_expires_at END,
           created_at,event_id
         LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .bind(&room)
    .bind(claimed_at)
    .bind(i32::try_from(GIGA_MAX_EVENT_ATTEMPTS).unwrap_or(i32::MAX))
    .fetch_optional(&mut *tx)
    .await?;
    let Some(selected) = selected else {
        let receipt = GigaEventClaimReceipt::new(
            request.room().clone(),
            request.worker_id().into(),
            claimed_at.to_rfc3339(),
            None,
            None,
            None,
        )
        .map_err(domain_error)?;
        tx.commit().await?;
        return Ok(receipt);
    };

    let event_id: String = selected.try_get("event_id")?;
    let previous_state: String = selected.try_get("queue_state")?;
    let replay_count: i32 = selected.try_get("replay_count")?;
    let previous_attempt: i32 = selected.try_get("attempt_count")?;
    if previous_state == "running" {
        sqlx::query(
            "UPDATE giga_event_attempts
             SET outcome='lease_expired',error_class='lease_expired',finished_at=$4
             WHERE event_id=$1 AND replay_count=$2 AND attempt_count=$3
               AND finished_at IS NULL",
        )
        .bind(&event_id)
        .bind(replay_count)
        .bind(previous_attempt)
        .bind(claimed_at)
        .execute(&mut *tx)
        .await?;
    }
    let attempt_count = previous_attempt + 1;
    sqlx::query(
        "UPDATE giga_events
         SET queue_state='running',attempt_count=$2,
             retry_count=retry_count+CASE WHEN $3='running' THEN 1 ELSE 0 END,
             locked_by=$4,locked_at=$5,lease_expires_at=$6,candidate_count=0,
             processed_at=NULL,last_finished_at=NULL,updated_at=$5
         WHERE event_id=$1",
    )
    .bind(&event_id)
    .bind(attempt_count)
    .bind(&previous_state)
    .bind(request.worker_id())
    .bind(claimed_at)
    .bind(lease_expires_at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO giga_event_attempts
         (event_id,replay_count,attempt_count,room,worker_id,claimed_at,lease_expires_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(&event_id)
    .bind(replay_count)
    .bind(attempt_count)
    .bind(&room)
    .bind(request.worker_id())
    .bind(claimed_at)
    .bind(lease_expires_at)
    .execute(&mut *tx)
    .await?;
    let event = event_from_store(&mut tx, &event_id).await?;
    let receipt =
        GigaEventClaimReceipt::new(
            request.room().clone(),
            request.worker_id().into(),
            claimed_at.to_rfc3339(),
            Some(event),
            Some(lease_expires_at.to_rfc3339()),
            Some(u32::try_from(attempt_count).map_err(|_| {
                AppError::Invalid("GIGA attempt count exceeds protocol bounds".into())
            })?),
        )
        .map_err(domain_error)?;
    tx.commit().await?;
    Ok(receipt)
}

async fn giga_event_finish_tx(
    tx: &mut Transaction<'_, Postgres>,
    request: &GigaEventFinishRequest,
) -> Result<GigaEventFinishReceipt, AppError> {
    let room = request.room().to_string();
    let event = sqlx::query(
        "SELECT room,queue_state,locked_by,locked_at,lease_expires_at,replay_count,attempt_count
         FROM giga_events WHERE event_id=$1 FOR UPDATE",
    )
    .bind(request.event_id())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::Invalid("GIGA event does not exist".into()))?;
    let finished_at = database_now(tx).await?;
    if event.try_get::<Option<String>, _>("room")?.as_deref() != Some(room.as_str()) {
        return Err(AppError::Invalid(
            "GIGA event finish crosses the room boundary".into(),
        ));
    }
    let queue_state: String = event.try_get("queue_state")?;
    let attempt_count: i32 = event.try_get("attempt_count")?;
    let replay_count: i32 = event.try_get("replay_count")?;
    if queue_state != "running" {
        let history = sqlx::query(
            "SELECT worker_id,outcome,candidate_count,error_class,available_at,finished_at
             FROM giga_event_attempts
             WHERE event_id=$1 AND replay_count=$2 AND attempt_count=$3",
        )
        .bind(request.event_id())
        .bind(replay_count)
        .bind(attempt_count)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(history) = history {
            let stored_finished: Option<DateTime<Utc>> = history.try_get("finished_at")?;
            let stored_available: Option<DateTime<Utc>> = history.try_get("available_at")?;
            let stored_retry_delay = match (&stored_available, &stored_finished) {
                (Some(available), Some(finished)) => u32::try_from(
                    available
                        .signed_duration_since(finished.clone())
                        .num_seconds(),
                )
                .ok(),
                _ => None,
            };
            let requested_retry_delay = (request.outcome() == GigaEventFinishOutcome::Retry)
                .then_some(request.retry_after_seconds().unwrap_or(0));
            let matches = history.try_get::<String, _>("worker_id")? == request.worker_id()
                && history.try_get::<Option<String>, _>("outcome")?.as_deref()
                    == Some(request.outcome().as_str())
                && history.try_get::<i32, _>("candidate_count")?
                    == i32::try_from(request.candidate_count()).unwrap_or(i32::MAX)
                && history
                    .try_get::<Option<String>, _>("error_class")?
                    .as_deref()
                    == request.error_class()
                && stored_retry_delay == requested_retry_delay
                && stored_finished.is_some();
            if matches {
                return GigaEventFinishReceipt::new(
                    request.room().clone(),
                    request.event_id().into(),
                    request.worker_id().into(),
                    request.outcome(),
                    GigaQueueState::parse(&queue_state).map_err(domain_error)?,
                    u32::try_from(attempt_count).map_err(|_| {
                        AppError::Invalid("GIGA attempt count exceeds protocol bounds".into())
                    })?,
                    request.candidate_count(),
                    stored_available.map(|value| value.to_rfc3339()),
                    stored_finished.unwrap().to_rfc3339(),
                )
                .map_err(domain_error);
            }
        }
        return Err(AppError::Invalid(
            "GIGA event is not owned by an active lease".into(),
        ));
    }
    if event.try_get::<Option<String>, _>("locked_by")?.as_deref() != Some(request.worker_id()) {
        return Err(AppError::Invalid(
            "GIGA event lease is owned by another worker".into(),
        ));
    }
    let locked_at: DateTime<Utc> = event
        .try_get::<Option<DateTime<Utc>>, _>("locked_at")?
        .ok_or_else(|| AppError::Invalid("GIGA event lease has no claim time".into()))?;
    if finished_at < locked_at {
        return Err(AppError::Invalid(
            "database time precedes the GIGA lease claim".into(),
        ));
    }
    let lease_expires_at: DateTime<Utc> = event
        .try_get::<Option<DateTime<Utc>>, _>("lease_expires_at")?
        .ok_or_else(|| AppError::Invalid("GIGA event lease has no expiry".into()))?;
    if finished_at >= lease_expires_at {
        return Err(AppError::Invalid(
            "GIGA event lease expired before finish".into(),
        ));
    }
    if request.outcome() == GigaEventFinishOutcome::Retry
        && attempt_count >= i32::try_from(GIGA_MAX_EVENT_ATTEMPTS).unwrap_or(i32::MAX)
    {
        return Err(AppError::Invalid(
            "GIGA retry limit is exhausted; finish the event as failed".into(),
        ));
    }
    let stored_candidates: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM giga_candidates WHERE event_id=$1")
            .bind(request.event_id())
            .fetch_one(&mut **tx)
            .await?;
    if stored_candidates != i64::from(request.candidate_count()) {
        return Err(AppError::Invalid(
            "GIGA candidate_count does not match durable candidates for the event".into(),
        ));
    }
    let (next_state, available_at) = match request.outcome() {
        GigaEventFinishOutcome::Succeeded => ("succeeded", None),
        GigaEventFinishOutcome::Retry => (
            "pending",
            Some(
                finished_at.clone()
                    + Duration::seconds(i64::from(request.retry_after_seconds().unwrap_or(0))),
            ),
        ),
        GigaEventFinishOutcome::Failed => ("failed", None),
    };
    let attempt = sqlx::query(
        "UPDATE giga_event_attempts
         SET outcome=$4,candidate_count=$5,error_class=$6,available_at=$7,finished_at=$8
         WHERE event_id=$1 AND replay_count=$2 AND attempt_count=$3
           AND finished_at IS NULL",
    )
    .bind(request.event_id())
    .bind(replay_count)
    .bind(attempt_count)
    .bind(request.outcome().as_str())
    .bind(i32::try_from(request.candidate_count()).unwrap_or(i32::MAX))
    .bind(request.error_class())
    .bind(available_at.clone())
    .bind(finished_at.clone())
    .execute(&mut **tx)
    .await?;
    if attempt.rows_affected() != 1 {
        return Err(AppError::Invalid(
            "GIGA active attempt could not be finished exactly once".into(),
        ));
    }
    sqlx::query(
        "UPDATE giga_events
         SET queue_state=$2,candidate_count=$3,last_error=$4,available_at=COALESCE($5,available_at),
             retry_count=retry_count+CASE WHEN $2='pending' THEN 1 ELSE 0 END,
             locked_by=NULL,locked_at=NULL,lease_expires_at=NULL,
             processed_at=CASE WHEN $2 IN ('succeeded','failed') THEN $6 ELSE NULL END,
             last_finished_at=$6,updated_at=$6
         WHERE event_id=$1",
    )
    .bind(request.event_id())
    .bind(next_state)
    .bind(i32::try_from(request.candidate_count()).unwrap_or(i32::MAX))
    .bind(request.error_class())
    .bind(available_at.clone())
    .bind(finished_at.clone())
    .execute(&mut **tx)
    .await?;
    GigaEventFinishReceipt::new(
        request.room().clone(),
        request.event_id().into(),
        request.worker_id().into(),
        request.outcome(),
        GigaQueueState::parse(next_state).map_err(domain_error)?,
        u32::try_from(attempt_count)
            .map_err(|_| AppError::Invalid("GIGA attempt count exceeds protocol bounds".into()))?,
        request.candidate_count(),
        available_at.map(|value| value.to_rfc3339()),
        finished_at.to_rfc3339(),
    )
    .map_err(domain_error)
}

pub async fn giga_event_finish(
    pool: &PgPool,
    request: GigaEventFinishRequest,
) -> Result<GigaEventFinishReceipt, AppError> {
    let mut tx = pool.begin().await?;
    let receipt = giga_event_finish_tx(&mut tx, &request).await?;
    tx.commit().await?;
    Ok(receipt)
}

pub(crate) async fn giga_candidate_store_and_finish(
    pool: &PgPool,
    candidate: GigaCandidate,
    finish: GigaEventFinishRequest,
) -> Result<GigaEventFinishReceipt, AppError> {
    let mut tx = pool.begin().await?;
    giga_candidate_store_tx(&mut tx, &candidate).await?;
    let receipt = giga_event_finish_tx(&mut tx, &finish).await?;
    tx.commit().await?;
    Ok(receipt)
}

pub async fn giga_event_replay(
    pool: &PgPool,
    request: GigaEventReplayRequest,
) -> Result<GigaEventReplayReceipt, AppError> {
    let room = request.room().to_string();
    let mut tx = pool.begin().await?;
    let event = sqlx::query(
        "SELECT room,queue_state,replay_count FROM giga_events WHERE event_id=$1 FOR UPDATE",
    )
    .bind(request.event_id())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Invalid("GIGA event does not exist".into()))?;
    let replayed_at = database_now(&mut tx).await?;
    if event.try_get::<Option<String>, _>("room")?.as_deref() != Some(room.as_str()) {
        return Err(AppError::Invalid(
            "GIGA event replay crosses the room boundary".into(),
        ));
    }
    let queue_state: String = event.try_get("queue_state")?;
    let replay_count: i32 = event.try_get("replay_count")?;
    if queue_state == "pending" {
        let replay = sqlx::query(
            "SELECT replayed_at FROM giga_event_replays
             WHERE event_id=$1 AND replay_count=$2 AND room=$3 AND operator_identity=$4
               AND authorization_basis=$5",
        )
        .bind(request.event_id())
        .bind(replay_count)
        .bind(&room)
        .bind(request.operator_identity())
        .bind(request.authorization_basis())
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(replay) = replay {
            let stored_replayed_at: DateTime<Utc> = replay.try_get("replayed_at")?;
            let receipt = GigaEventReplayReceipt::new(
                request.room().clone(),
                request.event_id().into(),
                request.operator_identity().into(),
                GigaQueueState::Failed,
                GigaQueueState::Pending,
                0,
                stored_replayed_at.to_rfc3339(),
            )
            .map_err(domain_error)?;
            tx.commit().await?;
            return Ok(receipt);
        }
    }
    if queue_state != "failed" {
        return Err(AppError::Invalid(
            "only failed GIGA work can be replayed".into(),
        ));
    }
    let missing_source_roles: bool = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM giga_event_sources WHERE event_id=$1 AND source_role IS NULL
         )",
    )
    .bind(request.event_id())
    .fetch_one(&mut *tx)
    .await?;
    if missing_source_roles {
        return Err(AppError::Invalid(
            "GIGA event cannot be replayed because its pre-0004 source roles are unavailable"
                .into(),
        ));
    }
    let next_replay_count = replay_count + 1;
    sqlx::query(
        "INSERT INTO giga_event_replays
         (event_id,replay_count,room,operator_identity,authorization_basis,previous_state,replayed_at)
         VALUES ($1,$2,$3,$4,$5,'failed',$6)",
    )
    .bind(request.event_id())
    .bind(next_replay_count)
    .bind(&room)
    .bind(request.operator_identity())
    .bind(request.authorization_basis())
    .bind(replayed_at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE giga_events
         SET queue_state='pending',attempt_count=0,retry_count=0,candidate_count=0,
             available_at=$2,locked_by=NULL,locked_at=NULL,lease_expires_at=NULL,
             processed_at=NULL,last_finished_at=NULL,replay_count=$3,updated_at=$2
         WHERE event_id=$1",
    )
    .bind(request.event_id())
    .bind(replayed_at)
    .bind(next_replay_count)
    .execute(&mut *tx)
    .await?;
    let receipt = GigaEventReplayReceipt::new(
        request.room().clone(),
        request.event_id().into(),
        request.operator_identity().into(),
        GigaQueueState::Failed,
        GigaQueueState::Pending,
        0,
        replayed_at.to_rfc3339(),
    )
    .map_err(domain_error)?;
    tx.commit().await?;
    Ok(receipt)
}

fn promotion_source_json(source: &GigaSourceRef) -> Value {
    let range = source
        .range()
        .map(|range| json!({"start": range.start(), "end": range.end()}));
    json!({
        "source_type": source.source_type().as_str(),
        "source_id": source.source_id(),
        "role": source.role(),
        "timestamp": source.timestamp(),
        "content_hash": source.content_hash(),
        "scope": {
            "room": source.scope().room().map(ToString::to_string),
            "project": source.scope().project(),
            "visibility": source.scope().visibility().as_str(),
            "publication_review_required": source.scope().publication_review_required(),
        },
        "range": range,
    })
}

fn promotion_sources_json(sources: &[GigaSourceRef]) -> Vec<Value> {
    let mut sources = sources.iter().collect::<Vec<_>>();
    sources.sort_by(|left, right| {
        (left.source_type().as_str(), left.source_id())
            .cmp(&(right.source_type().as_str(), right.source_id()))
    });
    sources.into_iter().map(promotion_source_json).collect()
}

fn promotion_payload_json(payload: &GigaPromotionPayload) -> Value {
    match payload {
        GigaPromotionPayload::Memory(payload) => json!({
            "kind": "memory",
            "payload": {
                "title": payload.title(),
                "body": payload.body(),
                "threads": payload.threads(),
            },
        }),
        GigaPromotionPayload::CodingLesson(payload) => json!({
            "kind": "coding_lesson",
            "payload": {
                "title": payload.title(),
                "body": payload.body(),
                "shape": payload.shape(),
                "proof_pattern": payload.proof_pattern(),
                "trigger_context": payload.trigger_context(),
                "tags": payload.tags(),
            },
        }),
        GigaPromotionPayload::ProjectLesson { payload, .. } => json!({
            "kind": "project_lesson",
            "payload": {
                "title": payload.title(),
                "body": payload.body(),
                "project": payload.project(),
                "proof_pattern": payload.proof_pattern(),
                "trigger_context": payload.trigger_context(),
                "language_keys": payload.language_keys(),
                "technology_keys": payload.technology_keys(),
                "thread_keys": payload.thread_keys(),
                "tags": payload.tags(),
            }
        }),
    }
}

fn publication_consent_json(payload: &GigaPromotionPayload) -> Option<Value> {
    match payload {
        GigaPromotionPayload::ProjectLesson { .. } => Some(json!({
            "operator_approved": true,
            "reviewer_approved": true,
        })),
        GigaPromotionPayload::Memory(_) | GigaPromotionPayload::CodingLesson(_) => None,
    }
}

fn promotion_digest(request: &GigaPromotionRequest) -> Result<String, AppError> {
    let canonical = json!({
        "candidate_id": request.candidate_id(),
        "room": request.room().to_string(),
        "reviewer_id": request.reviewer_id(),
        "operator_identity": request.operator_identity(),
        "authorization_basis": request.authorization_basis(),
        "source_refs": promotion_sources_json(request.source_refs()),
        "target": promotion_payload_json(request.payload()),
        "publication_consent": publication_consent_json(request.payload()),
        "reviewed_at": request.reviewed_at(),
    });
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|error| AppError::Protocol(format!("GIGA promotion digest failed: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn normalize_promotion_values(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn candidate_scope(row: &PgRow) -> Result<GigaScope, AppError> {
    GigaScope::new(
        row.try_get("scope_room")?,
        row.try_get("scope_project")?,
        GigaVisibility::parse(&row.try_get::<String, _>("scope_visibility")?)
            .map_err(domain_error)?,
        row.try_get("publication_review_required")?,
    )
    .map_err(domain_error)
}

async fn fresh_candidate_sources(
    tx: &mut Transaction<'_, Postgres>,
    candidate_id: &str,
) -> Result<Vec<GigaSourceRef>, AppError> {
    let rows = sqlx::query(
        "SELECT es.source_type,es.source_id,es.source_role,es.content_hash,
                es.scope_room,es.scope_project,es.scope_visibility,
                es.publication_review_required,es.range_start,es.range_end,
                es.created_at AS source_created_at,
                (
                  cs.source_role=es.source_role
                  AND cs.content_hash=es.content_hash
                  AND cs.scope_room IS NOT DISTINCT FROM es.scope_room
                  AND cs.scope_project IS NOT DISTINCT FROM es.scope_project
                  AND cs.scope_visibility=es.scope_visibility
                  AND cs.publication_review_required=es.publication_review_required
                  AND cs.range_start IS NOT DISTINCT FROM es.range_start
                  AND cs.range_end IS NOT DISTINCT FROM es.range_end
                ) AS exact
         FROM giga_candidate_sources cs
         JOIN giga_event_sources es
           ON es.event_id=cs.event_id
          AND es.source_type=cs.source_type
          AND es.source_id=cs.source_id
         WHERE cs.candidate_id=$1
         ORDER BY es.source_type,es.source_id",
    )
    .bind(candidate_id)
    .fetch_all(&mut **tx)
    .await?;
    if rows.is_empty()
        || rows
            .iter()
            .any(|row| !row.try_get::<bool, _>("exact").unwrap_or(false))
    {
        return Err(AppError::Invalid(
            "GIGA candidate sources no longer exactly match the parent event".into(),
        ));
    }
    rows.iter().map(source_from_row).collect()
}
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

fn typed_promotion_receipt(
    request: &GigaPromotionRequest,
    durable_id: u64,
    committed_at: String,
) -> Result<GigaPromotionReceipt, AppError> {
    let receipt = match request.payload() {
        GigaPromotionPayload::Memory(_) => GigaPromotionReceipt::memory(
            request.candidate_id().into(),
            durable_id,
            request.room().clone(),
            request.reviewer_id().into(),
            request.operator_identity().into(),
            request.reviewed_at().into(),
            committed_at,
        ),
        GigaPromotionPayload::CodingLesson(_) => GigaPromotionReceipt::coding_lesson(
            request.candidate_id().into(),
            durable_id,
            request.room().to_string(),
            request.reviewer_id().into(),
            request.operator_identity().into(),
            request.reviewed_at().into(),
            committed_at,
        ),
        GigaPromotionPayload::ProjectLesson { payload, .. } => {
            GigaPromotionReceipt::project_lesson(
                request.candidate_id().into(),
                durable_id,
                payload.project().into(),
                request.reviewer_id().into(),
                request.operator_identity().into(),
                request.reviewed_at().into(),
                committed_at,
            )
        }
    };
    receipt.map_err(domain_error)
}

fn idempotent_promotion_receipt(
    request: &GigaPromotionRequest,
    row: &PgRow,
) -> Result<GigaPromotionReceipt, AppError> {
    let target: Json<Value> = row.try_get("promotion_target")?;
    let kind = target
        .0
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Invalid("stored GIGA promotion target is invalid".into()))?;
    let durable_id = target
        .0
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::Invalid("stored GIGA promotion target is invalid".into()))?;
    let committed_at: DateTime<Utc> = row
        .try_get::<Option<DateTime<Utc>>, _>("committed_at")?
        .ok_or_else(|| AppError::Invalid("stored GIGA promotion receipt is incomplete".into()))?;
    let stored_kind = GigaPromotionKind::parse(kind).map_err(domain_error)?;
    if stored_kind != request.payload().kind() {
        return Err(AppError::Invalid(
            "stored GIGA promotion kind does not match the authorized request".into(),
        ));
    }
    typed_promotion_receipt(request, durable_id, committed_at.to_rfc3339())
}

pub async fn giga_promote(
    pool: &PgPool,
    cfg: &Config,
    request: GigaPromotionRequest,
) -> Result<GigaPromotionReceipt, AppError> {
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

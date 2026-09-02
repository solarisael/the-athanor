use super::error::domain_error;
use super::event_ingest::giga_event_ingest;
use crate::AppError;
use hearth::{
    GigaEvent, GigaEventType, GigaLifecycle, GigaScope, GigaSourceRef, GigaSourceType,
    GigaVisibility, RoomKey,
};
use protocol::{GigaConversationIngestParams, GigaEventIngestResult};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashSet;

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

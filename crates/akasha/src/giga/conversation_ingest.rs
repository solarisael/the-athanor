use super::error::domain_error;
use super::event_ingest::insert_event;
use crate::AppError;
use hearth::{
    GigaEvent, GigaEventType, GigaLifecycle, GigaScope, GigaSourceRef, GigaSourceType,
    GigaVisibility, RoomKey,
};
use protocol::{
    GIGA_CONVERSATION_INGEST_MAX_TURNS, GigaConversationIngestParams, GigaConversationIngestResult,
    GigaConversationTurnParams,
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

/// Id of the one queue job that classifies a captured turn. Text values cannot
/// hold NUL, so the separators keep distinct identities apart. Migration 0043
/// derives the same id in SQL for turns recovered from legacy windows.
fn turn_event_id(room: &RoomKey, session_id: &str, source_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"giga_turn_v1\0");
    digest.update(room.as_str().as_bytes());
    digest.update(b"\0");
    digest.update(session_id.as_bytes());
    digest.update(b"\0");
    digest.update(source_id.as_bytes());
    format!("{:x}", digest.finalize())
}

/// One captured turn as its canonical single-source queue event.
fn turn_event(
    room: &RoomKey,
    project_keys: &[String],
    turn: &GigaConversationTurnParams,
) -> Result<GigaEvent, AppError> {
    if !turn.has_stable_id
        || !matches!(turn.role.as_str(), "user" | "assistant")
        || turn.session_id.contains('\0')
        || turn.source_id.contains('\0')
    {
        return Err(AppError::Invalid(
            "GIGA conversation turns require stable identities and user/assistant roles".into(),
        ));
    }
    let scope = GigaScope::new(
        Some(room.to_string()),
        project_keys.first().cloned(),
        GigaVisibility::Private,
        true,
    )
    .map_err(domain_error)?;
    let source = GigaSourceRef::new(
        GigaSourceType::Turn,
        turn.source_id.clone(),
        turn.role.clone(),
        turn.timestamp.clone(),
        turn.content_hash.to_ascii_lowercase(),
        scope,
        None,
    )
    .map_err(domain_error)?;
    GigaEvent::new(
        turn_event_id(room, &turn.session_id, &turn.source_id),
        GigaEventType::ConversationWindow,
        room.clone(),
        turn.session_id.clone(),
        project_keys.to_vec(),
        vec![source],
        GigaLifecycle::conversation_window(),
        turn.timestamp.clone(),
    )
    .map_err(domain_error)
}

/// Persists each captured turn as its own waiting queue job, keyed by
/// room + session + source. A repeated identity is a duplicate only when its
/// role, content hash, and project scope agree; any disagreement refuses the
/// whole call, so a capture is stored completely or not at all.
pub async fn giga_conversation_ingest(
    pool: &PgPool,
    request: GigaConversationIngestParams,
) -> Result<GigaConversationIngestResult, AppError> {
    if request.turns.is_empty() || request.turns.len() > GIGA_CONVERSATION_INGEST_MAX_TURNS {
        return Err(AppError::Invalid(format!(
            "GIGA conversation capture must contain between one and {GIGA_CONVERSATION_INGEST_MAX_TURNS} turns"
        )));
    }
    if request.project_keys.len() > 1 {
        return Err(AppError::Invalid(
            "GIGA conversation capture accepts at most one project key".into(),
        ));
    }
    let room = RoomKey::new(request.room).map_err(domain_error)?;
    let events = request
        .turns
        .iter()
        .map(|turn| turn_event(&room, &request.project_keys, turn))
        .collect::<Result<Vec<_>, _>>()?;
    // One identity order for every caller, so overlapping captures queue on
    // the same row first instead of deadlocking.
    let mut order = (0..events.len()).collect::<Vec<_>>();
    order.sort_by(|&left, &right| {
        let (left, right) = (&events[left], &events[right]);
        (left.session_id(), left.source_refs()[0].source_id())
            .cmp(&(right.session_id(), right.source_refs()[0].source_id()))
    });

    let mut tx = pool.begin().await?;
    let mut owners: Vec<Option<String>> = vec![None; events.len()];
    let mut accepted_turns = 0;
    for index in order {
        let event = &events[index];
        let source = &event.source_refs()[0];
        let project = source.scope().project();
        let recorded = sqlx::query(
            "INSERT INTO giga_turn_identities
             (room,session_id,source_id,source_role,content_hash,scope_project,origin)
             VALUES ($1,$2,$3,$4,$5,$6,'capture')
             ON CONFLICT (room,session_id,source_id) DO NOTHING",
        )
        .bind(room.as_str())
        .bind(event.session_id())
        .bind(source.source_id())
        .bind(source.role())
        .bind(source.content_hash())
        .bind(project)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        if recorded {
            if !insert_event(&mut tx, event, true).await? {
                return Err(AppError::Invalid(
                    "GIGA turn job already exists without its turn identity".into(),
                ));
            }
            sqlx::query(
                "UPDATE giga_turn_identities SET event_id=$4
                 WHERE room=$1 AND session_id=$2 AND source_id=$3",
            )
            .bind(room.as_str())
            .bind(event.session_id())
            .bind(source.source_id())
            .bind(event.event_id())
            .execute(&mut *tx)
            .await?;
            accepted_turns += 1;
            owners[index] = Some(event.event_id().to_owned());
            continue;
        }
        let stored = sqlx::query(
            "SELECT state,source_role,content_hash,scope_project,event_id
             FROM giga_turn_identities WHERE room=$1 AND session_id=$2 AND source_id=$3",
        )
        .bind(room.as_str())
        .bind(event.session_id())
        .bind(source.source_id())
        .fetch_one(&mut *tx)
        .await?;
        if stored.try_get::<String, _>("state")? == "conflict" {
            return Err(AppError::Invalid(
                "GIGA turn identity holds conflicting historical evidence".into(),
            ));
        }
        if stored
            .try_get::<Option<String>, _>("source_role")?
            .as_deref()
            != Some(source.role())
            || stored
                .try_get::<Option<String>, _>("content_hash")?
                .as_deref()
                != Some(source.content_hash())
            || stored
                .try_get::<Option<String>, _>("scope_project")?
                .as_deref()
                != project
        {
            return Err(AppError::Invalid(
                "GIGA turn identity was captured with a different role, content hash, or project"
                    .into(),
            ));
        }
        // A retired identity (its job was purged) stays a duplicate without an owner.
        owners[index] = stored.try_get("event_id")?;
    }
    tx.commit().await?;

    let mut event_ids: Vec<String> = Vec::with_capacity(owners.len());
    for event_id in owners.into_iter().flatten() {
        if !event_ids.contains(&event_id) {
            event_ids.push(event_id);
        }
    }
    Ok(GigaConversationIngestResult {
        accepted_turns,
        duplicate_turns: events.len() - accepted_turns,
        event_ids,
    })
}

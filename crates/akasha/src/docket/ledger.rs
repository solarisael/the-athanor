use crate::config::AppError;
use serde_json::Value;
use sqlx::{Postgres, Transaction};

pub(super) async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    quest_id: &str,
    attempt_id: Option<&str>,
    event_kind: &str,
    principal: &str,
    detail: Value,
    idempotency_key: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO docket.quest_events (quest_id,attempt_id,event_kind,principal,detail,idempotency_key) VALUES ($1::text::uuid,$2::text::uuid,$3,$4,$5,$6)",
    )
    .bind(quest_id)
    .bind(attempt_id)
    .bind(event_kind)
    .bind(principal)
    .bind(detail)
    .bind(idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Goal transitions ledger through the same append-only relation; they have
/// no attempt, so the helper stays deliberately asymmetric to insert_event.
/// A replayed goalDraft reuses its mint key: the conflict target makes the
/// second ledger insert a no-op instead of an error.
pub(super) async fn insert_goal_event(
    tx: &mut Transaction<'_, Postgres>,
    goal_id: &str,
    event_kind: &str,
    principal: &str,
    detail: Value,
    idempotency_key: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO docket.quest_events (goal_id,event_kind,principal,detail,idempotency_key) VALUES ($1::text::uuid,$2,$3,$4,$5) ON CONFLICT (goal_id, idempotency_key) WHERE idempotency_key IS NOT NULL AND goal_id IS NOT NULL DO NOTHING",
    )
    .bind(goal_id)
    .bind(event_kind)
    .bind(principal)
    .bind(detail)
    .bind(idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

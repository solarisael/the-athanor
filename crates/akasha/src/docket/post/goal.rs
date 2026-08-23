use crate::config::AppError;
use crate::docket::ledger::insert_goal_event;
use crate::docket::validate::{principal, refusal, required};
use serde_json::json;
use sqlx::{Postgres, Row, Transaction};
use super::QuestPostResult;
use super::params::QuestPostParams;

pub(super) async fn post_goal_draft(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestPostParams,
) -> Result<QuestPostResult, AppError> {
    // Replay fence: one goal per (house, mint key). A replayed goalDraft
    // returns the existing goal instead of minting a twin; the no-op update
    // is what makes RETURNING yield the surviving row.
    let goal_id: String = sqlx::query_scalar(
        "INSERT INTO docket.goals (house_id,title,intent,priority,recurrence_interval,idempotency_key) VALUES ($1,$2,$3,$4,CASE WHEN $5::text IS NULL THEN NULL ELSE $5::interval END,$6) ON CONFLICT (house_id, idempotency_key) WHERE idempotency_key IS NOT NULL DO UPDATE SET updated_at = docket.goals.updated_at RETURNING goal_id::text",
    )
    .bind(required(&request.house_id, "houseId")?)
    .bind(required(&request.title, "title")?)
    .bind(required(&request.intent, "intent")?)
    .bind(request.priority.unwrap_or(0))
    .bind(request.recurrence_interval.as_deref())
    .bind(&request.idempotency_key)
    .fetch_one(&mut **tx)
    .await?;
    insert_goal_event(
        tx,
        &goal_id,
        "goal_drafted",
        &principal(&request.room, &request.spirit),
        json!({"action": "goalDraft"}),
        Some(&request.idempotency_key),
    )
    .await?;
    Ok(QuestPostResult {
        ok: true,
        action: request.action,
        goal_id: Some(goal_id),
        quest_id: None,
        state: "draft".into(),
    })
}

pub(super) async fn post_goal_activate(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestPostParams,
) -> Result<QuestPostResult, AppError> {
    let goal_id = required(&request.goal_id, "goalId")?;
    let row =
        sqlx::query("SELECT status FROM docket.goals WHERE goal_id=$1::text::uuid FOR UPDATE")
            .bind(goal_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| refusal("unknown_quest", "the requested goal does not exist"))?;
    let status: String = row.try_get("status")?;
    if !matches!(status.as_str(), "draft" | "offered") {
        return Err(AppError::Invalid("goal is not activatable".into()));
    }
    sqlx::query(
        "UPDATE docket.goals SET status='active',intent_authority_principal=$2,steward_room=$3,steward_spirit=$4,activated_at=NOW(),updated_at=NOW() WHERE goal_id=$1::text::uuid",
    )
    .bind(goal_id)
    .bind(required(
        &request.intent_authority_principal,
        "intentAuthorityPrincipal",
    )?)
    .bind(required(&request.steward_room, "stewardRoom")?)
    .bind(required(&request.steward_spirit, "stewardSpirit")?)
    .execute(&mut **tx)
    .await?;
    insert_goal_event(
        tx,
        goal_id,
        "goal_activated",
        &principal(&request.room, &request.spirit),
        json!({"action": "goalActivate"}),
        Some(&request.idempotency_key),
    )
    .await?;
    Ok(QuestPostResult {
        ok: true,
        action: request.action,
        goal_id: Some(goal_id.to_owned()),
        quest_id: None,
        state: "active".into(),
    })
}

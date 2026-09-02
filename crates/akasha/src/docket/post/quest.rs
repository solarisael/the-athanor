use super::QuestPostResult;
use super::params::QuestPostParams;
use crate::config::AppError;
use crate::docket::digest::sha256_hex;
use crate::docket::ledger::insert_event;
use crate::docket::validate::{principal, refusal, required};
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{Postgres, Row, Transaction};

pub(super) async fn post_quest_draft(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestPostParams,
) -> Result<QuestPostResult, AppError> {
    let deadline = request
        .deadline_at
        .as_deref()
        .map(DateTime::parse_from_rfc3339)
        .transpose()
        .map_err(|_| AppError::Invalid("deadlineAt must be RFC3339".into()))?
        .map(|value| value.with_timezone(&Utc));
    let quest_id: String = sqlx::query_scalar(
        "INSERT INTO docket.quests (house_id,goal_id,kind,title,body,importance,deadline_at,posted_by_room,posted_by_spirit) VALUES ($1,$2::text::uuid,$3,$4,$5,$6,$7,$8,$9) RETURNING quest_id::text",
    )
    .bind(required(&request.house_id, "houseId")?)
    .bind(request.goal_id.as_deref())
    .bind(required(&request.kind, "kind")?)
    .bind(required(&request.title, "title")?)
    .bind(required(&request.body, "body")?)
    .bind(request.importance.as_deref().unwrap_or("hint"))
    .bind(deadline)
    .bind(&request.room)
    .bind(&request.spirit)
    .fetch_one(&mut **tx)
    .await?;
    insert_event(
        tx,
        &quest_id,
        None,
        "drafted",
        &principal(&request.room, &request.spirit),
        json!({"action": "draft"}),
        Some(&request.idempotency_key),
    )
    .await?;
    Ok(QuestPostResult {
        ok: true,
        action: request.action,
        goal_id: request.goal_id.clone(),
        quest_id: Some(quest_id),
        state: "draft".into(),
    })
}

pub(super) async fn post_quest_activate(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestPostParams,
) -> Result<QuestPostResult, AppError> {
    let quest_id = required(&request.quest_id, "questId")?;
    let row = sqlx::query(
        "SELECT goal_id::text AS goal_id,state FROM docket.quests WHERE quest_id=$1::text::uuid FOR UPDATE",
    )
    .bind(quest_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| refusal("unknown_quest", "the requested quest does not exist"))?;
    let state: String = row.try_get("state")?;
    if state != "draft" {
        return Err(AppError::Invalid("quest is not activatable".into()));
    }

    let criteria = request
        .acceptance_criteria
        .as_ref()
        .ok_or_else(|| AppError::Invalid("acceptanceCriteria is required".into()))?;
    let acceptance_policy = json!({"criteria": criteria});
    let canonical = serde_json::to_vec(&acceptance_policy)
        .map_err(|error| AppError::Protocol(error.to_string()))?;
    let digest = sha256_hex(&canonical);
    let deadline = request
        .deadline_at
        .as_deref()
        .map(DateTime::parse_from_rfc3339)
        .transpose()
        .map_err(|_| AppError::Invalid("deadlineAt must be RFC3339".into()))?
        .map(|value| value.with_timezone(&Utc));
    // An omitted deadlineAt preserves the drafted deadline; only an explicit
    // value replaces it. Overwriting with NULL would silently unclock a quest.
    sqlx::query(
        "UPDATE docket.quests SET state='offered',intent_authority_principal=$2,acceptance_policy=$3,acceptance_policy_digest=$4,review_class=$5,importance=$6,deadline_at=COALESCE($7,deadline_at),activated_at=NOW(),updated_at=NOW() WHERE quest_id=$1::text::uuid",
    )
    .bind(quest_id)
    .bind(required(
        &request.intent_authority_principal,
        "intentAuthorityPrincipal",
    )?)
    .bind(&acceptance_policy)
    .bind(&digest)
    .bind(required(&request.review_class, "reviewClass")?)
    .bind(required(&request.importance, "importance")?)
    .bind(deadline)
    .execute(&mut **tx)
    .await?;
    for (index, criterion) in criteria.iter().enumerate() {
        sqlx::query(
            "INSERT INTO docket.quest_acceptance_items (quest_id,position,criterion) VALUES ($1::text::uuid,$2,$3)",
        )
        .bind(quest_id)
        .bind(i32::try_from(index + 1).map_err(|_| {
            AppError::Invalid("acceptanceCriteria contains too many items".into())
        })?)
        .bind(criterion)
        .execute(&mut **tx)
        .await?;
    }
    insert_event(
        tx,
        quest_id,
        None,
        "activated",
        &principal(&request.room, &request.spirit),
        json!({"reviewClass": request.review_class, "acceptancePolicyDigest": digest}),
        Some(&request.idempotency_key),
    )
    .await?;
    Ok(QuestPostResult {
        ok: true,
        action: request.action,
        goal_id: row.try_get("goal_id")?,
        quest_id: Some(quest_id.to_owned()),
        state: "offered".into(),
    })
}

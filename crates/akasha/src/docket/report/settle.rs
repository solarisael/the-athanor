use crate::config::AppError;
use crate::docket::ledger::insert_event;
use crate::docket::validate::principal;
use serde_json::json;
use sqlx::{Postgres, Row, Transaction};
use super::QuestReportResult;
use super::params::QuestReportParams;

const SETTLED_VERDICTS: &[&str] = &["met", "not_applicable"];
pub(super) async fn report_settle_item(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestReportParams,
) -> Result<QuestReportResult, AppError> {
    let position = request
        .item_position
        .ok_or_else(|| AppError::Invalid("itemPosition is required".into()))?;
    let verdict = request
        .verdict
        .as_deref()
        .ok_or_else(|| AppError::Invalid("verdict is required".into()))?;
    let item = sqlx::query(
        "SELECT verdict FROM docket.quest_acceptance_items WHERE quest_id=$1::text::uuid AND position=$2 FOR UPDATE",
    )
    .bind(&request.quest_id)
    .bind(position)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::Invalid("itemPosition does not name an acceptance item".into()))?;
    let previous: String = item.try_get("verdict")?;
    if previous != "pending" {
        return Err(AppError::Invalid(
            "acceptance item is already settled".into(),
        ));
    }
    let role = request.authored_role.as_deref().unwrap_or("executor");
    sqlx::query(
        "UPDATE docket.quest_acceptance_items SET verdict=$3,settled_by_role=$4,settled_by_room=$5,settled_by_spirit=$6,settled_at=NOW() WHERE quest_id=$1::text::uuid AND position=$2",
    )
    .bind(&request.quest_id)
    .bind(position)
    .bind(verdict)
    .bind(role)
    .bind(&request.room)
    .bind(&request.spirit)
    .execute(&mut **tx)
    .await?;
    insert_event(
        tx,
        &request.quest_id,
        Some(&request.attempt_id),
        "item_settled",
        &principal(&request.room, &request.spirit),
        json!({"position": position, "verdict": verdict, "role": role, "body": request.body}),
        Some(&request.idempotency_key),
    )
    .await?;

    let quest_state: String =
        sqlx::query_scalar("SELECT state FROM docket.quests WHERE quest_id=$1::text::uuid")
            .bind(&request.quest_id)
            .fetch_one(&mut **tx)
            .await?;
    let all_accepted: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM docket.quest_acceptance_items WHERE quest_id=$1::text::uuid AND verdict <> ALL($2::text[]))",
    )
    .bind(&request.quest_id)
    .bind(SETTLED_VERDICTS)
    .fetch_one(&mut **tx)
    .await?;
    let mut settled = false;
    let mut rearmed_quest_id = None;
    let final_state = if quest_state == "submitted" && all_accepted {
        sqlx::query(
            "UPDATE docket.quests SET state='settled',settled_at=NOW(),updated_at=NOW() WHERE quest_id=$1::text::uuid",
        )
        .bind(&request.quest_id)
        .execute(&mut **tx)
        .await?;
        insert_event(
            tx,
            &request.quest_id,
            Some(&request.attempt_id),
            "settled",
            &principal(&request.room, &request.spirit),
            json!({"settledByRole": role}),
            Some(&format!("settled:{}", request.idempotency_key)),
        )
        .await?;
        rearmed_quest_id = rearm_recurrent_quest(tx, request).await?;
        settled = true;
        "settled".to_owned()
    } else {
        quest_state
    };
    Ok(QuestReportResult {
        ok: true,
        action: request.action,
        receipt_id: None,
        quest_state: final_state,
        attempt_state: "yielded".into(),
        settled,
        rearmed_quest_id,
        lease_expires_at: None,
    })
}

async fn rearm_recurrent_quest(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestReportParams,
) -> Result<Option<String>, AppError> {
    // The re-armed occurrence must stay clock-visible: a NULL prior deadline
    // re-arms from NOW(), never NULL + interval = NULL (silent recurrence
    // death — the ping only speaks about due items).
    let new_quest_id: Option<String> = sqlx::query_scalar(
        "INSERT INTO docket.quests (house_id,goal_id,parent_quest_id,kind,title,body,authority_ceiling,required_capabilities,acceptance_policy,acceptance_policy_digest,review_class,settlement_policy,importance,deadline_at,intent_authority_principal,posted_by_room,posted_by_spirit,state,revision,activated_at) SELECT q.house_id,q.goal_id,q.quest_id,q.kind,q.title,q.body,q.authority_ceiling,q.required_capabilities,q.acceptance_policy,q.acceptance_policy_digest,q.review_class,q.settlement_policy,q.importance,COALESCE(q.deadline_at,NOW())+g.recurrence_interval,q.intent_authority_principal,q.posted_by_room,q.posted_by_spirit,'offered',q.revision,NOW() FROM docket.quests q JOIN docket.goals g ON g.goal_id=q.goal_id WHERE q.quest_id=$1::text::uuid AND g.recurrence_interval IS NOT NULL RETURNING quest_id::text",
    )
    .bind(&request.quest_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(new_quest_id) = new_quest_id else {
        return Ok(None);
    };
    sqlx::query(
        "INSERT INTO docket.quest_acceptance_items (quest_id,position,criterion) SELECT $1::text::uuid,position,criterion FROM docket.quest_acceptance_items WHERE quest_id=$2::text::uuid ORDER BY position",
    )
    .bind(&new_quest_id)
    .bind(&request.quest_id)
    .execute(&mut **tx)
    .await?;
    insert_event(
        tx,
        &request.quest_id,
        Some(&request.attempt_id),
        "rearmed",
        &principal(&request.room, &request.spirit),
        json!({"newQuestId": new_quest_id}),
        Some(&format!("rearm:{}", request.quest_id)),
    )
    .await?;
    Ok(Some(new_quest_id))
}

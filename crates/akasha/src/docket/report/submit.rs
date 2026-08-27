use crate::config::AppError;
use crate::docket::ledger::insert_event;
use crate::docket::validate::principal;
use serde_json::json;
use sqlx::{Postgres, Transaction};
use super::QuestReportResult;
use super::params::QuestReportParams;
use super::receipt::insert_receipt;

pub(super) async fn report_submit(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestReportParams,
) -> Result<QuestReportResult, AppError> {
    let receipt_id = insert_receipt(
        tx,
        request,
        request.kind.as_deref().unwrap_or("submission"),
        request.authored_role.as_deref().unwrap_or("executor"),
    )
    .await?;
    sqlx::query(
        "UPDATE docket.quests SET state='submitted',updated_at=NOW() WHERE quest_id=$1::text::uuid",
    )
    .bind(&request.quest_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE docket.quest_attempts SET state='yielded',ended_at=NOW() WHERE attempt_id=$1::text::uuid",
    )
    .bind(&request.attempt_id)
    .execute(&mut **tx)
    .await?;
    insert_event(
        tx,
        &request.quest_id,
        Some(&request.attempt_id),
        "submitted",
        &principal(&request.room, &request.spirit),
        json!({"receiptId": receipt_id}),
        Some(&request.idempotency_key),
    )
    .await?;
    Ok(QuestReportResult {
        ok: true,
        action: request.action,
        receipt_id: Some(receipt_id),
        quest_state: "submitted".into(),
        attempt_state: "yielded".into(),
        settled: false,
        rearmed_quest_id: None,
        lease_expires_at: None,
    })
}

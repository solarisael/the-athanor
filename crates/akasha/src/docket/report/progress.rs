use crate::config::AppError;
use crate::docket::ledger::insert_event;
use crate::docket::validate::principal;
use serde_json::json;
use sqlx::{Postgres, Transaction};
use super::QuestReportResult;
use super::params::QuestReportParams;
use super::receipt::insert_receipt;

pub(super) async fn report_progress(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestReportParams,
) -> Result<QuestReportResult, AppError> {
    let receipt_id = insert_receipt(
        tx,
        request,
        request.kind.as_deref().unwrap_or("progress"),
        request.authored_role.as_deref().unwrap_or("executor"),
    )
    .await?;
    insert_event(
        tx,
        &request.quest_id,
        Some(&request.attempt_id),
        "progress",
        &principal(&request.room, &request.spirit),
        json!({"receiptId": receipt_id}),
        Some(&request.idempotency_key),
    )
    .await?;
    let quest_state: String =
        sqlx::query_scalar("SELECT state FROM docket.quests WHERE quest_id=$1::text::uuid")
            .bind(&request.quest_id)
            .fetch_one(&mut **tx)
            .await?;
    Ok(QuestReportResult {
        ok: true,
        action: request.action,
        receipt_id: Some(receipt_id),
        quest_state,
        attempt_state: "active".into(),
        settled: false,
        rearmed_quest_id: None,
        lease_expires_at: None,
    })
}

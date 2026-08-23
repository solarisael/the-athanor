use crate::config::AppError;
use crate::docket::digest::sha256_hex;
use sqlx::{Postgres, Transaction};
use super::params::QuestReportParams;

pub(super) async fn insert_receipt(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestReportParams,
    kind: &str,
    role: &str,
) -> Result<String, AppError> {
    let digest = sha256_hex(request.body.as_bytes());
    let receipt_id: String = sqlx::query_scalar(
        "INSERT INTO docket.quest_receipts (quest_id,attempt_id,kind,body,digest,submitted_by_room,submitted_by_spirit,performed_by,authored_role,idempotency_key) VALUES ($1::text::uuid,$2::text::uuid,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING receipt_id::text",
    )
    .bind(&request.quest_id)
    .bind(&request.attempt_id)
    .bind(kind)
    .bind(&request.body)
    .bind(digest)
    .bind(&request.room)
    .bind(&request.spirit)
    .bind(request.performed_by.as_deref())
    .bind(role)
    .bind(&request.idempotency_key)
    .fetch_one(&mut **tx)
    .await?;
    Ok(receipt_id)
}

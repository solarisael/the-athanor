use super::error::domain_error;
use crate::AppError;
use chrono::{DateTime, Utc};
use hearth::{GigaPromotionKind, GigaPromotionPayload, GigaPromotionReceipt, GigaPromotionRequest};
use serde_json::Value;
use sqlx::{Row, postgres::PgRow, types::Json};

pub(super) fn typed_promotion_receipt(
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

pub(super) fn idempotent_promotion_receipt(
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

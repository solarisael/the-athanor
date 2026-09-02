use super::validate::{refusal, validate_identity, validate_uuid};
use crate::config::AppError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QuestEvidenceParams {
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub quest_id: String,
    #[serde(default = "default_evidence_limit")]
    pub limit: u32,
}

fn default_evidence_limit() -> u32 {
    50
}

impl QuestEvidenceParams {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_identity(&self.room, &self.spirit, &self.session)?;
        validate_uuid(&self.quest_id, "questId")?;
        if !(1..=200).contains(&self.limit) {
            return Err(AppError::Invalid(
                "limit must be an integer from 1 through 200".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestEvidenceReceipt {
    pub receipt_id: String,
    pub attempt_id: Option<String>,
    pub kind: String,
    pub body: String,
    pub submitted_by_room: String,
    pub submitted_by_spirit: String,
    pub performed_by: Option<String>,
    pub authored_role: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestEvidenceEvent {
    pub event_kind: String,
    pub principal: String,
    pub detail: Option<Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestEvidenceItem {
    pub position: i32,
    pub criterion: String,
    pub verdict: Option<String>,
    pub settled_by_room: Option<String>,
    pub settled_by_spirit: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestEvidenceResult {
    pub ok: bool,
    pub quest_id: String,
    pub receipts: Vec<QuestEvidenceReceipt>,
    pub events: Vec<QuestEvidenceEvent>,
    pub acceptance_items: Vec<QuestEvidenceItem>,
}

/// Read one quest's evidence: full receipt bodies, ledger events, and
/// acceptance items with their verdicts. A read with no capability, like the
/// board — an independent reviewer judges primary receipts, never a
/// claimant's summary of them (guild-hall #171).
pub async fn quest_evidence(
    pool: &PgPool,
    request: QuestEvidenceParams,
) -> Result<QuestEvidenceResult, AppError> {
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT quest_id::text FROM docket.quests WHERE quest_id=$1::text::uuid",
    )
    .bind(&request.quest_id)
    .fetch_optional(pool)
    .await?;
    if exists.is_none() {
        return Err(refusal(
            "unknown_quest",
            "the requested quest does not exist",
        ));
    }
    let receipts = sqlx::query(
        "SELECT receipt_id::text AS receipt_id,attempt_id::text AS attempt_id,kind,body,submitted_by_room,submitted_by_spirit,performed_by,authored_role,created_at FROM docket.quest_receipts WHERE quest_id=$1::text::uuid ORDER BY created_at ASC LIMIT $2",
    )
    .bind(&request.quest_id)
    .bind(i64::from(request.limit))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        Ok(QuestEvidenceReceipt {
            receipt_id: row.try_get("receipt_id")?,
            attempt_id: row.try_get("attempt_id")?,
            kind: row.try_get("kind")?,
            body: row.try_get("body")?,
            submitted_by_room: row.try_get("submitted_by_room")?,
            submitted_by_spirit: row.try_get("submitted_by_spirit")?,
            performed_by: row.try_get("performed_by")?,
            authored_role: row.try_get("authored_role")?,
            created_at: row.try_get("created_at")?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let events = sqlx::query(
        "SELECT event_kind,principal,detail,created_at FROM docket.quest_events WHERE quest_id=$1::text::uuid ORDER BY created_at ASC LIMIT $2",
    )
    .bind(&request.quest_id)
    .bind(i64::from(request.limit))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        Ok(QuestEvidenceEvent {
            event_kind: row.try_get("event_kind")?,
            principal: row.try_get("principal")?,
            detail: row.try_get("detail")?,
            created_at: row.try_get("created_at")?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let acceptance_items = sqlx::query(
        "SELECT position,criterion,verdict,settled_by_room,settled_by_spirit FROM docket.quest_acceptance_items WHERE quest_id=$1::text::uuid ORDER BY position ASC",
    )
    .bind(&request.quest_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        Ok(QuestEvidenceItem {
            position: row.try_get("position")?,
            criterion: row.try_get("criterion")?,
            verdict: row.try_get("verdict")?,
            settled_by_room: row.try_get("settled_by_room")?,
            settled_by_spirit: row.try_get("settled_by_spirit")?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(QuestEvidenceResult {
        ok: true,
        quest_id: request.quest_id,
        receipts,
        events,
        acceptance_items,
    })
}

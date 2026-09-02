use super::validate::{refusal, validate_identity, validate_uuid};
use crate::config::AppError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QuestChargebookParams {
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub quest_id: String,
    pub attempt_id: String,
}

impl QuestChargebookParams {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_identity(&self.room, &self.spirit, &self.session)?;
        validate_uuid(&self.quest_id, "questId")?;
        validate_uuid(&self.attempt_id, "attemptId")
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestChargebookRow {
    pub component: String,
    pub operation: String,
    pub events: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub bytes_in: i64,
    pub bytes_out: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestChargebookTotals {
    pub events: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub bytes_in: i64,
    pub bytes_out: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestChargebookResult {
    pub ok: bool,
    pub quest_id: String,
    pub attempt_id: String,
    pub claim_epoch: i64,
    pub claimant_room: String,
    pub claimant_spirit: String,
    /// The attempt's session lineage. Token volume is evidence of cost,
    /// never of merit (guild-hall #142 plane 2).
    pub sessions: Vec<String>,
    pub window_from: DateTime<Utc>,
    pub window_to: DateTime<Utc>,
    pub totals: QuestChargebookTotals,
    pub by_operation: Vec<QuestChargebookRow>,
}

/// Derive one attempt's chargebook over Insula: token and byte volume for
/// the attempt's session lineage inside the attempt window. A read with no
/// capability, like the board: it observes cost and grants nothing.
pub async fn quest_chargebook(
    pool: &PgPool,
    request: QuestChargebookParams,
) -> Result<QuestChargebookResult, AppError> {
    let attempt = sqlx::query(
        "SELECT claim_epoch,claimant_room,claimant_spirit,session_id,started_at FROM docket.quest_attempts WHERE attempt_id=$1::text::uuid AND quest_id=$2::text::uuid",
    )
    .bind(&request.attempt_id)
    .bind(&request.quest_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| refusal("unknown_attempt", "the requested attempt does not exist"))?;
    let session_id: String = attempt.try_get("session_id")?;
    let window_from: DateTime<Utc> = attempt.try_get("started_at")?;
    // enough: the window closes at NOW(); attempts carry no ended_at yet.
    let window_to = Utc::now();
    // enough: lineage is the claiming session only; dispatched worker
    // sessions join when dispatch packets stamp attempt_id (guild-hall #146).
    let sessions = vec![session_id];

    let rows = sqlx::query(
        "SELECT component,operation,COUNT(*) AS events,SUM(tokens_in)::bigint AS tokens_in,SUM(tokens_out)::bigint AS tokens_out,SUM(bytes_in)::bigint AS bytes_in,SUM(bytes_out)::bigint AS bytes_out FROM insula.log WHERE session_id = ANY($1) AND observed_at >= $2 AND observed_at <= $3 GROUP BY component,operation ORDER BY component,operation",
    )
    .bind(&sessions)
    .bind(window_from)
    .bind(window_to)
    .fetch_all(pool)
    .await?;
    let by_operation = rows
        .iter()
        .map(|row| {
            Ok(QuestChargebookRow {
                component: row.try_get("component")?,
                operation: row.try_get("operation")?,
                events: row.try_get("events")?,
                tokens_in: row.try_get("tokens_in")?,
                tokens_out: row.try_get("tokens_out")?,
                bytes_in: row.try_get("bytes_in")?,
                bytes_out: row.try_get("bytes_out")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let totals = by_operation.iter().fold(
        QuestChargebookTotals {
            events: 0,
            tokens_in: 0,
            tokens_out: 0,
            bytes_in: 0,
            bytes_out: 0,
        },
        |mut totals, row| {
            totals.events += row.events;
            totals.tokens_in += row.tokens_in;
            totals.tokens_out += row.tokens_out;
            totals.bytes_in += row.bytes_in;
            totals.bytes_out += row.bytes_out;
            totals
        },
    );
    Ok(QuestChargebookResult {
        ok: true,
        quest_id: request.quest_id,
        attempt_id: request.attempt_id,
        claim_epoch: attempt.try_get("claim_epoch")?,
        claimant_room: attempt.try_get("claimant_room")?,
        claimant_spirit: attempt.try_get("claimant_spirit")?,
        sessions,
        window_from,
        window_to,
        totals,
        by_operation,
    })
}

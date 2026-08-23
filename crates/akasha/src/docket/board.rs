use crate::config::AppError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use super::validate::{nonempty, validate_identity};

const QUEST_STATES: &[&str] = &[
    "draft",
    "offered",
    "claimed",
    "submitted",
    "settled",
    "refused",
    "blocked",
    "quarantined",
    "cancelled",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QuestBoardParams {
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub house_id: String,
    #[serde(default)]
    pub states: Vec<String>,
    #[serde(default = "default_board_limit")]
    pub limit: u32,
}

impl QuestBoardParams {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_identity(&self.room, &self.spirit, &self.session)?;
        nonempty(&self.house_id, "houseId")?;
        if !(1..=100).contains(&self.limit) {
            return Err(AppError::Invalid(
                "limit must be an integer from 1 through 100".into(),
            ));
        }
        if self
            .states
            .iter()
            .any(|state| !QUEST_STATES.contains(&state.as_str()))
        {
            return Err(AppError::Invalid("states contains an unknown state".into()));
        }
        Ok(())
    }
}

fn default_board_limit() -> u32 {
    50
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceSummary {
    pub met: i64,
    pub not_met: i64,
    pub not_applicable: i64,
    pub pending: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestBoardItem {
    pub quest_id: String,
    pub goal_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub importance: String,
    pub deadline_at: Option<DateTime<Utc>>,
    pub state: String,
    pub claim_epoch: i64,
    pub acceptance: AcceptanceSummary,
    /// Current-epoch attempt, when one exists: how a reviewer or a panel
    /// addresses the work precisely from outside the claimant room.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimant_room: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_state: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct QuestBoardResult {
    pub ok: bool,
    pub quests: Vec<QuestBoardItem>,
}

pub async fn quest_board(
    pool: &PgPool,
    request: QuestBoardParams,
) -> Result<QuestBoardResult, AppError> {
    let rows = sqlx::query(
        "SELECT q.quest_id::text AS quest_id,q.goal_id::text AS goal_id,q.kind,q.title,q.body,q.importance,q.deadline_at,q.state,q.claim_epoch,t.attempt_id::text AS attempt_id,t.claimant_room,t.state AS attempt_state,COUNT(a.item_id) FILTER (WHERE a.verdict='met') AS met,COUNT(a.item_id) FILTER (WHERE a.verdict='not_met') AS not_met,COUNT(a.item_id) FILTER (WHERE a.verdict='not_applicable') AS not_applicable,COUNT(a.item_id) FILTER (WHERE a.verdict NOT IN ('met','not_met','not_applicable')) AS pending FROM docket.quests q LEFT JOIN docket.quest_attempts t ON t.quest_id=q.quest_id AND t.claim_epoch=q.claim_epoch LEFT JOIN docket.quest_acceptance_items a ON a.quest_id=q.quest_id WHERE q.house_id=$1 AND (cardinality($2::text[])=0 OR q.state=ANY($2::text[])) GROUP BY q.quest_id,t.attempt_id,t.claimant_room,t.state ORDER BY q.deadline_at ASC NULLS LAST,q.created_at ASC LIMIT $3",
    )
    .bind(&request.house_id)
    .bind(&request.states)
    .bind(i64::from(request.limit))
    .fetch_all(pool)
    .await?;
    let quests = rows
        .into_iter()
        .map(|row| {
            Ok(QuestBoardItem {
                quest_id: row.try_get("quest_id")?,
                goal_id: row.try_get("goal_id")?,
                kind: row.try_get("kind")?,
                title: row.try_get("title")?,
                body: row.try_get("body")?,
                importance: row.try_get("importance")?,
                deadline_at: row.try_get("deadline_at")?,
                state: row.try_get("state")?,
                claim_epoch: row.try_get("claim_epoch")?,
                attempt_id: row.try_get("attempt_id")?,
                claimant_room: row.try_get("claimant_room")?,
                attempt_state: row.try_get("attempt_state")?,
                acceptance: AcceptanceSummary {
                    met: row.try_get("met")?,
                    not_met: row.try_get("not_met")?,
                    not_applicable: row.try_get("not_applicable")?,
                    pending: row.try_get("pending")?,
                },
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(QuestBoardResult { ok: true, quests })
}

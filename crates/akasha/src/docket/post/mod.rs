mod goal;
mod params;
mod quest;

use self::goal::{post_goal_activate, post_goal_draft};
use self::quest::{post_quest_activate, post_quest_draft};
use crate::config::AppError;
use crate::docket::capability::require_docket_capability;
use serde::Serialize;
use sqlx::PgPool;

pub use params::{QuestPostAction, QuestPostParams};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestPostResult {
    pub ok: bool,
    pub action: QuestPostAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quest_id: Option<String>,
    pub state: String,
}

pub async fn quest_post(
    pool: &PgPool,
    request: QuestPostParams,
) -> Result<QuestPostResult, AppError> {
    require_docket_capability(pool, &request.room, &request.capability).await?;
    let mut tx = pool.begin().await?;
    let result = match request.action {
        QuestPostAction::GoalDraft => post_goal_draft(&mut tx, &request).await?,
        QuestPostAction::GoalActivate => post_goal_activate(&mut tx, &request).await?,
        QuestPostAction::Draft => post_quest_draft(&mut tx, &request).await?,
        QuestPostAction::Activate => post_quest_activate(&mut tx, &request).await?,
    };
    tx.commit().await?;
    Ok(result)
}

use crate::config::AppError;
use crate::docket::validate::{nonempty, validate_uuid, validate_write_identity};
use serde::{Deserialize, Serialize};

const ITEM_VERDICTS: &[&str] = &[
    "met",
    "not_met",
    "unknown",
    "inconclusive",
    "not_applicable",
    "refused",
];
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QuestReportAction {
    Progress,
    Submit,
    SettleItem,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QuestReportParams {
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub capability: String,
    pub idempotency_key: String,
    pub quest_id: String,
    pub attempt_id: String,
    /// The claimant's bearer secret. Required by progress and submit; a
    /// settlement authenticates the reviewer instead and takes no token.
    #[serde(default)]
    pub lease_token: Option<String>,
    pub action: QuestReportAction,
    pub body: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub performed_by: Option<String>,
    #[serde(default)]
    pub authored_role: Option<String>,
    #[serde(default)]
    pub item_position: Option<i32>,
    #[serde(default)]
    pub verdict: Option<String>,
}

impl QuestReportParams {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_write_identity(
            &self.room,
            &self.spirit,
            &self.session,
            &self.capability,
            &self.idempotency_key,
        )?;
        validate_uuid(&self.quest_id, "questId")?;
        validate_uuid(&self.attempt_id, "attemptId")?;
        nonempty(&self.body, "body")?;
        match self.action {
            QuestReportAction::Progress | QuestReportAction::Submit => {
                nonempty(self.lease_token.as_deref().unwrap_or(""), "leaseToken")?;
                if let Some(role) = self.authored_role.as_deref()
                    && !matches!(role, "executor" | "reviewer")
                {
                    return Err(AppError::Invalid(
                        "authoredRole must be executor or reviewer for a receipt".into(),
                    ));
                }
                if self.item_position.is_some() || self.verdict.is_some() {
                    return Err(AppError::Invalid(
                        "itemPosition and verdict are only valid for settleItem".into(),
                    ));
                }
            }
            QuestReportAction::SettleItem => {
                let role = self.authored_role.as_deref().ok_or_else(|| {
                    AppError::Invalid("authoredRole is required for settleItem".into())
                })?;
                if !matches!(role, "executor" | "reviewer" | "steward") {
                    return Err(AppError::Invalid(
                        "settleItem role must be executor, reviewer, or steward".into(),
                    ));
                }
                if self.item_position.is_none_or(|position| position < 1) {
                    return Err(AppError::Invalid(
                        "itemPosition must be a positive integer".into(),
                    ));
                }
                let verdict = self.verdict.as_deref().ok_or_else(|| {
                    AppError::Invalid("verdict is required for settleItem".into())
                })?;
                if !ITEM_VERDICTS.contains(&verdict) {
                    return Err(AppError::Invalid("verdict is not settleable".into()));
                }
            }
        }
        Ok(())
    }
}

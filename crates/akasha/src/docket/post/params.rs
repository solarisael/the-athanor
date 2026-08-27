use crate::config::AppError;
use crate::docket::validate::{
    looks_like_iso8601_duration, reject_action_fields, required, validate_uuid,
    validate_write_identity,
};
use chrono::DateTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QuestPostAction {
    GoalDraft,
    GoalActivate,
    Draft,
    Activate,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QuestPostParams {
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub capability: String,
    pub idempotency_key: String,
    pub action: QuestPostAction,
    #[serde(default)]
    pub house_id: Option<String>,
    #[serde(default)]
    pub goal_id: Option<String>,
    #[serde(default)]
    pub quest_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub recurrence_interval: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub importance: Option<String>,
    #[serde(default)]
    pub deadline_at: Option<String>,
    #[serde(default)]
    pub intent_authority_principal: Option<String>,
    #[serde(default)]
    pub steward_room: Option<String>,
    #[serde(default)]
    pub steward_spirit: Option<String>,
    #[serde(default)]
    pub review_class: Option<String>,
    #[serde(default)]
    pub acceptance_criteria: Option<Vec<String>>,
}

impl QuestPostParams {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_write_identity(
            &self.room,
            &self.spirit,
            &self.session,
            &self.capability,
            &self.idempotency_key,
        )?;
        if let Some(deadline) = self.deadline_at.as_deref() {
            DateTime::parse_from_rfc3339(deadline)
                .map_err(|_| AppError::Invalid("deadlineAt must be RFC3339".into()))?;
        }
        if let Some(importance) = self.importance.as_deref()
            && !matches!(importance, "hint" | "blocker")
        {
            return Err(AppError::Invalid(
                "importance must be hint or blocker".into(),
            ));
        }

        match self.action {
            QuestPostAction::GoalDraft => {
                required(&self.house_id, "houseId")?;
                required(&self.title, "title")?;
                required(&self.intent, "intent")?;
                if let Some(interval) = self.recurrence_interval.as_deref()
                    && !looks_like_iso8601_duration(interval)
                {
                    return Err(AppError::Invalid(
                        "recurrenceInterval must be an ISO-8601 duration".into(),
                    ));
                }
                reject_action_fields(&[
                    ("goalId", self.goal_id.is_some()),
                    ("questId", self.quest_id.is_some()),
                    ("kind", self.kind.is_some()),
                    ("body", self.body.is_some()),
                    ("importance", self.importance.is_some()),
                    ("deadlineAt", self.deadline_at.is_some()),
                    (
                        "intentAuthorityPrincipal",
                        self.intent_authority_principal.is_some(),
                    ),
                    ("stewardRoom", self.steward_room.is_some()),
                    ("stewardSpirit", self.steward_spirit.is_some()),
                    ("reviewClass", self.review_class.is_some()),
                    ("acceptanceCriteria", self.acceptance_criteria.is_some()),
                ])?;
            }
            QuestPostAction::GoalActivate => {
                validate_uuid(required(&self.goal_id, "goalId")?, "goalId")?;
                required(&self.intent_authority_principal, "intentAuthorityPrincipal")?;
                required(&self.steward_room, "stewardRoom")?;
                required(&self.steward_spirit, "stewardSpirit")?;
                reject_action_fields(&[
                    ("houseId", self.house_id.is_some()),
                    ("questId", self.quest_id.is_some()),
                    ("title", self.title.is_some()),
                    ("intent", self.intent.is_some()),
                    ("priority", self.priority.is_some()),
                    ("recurrenceInterval", self.recurrence_interval.is_some()),
                    ("kind", self.kind.is_some()),
                    ("body", self.body.is_some()),
                    ("importance", self.importance.is_some()),
                    ("deadlineAt", self.deadline_at.is_some()),
                    ("reviewClass", self.review_class.is_some()),
                    ("acceptanceCriteria", self.acceptance_criteria.is_some()),
                ])?;
            }
            QuestPostAction::Draft => {
                required(&self.house_id, "houseId")?;
                if let Some(goal_id) = self.goal_id.as_deref() {
                    validate_uuid(goal_id, "goalId")?;
                }
                required(&self.kind, "kind")?;
                required(&self.title, "title")?;
                required(&self.body, "body")?;
                reject_action_fields(&[
                    ("questId", self.quest_id.is_some()),
                    ("intent", self.intent.is_some()),
                    ("priority", self.priority.is_some()),
                    ("recurrenceInterval", self.recurrence_interval.is_some()),
                    (
                        "intentAuthorityPrincipal",
                        self.intent_authority_principal.is_some(),
                    ),
                    ("stewardRoom", self.steward_room.is_some()),
                    ("stewardSpirit", self.steward_spirit.is_some()),
                    ("reviewClass", self.review_class.is_some()),
                    ("acceptanceCriteria", self.acceptance_criteria.is_some()),
                ])?;
            }
            QuestPostAction::Activate => {
                validate_uuid(required(&self.quest_id, "questId")?, "questId")?;
                required(&self.intent_authority_principal, "intentAuthorityPrincipal")?;
                let review_class = required(&self.review_class, "reviewClass")?;
                if !matches!(review_class, "R0" | "R1" | "R2" | "R3") {
                    return Err(AppError::Invalid(
                        "reviewClass must be R0, R1, R2, or R3".into(),
                    ));
                }
                required(&self.importance, "importance")?;
                let criteria = self
                    .acceptance_criteria
                    .as_ref()
                    .ok_or_else(|| AppError::Invalid("acceptanceCriteria is required".into()))?;
                if criteria.is_empty()
                    || criteria.iter().any(|criterion| criterion.trim().is_empty())
                {
                    return Err(AppError::Invalid(
                        "acceptanceCriteria must contain at least one non-empty item".into(),
                    ));
                }
                reject_action_fields(&[
                    ("houseId", self.house_id.is_some()),
                    ("goalId", self.goal_id.is_some()),
                    ("title", self.title.is_some()),
                    ("intent", self.intent.is_some()),
                    ("priority", self.priority.is_some()),
                    ("recurrenceInterval", self.recurrence_interval.is_some()),
                    ("kind", self.kind.is_some()),
                    ("body", self.body.is_some()),
                    ("stewardRoom", self.steward_room.is_some()),
                    ("stewardSpirit", self.steward_spirit.is_some()),
                ])?;
            }
        }
        Ok(())
    }
}

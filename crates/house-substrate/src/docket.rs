//! The Docket cooperation plane: offers, claims, leases, receipts, settlement.

use crate::config::{AppError, Config, ROOM_KEY_RE};
use crate::hallway::hallway_post;
use chrono::{DateTime, Utc};
use house_core::hallway::HallwayPostRequest;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

// The 15-minute lease is an unmeasured v1 hypothesis. Change it only from observed durations.
const LEASE_MINUTES: i64 = 15;
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
const SETTLED_VERDICTS: &[&str] = &["met", "not_applicable"];
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QuestClaimParams {
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub capability: String,
    pub idempotency_key: String,
    pub quest_id: String,
}

impl QuestClaimParams {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_write_identity(
            &self.room,
            &self.spirit,
            &self.session,
            &self.capability,
            &self.idempotency_key,
        )?;
        validate_uuid(&self.quest_id, "questId")
    }
}

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
    pub lease_token: String,
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
        nonempty(&self.lease_token, "leaseToken")?;
        nonempty(&self.body, "body")?;
        match self.action {
            QuestReportAction::Progress | QuestReportAction::Submit => {
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
}

#[derive(Debug, Serialize)]
pub struct QuestBoardResult {
    pub ok: bool,
    pub quests: Vec<QuestBoardItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestClaimResult {
    pub ok: bool,
    pub attempt_id: String,
    pub claim_epoch: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_token: Option<String>,
    pub lease_expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestReportResult {
    pub ok: bool,
    pub action: QuestReportAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<String>,
    pub quest_state: String,
    pub attempt_state: String,
    pub settled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rearmed_quest_id: Option<String>,
    /// Present on Progress: the renewed lease horizon for this attempt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<DateTime<Utc>>,
}

fn required<'a>(value: &'a Option<String>, field: &str) -> Result<&'a str, AppError> {
    let value = value
        .as_deref()
        .ok_or_else(|| AppError::Invalid(format!("{field} is required")))?;
    nonempty(value, field)?;
    Ok(value)
}

fn nonempty(value: &str, field: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        Err(AppError::Invalid(format!("{field} must not be empty")))
    } else {
        Ok(())
    }
}

fn validate_uuid(value: &str, field: &str) -> Result<(), AppError> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| AppError::Invalid(format!("{field} must be a UUID")))
}

fn reject_action_fields(fields: &[(&str, bool)]) -> Result<(), AppError> {
    if let Some((field, _)) = fields.iter().find(|(_, present)| *present) {
        Err(AppError::Invalid(format!(
            "{field} is not valid for this action"
        )))
    } else {
        Ok(())
    }
}

fn validate_identity(room: &str, spirit: &str, session: &str) -> Result<(), AppError> {
    if !ROOM_KEY_RE.is_match(room) {
        return Err(AppError::Invalid("room must be a lowercase slug".into()));
    }
    nonempty(spirit, "spirit")?;
    nonempty(session, "session")
}

fn validate_write_identity(
    room: &str,
    spirit: &str,
    session: &str,
    capability: &str,
    idempotency_key: &str,
) -> Result<(), AppError> {
    validate_identity(room, spirit, session)?;
    nonempty(capability, "capability")?;
    nonempty(idempotency_key, "idempotencyKey")
}

fn looks_like_iso8601_duration(value: &str) -> bool {
    value.starts_with('P')
        && value.bytes().any(|byte| byte.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || matches!(byte, b'P' | b'T' | b'D' | b'H' | b'M' | b'S' | b'.')
        })
}

fn principal(room: &str, spirit: &str) -> String {
    format!("{room}:{spirit}")
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let longest = left.len().max(right.len());
    for index in 0..longest {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(a ^ b);
    }
    difference == 0
}

fn refusal(code: &'static str, message: &'static str) -> AppError {
    AppError::Refusal { code, message }
}

/// Gate a Docket write before any cooperation-plane state is read or changed.
pub async fn require_docket_capability(
    pool: &PgPool,
    room: &str,
    capability: &str,
) -> Result<(), AppError> {
    let expected: Option<String> = sqlx::query_scalar(
        "SELECT capability_hash FROM docket.room_capabilities WHERE room=$1 AND operation_class='docket_write'",
    )
    .bind(room)
    .fetch_optional(pool)
    .await?;
    let supplied = sha256_hex(capability.as_bytes());
    if expected
        .as_deref()
        .is_none_or(|hash| !constant_time_equal(supplied.as_bytes(), hash.as_bytes()))
    {
        return Err(refusal(
            "docket_capability",
            "the room capability does not authorize Docket writes",
        ));
    }
    Ok(())
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

async fn post_goal_draft(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestPostParams,
) -> Result<QuestPostResult, AppError> {
    // Replay fence: one goal per (house, mint key). A replayed goalDraft
    // returns the existing goal instead of minting a twin; the no-op update
    // is what makes RETURNING yield the surviving row.
    let goal_id: String = sqlx::query_scalar(
        "INSERT INTO docket.goals (house_id,title,intent,priority,recurrence_interval,idempotency_key) VALUES ($1,$2,$3,$4,CASE WHEN $5::text IS NULL THEN NULL ELSE $5::interval END,$6) ON CONFLICT (house_id, idempotency_key) WHERE idempotency_key IS NOT NULL DO UPDATE SET updated_at = docket.goals.updated_at RETURNING goal_id::text",
    )
    .bind(required(&request.house_id, "houseId")?)
    .bind(required(&request.title, "title")?)
    .bind(required(&request.intent, "intent")?)
    .bind(request.priority.unwrap_or(0))
    .bind(request.recurrence_interval.as_deref())
    .bind(&request.idempotency_key)
    .fetch_one(&mut **tx)
    .await?;
    insert_goal_event(
        tx,
        &goal_id,
        "goal_drafted",
        &principal(&request.room, &request.spirit),
        json!({"action": "goalDraft"}),
        Some(&request.idempotency_key),
    )
    .await?;
    Ok(QuestPostResult {
        ok: true,
        action: request.action,
        goal_id: Some(goal_id),
        quest_id: None,
        state: "draft".into(),
    })
}

async fn post_goal_activate(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestPostParams,
) -> Result<QuestPostResult, AppError> {
    let goal_id = required(&request.goal_id, "goalId")?;
    let row =
        sqlx::query("SELECT status FROM docket.goals WHERE goal_id=$1::text::uuid FOR UPDATE")
            .bind(goal_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| refusal("unknown_quest", "the requested goal does not exist"))?;
    let status: String = row.try_get("status")?;
    if !matches!(status.as_str(), "draft" | "offered") {
        return Err(AppError::Invalid("goal is not activatable".into()));
    }
    sqlx::query(
        "UPDATE docket.goals SET status='active',intent_authority_principal=$2,steward_room=$3,steward_spirit=$4,activated_at=NOW(),updated_at=NOW() WHERE goal_id=$1::text::uuid",
    )
    .bind(goal_id)
    .bind(required(
        &request.intent_authority_principal,
        "intentAuthorityPrincipal",
    )?)
    .bind(required(&request.steward_room, "stewardRoom")?)
    .bind(required(&request.steward_spirit, "stewardSpirit")?)
    .execute(&mut **tx)
    .await?;
    insert_goal_event(
        tx,
        goal_id,
        "goal_activated",
        &principal(&request.room, &request.spirit),
        json!({"action": "goalActivate"}),
        Some(&request.idempotency_key),
    )
    .await?;
    Ok(QuestPostResult {
        ok: true,
        action: request.action,
        goal_id: Some(goal_id.to_owned()),
        quest_id: None,
        state: "active".into(),
    })
}

async fn post_quest_draft(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestPostParams,
) -> Result<QuestPostResult, AppError> {
    let deadline = request
        .deadline_at
        .as_deref()
        .map(DateTime::parse_from_rfc3339)
        .transpose()
        .map_err(|_| AppError::Invalid("deadlineAt must be RFC3339".into()))?
        .map(|value| value.with_timezone(&Utc));
    let quest_id: String = sqlx::query_scalar(
        "INSERT INTO docket.quests (house_id,goal_id,kind,title,body,importance,deadline_at,posted_by_room,posted_by_spirit) VALUES ($1,$2::text::uuid,$3,$4,$5,$6,$7,$8,$9) RETURNING quest_id::text",
    )
    .bind(required(&request.house_id, "houseId")?)
    .bind(request.goal_id.as_deref())
    .bind(required(&request.kind, "kind")?)
    .bind(required(&request.title, "title")?)
    .bind(required(&request.body, "body")?)
    .bind(request.importance.as_deref().unwrap_or("hint"))
    .bind(deadline)
    .bind(&request.room)
    .bind(&request.spirit)
    .fetch_one(&mut **tx)
    .await?;
    insert_event(
        tx,
        &quest_id,
        None,
        "drafted",
        &principal(&request.room, &request.spirit),
        json!({"action": "draft"}),
        Some(&request.idempotency_key),
    )
    .await?;
    Ok(QuestPostResult {
        ok: true,
        action: request.action,
        goal_id: request.goal_id.clone(),
        quest_id: Some(quest_id),
        state: "draft".into(),
    })
}

async fn post_quest_activate(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestPostParams,
) -> Result<QuestPostResult, AppError> {
    let quest_id = required(&request.quest_id, "questId")?;
    let row = sqlx::query(
        "SELECT goal_id::text AS goal_id,state FROM docket.quests WHERE quest_id=$1::text::uuid FOR UPDATE",
    )
    .bind(quest_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| refusal("unknown_quest", "the requested quest does not exist"))?;
    let state: String = row.try_get("state")?;
    if state != "draft" {
        return Err(AppError::Invalid("quest is not activatable".into()));
    }

    let criteria = request
        .acceptance_criteria
        .as_ref()
        .ok_or_else(|| AppError::Invalid("acceptanceCriteria is required".into()))?;
    let acceptance_policy = json!({"criteria": criteria});
    let canonical = serde_json::to_vec(&acceptance_policy)
        .map_err(|error| AppError::Protocol(error.to_string()))?;
    let digest = sha256_hex(&canonical);
    let deadline = request
        .deadline_at
        .as_deref()
        .map(DateTime::parse_from_rfc3339)
        .transpose()
        .map_err(|_| AppError::Invalid("deadlineAt must be RFC3339".into()))?
        .map(|value| value.with_timezone(&Utc));
    // An omitted deadlineAt preserves the drafted deadline; only an explicit
    // value replaces it. Overwriting with NULL would silently unclock a quest.
    sqlx::query(
        "UPDATE docket.quests SET state='offered',intent_authority_principal=$2,acceptance_policy=$3,acceptance_policy_digest=$4,review_class=$5,importance=$6,deadline_at=COALESCE($7,deadline_at),activated_at=NOW(),updated_at=NOW() WHERE quest_id=$1::text::uuid",
    )
    .bind(quest_id)
    .bind(required(
        &request.intent_authority_principal,
        "intentAuthorityPrincipal",
    )?)
    .bind(&acceptance_policy)
    .bind(&digest)
    .bind(required(&request.review_class, "reviewClass")?)
    .bind(required(&request.importance, "importance")?)
    .bind(deadline)
    .execute(&mut **tx)
    .await?;
    for (index, criterion) in criteria.iter().enumerate() {
        sqlx::query(
            "INSERT INTO docket.quest_acceptance_items (quest_id,position,criterion) VALUES ($1::text::uuid,$2,$3)",
        )
        .bind(quest_id)
        .bind(i32::try_from(index + 1).map_err(|_| {
            AppError::Invalid("acceptanceCriteria contains too many items".into())
        })?)
        .bind(criterion)
        .execute(&mut **tx)
        .await?;
    }
    insert_event(
        tx,
        quest_id,
        None,
        "activated",
        &principal(&request.room, &request.spirit),
        json!({"reviewClass": request.review_class, "acceptancePolicyDigest": digest}),
        Some(&request.idempotency_key),
    )
    .await?;
    Ok(QuestPostResult {
        ok: true,
        action: request.action,
        goal_id: row.try_get("goal_id")?,
        quest_id: Some(quest_id.to_owned()),
        state: "offered".into(),
    })
}

pub async fn quest_board(
    pool: &PgPool,
    request: QuestBoardParams,
) -> Result<QuestBoardResult, AppError> {
    let rows = sqlx::query(
        "SELECT q.quest_id::text AS quest_id,q.goal_id::text AS goal_id,q.kind,q.title,q.body,q.importance,q.deadline_at,q.state,q.claim_epoch,COUNT(a.item_id) FILTER (WHERE a.verdict='met') AS met,COUNT(a.item_id) FILTER (WHERE a.verdict='not_met') AS not_met,COUNT(a.item_id) FILTER (WHERE a.verdict='not_applicable') AS not_applicable,COUNT(a.item_id) FILTER (WHERE a.verdict NOT IN ('met','not_met','not_applicable')) AS pending FROM docket.quests q LEFT JOIN docket.quest_acceptance_items a ON a.quest_id=q.quest_id WHERE q.house_id=$1 AND (cardinality($2::text[])=0 OR q.state=ANY($2::text[])) GROUP BY q.quest_id ORDER BY q.deadline_at ASC NULLS LAST,q.created_at ASC LIMIT $3",
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

pub async fn quest_claim(
    pool: &PgPool,
    request: QuestClaimParams,
) -> Result<QuestClaimResult, AppError> {
    require_docket_capability(pool, &request.room, &request.capability).await?;
    let mut tx = pool.begin().await?;
    let quest = sqlx::query(
        "SELECT state,claim_epoch,revision FROM docket.quests WHERE quest_id=$1::text::uuid FOR UPDATE",
    )
    .bind(&request.quest_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| refusal("unknown_quest", "the requested quest does not exist"))?;

    if let Some(row) = sqlx::query(
        "SELECT attempt_id::text AS attempt_id,claim_epoch,lease_expires_at FROM docket.quest_attempts WHERE quest_id=$1::text::uuid AND idempotency_key=$2",
    )
    .bind(&request.quest_id)
    .bind(&request.idempotency_key)
    .fetch_optional(&mut *tx)
    .await?
    {
        let result = QuestClaimResult {
            ok: true,
            attempt_id: row.try_get("attempt_id")?,
            claim_epoch: row.try_get("claim_epoch")?,
            lease_token: None,
            lease_expires_at: row.try_get("lease_expires_at")?,
        };
        tx.commit().await?;
        return Ok(result);
    }

    let state: String = quest.try_get("state")?;
    let prior_epoch: i64 = quest.try_get("claim_epoch")?;
    // Reclaim door (0023 header): a claimed quest whose current attempt sits
    // active on an EXPIRED lease may be reclaimed under a new epoch. The old
    // epoch and lease hash fence the stale hand out of publishing.
    let reclaimed_attempt: Option<String> = if state == "claimed" {
        let stale = sqlx::query_scalar::<_, String>(
            "UPDATE docket.quest_attempts SET state='reclaimed' WHERE quest_id=$1::text::uuid AND claim_epoch=$2 AND state='active' AND lease_expires_at <= NOW() RETURNING attempt_id::text",
        )
        .bind(&request.quest_id)
        .bind(prior_epoch)
        .fetch_optional(&mut *tx)
        .await?;
        if stale.is_none() {
            return Err(refusal(
                "not_claimable",
                "the quest is claimed and its lease is still live",
            ));
        }
        stale
    } else if state != "offered" {
        return Err(refusal(
            "not_claimable",
            "only an offered quest can be claimed",
        ));
    } else {
        None
    };
    let claim_epoch: i64 = prior_epoch + 1;
    let quest_revision: i64 = quest.try_get("revision")?;
    let lease_token: String = sqlx::query_scalar("SELECT encode(gen_random_bytes(32),'hex')")
        .fetch_one(&mut *tx)
        .await?;
    let lease_hash = sha256_hex(lease_token.as_bytes());
    let row = sqlx::query(
        "INSERT INTO docket.quest_attempts (quest_id,claim_epoch,quest_revision,claimant_room,claimant_spirit,session_id,lease_token_hash,lease_expires_at,idempotency_key) VALUES ($1::text::uuid,$2,$3,$4,$5,$6,$7,NOW()+($8 * INTERVAL '1 minute'),$9) RETURNING attempt_id::text AS attempt_id,lease_expires_at",
    )
    .bind(&request.quest_id)
    .bind(claim_epoch)
    .bind(quest_revision)
    .bind(&request.room)
    .bind(&request.spirit)
    .bind(&request.session)
    .bind(&lease_hash)
    .bind(LEASE_MINUTES)
    .bind(&request.idempotency_key)
    .fetch_one(&mut *tx)
    .await?;
    let attempt_id: String = row.try_get("attempt_id")?;
    let lease_expires_at: DateTime<Utc> = row.try_get("lease_expires_at")?;
    sqlx::query(
        "UPDATE docket.quests SET state='claimed',claim_epoch=$2,updated_at=NOW() WHERE quest_id=$1::text::uuid",
    )
    .bind(&request.quest_id)
    .bind(claim_epoch)
    .execute(&mut *tx)
    .await?;
    if let Some(reclaimed) = &reclaimed_attempt {
        insert_event(
            &mut tx,
            &request.quest_id,
            Some(reclaimed),
            "reclaimed",
            &principal(&request.room, &request.spirit),
            json!({"priorEpoch": prior_epoch, "newEpoch": claim_epoch}),
            Some(&format!("reclaim:{}", request.idempotency_key)),
        )
        .await?;
    }
    insert_event(
        &mut tx,
        &request.quest_id,
        Some(&attempt_id),
        "claimed",
        &principal(&request.room, &request.spirit),
        json!({"claimEpoch": claim_epoch, "leaseMinutes": LEASE_MINUTES}),
        Some(&request.idempotency_key),
    )
    .await?;
    tx.commit().await?;
    Ok(QuestClaimResult {
        ok: true,
        attempt_id,
        claim_epoch,
        lease_token: Some(lease_token),
        lease_expires_at,
    })
}

pub async fn quest_report(
    pool: &PgPool,
    request: QuestReportParams,
) -> Result<QuestReportResult, AppError> {
    require_docket_capability(pool, &request.room, &request.capability).await?;
    let mut tx = pool.begin().await?;
    let quest = sqlx::query(
        "SELECT state,claim_epoch FROM docket.quests WHERE quest_id=$1::text::uuid FOR UPDATE",
    )
    .bind(&request.quest_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| refusal("unknown_quest", "the requested quest does not exist"))?;
    let attempt = sqlx::query(
        "SELECT claim_epoch,lease_token_hash,lease_expires_at,state,claimant_room FROM docket.quest_attempts WHERE attempt_id=$1::text::uuid AND quest_id=$2::text::uuid FOR UPDATE",
    )
    .bind(&request.attempt_id)
    .bind(&request.quest_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| refusal("unknown_attempt", "the requested attempt does not exist"))?;

    let quest_epoch: i64 = quest.try_get("claim_epoch")?;
    let attempt_epoch: i64 = attempt.try_get("claim_epoch")?;
    let expected_hash: String = attempt.try_get("lease_token_hash")?;
    let supplied_hash = sha256_hex(request.lease_token.as_bytes());
    let lease_expires_at: DateTime<Utc> = attempt.try_get("lease_expires_at")?;
    let attempt_state: String = attempt.try_get("state")?;
    let state_is_usable = match request.action {
        // Submission yields the attempt. Review then uses that same fenced lease.
        QuestReportAction::SettleItem => attempt_state == "yielded",
        QuestReportAction::Progress | QuestReportAction::Submit => attempt_state == "active",
    };
    if attempt_epoch != quest_epoch
        || lease_expires_at <= Utc::now()
        || !state_is_usable
        || !constant_time_equal(supplied_hash.as_bytes(), expected_hash.as_bytes())
    {
        return Err(refusal(
            "stale_lease",
            "the lease is expired, superseded, stale, or invalid",
        ));
    }

    // Symmetric room fence (guild-hall #159 ruling 2): the lease binds work
    // to the claimant room, so a leaked valid token from any other room
    // refuses Progress and Submit. SettleItem carries the inverse fence below.
    let claimant_room: String = attempt.try_get("claimant_room")?;
    if matches!(
        request.action,
        QuestReportAction::Progress | QuestReportAction::Submit
    ) && request.room != claimant_room
    {
        return Err(refusal(
            "claimant_binding",
            "only the claimant room may progress or submit this attempt",
        ));
    }

    let result = match request.action {
        QuestReportAction::Progress => {
            let mut result = report_progress(&mut tx, &request).await?;
            // Live work keeps the lease warm: progress extends the horizon,
            // never shortens an already longer one.
            let renewed: DateTime<Utc> = sqlx::query_scalar(
                "UPDATE docket.quest_attempts SET lease_expires_at=GREATEST(lease_expires_at,NOW()+($2 * INTERVAL '1 minute')),heartbeat_at=NOW() WHERE attempt_id=$1::text::uuid RETURNING lease_expires_at",
            )
            .bind(&request.attempt_id)
            .bind(LEASE_MINUTES)
            .fetch_one(&mut *tx)
            .await?;
            result.lease_expires_at = Some(renewed);
            result
        }
        QuestReportAction::Submit => {
            let quest_state: String = quest.try_get("state")?;
            if quest_state != "claimed" {
                return Err(refusal(
                    "stale_lease",
                    "the lease is expired, superseded, stale, or invalid",
                ));
            }
            report_submit(&mut tx, &request).await?
        }
        QuestReportAction::SettleItem => {
            let role = request.authored_role.as_deref().unwrap_or("executor");
            if role == "executor" {
                return Err(refusal(
                    "executor_cannot_settle",
                    "an executor cannot settle an acceptance item",
                ));
            }
            // Review independence (guild-hall #144): the settling principal
            // must differ from the claimant. The capability authenticates the
            // room and spirit text does not, so the enforceable fence is
            // room-level; spirit-level binding is a later door (0024 header).
            if request.room == claimant_room {
                return Err(refusal(
                    "review_independence",
                    "the claimant room cannot settle its own acceptance items",
                ));
            }
            let quest_state: String = quest.try_get("state")?;
            if quest_state != "submitted" {
                return Err(refusal(
                    "not_settleable",
                    "only a submitted quest can have acceptance items settled",
                ));
            }
            report_settle_item(&mut tx, &request).await?
        }
    };
    tx.commit().await?;
    Ok(result)
}

// The clock (guild-hall #136 rail, #145 boundary): it only reads and rings.
// It never measures, wakes, or judges a spirit. A clear board is silence,
// and a re-sweep over already-pinged deadlines is silence too.
const CLOCK_ROOM: &str = "clock";
const CLOCK_SPIRIT: &str = "Clock";
const CLOCK_HALLWAY_DEFAULT: &str = "guild-hall";
const CLOCK_HORIZON_MINUTES_DEFAULT: i64 = 1440;
// enough: 7-day ceiling; a calendar-shaped horizon only when a ritual earns it.
const CLOCK_HORIZON_MINUTES_MAX: i64 = 10080;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QuestClockParams {
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub capability: String,
    pub idempotency_key: String,
    pub house_id: String,
    #[serde(default)]
    pub horizon_minutes: Option<i64>,
    #[serde(default)]
    pub hallway: Option<String>,
}

impl QuestClockParams {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_write_identity(
            &self.room,
            &self.spirit,
            &self.session,
            &self.capability,
            &self.idempotency_key,
        )?;
        nonempty(&self.house_id, "houseId")?;
        if let Some(horizon) = self.horizon_minutes
            && !(1..=CLOCK_HORIZON_MINUTES_MAX).contains(&horizon)
        {
            return Err(AppError::Invalid(format!(
                "horizonMinutes must be between 1 and {CLOCK_HORIZON_MINUTES_MAX}"
            )));
        }
        if let Some(hallway) = &self.hallway {
            nonempty(hallway, "hallway")?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestClockDueItem {
    pub quest_id: String,
    pub title: String,
    pub state: String,
    pub deadline_at: DateTime<Utc>,
    pub recipient_room: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestClockResult {
    pub ok: bool,
    /// Every live quest whose deadline sits inside the horizon.
    pub due: Vec<QuestClockDueItem>,
    /// Quest ids whose ping event was newly written by THIS sweep.
    pub pinged: Vec<String>,
    /// True only when this sweep posted a Bell-carrying ring.
    pub rang: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bell_message_id: Option<i64>,
    /// Recipient rooms the ring could not reach: not allowed in the hallway.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub silent_rooms: Vec<String>,
}

/// One sweep of the board. Reads due quests, rings the Bell through the
/// ordinary hallway door as the named presence clock/Clock, then writes one
/// clock_ping event per newly due (quest, deadline) attributed to the clock
/// principal. The clock decides nothing else: no wake, no judgment.
/// Ordering is ring-then-ping, and every step is idempotent: a torn sweep
/// re-rings into the hallway idempotency key (derived from the pinged set)
/// and re-pings into the ledger's ON CONFLICT dedupe, so it converges with
/// no lost and no doubled Bell. No transaction is held across the ring.
pub async fn quest_clock(
    pool: &PgPool,
    config: &Config,
    request: QuestClockParams,
) -> Result<QuestClockResult, AppError> {
    require_docket_capability(pool, &request.room, &request.capability).await?;
    let horizon = request
        .horizon_minutes
        .unwrap_or(CLOCK_HORIZON_MINUTES_DEFAULT);
    let hallway = request
        .hallway
        .clone()
        .unwrap_or_else(|| CLOCK_HALLWAY_DEFAULT.to_string());
    let rows = sqlx::query(
        "SELECT q.quest_id::text AS quest_id,q.title,q.state,q.deadline_at,COALESCE(a.claimant_room,q.posted_by_room) AS recipient_room FROM docket.quests q LEFT JOIN docket.quest_attempts a ON a.quest_id=q.quest_id AND a.claim_epoch=q.claim_epoch AND q.state IN ('claimed','submitted') WHERE q.house_id=$1 AND q.deadline_at IS NOT NULL AND q.state IN ('offered','claimed','submitted') AND q.deadline_at <= NOW()+($2 * INTERVAL '1 minute') ORDER BY q.deadline_at,q.quest_id",
    )
    .bind(&request.house_id)
    .bind(horizon)
    .fetch_all(pool)
    .await?;
    let due = rows
        .iter()
        .map(|row| {
            Ok(QuestClockDueItem {
                quest_id: row.try_get("quest_id")?,
                title: row.try_get("title")?,
                state: row.try_get("state")?,
                deadline_at: row.try_get("deadline_at")?,
                recipient_room: row.try_get("recipient_room")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;

    // A (quest, deadline) already pinged stays pinged; only new ones ring.
    let due_ids: Vec<String> = due.iter().map(|item| item.quest_id.clone()).collect();
    let existing: Vec<(String, String)> = sqlx::query_as(
        "SELECT quest_id::text, idempotency_key FROM docket.quest_events WHERE event_kind='clock_ping' AND idempotency_key IS NOT NULL AND quest_id::text = ANY($1)",
    )
    .bind(&due_ids)
    .fetch_all(pool)
    .await?;
    let ping_key = |item: &QuestClockDueItem| format!("clock:{}", item.deadline_at.to_rfc3339());
    let pinged: Vec<&QuestClockDueItem> = due
        .iter()
        .filter(|item| {
            !existing
                .iter()
                .any(|(id, key)| *id == item.quest_id && *key == ping_key(item))
        })
        .collect();

    if pinged.is_empty() {
        // A clear board is silence; so is a board already pinged.
        return Ok(QuestClockResult {
            ok: true,
            due,
            pinged: Vec::new(),
            rang: false,
            bell_message_id: None,
            silent_rooms: Vec::new(),
        });
    }

    let mut recipients: Vec<String> = pinged
        .iter()
        .map(|item| item.recipient_room.clone())
        .collect();
    recipients.sort_unstable();
    recipients.dedup();
    let allowed: Vec<String> = sqlx::query_scalar(
        "SELECT room FROM hallway_allowed_rooms WHERE hallway_id=(SELECT id FROM hallway_channels WHERE hallway_key=$1) AND room=ANY($2)",
    )
    .bind(&hallway)
    .bind(&recipients)
    .fetch_all(pool)
    .await?;
    let silent_rooms: Vec<String> = recipients
        .iter()
        .filter(|room| !allowed.contains(room))
        .cloned()
        .collect();

    let mut body = format!(
        "Clock ping. {} quest(s) near or past deadline:\n",
        pinged.len()
    );
    let mut digest_lines = String::new();
    for item in &pinged {
        body.push_str(&format!(
            "- {} — {} — {} — due {}\n",
            item.quest_id,
            item.title,
            item.state,
            item.deadline_at.to_rfc3339()
        ));
        digest_lines.push_str(&format!(
            "{}@{}\n",
            item.quest_id,
            item.deadline_at.to_rfc3339()
        ));
    }
    body.push_str("An unanswered ping is board state, never delinquency.");
    let ring_key = format!("clock:{:x}", Sha256::digest(digest_lines.as_bytes()));

    // Ring before pinging the ledger: a sweep torn between the two re-rings
    // on retry and the hallway idempotency key collapses the duplicate.
    let receipt = hallway_post(
        pool,
        config,
        HallwayPostRequest {
            hallway,
            room: CLOCK_ROOM.to_string(),
            spirit: CLOCK_SPIRIT.to_string(),
            session: format!("clock:{}", request.house_id),
            idempotency_key: ring_key,
            body,
            reply_to: None,
            to_rooms: allowed,
        },
    )
    .await?;

    let clock_principal = principal(CLOCK_ROOM, CLOCK_SPIRIT);
    let triggered_by = principal(&request.room, &request.spirit);
    let mut tx = pool.begin().await?;
    for item in &pinged {
        sqlx::query(
            "INSERT INTO docket.quest_events (quest_id,event_kind,principal,detail,idempotency_key) VALUES ($1::text::uuid,'clock_ping',$2,$3,$4) ON CONFLICT (quest_id, idempotency_key) WHERE idempotency_key IS NOT NULL AND quest_id IS NOT NULL DO NOTHING",
        )
        .bind(&item.quest_id)
        .bind(&clock_principal)
        .bind(json!({
            "deadlineAt": item.deadline_at,
            "state": item.state,
            "recipientRoom": item.recipient_room,
            "triggeredBy": triggered_by,
        }))
        .bind(&ping_key(item))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    let pinged_ids = pinged.iter().map(|item| item.quest_id.clone()).collect();
    Ok(QuestClockResult {
        ok: true,
        due,
        pinged: pinged_ids,
        rang: true,
        bell_message_id: Some(receipt.message.id),
        silent_rooms,
    })
}

async fn report_progress(
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

async fn report_submit(
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

async fn report_settle_item(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestReportParams,
) -> Result<QuestReportResult, AppError> {
    let position = request
        .item_position
        .ok_or_else(|| AppError::Invalid("itemPosition is required".into()))?;
    let verdict = request
        .verdict
        .as_deref()
        .ok_or_else(|| AppError::Invalid("verdict is required".into()))?;
    let item = sqlx::query(
        "SELECT verdict FROM docket.quest_acceptance_items WHERE quest_id=$1::text::uuid AND position=$2 FOR UPDATE",
    )
    .bind(&request.quest_id)
    .bind(position)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::Invalid("itemPosition does not name an acceptance item".into()))?;
    let previous: String = item.try_get("verdict")?;
    if previous != "pending" {
        return Err(AppError::Invalid(
            "acceptance item is already settled".into(),
        ));
    }
    let role = request.authored_role.as_deref().unwrap_or("executor");
    sqlx::query(
        "UPDATE docket.quest_acceptance_items SET verdict=$3,settled_by_role=$4,settled_by_room=$5,settled_by_spirit=$6,settled_at=NOW() WHERE quest_id=$1::text::uuid AND position=$2",
    )
    .bind(&request.quest_id)
    .bind(position)
    .bind(verdict)
    .bind(role)
    .bind(&request.room)
    .bind(&request.spirit)
    .execute(&mut **tx)
    .await?;
    insert_event(
        tx,
        &request.quest_id,
        Some(&request.attempt_id),
        "item_settled",
        &principal(&request.room, &request.spirit),
        json!({"position": position, "verdict": verdict, "role": role, "body": request.body}),
        Some(&request.idempotency_key),
    )
    .await?;

    let quest_state: String =
        sqlx::query_scalar("SELECT state FROM docket.quests WHERE quest_id=$1::text::uuid")
            .bind(&request.quest_id)
            .fetch_one(&mut **tx)
            .await?;
    let all_accepted: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM docket.quest_acceptance_items WHERE quest_id=$1::text::uuid AND verdict <> ALL($2::text[]))",
    )
    .bind(&request.quest_id)
    .bind(SETTLED_VERDICTS)
    .fetch_one(&mut **tx)
    .await?;
    let mut settled = false;
    let mut rearmed_quest_id = None;
    let final_state = if quest_state == "submitted" && all_accepted {
        sqlx::query(
            "UPDATE docket.quests SET state='settled',settled_at=NOW(),updated_at=NOW() WHERE quest_id=$1::text::uuid",
        )
        .bind(&request.quest_id)
        .execute(&mut **tx)
        .await?;
        insert_event(
            tx,
            &request.quest_id,
            Some(&request.attempt_id),
            "settled",
            &principal(&request.room, &request.spirit),
            json!({"settledByRole": role}),
            Some(&format!("settled:{}", request.idempotency_key)),
        )
        .await?;
        rearmed_quest_id = rearm_recurrent_quest(tx, request).await?;
        settled = true;
        "settled".to_owned()
    } else {
        quest_state
    };
    Ok(QuestReportResult {
        ok: true,
        action: request.action,
        receipt_id: None,
        quest_state: final_state,
        attempt_state: "yielded".into(),
        settled,
        rearmed_quest_id,
        lease_expires_at: None,
    })
}

async fn rearm_recurrent_quest(
    tx: &mut Transaction<'_, Postgres>,
    request: &QuestReportParams,
) -> Result<Option<String>, AppError> {
    // The re-armed occurrence must stay clock-visible: a NULL prior deadline
    // re-arms from NOW(), never NULL + interval = NULL (silent recurrence
    // death — the ping only speaks about due items).
    let new_quest_id: Option<String> = sqlx::query_scalar(
        "INSERT INTO docket.quests (house_id,goal_id,parent_quest_id,kind,title,body,authority_ceiling,required_capabilities,acceptance_policy,acceptance_policy_digest,review_class,settlement_policy,importance,deadline_at,intent_authority_principal,posted_by_room,posted_by_spirit,state,revision,activated_at) SELECT q.house_id,q.goal_id,q.quest_id,q.kind,q.title,q.body,q.authority_ceiling,q.required_capabilities,q.acceptance_policy,q.acceptance_policy_digest,q.review_class,q.settlement_policy,q.importance,COALESCE(q.deadline_at,NOW())+g.recurrence_interval,q.intent_authority_principal,q.posted_by_room,q.posted_by_spirit,'offered',q.revision,NOW() FROM docket.quests q JOIN docket.goals g ON g.goal_id=q.goal_id WHERE q.quest_id=$1::text::uuid AND g.recurrence_interval IS NOT NULL RETURNING quest_id::text",
    )
    .bind(&request.quest_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(new_quest_id) = new_quest_id else {
        return Ok(None);
    };
    sqlx::query(
        "INSERT INTO docket.quest_acceptance_items (quest_id,position,criterion) SELECT $1::text::uuid,position,criterion FROM docket.quest_acceptance_items WHERE quest_id=$2::text::uuid ORDER BY position",
    )
    .bind(&new_quest_id)
    .bind(&request.quest_id)
    .execute(&mut **tx)
    .await?;
    insert_event(
        tx,
        &request.quest_id,
        Some(&request.attempt_id),
        "rearmed",
        &principal(&request.room, &request.spirit),
        json!({"newQuestId": new_quest_id}),
        Some(&format!("rearm:{}", request.quest_id)),
    )
    .await?;
    Ok(Some(new_quest_id))
}

async fn insert_receipt(
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

async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    quest_id: &str,
    attempt_id: Option<&str>,
    event_kind: &str,
    principal: &str,
    detail: Value,
    idempotency_key: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO docket.quest_events (quest_id,attempt_id,event_kind,principal,detail,idempotency_key) VALUES ($1::text::uuid,$2::text::uuid,$3,$4,$5,$6)",
    )
    .bind(quest_id)
    .bind(attempt_id)
    .bind(event_kind)
    .bind(principal)
    .bind(detail)
    .bind(idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Goal transitions ledger through the same append-only relation; they have
/// no attempt, so the helper stays deliberately asymmetric to insert_event.
/// A replayed goalDraft reuses its mint key: the conflict target makes the
/// second ledger insert a no-op instead of an error.
async fn insert_goal_event(
    tx: &mut Transaction<'_, Postgres>,
    goal_id: &str,
    event_kind: &str,
    principal: &str,
    detail: Value,
    idempotency_key: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO docket.quest_events (goal_id,event_kind,principal,detail,idempotency_key) VALUES ($1::text::uuid,$2,$3,$4,$5) ON CONFLICT (goal_id, idempotency_key) WHERE idempotency_key IS NOT NULL AND goal_id IS NOT NULL DO NOTHING",
    )
    .bind(goal_id)
    .bind(event_kind)
    .bind(principal)
    .bind(detail)
    .bind(idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

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

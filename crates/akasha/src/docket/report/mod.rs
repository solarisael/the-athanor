mod params;
mod progress;
mod receipt;
mod settle;
mod submit;

use crate::config::AppError;
use crate::docket::capability::require_docket_capability;
use crate::docket::claim::LEASE_MINUTES;
use crate::docket::digest::{constant_time_equal, sha256_hex};
use crate::docket::validate::refusal;
use self::progress::report_progress;
use self::settle::report_settle_item;
use self::submit::report_submit;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};

pub use params::{QuestReportAction, QuestReportParams};

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
    let supplied_hash = sha256_hex(request.lease_token.as_deref().unwrap_or("").as_bytes());
    let lease_expires_at: DateTime<Utc> = attempt.try_get("lease_expires_at")?;
    let attempt_state: String = attempt.try_get("state")?;
    let state_is_usable = match request.action {
        QuestReportAction::SettleItem => attempt_state == "yielded",
        QuestReportAction::Progress | QuestReportAction::Submit => attempt_state == "active",
    };
    // The bearer token binds the CLAIMANT'S work: Progress and Submit demand
    // a live, matching lease. Settlement authenticates the REVIEWER against
    // the attempt row instead (guild-hall #167): requiring the executor's
    // secret would make review independence depend on secret-sharing. A
    // yielded attempt therefore stays settleable after its lease dies, and
    // review delay is board state, never delinquency.
    let lease_is_valid = match request.action {
        QuestReportAction::SettleItem => true,
        QuestReportAction::Progress | QuestReportAction::Submit => {
            lease_expires_at > Utc::now()
                && constant_time_equal(supplied_hash.as_bytes(), expected_hash.as_bytes())
        }
    };
    if attempt_epoch != quest_epoch || !state_is_usable || !lease_is_valid {
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

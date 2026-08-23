use crate::config::AppError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use super::capability::require_docket_capability;
use super::digest::sha256_hex;
use super::ledger::insert_event;
use super::validate::{principal, refusal, validate_uuid, validate_write_identity};

// The 15-minute lease is an unmeasured v1 hypothesis. Change it only from observed durations.
pub(super) const LEASE_MINUTES: i64 = 15;

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

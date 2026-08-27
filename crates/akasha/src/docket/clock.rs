use crate::config::{AppError, Config};
use crate::hallway::hallway_post;
use chrono::{DateTime, Utc};
use hearth::hallway::HallwayPostRequest;
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use super::capability::require_docket_capability;
use super::validate::{nonempty, principal, validate_write_identity};

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

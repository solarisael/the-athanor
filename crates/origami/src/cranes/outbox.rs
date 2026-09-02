
use crate::{
    boats,
    cranes::{
        broker::{BOAT_READY_CONSUMER_NAME, CRANE_CONSUMER_NAME},
        envelope::CraneEvent,
        lanes::{Lane, RecipientKind},
    },
    sea,
};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgPoolOptions};
use uuid::Uuid;

/// The lowest PostgreSQL schema this build will speak to.
pub const REQUIRED_SCHEMA_VERSION: i32 = hearth::SUBSTRATE_SCHEMA_VERSION as i32;
/// How long a claimed outbox row stays claimed before another publisher may
/// take it. Sibling of `crate::hallways::knocks::KNOCK_CLAIM_LEASE_SECONDS`:
/// same 30-second grip on a claimed row, different ledger.
pub const LEASE_SECONDS: i32 = 30;
pub const MAX_PUBLISH_ATTEMPTS: i32 = 10;

#[derive(Clone)]
pub struct Store {
    pool: PgPool,
}

#[derive(Debug)]
pub struct ClaimedEvent {
    pub event_id: Uuid,
    pub event_kind: String,
    pub aggregate_id: i64,
    pub room: String,
    pub payload: Value,
    pub integrity_sha256: String,
    pub attempts: i32,
    pub recipient_kind: Option<String>,
    pub recipient_key: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl ClaimedEvent {
    pub fn lane(&self) -> Option<Lane> {
        match (&self.recipient_kind, &self.recipient_key) {
            (None, None) => Some(Lane::BoatReady),
            (Some(kind), Some(key)) => Some(Lane::Addressed {
                recipient_kind: kind.parse::<RecipientKind>().ok()?,
                recipient_key: key.clone(),
            }),
            _ => None,
        }
    }

    pub fn subject(&self) -> Option<String> {
        Some(self.lane()?.subject())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptDisposition {
    Inserted,
    Replayed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedReceipt {
    pub disposition: ReceiptDisposition,
    pub event_id: Uuid,
    pub record_id: i64,
    pub room: String,
    pub processed_at: DateTime<Utc>,
    pub original_stream_sequence: u64,
    pub integrity_sha256: String,
}

#[derive(Debug, serde::Serialize)]
pub struct DatabaseHealth {
    pub schema_version: i32,
    pub pending: i64,
    pub leased: i64,
    pub published: i64,
    pub dead_letters: i64,
    pub receipts: i64,
}

impl Store {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(database_url)
            .await
            .context("connect to PostgreSQL delivery authority")?;
        let store = Self { pool };
        store.require_schema().await?;
        Ok(store)
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn require_schema(&self) -> Result<()> {
        let version: i32 =
            sqlx::query_scalar("SELECT coalesce(max(version), 0)::integer FROM schema_migrations")
                .fetch_one(&self.pool)
                .await
                .context("read PostgreSQL schema version")?;
        if version < REQUIRED_SCHEMA_VERSION {
            bail!(
                "PostgreSQL schema {version} is older than required schema {REQUIRED_SCHEMA_VERSION}"
            );
        }
        Ok(())
    }

    pub async fn claim_next(&self, lease_owner: Uuid) -> Result<Option<ClaimedEvent>> {
        let mut transaction = self.pool.begin().await?;
        let claimed = sqlx::query(
            "WITH candidate AS (
               SELECT event_id
               FROM crane_outbox
               WHERE (state = 'pending' AND available_at <= NOW())
                  OR (state = 'leased' AND lease_expires_at <= NOW())
               ORDER BY available_at, created_at, event_id
               FOR UPDATE SKIP LOCKED
               LIMIT 1
             )
             UPDATE crane_outbox event
             SET state = 'leased',
                 attempts = event.attempts + 1,
                 lease_owner = $1,
                 lease_expires_at = NOW() + make_interval(secs => $2),
                 last_error = NULL,
                 updated_at = NOW()
             FROM candidate
             WHERE event.event_id = candidate.event_id
             RETURNING event.event_id, event.event_kind, event.aggregate_id, event.room,
                       event.payload, event.integrity_sha256, event.attempts,
                       event.recipient_kind, event.recipient_key, event.expires_at",
        )
        .bind(lease_owner)
        .bind(LEASE_SECONDS)
        .fetch_optional(&mut *transaction)
        .await
        .context("claim pending crane outbox event")?;
        let claimed = claimed
            .map(|row| -> std::result::Result<ClaimedEvent, sqlx::Error> {
                Ok(ClaimedEvent {
                    event_id: row.try_get("event_id")?,
                    event_kind: row.try_get("event_kind")?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    room: row.try_get("room")?,
                    payload: row.try_get("payload")?,
                    integrity_sha256: row.try_get("integrity_sha256")?,
                    attempts: row.try_get("attempts")?,
                    recipient_kind: row.try_get("recipient_kind")?,
                    recipient_key: row.try_get("recipient_key")?,
                    expires_at: row.try_get("expires_at")?,
                })
            })
            .transpose()?;
        transaction.commit().await?;
        Ok(claimed)
    }

    pub async fn mark_published(
        &self,
        event_id: Uuid,
        lease_owner: Uuid,
        stream_sequence: u64,
    ) -> Result<()> {
        let updated = sqlx::query(
            "UPDATE crane_outbox
             SET state = 'published', published_at = NOW(), lease_owner = NULL,
                 lease_expires_at = NULL, last_error = NULL, updated_at = NOW()
             WHERE event_id = $1 AND state = 'leased' AND lease_owner = $2",
        )
        .bind(event_id)
        .bind(lease_owner)
        .execute(&self.pool)
        .await
        .context("record acknowledged JetStream publish in PostgreSQL")?;
        if updated.rows_affected() != 1 {
            bail!(
                "outbox publish acknowledgement lost its PostgreSQL lease for event {event_id} at stream sequence {stream_sequence}"
            );
        }
        Ok(())
    }

    pub async fn mark_publish_failure(
        &self,
        claimed: &ClaimedEvent,
        lease_owner: Uuid,
        error: &str,
    ) -> Result<()> {
        let safe_error = bounded_reason(error);
        let mut transaction = self.pool.begin().await?;
        let exhausted = claimed.attempts >= MAX_PUBLISH_ATTEMPTS;
        let updated = if exhausted {
            sqlx::query(
                "UPDATE crane_outbox
                 SET state = 'dead_letter', dead_lettered_at = NOW(), lease_owner = NULL,
                     lease_expires_at = NULL, last_error = $3, updated_at = NOW()
                 WHERE event_id = $1 AND state = 'leased' AND lease_owner = $2",
            )
            .bind(claimed.event_id)
            .bind(lease_owner)
            .bind(&safe_error)
            .execute(&mut *transaction)
            .await?
        } else {
            let delay = publish_retry_seconds(claimed.attempts);
            sqlx::query(
                "UPDATE crane_outbox
                 SET state = 'pending', available_at = NOW() + make_interval(secs => $3),
                     lease_owner = NULL, lease_expires_at = NULL, last_error = $4, updated_at = NOW()
                 WHERE event_id = $1 AND state = 'leased' AND lease_owner = $2",
            )
            .bind(claimed.event_id)
            .bind(lease_owner)
            .bind(delay)
            .bind(&safe_error)
            .execute(&mut *transaction)
            .await?
        };
        if updated.rows_affected() != 1 {
            bail!(
                "cannot record publish failure for outbox event {} without its active lease",
                claimed.event_id
            );
        }
        if exhausted {
            self.insert_dead_letter_tx(
                &mut transaction,
                Some(claimed.event_id),
                "publisher",
                claimed.subject().as_deref(),
                "publish_exhausted",
                &safe_error,
                serde_json::to_vec(&claimed.payload)?.as_slice(),
                None,
            )
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn dead_letter_claim(
        &self,
        claimed: &ClaimedEvent,
        lease_owner: Uuid,
        reason_code: &str,
        reason: &str,
    ) -> Result<()> {
        let payload = serde_json::to_vec(&claimed.payload)?;
        let mut transaction = self.pool.begin().await?;
        let updated = sqlx::query(
            "UPDATE crane_outbox
             SET state = 'dead_letter', dead_lettered_at = NOW(), lease_owner = NULL,
                 lease_expires_at = NULL, last_error = $3, updated_at = NOW()
             WHERE event_id = $1 AND state = 'leased' AND lease_owner = $2",
        )
        .bind(claimed.event_id)
        .bind(lease_owner)
        .bind(bounded_reason(reason))
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            bail!("cannot dead-letter outbox event without its active lease");
        }
        self.insert_dead_letter_tx(
            &mut transaction,
            Some(claimed.event_id),
            "publisher",
            claimed.subject().as_deref(),
            reason_code,
            reason,
            &payload,
            None,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// The idempotency claim coding#368 requires: lock, verify the
    /// pointed record, insert once, commit BEFORE the caller acks.
    /// Redelivery then replays the recorded outcome. Never simplify.
    pub async fn record_receipt(
        &self,
        event: &CraneEvent,
        stream_sequence: u64,
        delivery_count: i64,
    ) -> Result<RecordedReceipt> {
        let lane = event.lane();
        let consumer_name = lane_consumer_name(&lane);
        let mut transaction = self.pool.begin().await?;
        let existing = sqlx::query(
            "SELECT integrity_sha256, aggregate_id, room, processed_at, stream_sequence
             FROM crane_receipts
             WHERE consumer_name = $1 AND event_id = $2
             FOR UPDATE",
        )
        .bind(consumer_name)
        .bind(event.event_id)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(row) = existing {
            let integrity: String = row.try_get("integrity_sha256")?;
            let aggregate_id: i64 = row.try_get("aggregate_id")?;
            let room: String = row.try_get("room")?;
            let processed_at: DateTime<Utc> = row.try_get("processed_at")?;
            let stored_stream_sequence: i64 = row.try_get("stream_sequence")?;
            if integrity != event.integrity_sha256
                || aggregate_id != event.record_id_i64()
                || room != event.room
            {
                bail!("receipt_conflict: event id was replayed with different pointer metadata");
            }
            let receipt = RecordedReceipt {
                disposition: ReceiptDisposition::Replayed,
                event_id: event.event_id,
                record_id: aggregate_id,
                room,
                processed_at,
                original_stream_sequence: u64::try_from(stored_stream_sequence)
                    .context("stored stream sequence is negative")?,
                integrity_sha256: integrity,
            };
            transaction.commit().await?;
            return Ok(receipt);
        }

        let memory = sqlx::query("SELECT room, type, body FROM memories WHERE id = $1 FOR SHARE")
            .bind(event.record_id_i64())
            .fetch_optional(&mut *transaction)
            .await?;
        let Some(memory) = memory else {
            bail!("record_mismatch: pointed memory does not exist");
        };
        let room: String = memory.try_get("room")?;
        let memory_type: String = memory.try_get("type")?;
        let body: String = memory.try_get("body")?;
        match lane {
            Lane::BoatReady => {
                if memory_type != boats::MEMORY_KIND || room != event.room {
                    bail!(
                        "record_mismatch: pointer does not identify the declared paper-boat room"
                    );
                }
                if sea::payload_digest(body.as_bytes()) != event.integrity_sha256 {
                    bail!("integrity_mismatch: pointed paper-boat body digest differs");
                }
            }
            Lane::Addressed { .. } => {
                if room != event.room {
                    bail!("record_mismatch: pointer does not identify the declared room");
                }
            }
        }

        let stream_sequence_i64 =
            i64::try_from(stream_sequence).context("stream sequence exceeds PostgreSQL bigint")?;
        let first_delivery_count =
            i32::try_from(delivery_count).context("delivery count exceeds PostgreSQL integer")?;

        // Keys are crane_receipts columns (0016_boat_ready_delivery.sql:59-70,
        // renamed 0017:21), matched by NAME. processed_at merges below because
        // SELECT * nulls its DEFAULT -- quietly, for any later nullable column.
        let inserted = sqlx::query(
            "INSERT INTO crane_receipts
             SELECT * FROM jsonb_populate_record(
               NULL::crane_receipts,
               $1::jsonb || jsonb_build_object('processed_at', NOW())
             )
             RETURNING processed_at",
        )
        .bind(serde_json::json!({
            "consumer_name": consumer_name,
            "event_id": event.event_id,
            "event_kind": event.event_kind,
            "aggregate_id": event.record_id_i64(),
            "room": event.room,
            "integrity_sha256": event.integrity_sha256,
            "stream_sequence": stream_sequence_i64,
            "first_delivery_count": first_delivery_count,
        }))
        .fetch_one(&mut *transaction)
        .await?;
        let receipt = RecordedReceipt {
            disposition: ReceiptDisposition::Inserted,
            event_id: event.event_id,
            record_id: event.record_id_i64(),
            room: event.room.clone(),
            processed_at: inserted.try_get("processed_at")?,
            original_stream_sequence: stream_sequence,
            integrity_sha256: event.integrity_sha256.clone(),
        };
        transaction.commit().await?;
        Ok(receipt)
    }

    pub async fn insert_consumer_dead_letter(
        &self,
        event_id: Option<Uuid>,
        subject: &str,
        reason_code: &str,
        reason: &str,
        payload: &[u8],
        delivery_count: i64,
    ) -> Result<()> {
        let mut transaction = self.pool.begin().await?;
        self.insert_dead_letter_tx(
            &mut transaction,
            event_id,
            "consumer",
            Some(subject),
            reason_code,
            reason,
            payload,
            Some(delivery_count),
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_dead_letter_tx(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        event_id: Option<Uuid>,
        source: &str,
        subject: Option<&str>,
        reason_code: &str,
        reason: &str,
        payload: &[u8],
        delivery_count: Option<i64>,
    ) -> Result<()> {
        // Keys are crane_dead_letters columns (0016_boat_ready_delivery.sql:75-90;
        // subject lost NOT NULL at 0017:209), matched by NAME. Its two DEFAULT
        // columns merge below because SELECT * nulls them; later ones must too.
        sqlx::query(
            "INSERT INTO crane_dead_letters
             SELECT * FROM jsonb_populate_record(
               NULL::crane_dead_letters,
               $1::jsonb || jsonb_build_object(
                 'dead_letter_id', gen_random_uuid(), 'observed_at', NOW()
               )
             )
             ON CONFLICT (source, event_id, payload_sha256, reason_code)
             DO UPDATE SET observed_at = NOW(), delivery_count = GREATEST(
               crane_dead_letters.delivery_count, EXCLUDED.delivery_count
             )",
        )
        .bind(serde_json::json!({
            "event_id": event_id,
            "source": source,
            "subject": subject,
            "reason_code": reason_code,
            "reason": bounded_reason(reason),
            "payload_sha256": sea::payload_digest(payload),
            "payload_bytes": i32::try_from(payload.len()).unwrap_or(i32::MAX),
            "delivery_count": delivery_count.and_then(|count| i32::try_from(count).ok()),
        }))
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }

    pub async fn health(&self) -> Result<DatabaseHealth> {
        self.require_schema().await?;
        let row = sqlx::query(
            "SELECT
               (SELECT max(version)::integer FROM schema_migrations) AS schema_version,
               count(*) FILTER (WHERE state = 'pending') AS pending,
               count(*) FILTER (WHERE state = 'leased') AS leased,
               count(*) FILTER (WHERE state = 'published') AS published,
               (SELECT count(*) FROM crane_dead_letters) AS dead_letters,
               (SELECT count(*) FROM crane_receipts) AS receipts
             FROM crane_outbox",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(DatabaseHealth {
            schema_version: row.try_get("schema_version")?,
            pending: row.try_get("pending")?,
            leased: row.try_get("leased")?,
            published: row.try_get("published")?,
            dead_letters: row.try_get("dead_letters")?,
            receipts: row.try_get("receipts")?,
        })
    }
}

fn lane_consumer_name(lane: &Lane) -> &'static str {
    match lane {
        Lane::BoatReady => BOAT_READY_CONSUMER_NAME,
        Lane::Addressed { .. } => CRANE_CONSUMER_NAME,
    }
}

fn publish_retry_seconds(attempt: i32) -> i32 {
    match attempt {
        ..=1 => 1,
        2 => 5,
        3 => 15,
        4 => 30,
        5 => 60,
        6 => 120,
        7 => 300,
        _ => 600,
    }
}

fn bounded_reason(reason: &str) -> String {
    reason.chars().take(512).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publisher_retry_is_bounded_and_monotonic() {
        let delays: Vec<_> = (1..MAX_PUBLISH_ATTEMPTS)
            .map(publish_retry_seconds)
            .collect();
        assert!(delays.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(delays[0], 1);
        assert_eq!(*delays.last().unwrap(), 600);
    }

    #[test]
    fn durable_error_text_is_bounded() {
        assert_eq!(bounded_reason(&"x".repeat(700)).len(), 512);
    }

    #[test]
    fn claimed_columns_name_the_lane_postgresql_recorded() {
        let mut claimed = ClaimedEvent {
            event_id: Uuid::nil(),
            event_kind: "boat.ready".into(),
            aggregate_id: 42,
            room: "kintsu".into(),
            payload: Value::Null,
            integrity_sha256: "a".repeat(64),
            attempts: 1,
            recipient_kind: None,
            recipient_key: None,
            expires_at: None,
        };
        assert_eq!(claimed.lane(), Some(Lane::BoatReady));
        assert_eq!(claimed.subject().as_deref(), Some("athanor.boat.ready"));
        assert_eq!(
            lane_consumer_name(&claimed.lane().unwrap()),
            BOAT_READY_CONSUMER_NAME
        );

        claimed.event_kind = "crane.letter".into();
        claimed.recipient_kind = Some("room".into());
        claimed.recipient_key = Some("kodo".into());
        assert_eq!(
            claimed.subject().as_deref(),
            Some("athanor.crane.room.kodo")
        );
        assert_eq!(
            lane_consumer_name(&claimed.lane().unwrap()),
            CRANE_CONSUMER_NAME
        );

        claimed.recipient_key = None;
        assert_eq!(claimed.lane(), None, "half addressing is unroutable");
        claimed.recipient_kind = Some("ghost".into());
        claimed.recipient_key = Some("kodo".into());
        assert_eq!(claimed.lane(), None, "unknown recipient kind is unroutable");
    }
}

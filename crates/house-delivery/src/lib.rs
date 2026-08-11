pub mod broker;
pub mod model;
pub mod store;

use anyhow::{Context, Result, bail};
use async_nats::jetstream::message::AckKind;
#[cfg(test)]
use broker::CONSUMER_NAME;
use broker::{Broker, CONSUMER_BACKOFF, MAX_DELIVER, SUBJECT};
use futures_util::StreamExt;
use house_protocol::{BOAT_RECEIPT_SCHEMA_VERSION, BoatReceiptProjection};
use model::{BoatReadyEvent, classify_invalid_payload, event_id_hint};
use serde::Serialize;
use std::time::Duration;
use store::{ClaimedEvent, ReceiptDisposition, RecordedReceipt, Store};
use uuid::Uuid;

#[derive(Clone)]
pub struct DeliveryService {
    store: Store,
    broker: Broker,
    lease_owner: Uuid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishOutcome {
    Idle,
    Published,
    RetryScheduled,
    DeadLettered,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsumeOutcome {
    Idle,
    Received,
    ReceiptReplay,
    DeadLettered,
    RetryRequested,
}

#[derive(Debug, Serialize)]
pub struct OnceOutcome {
    pub publisher: PublishOutcome,
    pub consumer: ConsumeOutcome,
}

#[derive(Debug, Serialize)]
pub struct DeliveryHealth {
    pub ok: bool,
    pub authority: &'static str,
    pub delivery: &'static str,
    pub database: store::DatabaseHealth,
    pub broker: broker::BrokerHealth,
}

impl DeliveryService {
    pub async fn connect(database_url: &str, nats_url: &str, lease_owner: Uuid) -> Result<Self> {
        let store = Store::connect(database_url).await?;
        let broker = Broker::connect(nats_url).await?;
        broker.configure().await?;
        Ok(Self {
            store,
            broker,
            lease_owner,
        })
    }

    pub fn new(store: Store, broker: Broker, lease_owner: Uuid) -> Self {
        Self {
            store,
            broker,
            lease_owner,
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn broker(&self) -> &Broker {
        &self.broker
    }

    pub async fn publish_once(&self) -> Result<PublishOutcome> {
        let Some(claimed) = self.store.claim_next(self.lease_owner).await? else {
            return Ok(PublishOutcome::Idle);
        };
        let payload = serde_json::to_vec(&claimed.payload)?;
        let event = match BoatReadyEvent::parse(&payload) {
            Ok(event) => event,
            Err(error) => {
                let reason_code = classify_invalid_payload(&payload);
                self.store
                    .dead_letter_claim(&claimed, self.lease_owner, reason_code, &error.to_string())
                    .await?;
                return Ok(PublishOutcome::DeadLettered);
            }
        };
        if let Err(error) = validate_claim(&claimed, &event) {
            self.store
                .dead_letter_claim(
                    &claimed,
                    self.lease_owner,
                    "record_mismatch",
                    &error.to_string(),
                )
                .await?;
            return Ok(PublishOutcome::DeadLettered);
        }

        let sequence = match self.broker.publish(event.event_id, payload).await {
            Ok(sequence) => sequence,
            Err(error) => {
                self.store
                    .mark_publish_failure(&claimed, self.lease_owner, &error.to_string())
                    .await?;
                return Ok(if claimed.attempts >= store::MAX_PUBLISH_ATTEMPTS {
                    PublishOutcome::DeadLettered
                } else {
                    PublishOutcome::RetryScheduled
                });
            }
        };

        // This update intentionally occurs only after the JetStream publish acknowledgement.
        // If it fails, the lease expires and PostgreSQL replays the same Nats-Msg-Id.
        self.store
            .mark_published(event.event_id, self.lease_owner, sequence)
            .await?;
        Ok(PublishOutcome::Published)
    }

    pub async fn consume_once(&self, wait: Duration) -> Result<ConsumeOutcome> {
        let consumer = self.broker.consumer().await?;
        let mut messages = consumer
            .fetch()
            .max_messages(1)
            .expires(wait)
            .messages()
            .await
            .context("open bounded boat.ready pull request")?;
        let Some(next) = messages.next().await else {
            return Ok(ConsumeOutcome::Idle);
        };
        let message =
            next.map_err(|error| anyhow::anyhow!("receive boat.ready JetStream message: {error}"))?;
        let info = message
            .info()
            .map_err(|error| anyhow::anyhow!("read JetStream delivery metadata: {error}"))?;
        let delivery_count = info.delivered;
        let stream_sequence = info.stream_sequence;
        let payload = message.payload.as_ref();

        if message.subject.as_str() != SUBJECT {
            self.store
                .insert_consumer_dead_letter(
                    event_id_hint(payload),
                    "unknown_event",
                    "message arrived on an unowned subject",
                    payload,
                    delivery_count,
                )
                .await?;
            message
                .ack_with(AckKind::Term)
                .await
                .map_err(|error| anyhow::anyhow!("terminate unknown delivery: {error}"))?;
            return Ok(ConsumeOutcome::DeadLettered);
        }

        let event = match BoatReadyEvent::parse(payload) {
            Ok(event) => event,
            Err(error) => {
                self.store
                    .insert_consumer_dead_letter(
                        event_id_hint(payload),
                        classify_invalid_payload(payload),
                        &error.to_string(),
                        payload,
                        delivery_count,
                    )
                    .await?;
                message
                    .ack_with(AckKind::Term)
                    .await
                    .map_err(|error| anyhow::anyhow!("terminate malformed delivery: {error}"))?;
                return Ok(ConsumeOutcome::DeadLettered);
            }
        };

        match self
            .store
            .record_receipt(&event, stream_sequence, delivery_count)
            .await
        {
            Ok(receipt) => {
                // The database commit is authoritative. Publish the same sanitized
                // projection on both insert and replay before acknowledging so a
                // process death in this gap heals without producing a second row.
                let projection = receipt_projection(&receipt);
                let projection_payload = serde_json::to_vec(&projection)
                    .context("serialize sanitized boat receipt projection")?;
                self.broker
                    .publish_receipt(receipt.event_id, projection_payload)
                    .await
                    .context("publish committed boat receipt projection before acknowledgement")?;
                message
                    .double_ack()
                    .await
                    .map_err(|error| anyhow::anyhow!("acknowledge durable receipt: {error}"))?;
                Ok(match receipt.disposition {
                    ReceiptDisposition::Inserted => ConsumeOutcome::Received,
                    ReceiptDisposition::Replayed => ConsumeOutcome::ReceiptReplay,
                })
            }
            Err(error) if poison_reason_code(&error).is_some() => {
                self.store
                    .insert_consumer_dead_letter(
                        Some(event.event_id),
                        poison_reason_code(&error).expect("matched poison error"),
                        &error.to_string(),
                        payload,
                        delivery_count,
                    )
                    .await?;
                message
                    .ack_with(AckKind::Term)
                    .await
                    .map_err(|error| anyhow::anyhow!("terminate poison delivery: {error}"))?;
                Ok(ConsumeOutcome::DeadLettered)
            }
            Err(error) if delivery_count >= MAX_DELIVER => {
                self.store
                    .insert_consumer_dead_letter(
                        Some(event.event_id),
                        "delivery_exhausted",
                        &error.to_string(),
                        payload,
                        delivery_count,
                    )
                    .await?;
                message
                    .ack_with(AckKind::Term)
                    .await
                    .map_err(|error| anyhow::anyhow!("terminate exhausted delivery: {error}"))?;
                Ok(ConsumeOutcome::DeadLettered)
            }
            Err(_) => {
                message
                    .ack_with(AckKind::Nak(Some(consumer_retry_delay(delivery_count))))
                    .await
                    .map_err(|error| anyhow::anyhow!("request delivery retry: {error}"))?;
                Ok(ConsumeOutcome::RetryRequested)
            }
        }
    }

    pub async fn once(&self, wait: Duration) -> Result<OnceOutcome> {
        Ok(OnceOutcome {
            publisher: self.publish_once().await?,
            consumer: self.consume_once(wait).await?,
        })
    }

    pub async fn health(&self) -> Result<DeliveryHealth> {
        let database = self.store.health().await?;
        let broker = self.broker.health().await?;
        Ok(DeliveryHealth {
            ok: true,
            authority: "postgresql",
            delivery: "nats-jetstream",
            database,
            broker,
        })
    }

    pub async fn run(&self) -> Result<()> {
        loop {
            let published = self.publish_once().await?;
            let consumed = self.consume_once(Duration::from_secs(1)).await?;
            if published == PublishOutcome::Idle && consumed == ConsumeOutcome::Idle {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
}

fn validate_claim(claimed: &ClaimedEvent, event: &BoatReadyEvent) -> Result<()> {
    if claimed.event_id != event.event_id
        || claimed.event_kind != event.event_kind
        || claimed.aggregate_id != event.record_id_i64()
        || claimed.room != event.room
        || claimed.integrity_sha256 != event.integrity_sha256
    {
        bail!("outbox columns and pointer payload disagree");
    }
    Ok(())
}

fn poison_reason_code(error: &anyhow::Error) -> Option<&'static str> {
    let message = error.to_string();
    ["receipt_conflict", "record_mismatch", "integrity_mismatch"]
        .into_iter()
        .find(|code| message.starts_with(code))
}

fn receipt_projection(receipt: &RecordedReceipt) -> BoatReceiptProjection {
    BoatReceiptProjection {
        schema_version: BOAT_RECEIPT_SCHEMA_VERSION,
        event_id: receipt.event_id.to_string(),
        record_id: receipt.record_id.to_string(),
        room: receipt.room.clone(),
        processed_at: receipt.processed_at.to_rfc3339(),
        original_stream_sequence: receipt.original_stream_sequence,
        integrity_sha256: receipt.integrity_sha256.clone(),
    }
}

fn consumer_retry_delay(delivery_count: i64) -> Duration {
    let index = usize::try_from(delivery_count.saturating_sub(1))
        .unwrap_or(usize::MAX)
        .min(CONSUMER_BACKOFF.len() - 1);
    CONSUMER_BACKOFF[index]
}

pub const CONFIGURATION_CONTRACT: &str = "PostgreSQL is authoritative. Set DATABASE_URL (required), SOLARISAEL_NATS_URL (default nats://127.0.0.1:4222), and optionally SOLARISAEL_DELIVERY_INSTANCE_ID (UUID). Commands: configure, publish-once, consume-once, once, run, health.";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn outbox_columns_must_match_the_strict_payload() {
        let event = BoatReadyEvent::parse(
            &serde_json::to_vec(&json!({
                "schema_version": 1,
                "event_id": "b93446b7-6a6c-4494-ac0c-97390faaca9c",
                "event_kind": "boat.ready",
                "record_id": "42",
                "room": "kintsu",
                "created_at": "2026-08-10T12:00:00Z",
                "integrity_sha256": "a".repeat(64)
            }))
            .unwrap(),
        )
        .unwrap();
        let claimed = ClaimedEvent {
            event_id: event.event_id,
            event_kind: event.event_kind.clone(),
            aggregate_id: 42,
            room: event.room.clone(),
            payload: Value::Null,
            integrity_sha256: event.integrity_sha256.clone(),
            attempts: 1,
        };
        assert!(validate_claim(&claimed, &event).is_ok());

        let mut falsified = claimed;
        falsified.aggregate_id = 43;
        assert!(validate_claim(&falsified, &event).is_err());
    }

    #[test]
    fn durable_receipt_projects_only_the_sanitized_pointer() {
        let receipt = RecordedReceipt {
            disposition: ReceiptDisposition::Replayed,
            event_id: Uuid::parse_str("8d2c04ae-ef20-4fbc-8141-d0259cbf495f").unwrap(),
            record_id: 42,
            room: "kintsu".into(),
            processed_at: chrono::DateTime::parse_from_rfc3339("2026-08-10T09:30:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            original_stream_sequence: 7,
            integrity_sha256: "a".repeat(64),
        };
        assert_eq!(
            serde_json::to_value(receipt_projection(&receipt)).unwrap(),
            json!({
                "schema_version": 1,
                "event_id": "8d2c04ae-ef20-4fbc-8141-d0259cbf495f",
                "record_id": "42",
                "room": "kintsu",
                "processed_at": "2026-08-10T09:30:00+00:00",
                "original_stream_sequence": 7,
                "integrity_sha256": "a".repeat(64)
            })
        );
    }

    #[test]
    fn poison_errors_map_only_to_constrained_dead_letter_codes() {
        for code in ["receipt_conflict", "record_mismatch", "integrity_mismatch"] {
            let error = anyhow::anyhow!("{code}: evidence");
            assert_eq!(poison_reason_code(&error), Some(code));
        }
        assert_eq!(
            poison_reason_code(&anyhow::anyhow!("database unavailable")),
            None
        );
        assert!(CONFIGURATION_CONTRACT.contains("PostgreSQL is authoritative"));
        assert!(CONFIGURATION_CONTRACT.contains(CONSUMER_NAME) == false);
        assert_eq!(consumer_retry_delay(1), CONSUMER_BACKOFF[0]);
        assert_eq!(
            consumer_retry_delay(MAX_DELIVER),
            *CONSUMER_BACKOFF.last().unwrap()
        );
    }
}

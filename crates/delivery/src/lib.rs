//! The delivery service: the composition that drives the crane shape.
//!
//! Every crane mechanic — lanes, envelope, outbox ledger, JetStream
//! broker — lives in `origami::cranes`. This crate owns only the loop
//! that walks them: claim, publish, consume, record, acknowledge. The
//! three modules below are re-export shims for existing callers.

pub mod broker;
pub mod model;
pub mod store;

use anyhow::{Context, Result, bail};
use async_nats::jetstream::{consumer::PullConsumer, message::AckKind};
#[cfg(test)]
use origami::cranes::broker::{BOAT_READY_CONSUMER_NAME, CRANE_CONSUMER_NAME};
use origami::{
    cranes::{
        broker::{
            BOAT_READY_SUBJECT, Broker, CONSUMER_BACKOFF, CRANE_SUBJECT_FILTER, MAX_DELIVER,
        },
        envelope::{CraneEvent, classify_invalid_payload, event_id_hint},
        lanes::Lane,
        outbox::{ClaimedEvent, ReceiptDisposition, RecordedReceipt, Store},
    },
    sea::subject_owns,
};
use chrono::Utc;
use futures_util::StreamExt;
use protocol::{BOAT_RECEIPT_SCHEMA_VERSION, BoatReceiptProjection};
use serde::Serialize;
use std::time::Duration;
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
    pub database: origami::cranes::outbox::DatabaseHealth,
    pub broker: origami::cranes::broker::BrokerHealth,
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

    pub async fn drain(&self) -> Result<()> {
        self.broker.drain().await
    }

    pub async fn publish_once(&self) -> Result<PublishOutcome> {
        let Some(claimed) = self.store.claim_next(self.lease_owner).await? else {
            return Ok(PublishOutcome::Idle);
        };
        let payload = serde_json::to_vec(&claimed.payload)?;
        let event = match CraneEvent::parse(&payload) {
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

        let subject = event.lane().subject();
        let sequence = match self.broker.publish(subject, event.event_id, payload).await {
            Ok(sequence) => sequence,
            Err(error) => {
                self.store
                    .mark_publish_failure(&claimed, self.lease_owner, &error.to_string())
                    .await?;
                return Ok(if claimed.attempts >= origami::cranes::outbox::MAX_PUBLISH_ATTEMPTS {
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

    /// Drains one message, boat.ready lane first, then the addressed Crane lane.
    pub async fn consume_once(&self, wait: Duration) -> Result<ConsumeOutcome> {
        let consumers = self.broker.consumers().await?;
        let boat = self
            .consume_lane(&consumers.boat_ready, BOAT_READY_SUBJECT, wait)
            .await?;
        if boat != ConsumeOutcome::Idle {
            return Ok(boat);
        }
        self.consume_lane(&consumers.crane, CRANE_SUBJECT_FILTER, wait)
            .await
    }

    async fn consume_lane(
        &self,
        consumer: &PullConsumer,
        filter: &str,
        wait: Duration,
    ) -> Result<ConsumeOutcome> {
        let mut messages = consumer
            .fetch()
            .max_messages(1)
            .expires(wait)
            .messages()
            .await
            .with_context(|| format!("open bounded {filter} pull request"))?;
        let Some(next) = messages.next().await else {
            return Ok(ConsumeOutcome::Idle);
        };
        let message =
            next.map_err(|error| anyhow::anyhow!("receive {filter} JetStream message: {error}"))?;
        let info = message
            .info()
            .map_err(|error| anyhow::anyhow!("read JetStream delivery metadata: {error}"))?;
        let delivery_count = info.delivered;
        let stream_sequence = info.stream_sequence;
        let payload = message.payload.as_ref();
        let subject = message.subject.as_str();

        // Exact subject ownership first: the subject names the lane, and the lane
        // chooses the parser that runs before the shared receipt ledger.
        let lane = Lane::from_subject(subject).filter(|_| subject_owns(subject, filter));
        let Some(lane) = lane else {
            self.store
                .insert_consumer_dead_letter(
                    event_id_hint(payload),
                    subject,
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
        };

        let event = match CraneEvent::parse(payload) {
            Ok(event) => event,
            Err(error) => {
                self.store
                    .insert_consumer_dead_letter(
                        event_id_hint(payload),
                        subject,
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
        if let Some(refusal) = lane_refusal(&lane, &event) {
            self.store
                .insert_consumer_dead_letter(
                    Some(event.event_id),
                    subject,
                    refusal,
                    "envelope does not belong to the subject it arrived on",
                    payload,
                    delivery_count,
                )
                .await?;
            message
                .ack_with(AckKind::Term)
                .await
                .map_err(|error| anyhow::anyhow!("terminate misrouted delivery: {error}"))?;
            return Ok(ConsumeOutcome::DeadLettered);
        }

        // Expiry is enforced before the ledger, so an expired Crane is never applied.
        if event.is_expired(Utc::now()) {
            self.store
                .insert_consumer_dead_letter(
                    Some(event.event_id),
                    subject,
                    "expired",
                    "crane expired before it was applied",
                    payload,
                    delivery_count,
                )
                .await?;
            message
                .ack_with(AckKind::Term)
                .await
                .map_err(|error| anyhow::anyhow!("terminate expired delivery: {error}"))?;
            return Ok(ConsumeOutcome::DeadLettered);
        }

        match self
            .store
            .record_receipt(&event, stream_sequence, delivery_count)
            .await
        {
            Ok(receipt) => {
                // The database commit is authoritative. The boat.ready lane publishes the
                // same sanitized projection on both insert and replay before acknowledging,
                // so a process death in this gap heals without producing a second row.
                if lane == Lane::BoatReady {
                    let projection = receipt_projection(&receipt);
                    let projection_payload = serde_json::to_vec(&projection)
                        .context("serialize sanitized boat receipt projection")?;
                    self.broker
                        .publish_receipt(receipt.event_id, projection_payload)
                        .await
                        .context(
                            "publish committed boat receipt projection before acknowledgement",
                        )?;
                }
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
                        subject,
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
                        subject,
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

fn validate_claim(claimed: &ClaimedEvent, event: &CraneEvent) -> Result<()> {
    if claimed.event_id != event.event_id
        || claimed.event_kind != event.event_kind
        || claimed.aggregate_id != event.record_id_i64()
        || claimed.room != event.room
        || claimed.integrity_sha256 != event.integrity_sha256
        || claimed.recipient_kind.as_deref() != event.recipient_kind.map(|kind| kind.as_str())
        || claimed.recipient_key != event.recipient_key
        || claimed.expires_at != event.expires_at
    {
        bail!("outbox columns and pointer payload disagree");
    }
    Ok(())
}

/// The dead-letter reason when a parsed envelope does not belong to the lane whose
/// subject delivered it, or `None` when subject and envelope agree.
fn lane_refusal(lane: &Lane, event: &CraneEvent) -> Option<&'static str> {
    let observed = event.lane();
    if *lane == observed {
        return None;
    }
    match (lane, &observed) {
        (Lane::Addressed { .. }, Lane::Addressed { .. }) => Some("recipient_mismatch"),
        _ => Some("unknown_event"),
    }
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

    fn boat_event() -> CraneEvent {
        CraneEvent::parse(
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
        .unwrap()
    }

    fn addressed_event() -> CraneEvent {
        CraneEvent::parse(
            &serde_json::to_vec(&json!({
                "schema_version": 1,
                "event_id": "0f6f6c1e-3d1a-4a3b-9d51-cf5c9f1c9a10",
                "event_kind": "crane.letter",
                "record_id": "42",
                "room": "kintsu",
                "created_at": "2026-08-10T12:00:00Z",
                "integrity_sha256": "a".repeat(64),
                "recipient_kind": "room",
                "recipient_key": "kodo",
                "expires_at": "2026-08-20T12:00:00Z"
            }))
            .unwrap(),
        )
        .unwrap()
    }

    fn claim_for(event: &CraneEvent) -> ClaimedEvent {
        ClaimedEvent {
            event_id: event.event_id,
            event_kind: event.event_kind.clone(),
            aggregate_id: 42,
            room: event.room.clone(),
            payload: Value::Null,
            integrity_sha256: event.integrity_sha256.clone(),
            attempts: 1,
            recipient_kind: event.recipient_kind.map(|kind| kind.as_str().to_owned()),
            recipient_key: event.recipient_key.clone(),
            expires_at: event.expires_at,
        }
    }

    #[test]
    fn outbox_columns_must_match_the_strict_payload() {
        let event = boat_event();
        let claimed = claim_for(&event);
        assert!(validate_claim(&claimed, &event).is_ok());

        let mut falsified = claim_for(&event);
        falsified.aggregate_id = 43;
        assert!(validate_claim(&falsified, &event).is_err());

        let addressed = addressed_event();
        assert!(validate_claim(&claim_for(&addressed), &addressed).is_ok());
        for falsify in [
            |claim: &mut ClaimedEvent| claim.recipient_kind = Some("worker".into()),
            |claim: &mut ClaimedEvent| claim.recipient_key = Some("other".into()),
            |claim: &mut ClaimedEvent| claim.expires_at = None,
        ] {
            let mut claim = claim_for(&addressed);
            falsify(&mut claim);
            assert!(validate_claim(&claim, &addressed).is_err());
        }
    }

    #[test]
    fn publisher_routes_each_lane_to_its_own_subject() {
        assert_eq!(boat_event().lane().subject(), BOAT_READY_SUBJECT);
        assert_eq!(
            addressed_event().lane().subject(),
            "athanor.crane.room.kodo"
        );
    }

    #[test]
    fn subject_and_envelope_must_name_the_same_lane() {
        let boat = boat_event();
        let addressed = addressed_event();
        assert_eq!(lane_refusal(&Lane::BoatReady, &boat), None);
        assert_eq!(lane_refusal(&addressed.lane(), &addressed), None);
        assert_eq!(
            lane_refusal(&Lane::BoatReady, &addressed),
            Some("unknown_event")
        );
        assert_eq!(
            lane_refusal(&addressed.lane(), &boat),
            Some("unknown_event")
        );
        assert_eq!(
            lane_refusal(
                &Lane::from_subject("athanor.crane.room.elsewhere").unwrap(),
                &addressed
            ),
            Some("recipient_mismatch")
        );
    }

    #[test]
    fn expiry_is_decided_against_the_envelope_deadline() {
        let addressed = addressed_event();
        assert!(!addressed.is_expired("2026-08-20T11:59:59Z".parse().unwrap()));
        assert!(addressed.is_expired("2026-08-20T12:00:00Z".parse().unwrap()));
        assert!(!boat_event().is_expired(Utc::now()));
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
        assert!(!CONFIGURATION_CONTRACT.contains(BOAT_READY_CONSUMER_NAME));
        assert!(!CONFIGURATION_CONTRACT.contains(CRANE_CONSUMER_NAME));
        assert_eq!(consumer_retry_delay(1), CONSUMER_BACKOFF[0]);
        assert_eq!(
            consumer_retry_delay(MAX_DELIVER),
            *CONSUMER_BACKOFF.last().unwrap()
        );
    }
}

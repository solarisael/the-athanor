
use anyhow::{Context, Result, bail};
use async_nats::{
    HeaderMap,
    jetstream::{
        self,
        consumer::{self, AckPolicy},
        stream::{DiscardPolicy, RetentionPolicy, StorageType},
    },
};
use std::time::Duration;
use uuid::Uuid;

// Live wire names — JetStream and house-host both hold them, so a
// change here is a migration, never a rename.
pub const BOAT_READY_STREAM_NAME: &str = "ATHANOR_BOAT_READY";
pub const BOAT_READY_SUBJECT: &str = "athanor.boat.ready";
pub const BOAT_READY_CONSUMER_NAME: &str = "athanor-boat-ready-receipts-v1";
pub const CRANE_STREAM_NAME: &str = "ATHANOR_CRANE";
pub const CRANE_SUBJECT_PREFIX: &str = "athanor.crane.";
pub const CRANE_SUBJECT_FILTER: &str = "athanor.crane.>";
pub const CRANE_CONSUMER_NAME: &str = "athanor-crane-receipts-v1";
pub use protocol::{
    BOAT_RECEIPT_STREAM_NAME as RECEIPT_STREAM_NAME, BOAT_RECEIPT_SUBJECT as RECEIPT_SUBJECT,
};
pub const STREAM_MAX_MESSAGES: i64 = 100_000;
pub const STREAM_MAX_BYTES: i64 = 512 * 1024 * 1024;
pub const STREAM_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub const DUPLICATE_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);
// The ack discipline is the House's, not the cranes': host builds its receipt
// replay consumer from the same protocol pins. Kept re-exported under the
// cranes names so callers that already say broker::ACK_WAIT keep working.
pub use protocol::{
    JETSTREAM_ACK_WAIT as ACK_WAIT, JETSTREAM_MAX_ACK_PENDING as MAX_ACK_PENDING,
    JETSTREAM_MAX_BATCH as MAX_BATCH, JETSTREAM_MAX_DELIVER as MAX_DELIVER,
    JETSTREAM_MAX_EXPIRES as MAX_EXPIRES, JETSTREAM_NUM_REPLICAS as NUM_REPLICAS,
};
pub const CONSUMER_BACKOFF: [Duration; 5] = [
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
    Duration::from_secs(300),
    Duration::from_secs(600),
];

#[derive(Clone)]
pub struct Broker {
    client: async_nats::Client,
    context: jetstream::Context,
}

impl Broker {
    pub async fn connect(url: &str) -> Result<Self> {
        let client = async_nats::connect(url)
            .await
            .with_context(|| format!("connect to NATS delivery endpoint {url}"))?;
        Ok(Self {
            context: jetstream::new(client.clone()),
            client,
        })
    }

    pub async fn drain(&self) -> Result<()> {
        self.client
            .drain()
            .await
            .context("drain NATS delivery connection")
    }

    pub fn boat_ready_stream_config() -> jetstream::stream::Config {
        Self::lane_stream_config(
            BOAT_READY_STREAM_NAME,
            "Delivery-only stream for PostgreSQL-authoritative paper-boat pointers",
            BOAT_READY_SUBJECT,
        )
    }

    pub fn crane_stream_config() -> jetstream::stream::Config {
        Self::lane_stream_config(
            CRANE_STREAM_NAME,
            "Delivery-only stream for PostgreSQL-authoritative addressed Crane pointers",
            CRANE_SUBJECT_FILTER,
        )
    }

    fn lane_stream_config(
        name: &str,
        description: &str,
        subject: &str,
    ) -> jetstream::stream::Config {
        jetstream::stream::Config {
            name: name.to_owned(),
            description: Some(description.to_owned()),
            subjects: vec![subject.to_owned()],
            retention: RetentionPolicy::Limits,
            discard: DiscardPolicy::Old,
            storage: StorageType::File,
            max_messages: STREAM_MAX_MESSAGES,
            max_bytes: STREAM_MAX_BYTES,
            max_age: STREAM_MAX_AGE,
            max_message_size: 4096,
            max_consumers: 1,
            num_replicas: 1,
            duplicate_window: DUPLICATE_WINDOW,
            deny_delete: true,
            deny_purge: true,
            ..Default::default()
        }
    }

    pub fn receipt_stream_config() -> jetstream::stream::Config {
        jetstream::stream::Config {
            name: RECEIPT_STREAM_NAME.to_owned(),
            description: Some(
                "Sanitized, idempotent projections of committed PostgreSQL paper-boat receipts"
                    .to_owned(),
            ),
            subjects: vec![RECEIPT_SUBJECT.to_owned()],
            retention: RetentionPolicy::Limits,
            discard: DiscardPolicy::Old,
            storage: StorageType::File,
            max_messages: STREAM_MAX_MESSAGES,
            max_bytes: STREAM_MAX_BYTES,
            max_age: STREAM_MAX_AGE,
            max_message_size: 4096,
            max_consumers: 64,
            num_replicas: 1,
            duplicate_window: DUPLICATE_WINDOW,
            deny_delete: true,
            deny_purge: true,
            ..Default::default()
        }
    }

    pub fn boat_ready_consumer_config() -> consumer::pull::Config {
        Self::lane_consumer_config(
            BOAT_READY_CONSUMER_NAME,
            "Durable PostgreSQL receipt writer for boat.ready pointer events",
            BOAT_READY_SUBJECT,
        )
    }

    pub fn crane_consumer_config() -> consumer::pull::Config {
        Self::lane_consumer_config(
            CRANE_CONSUMER_NAME,
            "Durable PostgreSQL receipt writer for addressed Crane pointer events",
            CRANE_SUBJECT_FILTER,
        )
    }

    fn lane_consumer_config(
        name: &str,
        description: &str,
        filter_subject: &str,
    ) -> consumer::pull::Config {
        consumer::pull::Config {
            durable_name: Some(name.to_owned()),
            name: Some(name.to_owned()),
            description: Some(description.to_owned()),
            ack_policy: AckPolicy::Explicit,
            ack_wait: ACK_WAIT,
            max_deliver: MAX_DELIVER,
            filter_subject: filter_subject.to_owned(),
            max_ack_pending: MAX_ACK_PENDING,
            max_batch: MAX_BATCH,
            max_expires: MAX_EXPIRES,
            num_replicas: NUM_REPLICAS,
            // The lane consumers are the durable ledger writers: their cursor
            // must survive a broker restart, so storage is file-backed.
            memory_storage: false,
            backoff: CONSUMER_BACKOFF.to_vec(),
            ..Default::default()
        }
    }

    pub async fn configure(&self) -> Result<LaneConsumers> {
        let expected_receipt_stream = Self::receipt_stream_config();
        let receipt_stream = self
            .context
            .get_or_create_stream(expected_receipt_stream.clone())
            .await
            .context("configure sanitized boat receipt JetStream stream")?;
        verify_stream_config(
            &receipt_stream.cached_info().config,
            &expected_receipt_stream,
        )?;
        Ok(LaneConsumers {
            boat_ready: self
                .lane_consumer(
                    Self::boat_ready_stream_config(),
                    Self::boat_ready_consumer_config(),
                )
                .await?,
            crane: self
                .lane_consumer(Self::crane_stream_config(), Self::crane_consumer_config())
                .await?,
        })
    }

    async fn lane_consumer(
        &self,
        expected_stream: jetstream::stream::Config,
        expected_consumer: consumer::pull::Config,
    ) -> Result<consumer::PullConsumer> {
        let stream = self
            .context
            .get_or_create_stream(expected_stream.clone())
            .await
            .with_context(|| format!("configure {} JetStream stream", expected_stream.name))?;
        verify_stream_config(&stream.cached_info().config, &expected_stream)?;
        let durable_name = expected_consumer
            .durable_name
            .clone()
            .expect("every lane consumer is durable");
        let consumer = stream
            .get_or_create_consumer(&durable_name, expected_consumer.clone())
            .await
            .with_context(|| format!("configure durable pull consumer {durable_name}"))?;
        verify_consumer_config(&consumer.cached_info().config, &expected_consumer)?;
        Ok(consumer)
    }

    pub async fn consumers(&self) -> Result<LaneConsumers> {
        self.configure().await
    }

    pub async fn publish(&self, subject: String, event_id: Uuid, payload: Vec<u8>) -> Result<u64> {
        let mut headers = HeaderMap::new();
        headers.insert("Nats-Msg-Id", event_id.to_string());
        let acknowledgement = self
            .context
            .publish_with_headers(subject.clone(), headers, payload.into())
            .await
            .with_context(|| format!("send {subject} publish request"))?
            .await
            .with_context(|| format!("receive {subject} JetStream publish acknowledgement"))?;
        Ok(acknowledgement.sequence)
    }
    pub async fn publish_receipt(&self, event_id: Uuid, payload: Vec<u8>) -> Result<u64> {
        let mut headers = HeaderMap::new();
        headers.insert("Nats-Msg-Id", event_id.to_string());
        let acknowledgement = self
            .context
            .publish_with_headers(RECEIPT_SUBJECT, headers, payload.into())
            .await
            .context("send sanitized boat receipt publish request")?
            .await
            .context("receive sanitized boat receipt JetStream acknowledgement")?;
        Ok(acknowledgement.sequence)
    }

    pub async fn health(&self) -> Result<BrokerHealth> {
        let consumers = self.configure().await?;
        Ok(BrokerHealth {
            boat_ready: lane_health(
                BOAT_READY_STREAM_NAME,
                BOAT_READY_SUBJECT,
                BOAT_READY_CONSUMER_NAME,
                consumers.boat_ready.cached_info(),
            ),
            crane: lane_health(
                CRANE_STREAM_NAME,
                CRANE_SUBJECT_FILTER,
                CRANE_CONSUMER_NAME,
                consumers.crane.cached_info(),
            ),
            receipt_stream: RECEIPT_STREAM_NAME,
            receipt_subject: RECEIPT_SUBJECT,
        })
    }
}

pub struct LaneConsumers {
    pub boat_ready: consumer::PullConsumer,
    pub crane: consumer::PullConsumer,
}

#[derive(Debug, serde::Serialize)]
pub struct BrokerHealth {
    pub boat_ready: LaneHealth,
    pub crane: LaneHealth,
    pub receipt_stream: &'static str,
    pub receipt_subject: &'static str,
}

#[derive(Debug, serde::Serialize)]
pub struct LaneHealth {
    pub stream: &'static str,
    pub subject: &'static str,
    pub consumer: &'static str,
    pub pending: u64,
    pub ack_pending: usize,
    pub redelivered: usize,
}

fn lane_health(
    stream: &'static str,
    subject: &'static str,
    consumer: &'static str,
    info: &consumer::Info,
) -> LaneHealth {
    LaneHealth {
        stream,
        subject,
        consumer,
        pending: info.num_pending,
        ack_pending: info.num_ack_pending,
        redelivered: info.num_redelivered,
    }
}

fn verify_stream_config(
    actual: &jetstream::stream::Config,
    expected: &jetstream::stream::Config,
) -> Result<()> {
    if actual.name != expected.name
        || actual.subjects != expected.subjects
        || actual.retention != expected.retention
        || actual.discard != expected.discard
        || actual.storage != expected.storage
        || actual.max_messages != expected.max_messages
        || actual.max_bytes != expected.max_bytes
        || actual.max_age != expected.max_age
        || actual.max_message_size != expected.max_message_size
        || actual.max_consumers != expected.max_consumers
        || actual.num_replicas != expected.num_replicas
        || actual.duplicate_window != expected.duplicate_window
        || actual.deny_delete != expected.deny_delete
        || actual.deny_purge != expected.deny_purge
    {
        bail!(
            "existing JetStream stream {} does not match the compiled delivery contract",
            expected.name
        );
    }
    Ok(())
}

fn verify_consumer_config(
    actual: &consumer::Config,
    expected: &consumer::pull::Config,
) -> Result<()> {
    if actual.durable_name != expected.durable_name
        || actual.name != expected.name
        || actual.ack_policy != expected.ack_policy
        || actual.ack_wait != expected.ack_wait
        || actual.max_deliver != expected.max_deliver
        || actual.filter_subject != expected.filter_subject
        || actual.max_ack_pending != expected.max_ack_pending
        || actual.max_batch != expected.max_batch
        || actual.max_expires != expected.max_expires
        || actual.num_replicas != expected.num_replicas
        || actual.memory_storage != expected.memory_storage
        || actual.backoff != expected.backoff
    {
        bail!(
            "existing JetStream consumer {:?} does not match the compiled delivery contract",
            expected.durable_name
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sea::subject_owns;

    #[test]
    fn both_lane_contracts_are_bounded_and_durable() {
        for stream in [
            Broker::boat_ready_stream_config(),
            Broker::crane_stream_config(),
        ] {
            assert_eq!(stream.storage, StorageType::File);
            assert_eq!(stream.retention, RetentionPolicy::Limits);
            assert_eq!(stream.duplicate_window, DUPLICATE_WINDOW);
            assert!(stream.max_messages > 0);
            assert!(stream.max_bytes > 0);
            assert!(stream.max_age > DUPLICATE_WINDOW);
        }
        let boat = Broker::boat_ready_stream_config();
        let crane = Broker::crane_stream_config();
        assert_eq!(boat.subjects, vec![BOAT_READY_SUBJECT.to_owned()]);
        assert_eq!(crane.subjects, vec![CRANE_SUBJECT_FILTER.to_owned()]);
        assert_ne!(boat.name, crane.name);

        let receipt_stream = Broker::receipt_stream_config();
        assert_eq!(receipt_stream.subjects, vec![RECEIPT_SUBJECT.to_owned()]);
        assert_ne!(receipt_stream.name, boat.name);
        assert_eq!(receipt_stream.storage, StorageType::File);
        assert_eq!(receipt_stream.duplicate_window, DUPLICATE_WINDOW);
        assert_eq!(receipt_stream.max_message_size, 4096);
        assert_eq!(receipt_stream.max_consumers, 64);

        for consumer in [
            Broker::boat_ready_consumer_config(),
            Broker::crane_consumer_config(),
        ] {
            assert_eq!(consumer.ack_policy, AckPolicy::Explicit);
            assert_eq!(consumer.max_deliver, CONSUMER_BACKOFF.len() as i64);
            assert_eq!(consumer.ack_wait, CONSUMER_BACKOFF[0]);
            assert_eq!(consumer.durable_name, consumer.name);
        }
        assert_eq!(
            Broker::boat_ready_consumer_config().filter_subject,
            BOAT_READY_SUBJECT
        );
        assert_eq!(
            Broker::crane_consumer_config().filter_subject,
            CRANE_SUBJECT_FILTER
        );
    }

    #[test]
    fn consumer_filters_own_exactly_their_lane() {
        assert!(subject_owns(BOAT_READY_SUBJECT, BOAT_READY_SUBJECT));
        assert!(!subject_owns("athanor.boat.readyish", BOAT_READY_SUBJECT));
        assert!(subject_owns("athanor.crane.room.kodo", CRANE_SUBJECT_FILTER));
        assert!(!subject_owns(BOAT_READY_SUBJECT, CRANE_SUBJECT_FILTER));
        assert!(!subject_owns("athanor.crane.", CRANE_SUBJECT_FILTER));
    }
}

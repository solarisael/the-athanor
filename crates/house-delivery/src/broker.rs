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

pub const STREAM_NAME: &str = "ATHANOR_BOAT_READY";
pub const SUBJECT: &str = "athanor.boat.ready";
pub const CONSUMER_NAME: &str = "athanor-boat-ready-receipts-v1";
pub use house_protocol::{
    BOAT_RECEIPT_STREAM_NAME as RECEIPT_STREAM_NAME, BOAT_RECEIPT_SUBJECT as RECEIPT_SUBJECT,
};
pub const STREAM_MAX_MESSAGES: i64 = 100_000;
pub const STREAM_MAX_BYTES: i64 = 512 * 1024 * 1024;
pub const STREAM_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub const DUPLICATE_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);
pub const ACK_WAIT: Duration = Duration::from_secs(30);
pub const MAX_DELIVER: i64 = 5;
pub const MAX_ACK_PENDING: i64 = 64;
pub const CONSUMER_BACKOFF: [Duration; 5] = [
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
    Duration::from_secs(300),
    Duration::from_secs(600),
];

#[derive(Clone)]
pub struct Broker {
    context: jetstream::Context,
}

impl Broker {
    pub async fn connect(url: &str) -> Result<Self> {
        let client = async_nats::connect(url)
            .await
            .with_context(|| format!("connect to NATS delivery endpoint {url}"))?;
        Ok(Self {
            context: jetstream::new(client),
        })
    }

    pub fn stream_config() -> jetstream::stream::Config {
        jetstream::stream::Config {
            name: STREAM_NAME.to_owned(),
            description: Some(
                "Delivery-only stream for PostgreSQL-authoritative paper-boat pointers".to_owned(),
            ),
            subjects: vec![SUBJECT.to_owned()],
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

    pub fn consumer_config() -> consumer::pull::Config {
        consumer::pull::Config {
            durable_name: Some(CONSUMER_NAME.to_owned()),
            name: Some(CONSUMER_NAME.to_owned()),
            description: Some(
                "Durable PostgreSQL receipt writer for boat.ready pointer events".to_owned(),
            ),
            ack_policy: AckPolicy::Explicit,
            ack_wait: ACK_WAIT,
            max_deliver: MAX_DELIVER,
            filter_subject: SUBJECT.to_owned(),
            max_ack_pending: MAX_ACK_PENDING,
            max_batch: 64,
            max_expires: Duration::from_secs(5),
            num_replicas: 1,
            memory_storage: false,
            backoff: CONSUMER_BACKOFF.to_vec(),
            ..Default::default()
        }
    }

    pub async fn configure(&self) -> Result<consumer::PullConsumer> {
        let expected_stream = Self::stream_config();
        let stream = self
            .context
            .get_or_create_stream(expected_stream.clone())
            .await
            .context("configure boat.ready JetStream stream")?;
        verify_stream_config(&stream.cached_info().config, &expected_stream)?;
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

        let expected_consumer = Self::consumer_config();
        let consumer = stream
            .get_or_create_consumer(CONSUMER_NAME, expected_consumer.clone())
            .await
            .context("configure durable boat.ready pull consumer")?;
        verify_consumer_config(&consumer.cached_info().config, &expected_consumer)?;
        Ok(consumer)
    }

    pub async fn consumer(&self) -> Result<consumer::PullConsumer> {
        self.configure().await
    }

    pub async fn publish(&self, event_id: Uuid, payload: Vec<u8>) -> Result<u64> {
        let mut headers = HeaderMap::new();
        headers.insert("Nats-Msg-Id", event_id.to_string());
        let acknowledgement = self
            .context
            .publish_with_headers(SUBJECT, headers, payload.into())
            .await
            .context("send boat.ready publish request")?
            .await
            .context("receive boat.ready JetStream publish acknowledgement")?;
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
        let consumer = self.configure().await?;
        let info = consumer.cached_info();
        Ok(BrokerHealth {
            stream: STREAM_NAME,
            subject: SUBJECT,
            consumer: CONSUMER_NAME,
            receipt_stream: RECEIPT_STREAM_NAME,
            receipt_subject: RECEIPT_SUBJECT,
            pending: info.num_pending,
            ack_pending: info.num_ack_pending,
            redelivered: info.num_redelivered,
        })
    }
}

#[derive(Debug, serde::Serialize)]
pub struct BrokerHealth {
    pub stream: &'static str,
    pub subject: &'static str,
    pub consumer: &'static str,
    pub receipt_stream: &'static str,
    pub receipt_subject: &'static str,
    pub pending: u64,
    pub ack_pending: usize,
    pub redelivered: usize,
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
            "existing JetStream consumer {CONSUMER_NAME} does not match the compiled delivery contract"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broker_contract_is_bounded_and_durable() {
        let stream = Broker::stream_config();
        assert_eq!(stream.subjects, vec![SUBJECT.to_owned()]);
        assert_eq!(stream.storage, StorageType::File);
        assert_eq!(stream.retention, RetentionPolicy::Limits);
        assert_eq!(stream.duplicate_window, DUPLICATE_WINDOW);
        assert!(stream.max_messages > 0);
        assert!(stream.max_bytes > 0);
        assert!(stream.max_age > DUPLICATE_WINDOW);

        let receipt_stream = Broker::receipt_stream_config();
        assert_eq!(receipt_stream.subjects, vec![RECEIPT_SUBJECT.to_owned()]);
        assert_ne!(receipt_stream.name, stream.name);
        assert_eq!(receipt_stream.storage, StorageType::File);
        assert_eq!(receipt_stream.duplicate_window, DUPLICATE_WINDOW);
        assert_eq!(receipt_stream.max_message_size, 4096);
        assert_eq!(receipt_stream.max_consumers, 64);

        let consumer = Broker::consumer_config();
        assert_eq!(consumer.durable_name.as_deref(), Some(CONSUMER_NAME));
        assert_eq!(consumer.ack_policy, AckPolicy::Explicit);
        assert_eq!(consumer.filter_subject, SUBJECT);
        assert_eq!(consumer.max_deliver, CONSUMER_BACKOFF.len() as i64);
        assert_eq!(consumer.ack_wait, CONSUMER_BACKOFF[0]);
    }
}

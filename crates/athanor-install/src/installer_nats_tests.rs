use super::{RuntimeSecrets, nats_server_config};
use anyhow::{Context, Result};
use futures_util::StreamExt;
use origami::cranes::broker::{
    Broker, NatsAuth, RECEIPT_STREAM_NAME, RECEIPT_SUBJECT, connect_options,
};
use origami::hallways::sea::HallwayPostProjection;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::sync::mpsc;

struct OwnedBroker(Child);

impl Drop for OwnedBroker {
    fn drop(&mut self) {
        if let Err(error) = self.0.kill() {
            eprintln!("cannot stop test-owned NATS process: {error}");
        }
        if let Err(error) = self.0.wait() {
            eprintln!("cannot observe test-owned NATS exit: {error}");
        }
    }
}

async fn permission_refusal(errors: &mut mpsc::UnboundedReceiver<String>) -> Result<()> {
    let error = tokio::time::timeout(Duration::from_secs(3), errors.recv())
        .await?
        .context("NATS error channel closed before the refusal")?;
    anyhow::ensure!(error.to_lowercase().contains("permission"), "{error}");
    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_NATS_TEST_SERVER pointing to the real NATS 2.14.4 executable"]
async fn generated_credentials_enforce_delivery_and_reply_boundaries() -> Result<()> {
    let executable = std::env::var_os("ATHANOR_NATS_TEST_SERVER")
        .context("ATHANOR_NATS_TEST_SERVER is required")?;
    let directory = tempfile::tempdir()?;
    let auth = |username: &str| NatsAuth {
        username: username.into(),
        password: format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        ),
    };
    let host_auth = auth("athanor-host");
    let producer_auth = auth("athanor-akasha");
    let secrets = RuntimeSecrets {
        host_token: String::new(),
        postgres_password: String::new(),
        external_database_url: None,
        host_nats_auth: Some(host_auth.clone()),
        akasha_nats_auth: Some(producer_auth.clone()),
    };
    let config = directory.path().join("nats-server.conf");
    std::fs::write(&config, nats_server_config(&secrets)?)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    let mut broker_process = OwnedBroker(
        Command::new(executable)
            .args([
                "-js",
                "-a",
                "127.0.0.1",
                "-p",
                &address.port().to_string(),
                "-c",
            ])
            .arg(&config)
            .arg("-sd")
            .arg(directory.path().join("store"))
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?,
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(status) = broker_process.0.try_wait()? {
                anyhow::bail!("test NATS exited before readiness: {status}");
            }
            if tokio::net::TcpStream::connect(address).await.is_ok() {
                return Ok::<(), anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await??;
    let url = format!("nats://{address}");
    assert!(Broker::connect(&url, None).await.is_err());
    assert!(
        Broker::connect(&url, Some(&auth("athanor-host")))
            .await
            .is_err()
    );

    let host = Broker::connect(&url, Some(&host_auth)).await?;
    let _lanes = host.configure().await?;
    let client = connect_options(Some(&host_auth)).connect(&url).await?;
    let context = async_nats::jetstream::new(client.clone());
    let hallway = Broker::hallway_consumer(&context, "proof-room").await?;
    let receipts = context.get_stream(RECEIPT_STREAM_NAME).await?;
    let _receipt_consumer = receipts
        .create_consumer(async_nats::jetstream::consumer::pull::Config {
            name: Some(format!(
                "athanor-host-receipts-{}",
                uuid::Uuid::new_v4().simple()
            )),
            filter_subject: RECEIPT_SUBJECT.into(),
            ..Default::default()
        })
        .await?;
    let producer = Broker::connect(&url, Some(&producer_auth)).await?;
    let projection = HallwayPostProjection {
        schema_version: 1,
        hallway: "proof-hallway".into(),
        sequence: 1,
        message_id: 1,
        from_room: "sender-room".into(),
        from_spirit: "Sender".into(),
        created_at: "2026-10-05T00:00:00Z".into(),
        to_rooms: vec!["proof-room".into()],
    };
    producer
        .publish_hallway(&projection, &[], "proof-message")
        .await?;
    let mut messages = hallway
        .fetch()
        .max_messages(1)
        .expires(Duration::from_secs(2))
        .messages()
        .await?;
    let message = tokio::time::timeout(Duration::from_secs(3), messages.next())
        .await?
        .context("Hallway trigger was not delivered")?
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    assert_eq!(
        serde_json::from_slice::<HallwayPostProjection>(&message.payload)?,
        projection
    );
    message
        .double_ack()
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    let mut hosted_projection = projection.clone();
    hosted_projection.sequence = 2;
    hosted_projection.message_id = 2;
    host.publish_hallway(&hosted_projection, &[], "proof-host-message")
        .await?;
    let mut hosted_messages = hallway
        .fetch()
        .max_messages(1)
        .expires(Duration::from_secs(2))
        .messages()
        .await?;
    let hosted_message = tokio::time::timeout(Duration::from_secs(3), hosted_messages.next())
        .await?
        .context("Host-owned Hallway trigger was not delivered")?
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    assert_eq!(
        serde_json::from_slice::<HallwayPostProjection>(&hosted_message.payload)?,
        hosted_projection
    );
    hosted_message
        .double_ack()
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    let (events, mut errors) = mpsc::unbounded_channel();
    let restricted = connect_options(Some(&producer_auth))
        .event_callback(move |event| {
            let events = events.clone();
            async move {
                if let async_nats::Event::ServerError(error) = event {
                    let _ = events.send(error.to_string());
                }
            }
        })
        .connect(&url)
        .await?;
    for subject in [
        "athanor.boat.ready",
        "$JS.API.STREAM.CREATE.UNOWNED",
        "unrelated.proof",
    ] {
        restricted.publish(subject, "forbidden".into()).await?;
        restricted.flush().await?;
        permission_refusal(&mut errors).await?;
    }
    for subject in ["athanor.boat.ready", "_INBOX.athanor-host.>"] {
        let _subscription = restricted.subscribe(subject).await?;
        restricted.flush().await?;
        permission_refusal(&mut errors).await?;
    }
    restricted.drain().await?;
    producer.drain().await?;
    client.drain().await?;
    host.drain().await?;
    Ok(())
}

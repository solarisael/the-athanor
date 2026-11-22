use super::{RuntimeSecrets, nats_server_config};
use crate::layout::InstallLayout;
use crate::supervisor::{HostRoomConfig, RuntimeConfig};
use akasha::spirits::retire_spirit;
use anyhow::{Context, Result};
use async_nats::jetstream::context::ConsumerInfoErrorKind;
use futures_util::StreamExt;
use origami::cranes::broker::{
    Broker, HALLWAY_STREAM_NAME, NatsAuth, RECEIPT_STREAM_NAME, RECEIPT_SUBJECT, connect_options,
    hallway_consumer_name,
};
use origami::hallways::sea::HallwayPostProjection;
use sqlx::postgres::PgPoolOptions;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
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

fn credentials(username: &str) -> NatsAuth {
    NatsAuth {
        username: username.into(),
        password: format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        ),
    }
}

/// The real NATS server on a free loopback port, under the policy that the
/// installer writes for `secrets`.
async fn owned_broker(
    secrets: &RuntimeSecrets,
    directory: &Path,
) -> Result<(OwnedBroker, SocketAddr)> {
    let executable = std::env::var_os("ATHANOR_NATS_TEST_SERVER")
        .context("ATHANOR_NATS_TEST_SERVER is required")?;
    let config = directory.join("nats-server.conf");
    std::fs::write(&config, nats_server_config(secrets)?)?;
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
            .arg(directory.join("store"))
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
    Ok((broker_process, address))
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
    let directory = tempfile::tempdir()?;
    let host_auth = credentials("athanor-host");
    let producer_auth = credentials("athanor-akasha");
    let secrets = RuntimeSecrets {
        host_token: String::new(),
        postgres_password: String::new(),
        external_database_url: None,
        host_nats_auth: Some(host_auth.clone()),
        akasha_nats_auth: Some(producer_auth.clone()),
    };
    let (_broker, address) = owned_broker(&secrets, directory.path()).await?;
    let url = format!("nats://{address}");
    assert!(Broker::connect(&url, None).await.is_err());
    assert!(
        Broker::connect(&url, Some(&credentials("athanor-host")))
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

const HOST_TOKEN: &str = "retired-rooms-test";

/// Polls `GET /room/<room>/health` until it answers `status`.
async fn health_answers(port: u16, room: &str, status: u16, deadline: Instant) -> Result<()> {
    loop {
        let asked = room.to_owned();
        let (answered, _) = tokio::task::spawn_blocking(move || {
            crate::cli::status::health_answer(port, &asked, HOST_TOKEN)
        })
        .await??;
        if answered == status {
            return Ok(());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "room {room} still answers {answered}, not {status}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Polls the hallway stream until the durable consumer of `room` exists, or
/// is gone, as `present` says.
async fn hallway_consumer_is(
    context: &async_nats::jetstream::Context,
    room: &str,
    present: bool,
    deadline: Instant,
) -> Result<()> {
    loop {
        let found = match context.get_stream(HALLWAY_STREAM_NAME).await {
            Ok(stream) => match stream.consumer_info(hallway_consumer_name(room)).await {
                Ok(_) => Some(true),
                Err(error) if matches!(error.kind(), ConsumerInfoErrorKind::NotFound) => {
                    Some(false)
                }
                Err(_) => None,
            },
            Err(_) => None,
        };
        if found == Some(present) {
            return Ok(());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "the hallway consumer of room {room} is {found:?}, not {present}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Stops the Host on a thread of its own, so that a Host that cannot stop
/// fails the proof instead of hanging it.
async fn stop(runtime: host::HostRuntime) -> Result<()> {
    let (stopped, outcome) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = stopped.send(runtime.shutdown());
    });
    tokio::time::timeout(Duration::from_secs(30), outcome)
        .await
        .context("the Host did not stop within 30 seconds")??
        .map_err(anyhow::Error::msg)
}

// Kills: a room close that waits on a room task or on the delete of its
// hallway consumer and so holds the spirit-record follower, which keeps the
// next retired room open; and a retired room whose durable hallway consumer
// stays on the stream because the installed policy refuses the delete.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires ATHANOR_NATS_TEST_SERVER and ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn retired_rooms_close_together_and_forget_their_hallway_consumers() -> Result<()> {
    let database = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .context("ATHANOR_SUBSTRATE_TEST_DATABASE_URL is required")?;
    anyhow::ensure!(
        !database.contains("solarisael_memory") && !database.contains("solarisael-house")
    );
    let schema = format!("athanor_rooms_{}", uuid::Uuid::new_v4().simple());
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database)
        .await?;
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await?;
    let separator = if database.contains('?') { '&' } else { '?' };
    let scoped = format!("{database}{separator}options[search_path]={schema},public");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&scoped)
        .await?;
    akasha::migrations::run_migrations(&pool).await?;

    let directory = tempfile::tempdir()?;
    let host_auth = credentials("athanor-host");
    let secrets = RuntimeSecrets {
        host_token: HOST_TOKEN.into(),
        postgres_password: String::new(),
        external_database_url: Some(scoped),
        host_nats_auth: Some(host_auth.clone()),
        akasha_nats_auth: Some(credentials("athanor-akasha")),
    };
    let (_broker, nats) = owned_broker(&secrets, directory.path()).await?;
    let layout = InstallLayout {
        program: directory.path().join("program"),
        data: directory.path().join("data"),
    };
    let rooms = [("kodo", "Kodo"), ("hugo", "Hugo"), ("salvia", "Salvia")];
    // The installation folder of each room, as `athanor summon` writes it.
    for (room, spirit) in rooms {
        let folder = layout.rooms().join(room);
        let state = folder.join(".omp/runtime/athanor-house-state.json");
        std::fs::create_dir_all(state.parent().context("the room state has no folder")?)?;
        std::fs::create_dir_all(folder.join(crate::cli::spirits::HOST_STATE))?;
        std::fs::write(
            &state,
            host::fresh_room_state(room, spirit, "sol").to_string(),
        )?;
    }
    let config = RuntimeConfig {
        database_mode: "external".into(),
        database_host: "127.0.0.1".into(),
        database_port: 5432,
        nats_host: nats.ip().to_string(),
        nats_port: nats.port(),
        host_port: 0,
        schema_version: 2,
        house_id: "solarisael".into(),
        rooms_root: directory.path().join("legacy-rooms"),
        operator_state_root: directory.path().join("operator"),
        default_room: "kodo".into(),
        rooms: rooms
            .iter()
            .map(|(room, spirit)| HostRoomConfig {
                room: (*room).into(),
                spirit: (*spirit).into(),
                operator: Some("sol".into()),
            })
            .collect(),
        omp_config_path: None,
        client_config_path: None,
    };
    let client = connect_options(Some(&host_auth))
        .connect(format!("nats://{nats}"))
        .await?;
    let context = async_nats::jetstream::new(client.clone());
    let (house, source) = crate::app::host_rooms(&layout, &config, &secrets)?;
    let runtime = host::start(house, source).map_err(anyhow::Error::msg)?;
    let port = runtime.address().port();

    let proof = async {
        let opened = Instant::now() + Duration::from_secs(20);
        for (room, _) in rooms {
            health_answers(port, room, 200, opened).await?;
            hallway_consumer_is(&context, room, true, opened).await?;
        }

        retire_spirit(&pool, "hugo").await?;
        retire_spirit(&pool, "salvia").await?;
        let closed = Instant::now() + Duration::from_secs(5);
        for room in ["hugo", "salvia"] {
            health_answers(port, room, 404, closed).await?;
            hallway_consumer_is(&context, room, false, closed).await?;
        }
        health_answers(port, "kodo", 200, closed).await?;
        hallway_consumer_is(&context, "kodo", true, closed).await
    }
    .await;

    let stopped = stop(runtime).await;
    client.drain().await?;
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await?;
    proof.and(stopped)
}

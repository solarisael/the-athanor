use athanor_substrate::{
    IngestBatch, ObservationEvent, TrustedBinding, derive_idempotency_key_v1,
    derive_semantic_hash_v1,
};
use chrono::Utc;
use house_host::{Host, HostConfig};
use reqwest::StatusCode;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};
use uuid::Uuid;

const INSULA_MIGRATION: &str = include_str!("../../../substrate/migrations/0022_insula.sql");

fn isolated_database_url() -> String {
    let url = std::env::var("SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL")
        .expect("Host Insula proof requires a dedicated PostgreSQL URL");
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
        "refusing a live or production-looking database"
    );
    url
}

fn host_event(binding: &TrustedBinding) -> ObservationEvent {
    let mut event: ObservationEvent = serde_json::from_value(json!({
        "eventId": Uuid::new_v4().to_string(),
        "spanId": Uuid::new_v4().to_string(),
        "traceId": Uuid::new_v4().to_string(),
        "writerId": Uuid::new_v4().to_string(),
        "writerSequence": 1,
        "component": "host_boundary_test",
        "layer": "host",
        "operation": "ingest",
        "phase": "point",
        "observedAt": Utc::now().to_rfc3339(),
        "outcomeClass": "ok",
        "bytesIn": 0,
        "bytesOut": 0,
        "tokensIn": 0,
        "tokensOut": 0,
        "idempotencyVersion": 1,
        "idempotencyScope": "room_operation",
        "receiptKind": "test_receipt",
        "receiptId": "receipt-1",
        "dropCount": 0
    }))
    .expect("test observation must satisfy the strict public DTO");
    event.idempotency_key =
        derive_idempotency_key_v1(binding, &event).expect("test idempotency key");
    event.semantic_hash = derive_semantic_hash_v1(binding, &event).expect("test semantic hash");
    event
}

const TOKEN: &str = "test-only-insula-host-token";

struct RunningHost {
    address: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<Result<(), String>>,
}

impl RunningHost {
    async fn stop(mut self) {
        self.shutdown.take().expect("shutdown sender").send(()).ok();
        timeout(Duration::from_secs(2), self.task)
            .await
            .expect("Host shutdown timed out")
            .expect("Host task panicked")
            .expect("Host shutdown failed");
    }
}

fn write_room_state(root: &Path) {
    let runtime = root.join("room").join(".omp").join("runtime");
    std::fs::create_dir_all(&runtime).expect("create room runtime");
    std::fs::write(
        runtime.join("solarisael-house-state.json"),
        serde_json::to_vec(&json!({
            "version": 1,
            "room": "kintsu",
            "operator": "Sol",
            "recallPolicy": {
                "requestedMode": "auto",
                "resolvedMode": "conversation",
                "activeProject": null,
                "resolutionReason": "default",
                "lastRefreshReason": null,
                "lastRefreshAt": null,
                "workingSetEntries": 0,
                "recoveryPending": false,
                "recoveryTerms": [],
                "degraded": null,
                "updatedAt": null
            }
        }))
        .expect("serialize room state"),
    )
    .expect("write room state");
}

fn config(root: &Path) -> HostConfig {
    HostConfig {
        bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        ws_path: "/athanor/v1/ws".into(),
        bearer_token: TOKEN.into(),
        room_dir: root.join("room"),
        state_dir: root.join("host-state"),
        house_id: "solarisael".into(),
        room: "kintsu".into(),
        spirit: "Kintsu".into(),
        session: "configured-session".into(),
        recipient: "house-host".into(),
        database_url: None,
        akasha_enabled: false,
        nats_url: None,
    }
}

fn database_config(root: &Path, database_url: String) -> HostConfig {
    let mut configured = config(root);
    configured.database_url = Some(database_url);
    configured
}

async fn start(root: &Path) -> RunningHost {
    let host = Host::new(config(root)).expect("construct Host");
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("bind test Host");
    let address = listener.local_addr().expect("test Host address");
    let (shutdown, receiver) = oneshot::channel();
    let task = tokio::spawn(host.serve(listener, async move {
        let _ = receiver.await;
    }));
    RunningHost {
        address,
        shutdown: Some(shutdown),
        task,
    }
}

async fn start_with_config(config: HostConfig) -> RunningHost {
    let host = Host::new(config).expect("construct Host");
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("bind test Host");
    let address = listener.local_addr().expect("test Host address");
    let (shutdown, receiver) = oneshot::channel();
    let task = tokio::spawn(host.serve(listener, async move {
        let _ = receiver.await;
    }));
    RunningHost {
        address,
        shutdown: Some(shutdown),
        task,
    }
}

fn endpoint(host: &RunningHost, path: &str) -> String {
    format!("http://{}{}", host.address, path)
}

async fn error(response: reqwest::Response, expected: StatusCode) -> Value {
    assert_eq!(response.status(), expected);
    let body: Value = response.json().await.expect("mechanical error JSON");
    assert_eq!(body["schemaVersion"], 1);
    assert!(
        body["error"].is_string(),
        "error must remain a mechanical code"
    );
    body
}

#[test]
fn host_refuses_an_invalid_insula_binding_at_startup() {
    let root = TempDir::new().expect("temporary Host root");
    let mut configured = config(root.path());
    configured.house_id = "Caller Selected House".into();

    assert!(
        Host::new(configured).is_err(),
        "Host must fail startup instead of advertising an Insula binding every ingest will reject"
    );
}

#[tokio::test]
async fn insula_http_boundary_authenticates_before_parsing() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let client = reqwest::Client::new();

    let unauthenticated = client
        .post(endpoint(&host, "/athanor/v1/insula/events"))
        .body("this is not JSON")
        .send()
        .await
        .expect("unauthenticated request completes");
    let unauthenticated_body = error(unauthenticated, StatusCode::UNAUTHORIZED).await;
    assert_eq!(unauthenticated_body["error"], "unauthenticated");

    host.stop().await;
}

#[tokio::test]
async fn insula_http_boundary_refuses_oversized_ingest_bodies_without_reaching_database() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start(root.path()).await;

    let response = reqwest::Client::new()
        .post(endpoint(&host, "/athanor/v1/insula/events"))
        .bearer_auth(TOKEN)
        .header("content-type", "application/json")
        .body("x".repeat(512 * 1024 + 1))
        .send()
        .await
        .expect("oversized request completes");
    let body = error(response, StatusCode::PAYLOAD_TOO_LARGE).await;
    assert_eq!(body["error"], "body_too_large");

    host.stop().await;
}

#[tokio::test]
async fn insula_http_boundary_refuses_batches_larger_than_128_before_database_access() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let binding = TrustedBinding {
        house_id: "solarisael".into(),
        room: "kintsu".into(),
        spirit: "Kintsu".into(),
        session_id: "configured-session".into(),
    };
    let event = host_event(&binding);

    let response = reqwest::Client::new()
        .post(endpoint(&host, "/athanor/v1/insula/events"))
        .bearer_auth(TOKEN)
        .json(&IngestBatch {
            events: vec![event; 129],
        })
        .send()
        .await
        .expect("oversized batch request completes");
    let body = error(response, StatusCode::PAYLOAD_TOO_LARGE).await;
    assert_eq!(body["error"], "batch_too_large");

    host.stop().await;
}

#[tokio::test]
async fn health_reports_insula_unavailable_without_degrading_the_existing_host_surface() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start(root.path()).await;

    let response = reqwest::get(endpoint(&host, "/health"))
        .await
        .expect("health request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let health: Value = response.json().await.expect("health JSON");
    assert_eq!(health["status"], "ok");
    assert_eq!(health["insula"]["schemaVersion"], 1);
    assert_eq!(health["insula"]["status"], "unavailable");
    assert_eq!(health["insula"]["successfulOperations"], 0);
    assert_eq!(health["insula"]["failedOperations"], 0);

    host.stop().await;
}

#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Insula schema"]
async fn insula_ingest_stamps_only_host_config_identity() -> Result<(), Box<dyn std::error::Error>>
{
    let database_url = isolated_database_url();
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;
    sqlx::query("DROP SCHEMA IF EXISTS insula CASCADE")
        .execute(&pool)
        .await?;
    sqlx::raw_sql(INSULA_MIGRATION).execute(&pool).await?;

    let root = TempDir::new()?;
    write_room_state(root.path());
    let configured = database_config(root.path(), database_url);
    let binding = TrustedBinding {
        house_id: configured.house_id.clone(),
        room: configured.room.clone(),
        spirit: configured.spirit.clone(),
        session_id: configured.session.clone(),
    };
    let host = start_with_config(configured).await;

    let response = reqwest::Client::new()
        .post(endpoint(&host, "/athanor/v1/insula/events"))
        .bearer_auth(TOKEN)
        .json(&IngestBatch {
            events: vec![host_event(&binding)],
        })
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let receipt: Value = response.json().await?;
    assert_eq!(receipt["schemaVersion"], 1);
    assert_eq!(receipt["acceptedCount"], 1);
    assert_eq!(receipt["duplicateCount"], 0);

    let identity: (String, String, String, String) =
        sqlx::query_as("SELECT house_id, room, spirit, session_id FROM insula.log")
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        identity,
        (
            "solarisael".into(),
            "kintsu".into(),
            "Kintsu".into(),
            "configured-session".into()
        ),
        "the Host must stamp its configured binding; requests carry no authority fields"
    );

    host.stop().await;
    sqlx::query("DROP SCHEMA insula CASCADE")
        .execute(&pool)
        .await?;
    Ok(())
}

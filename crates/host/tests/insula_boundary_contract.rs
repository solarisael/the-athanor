use akasha::{
    IngestBatch, ObservationEvent, TrustedBinding, derive_idempotency_key_v1,
    derive_semantic_hash_v1,
};
use chrono::Utc;
use host::{Host, HostConfig, KnockAutonomy};
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
const RESTART_MIGRATION: &str = include_str!("../../../substrate/migrations/0026_restart.sql");

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
    host_event_with_operation(binding, "ingest")
}

fn host_event_with_operation(binding: &TrustedBinding, operation: &str) -> ObservationEvent {
    let mut event: ObservationEvent = serde_json::from_value(json!({
        "eventId": Uuid::new_v4().to_string(),
        "spanId": Uuid::new_v4().to_string(),
        "traceId": Uuid::new_v4().to_string(),
        "writerId": Uuid::new_v4().to_string(),
        "writerSequence": 1,
        "component": "host_boundary_test",
        "layer": "host",
        "operation": operation,
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
        knock_autonomy: KnockAutonomy::Off,
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

fn vitals_request() -> Value {
    json!({
        "component": "host_boundary_test",
        "layer": "host",
        "operation": "ingest",
        "phase": "point",
        "outcomeClass": "ok",
        "start": "2026-08-20T00:00:00Z",
        "end": "2026-08-20T01:00:00Z"
    })
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
async fn insula_vitals_authenticates_before_parsing_and_refuses_query_strings() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let client = reqwest::Client::new();

    let unauthenticated = client
        .post(endpoint(&host, "/athanor/v1/insula/vitals"))
        .body("this is not JSON")
        .send()
        .await
        .expect("unauthenticated Vitals request completes");
    let unauthenticated_body = error(unauthenticated, StatusCode::UNAUTHORIZED).await;
    assert_eq!(unauthenticated_body["error"], "unauthenticated");

    let unexpected_query = client
        .post(endpoint(
            &host,
            "/athanor/v1/insula/vitals?room=caller-selected",
        ))
        .bearer_auth(TOKEN)
        .body("this is also not JSON")
        .send()
        .await
        .expect("query-bearing Vitals request completes");
    let unexpected_query_body = error(unexpected_query, StatusCode::BAD_REQUEST).await;
    assert_eq!(unexpected_query_body["error"], "unexpected_query");

    host.stop().await;
}

#[tokio::test]
async fn insula_vitals_dto_refuses_unknown_and_authority_fields_before_database_access() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let client = reqwest::Client::new();

    for (field, value) in [
        ("unknownField", json!("not-in-the-contract")),
        ("houseId", json!("caller-house")),
        ("room", json!("caller-room")),
        ("spirit", json!("CallerSpirit")),
        ("sessionId", json!("caller-session")),
    ] {
        let mut request = vitals_request();
        request
            .as_object_mut()
            .expect("Vitals fixture is an object")
            .insert(field.into(), value);
        let response = client
            .post(endpoint(&host, "/athanor/v1/insula/vitals"))
            .bearer_auth(TOKEN)
            .json(&request)
            .send()
            .await
            .expect("strict Vitals request completes");
        let body = error(response, StatusCode::BAD_REQUEST).await;
        assert_eq!(
            body["error"], "invalid_json",
            "field {field} must be refused"
        );
    }

    host.stop().await;
}

#[tokio::test]
async fn insula_vitals_refuses_invalid_bounds_and_limits_before_database_access() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let client = reqwest::Client::new();
    let invalid_requests = [
        json!({
            "start": "2026-08-20T00:00:00Z",
            "end": "2026-08-20T00:00:00Z"
        }),
        json!({
            "start": "2026-08-20T01:00:00Z",
            "end": "2026-08-20T00:00:00Z"
        }),
        json!({
            "start": "2025-08-18T00:00:00Z",
            "end": "2026-08-20T00:00:00Z"
        }),
        json!({
            "start": "2026-08-20T00:00:00Z",
            "end": "2026-08-20T01:00:00Z",
            "limit": 0
        }),
        json!({
            "start": "2026-08-20T00:00:00Z",
            "end": "2026-08-20T01:00:00Z",
            "limit": 5000
        }),
    ];

    for request in invalid_requests {
        let response = client
            .post(endpoint(&host, "/athanor/v1/insula/vitals"))
            .bearer_auth(TOKEN)
            .json(&request)
            .send()
            .await
            .expect("invalid Vitals request completes");
        let body = error(response, StatusCode::UNPROCESSABLE_ENTITY).await;
        assert_eq!(body["error"], "invalid_request");
    }

    host.stop().await;
}

#[tokio::test]
async fn insula_vitals_reports_an_unavailable_pool_after_accepting_the_default_limit() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start(root.path()).await;

    let response = reqwest::Client::new()
        .post(endpoint(&host, "/athanor/v1/insula/vitals"))
        .bearer_auth(TOKEN)
        .json(&vitals_request())
        .send()
        .await
        .expect("valid Vitals request completes");
    let body = error(response, StatusCode::SERVICE_UNAVAILABLE).await;
    assert_eq!(body["error"], "insula_unavailable");

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
async fn insula_ingest_and_vitals_stamp_only_host_config_identity()
-> Result<(), Box<dyn std::error::Error>> {
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

    let client = reqwest::Client::new();
    let response = client
        .post(endpoint(&host, "/athanor/v1/insula/events"))
        .bearer_auth(TOKEN)
        .json(&IngestBatch {
            events: vec![
                host_event(&binding),
                host_event_with_operation(&binding, "second-operation"),
            ],
        })
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let receipt: Value = response.json().await?;
    assert_eq!(receipt["schemaVersion"], 1);
    assert_eq!(receipt["acceptedCount"], 2);
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

    let start = Utc::now() - chrono::Duration::hours(1);
    let end = Utc::now() + chrono::Duration::hours(1);
    let response = client
        .post(endpoint(&host, "/athanor/v1/insula/vitals"))
        .bearer_auth(TOKEN)
        .json(&json!({
            "start": start,
            "end": end,
            "limit": 1
        }))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let vitals: Value = response.json().await?;
    assert_eq!(vitals["schemaVersion"], 1);
    assert_eq!(vitals["queryName"], "insula.vitals.minute");
    assert_eq!(vitals["queryVersion"], 1);
    assert_eq!(vitals["houseId"], "solarisael");
    assert_eq!(vitals["room"], "kintsu");
    assert_eq!(vitals["spirit"], "Kintsu");
    assert_eq!(vitals["start"], json!(start));
    assert_eq!(vitals["end"], json!(end));
    assert_eq!(vitals["limit"], 1);
    assert_eq!(vitals["truncated"], true);
    assert_eq!(vitals["rows"].as_array().expect("Vitals rows").len(), 1);
    assert_eq!(vitals["rows"][0]["houseId"], "solarisael");
    assert_eq!(vitals["rows"][0]["room"], "kintsu");
    assert_eq!(vitals["rows"][0]["spirit"], "Kintsu");

    host.stop().await;
    sqlx::query("DROP SCHEMA insula CASCADE")
        .execute(&pool)
        .await?;
    Ok(())
}

fn traced_event(binding: &TrustedBinding, operation: &str, trace_id: &str) -> ObservationEvent {
    let mut event = host_event_with_operation(binding, operation);
    event.trace_id = trace_id.into();
    event.idempotency_key =
        derive_idempotency_key_v1(binding, &event).expect("test idempotency key");
    event.semantic_hash = derive_semantic_hash_v1(binding, &event).expect("test semantic hash");
    event
}

fn aged_event(
    binding: &TrustedBinding,
    operation: &str,
    observed_at: chrono::DateTime<Utc>,
) -> ObservationEvent {
    let mut event = host_event_with_operation(binding, operation);
    event.observed_at = observed_at;
    event.idempotency_key =
        derive_idempotency_key_v1(binding, &event).expect("test idempotency key");
    event.semantic_hash = derive_semantic_hash_v1(binding, &event).expect("test semantic hash");
    event
}

#[tokio::test]
async fn insula_reads_authenticate_before_parsing_and_refuse_query_strings() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let client = reqwest::Client::new();

    for path in ["/athanor/v1/insula/trace", "/athanor/v1/insula/retention"] {
        let unauthenticated = client
            .post(endpoint(&host, path))
            .body("this is not JSON")
            .send()
            .await
            .expect("unauthenticated read completes");
        let body = error(unauthenticated, StatusCode::UNAUTHORIZED).await;
        assert_eq!(body["error"], "unauthenticated", "{path} must authenticate");

        let unexpected_query = client
            .post(endpoint(&host, &format!("{path}?houseId=caller-house")))
            .bearer_auth(TOKEN)
            .body("this is also not JSON")
            .send()
            .await
            .expect("query-bearing read completes");
        let body = error(unexpected_query, StatusCode::BAD_REQUEST).await;
        assert_eq!(
            body["error"], "unexpected_query",
            "{path} must refuse a query string"
        );
    }

    host.stop().await;
}

#[tokio::test]
async fn insula_read_dtos_refuse_unknown_and_authority_fields_before_database_access() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let client = reqwest::Client::new();
    let trace = Uuid::new_v4().to_string();

    for (path, base) in [
        ("/athanor/v1/insula/trace", json!({ "traceId": trace })),
        ("/athanor/v1/insula/retention", json!({})),
    ] {
        for (field, value) in [
            ("unknownField", json!("not-in-the-contract")),
            ("houseId", json!("caller-house")),
            ("room", json!("caller-room")),
            ("spirit", json!("CallerSpirit")),
            ("sessionId", json!("caller-session")),
        ] {
            let mut request = base.clone();
            request
                .as_object_mut()
                .expect("read fixture is an object")
                .insert(field.into(), value);
            let response = client
                .post(endpoint(&host, path))
                .bearer_auth(TOKEN)
                .json(&request)
                .send()
                .await
                .expect("strict read request completes");
            let body = error(response, StatusCode::BAD_REQUEST).await;
            assert_eq!(
                body["error"], "invalid_json",
                "{path} must refuse field {field}"
            );
        }
    }

    // A Retention read carries no trace authority, and a Trace read cannot omit
    // the one identifier it is allowed to name.
    for (path, request) in [
        (
            "/athanor/v1/insula/retention",
            json!({ "traceId": Uuid::new_v4().to_string() }),
        ),
        ("/athanor/v1/insula/trace", json!({ "limit": 10 })),
    ] {
        let response = client
            .post(endpoint(&host, path))
            .bearer_auth(TOKEN)
            .json(&request)
            .send()
            .await
            .expect("strict read request completes");
        let body = error(response, StatusCode::BAD_REQUEST).await;
        assert_eq!(body["error"], "invalid_json", "{path} must refuse it");
    }

    host.stop().await;
}

// Kills: an unverified-exit route that lets a bearer choose the room or the
// workspace it reads. The scope belongs to the Host's own binding, because the
// rows carry another room's workspace path and requester session.
// red-proof: add a room or workspace field to UnverifiedExitRequest and pass it
// to query_unverified_exit.
#[tokio::test]
async fn unverified_exit_route_takes_no_scope_from_the_caller() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let client = reqwest::Client::new();
    let path = "/athanor/v1/insula/unverified-exit";

    let unauthenticated = client
        .post(endpoint(&host, path))
        .json(&json!({}))
        .send()
        .await
        .expect("unauthenticated read completes");
    error(unauthenticated, StatusCode::UNAUTHORIZED).await;

    for (field, value) in [
        ("room", json!("tuner")),
        ("workspace", json!("D:/athanor-wt/somebody-else")),
        ("requesterSession", json!("caller-session")),
        ("houseId", json!("caller-house")),
        ("unknownField", json!(true)),
    ] {
        let response = client
            .post(endpoint(&host, path))
            .bearer_auth(TOKEN)
            .json(&json!({ field: value }))
            .send()
            .await
            .expect("strict read request completes");
        let body = error(response, StatusCode::BAD_REQUEST).await;
        assert_eq!(
            body["error"], "invalid_json",
            "the divergence read must refuse field {field}"
        );
    }

    // The empty body is the whole legal request: limit defaults, scope comes
    // from configuration, and the read stops at the missing pool.
    let accepted = client
        .post(endpoint(&host, path))
        .bearer_auth(TOKEN)
        .json(&json!({}))
        .send()
        .await
        .expect("default read completes");
    let body = error(accepted, StatusCode::SERVICE_UNAVAILABLE).await;
    assert_eq!(body["error"], "insula_unavailable");

    host.stop().await;
}

#[tokio::test]
async fn insula_reads_refuse_out_of_range_limits_and_garbage_traces_before_database_access() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let client = reqwest::Client::new();
    let trace = Uuid::new_v4().to_string();
    let invalid_requests = [
        (
            "/athanor/v1/insula/trace",
            json!({ "traceId": trace, "limit": 0 }),
        ),
        (
            "/athanor/v1/insula/trace",
            json!({ "traceId": trace, "limit": 1000 }),
        ),
        (
            "/athanor/v1/insula/trace",
            json!({ "traceId": "not-a-trace" }),
        ),
        (
            "/athanor/v1/insula/trace",
            json!({ "traceId": trace.to_ascii_uppercase() }),
        ),
        ("/athanor/v1/insula/retention", json!({ "limit": 0 })),
        ("/athanor/v1/insula/retention", json!({ "limit": 100 })),
    ];

    for (path, request) in invalid_requests {
        let response = client
            .post(endpoint(&host, path))
            .bearer_auth(TOKEN)
            .json(&request)
            .send()
            .await
            .expect("out-of-range read completes");
        let body = error(response, StatusCode::UNPROCESSABLE_ENTITY).await;
        assert_eq!(
            body["error"], "invalid_request",
            "{path} must refuse {request} without a pool"
        );
    }

    host.stop().await;
}

#[tokio::test]
async fn insula_reads_report_an_unavailable_pool_after_accepting_the_default_limits() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let client = reqwest::Client::new();

    for (path, request) in [
        (
            "/athanor/v1/insula/trace",
            json!({ "traceId": Uuid::new_v4().to_string() }),
        ),
        ("/athanor/v1/insula/retention", json!({})),
    ] {
        let response = client
            .post(endpoint(&host, path))
            .bearer_auth(TOKEN)
            .json(&request)
            .send()
            .await
            .expect("valid read completes");
        let body = error(response, StatusCode::SERVICE_UNAVAILABLE).await;
        assert_eq!(body["error"], "insula_unavailable", "{path} without a pool");
    }

    host.stop().await;
}

#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Insula schema"]
async fn insula_trace_and_retention_reads_return_ingested_rows()
-> Result<(), Box<dyn std::error::Error>> {
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
    let client = reqwest::Client::new();

    // Observations old enough to have expired are what a sweep may delete; the
    // live trace ingested afterwards must survive it.
    let expired_at = Utc::now() - chrono::Duration::days(15);
    let response = client
        .post(endpoint(&host, "/athanor/v1/insula/events"))
        .bearer_auth(TOKEN)
        .json(&IngestBatch {
            events: vec![
                aged_event(&binding, "aged-first", expired_at),
                aged_event(&binding, "aged-second", expired_at),
            ],
        })
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    let sweep = akasha::run_retention(&pool, &binding.house_id, Utc::now(), 14).await?;
    assert_eq!(sweep.event_count, 2, "the aged observations must be swept");

    let trace = Uuid::new_v4().to_string();
    let response = client
        .post(endpoint(&host, "/athanor/v1/insula/events"))
        .bearer_auth(TOKEN)
        .json(&IngestBatch {
            events: vec![
                traced_event(&binding, "trace-first", &trace),
                traced_event(&binding, "trace-second", &trace),
            ],
        })
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);

    // The room fence: a foreign room's writer records spans under the very same
    // trace directly in the substrate; the Host trace read below must never
    // surface them, because its scope is pinned to the configured room.
    let foreign_binding = TrustedBinding {
        house_id: binding.house_id.clone(),
        room: "tuner".into(),
        spirit: "Tuner".into(),
        session_id: "foreign-session".into(),
    };
    let foreign = akasha::ingest_batch(
        &pool,
        &foreign_binding,
        IngestBatch {
            events: vec![traced_event(&foreign_binding, "trace-foreign", &trace)],
        },
    )
    .await?;
    assert_eq!(foreign.accepted_count, 1, "the foreign span must persist");

    let response = client
        .post(endpoint(&host, "/athanor/v1/insula/trace"))
        .bearer_auth(TOKEN)
        .json(&json!({ "traceId": trace, "limit": 10 }))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let read: Value = response.json().await?;
    assert_eq!(read["schemaVersion"], 1);
    assert_eq!(read["queryName"], "insula.trace");
    assert_eq!(read["queryVersion"], 1);
    assert_eq!(read["houseId"], "solarisael");
    assert_eq!(read["room"], "kintsu");
    assert_eq!(read["traceId"], trace);
    assert_eq!(read["limit"], 10);
    assert_eq!(read["truncated"], false);
    let rows = read["rows"].as_array().expect("trace rows");
    assert_eq!(
        rows.len(),
        2,
        "the foreign room's span under the same trace must stay invisible"
    );
    for row in rows {
        assert_eq!(row["traceId"], trace);
        assert_eq!(row["houseId"], "solarisael");
        assert_eq!(row["room"], "kintsu");
        assert_eq!(row["sessionId"], "configured-session");
    }

    let response = client
        .post(endpoint(&host, "/athanor/v1/insula/trace"))
        .bearer_auth(TOKEN)
        .json(&json!({ "traceId": trace, "limit": 1 }))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let truncated: Value = response.json().await?;
    assert_eq!(truncated["truncated"], true);
    assert_eq!(truncated["rows"].as_array().expect("trace rows").len(), 1);

    let response = client
        .post(endpoint(&host, "/athanor/v1/insula/retention"))
        .bearer_auth(TOKEN)
        .json(&json!({ "limit": 5 }))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let receipts: Value = response.json().await?;
    assert_eq!(receipts["schemaVersion"], 1);
    assert_eq!(receipts["queryName"], "insula.retention.receipts");
    assert_eq!(receipts["queryVersion"], 1);
    assert_eq!(receipts["houseId"], "solarisael");
    assert_eq!(receipts["limit"], 5);
    assert_eq!(receipts["truncated"], false);
    let rows = receipts["rows"].as_array().expect("retention rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["receiptId"], json!(sweep.receipt_id));
    assert_eq!(rows[0]["receiptKind"], "insula.retention.raw_delete");
    assert_eq!(rows[0]["houseId"], "solarisael");
    assert_eq!(rows[0]["retentionDays"], 14);
    assert_eq!(rows[0]["eventCount"], 2);
    assert_eq!(rows[0]["rollupQueryName"], "insula.vitals.minute");
    assert_eq!(
        rows[0]["tombstoneEventCount"], rows[0]["eventCount"],
        "the joined tombstones must account for every swept event"
    );
    assert_eq!(
        rows[0]["tombstoneWriterCount"], rows[0]["writerCount"],
        "one tombstone per writer proves the delete"
    );

    host.stop().await;
    sqlx::query("DROP SCHEMA insula CASCADE")
        .execute(&pool)
        .await?;
    Ok(())
}

/// Seed one intent that reached `exiting` before the stage window closed. The
/// ledger refuses UPDATE, so the arrival is written already old.
async fn seed_exiting_intent(
    pool: &sqlx::PgPool,
    workspace: &str,
    room: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let intent_id: String = sqlx::query_scalar(
        "INSERT INTO restart.intents (harness,workspace,mode,session_id,reason,consent_source,requester_room,requester_spirit,requester_session,idempotency_key,state,expires_at) VALUES ('omp',$1,'resume',$2,'the loader installed a newer release','operator-standing-policy',$3,'Spirit',$2,$4,'exiting',NOW()+INTERVAL '300 seconds') RETURNING intent_id::text",
    )
    .bind(workspace)
    .bind(format!("session-{room}"))
    .bind(room)
    .bind(format!("seed-{room}-{workspace}"))
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO restart.intent_events (intent_id,event_kind,principal,created_at) VALUES ($1::text::uuid,'exiting',$2,NOW()-INTERVAL '10 minutes')",
    )
    .bind(&intent_id)
    .bind(format!("{room}:Spirit"))
    .execute(pool)
    .await?;
    Ok(intent_id)
}

/// The successor came back. The schema writes the verified state and its
/// successor triple together or not at all, so this is one statement.
async fn mark_verified(
    pool: &sqlx::PgPool,
    intent_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        "UPDATE restart.intents SET state='verified',successor_session='successor',successor_room=requester_room,successor_spirit=requester_spirit,verified_at=NOW() WHERE intent_id=$1::text::uuid",
    )
    .bind(intent_id)
    .execute(pool)
    .await?;
    Ok(())
}

// Kills: a divergence route that answers from the wrong room, and a proof that
// only ever reaches insula_unavailable. This one carries a real pool, a real
// divergence row, and the real HTTP surface.
// red-proof: drop the requester_room predicate from query_unverified_exit, or
// pass a caller-supplied room instead of state.binding.room.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated restart schema"]
async fn unverified_exit_route_reports_this_rooms_divergence_over_http()
-> Result<(), Box<dyn std::error::Error>> {
    let database_url = isolated_database_url();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;
    // This proof owns the restart schema of the scratch database, exactly like
    // the substrate's own lifecycle proof: the two must not run together.
    sqlx::query("DROP SCHEMA IF EXISTS restart CASCADE")
        .execute(&pool)
        .await?;
    sqlx::raw_sql(RESTART_MIGRATION).execute(&pool).await?;

    let diverged = seed_exiting_intent(&pool, "D:/athanor-wt/host-diverged", "kintsu").await?;
    let returned = seed_exiting_intent(&pool, "D:/athanor-wt/host-returned", "kintsu").await?;
    mark_verified(&pool, &returned).await?;
    let foreign = seed_exiting_intent(&pool, "D:/athanor-wt/host-foreign", "kodo").await?;

    let root = TempDir::new()?;
    write_room_state(root.path());
    let host = start_with_config(database_config(root.path(), database_url)).await;
    let response = reqwest::Client::new()
        .post(endpoint(&host, "/athanor/v1/insula/unverified-exit"))
        .bearer_auth(TOKEN)
        .json(&json!({ "limit": 10 }))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await?;

    assert_eq!(body["queryName"], "insula.session.unverified_exit");
    assert_eq!(
        body["room"], "kintsu",
        "the read names the room it was scoped to, from configuration"
    );
    assert_eq!(body["windowSecs"], 180);
    let rows = body["rows"].as_array().expect("divergence rows");
    let reported: Vec<&str> = rows
        .iter()
        .map(|row| row["intentId"].as_str().expect("row intent id"))
        .collect();
    assert_eq!(
        reported,
        vec![diverged.as_str()],
        "one room's unreturned exit, and nothing else: a verified intent stays silent and another room's workspace never rides this read"
    );
    assert_eq!(rows[0]["workspace"], "D:/athanor-wt/host-diverged");
    assert_eq!(rows[0]["requesterRoom"], "kintsu");
    assert_eq!(rows[0]["state"], "exiting");
    assert!(
        !reported.contains(&foreign.as_str()),
        "the other room's row exists and is still not this room's business"
    );

    host.stop().await;
    sqlx::query("DROP SCHEMA restart CASCADE")
        .execute(&pool)
        .await?;
    Ok(())
}

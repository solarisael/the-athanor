use athanor_house_delivery::broker::Broker;
use chrono::{Duration, SecondsFormat, Utc};
use futures_util::{SinkExt, StreamExt};
use house_host::{Host, HostConfig, KnockAutonomy};
use house_protocol::{
    BOAT_RECEIPT_SCHEMA_VERSION, BOAT_RECEIPT_SUBJECT, BoatReceiptProjection, CONTEXT_ANALYZE,
    CONTEXT_ANALYZED, CONTEXT_PROJECTION_ID, HALLWAY_KNOCK_CLAIM, HALLWAY_KNOCK_COMMAND_FAILED,
    HALLWAY_KNOCK_COMMAND_REFUSED, HALLWAY_KNOCK_SETTLE, HALLWAY_PROJECTION_ID,
    PAPER_BOAT_RECEIPT_PROJECTION_ID, PAPER_BOAT_RECEIPT_SNAPSHOT, PAPER_BOAT_RECEIPT_SUBSCRIBE,
    RECALL_POLICY_COMMAND_ACCEPTED, RECALL_POLICY_COMMAND_REFUSED, RECALL_POLICY_COMPLETE_REFRESH,
    RECALL_POLICY_DELTA, RECALL_POLICY_EVALUATE, RECALL_POLICY_FAIL_REFRESH,
    RECALL_POLICY_INVALIDATE_AFTER_COMPACTION, RECALL_POLICY_RESYNC,
    RECALL_POLICY_SET_REQUESTED_MODE, RECALL_POLICY_SNAPSHOT, RECALL_POLICY_SUBSCRIBE,
    SHELL_CONVERSATION_LOG, SHELL_PROJECTION_ID, SHELL_RESULT,
};
use serde_json::{Value, json};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use tempfile::TempDir;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{Duration as TokioDuration, timeout};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderValue, StatusCode, header};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const TOKEN: &str = "test-only-host-token";
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct RunningHost {
    address: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<Result<(), String>>,
}

impl RunningHost {
    async fn stop(mut self) {
        self.shutdown.take().expect("shutdown sender").send(()).ok();
        timeout(TokioDuration::from_secs(2), self.task)
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
        serde_json::to_vec_pretty(&json!({
            "version": 1,
            "room": "kintsu",
            "operator": "Sol",
            "customTop": { "preserve": true },
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
                "updatedAt": null,
                "customPolicy": ["preserve", 7]
            }
        }))
        .expect("serialize fixture"),
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
        session: "operator-session".into(),
        recipient: "house-host".into(),
        database_url: None,
        akasha_enabled: false,
        nats_url: None,
        knock_autonomy: KnockAutonomy::Off,
    }
}

async fn start(root: &Path) -> RunningHost {
    start_with_config(config(root)).await
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

fn ws_url(host: &RunningHost) -> String {
    format!("ws://{}/athanor/v1/ws", host.address)
}

async fn connect(host: &RunningHost) -> Socket {
    let mut request = ws_url(host)
        .into_client_request()
        .expect("WebSocket request");
    request.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer test-only-host-token"),
    );
    connect_async(request)
        .await
        .expect("authenticated socket")
        .0
}

async fn send(socket: &mut Socket, value: &Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .expect("send command");
}

async fn receive(socket: &mut Socket) -> Value {
    let message = timeout(TokioDuration::from_secs(2), socket.next())
        .await
        .expect("Host response timed out")
        .expect("Host closed before response")
        .expect("WebSocket response failed");
    serde_json::from_str(message.to_text().expect("text response")).expect("JSON response")
}

async fn receive_for(socket: &mut Socket, correlation_id: &str, kind: &str) -> Value {
    loop {
        let value = receive(socket).await;
        if value["correlation_id"] == correlation_id && value["command_or_event_type"] == kind {
            return value;
        }
    }
}

fn command(kind: &str, message_id: &str, idempotency_key: &str, bound: bool) -> Value {
    let expires = (Utc::now() + Duration::minutes(2)).to_rfc3339_opts(SecondsFormat::Millis, true);
    json!({
        "schema_version": 1,
        "message_id": message_id,
        "house_id": if bound { "solarisael" } else { "" },
        "sender_room": if bound { "kintsu" } else { "" },
        "sender_spirit": if bound { "Kintsu" } else { "" },
        "sender_session": if bound { "operator-session" } else { "" },
        "recipient": if bound { "house-host" } else { "" },
        "command_or_event_type": kind,
        "correlation_id": message_id,
        "causation_id": "",
        "reply_target": if bound { "operator-session" } else { "" },
        "idempotency_key": idempotency_key,
        "source_record_refs": [],
        "scope": if bound { "room:kintsu:recall_policy" } else { "" },
        "visibility": if bound { "operator" } else { "" },
        "authority_class": if bound { "room_state" } else { "" },
        "created_at": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        "expires_at": expires,
        "max_hops": 1,
        "projection_id": "recall_policy"
    })
}

fn receipt_subscribe_command(message_id: &str) -> Value {
    let mut value = command(PAPER_BOAT_RECEIPT_SUBSCRIBE, message_id, message_id, false);
    value["projection_id"] = json!(PAPER_BOAT_RECEIPT_PROJECTION_ID);
    value
}

fn context_command(message_id: &str, prompt: &str) -> Value {
    let mut value = command(CONTEXT_ANALYZE, message_id, message_id, true);
    value["projection_id"] = json!(CONTEXT_PROJECTION_ID);
    value["context_request"] = json!({
        "prompt": prompt,
        "recognizedEntities": [],
        "contextCharacters": 160_000,
        "activeSpirit": "Kintsu",
        "operator": "Sol",
        "routingModeEnabled": true
    });
    value
}

fn conversation_command(root: &Path) -> Value {
    let mut value = command(
        SHELL_CONVERSATION_LOG,
        "conversation-1",
        "conversation-key-1",
        true,
    );
    value["projection_id"] = json!(SHELL_PROJECTION_ID);
    value["conversation_request"] = json!({
        "roomDir": root.join("room"),
        "sessionId": "session-1",
        "operator": "Sol",
        "spirit": "Kintsu",
        "source": "context",
        "persist": true,
        "messages": [{
            "role": "user",
            "id": "turn-1",
            "text": "Exact durable source.",
            "timestamp": "2026-08-12T23:30:00.000Z"
        }]
    });
    value
}

fn set_command(message_id: &str, key: &str, base_version: u64, mode: &str) -> Value {
    let mut value = command(RECALL_POLICY_SET_REQUESTED_MODE, message_id, key, true);
    let object = value.as_object_mut().expect("command object");
    object.insert("base_version".into(), json!(base_version));
    object.insert(
        "mutations".into(),
        json!([{ "mutation_type": "field_update", "field": "requested_mode", "value": mode }]),
    );
    value
}

fn evaluate_command(
    message_id: &str,
    key: &str,
    session: &str,
    intent: &str,
    terms: &[&str],
    active_project: Option<&str>,
    conversation_tokens: u64,
    working_set_present: bool,
) -> Value {
    let mut value = command(RECALL_POLICY_EVALUATE, message_id, key, true);
    let object = value.as_object_mut().expect("command object");
    object.insert("sender_session".into(), json!(session));
    object.insert("reply_target".into(), json!(session));
    object.insert(
        "facts".into(),
        json!({
            "query_route": {
                "intent": intent,
                "terms": terms,
                "required_terms": terms,
                "recognized_entities": []
            },
            "active_project": active_project,
            "conversation_tokens": conversation_tokens,
            "working_set_present": working_set_present
        }),
    );
    value
}

/// The adapter sends `tool_evidence` only when hands actually touched files, so
/// every other evaluate helper here keeps proving the absent-field path.
fn with_tool_evidence(mut value: Value) -> Value {
    value
        .get_mut("facts")
        .and_then(Value::as_object_mut)
        .expect("facts object")
        .insert("tool_evidence".into(), json!(true));
    value
}

fn complete_command(message_id: &str, key: &str, session: &str, query_terms: &[&str]) -> Value {
    let mut value = command(RECALL_POLICY_COMPLETE_REFRESH, message_id, key, true);
    let object = value.as_object_mut().expect("command object");
    object.insert("sender_session".into(), json!(session));
    object.insert("reply_target".into(), json!(session));
    object.insert(
        "refresh".into(),
        json!({
            "query_terms": query_terms,
            "refresh_reason": "empty-working-set",
            "entries": 2,
            "has_working_set": true,
            "warning": null
        }),
    );
    value
}

fn fail_command(message_id: &str, key: &str, session: &str, reason: &str) -> Value {
    let mut value = command(RECALL_POLICY_FAIL_REFRESH, message_id, key, true);
    let object = value.as_object_mut().expect("command object");
    object.insert("sender_session".into(), json!(session));
    object.insert("reply_target".into(), json!(session));
    object.insert("failure_reason".into(), json!(reason));
    value
}

fn regenerate_envelope(mut value: Value, message_id: &str) -> Value {
    let object = value.as_object_mut().expect("command object");
    object.insert("message_id".into(), json!(message_id));
    object.insert("correlation_id".into(), json!(message_id));
    object.insert("causation_id".into(), json!("retry-attempt"));
    object.insert(
        "created_at".into(),
        json!(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)),
    );
    object.insert(
        "expires_at".into(),
        json!((Utc::now() + Duration::minutes(2)).to_rfc3339_opts(SecondsFormat::Millis, true)),
    );
    value
}

fn invalidate_command(message_id: &str, key: &str, session: &str, summary: &str) -> Value {
    let mut value = command(
        RECALL_POLICY_INVALIDATE_AFTER_COMPACTION,
        message_id,
        key,
        true,
    );
    let object = value.as_object_mut().expect("command object");
    object.insert("sender_session".into(), json!(session));
    object.insert("reply_target".into(), json!(session));
    object.insert("compaction_summary".into(), json!(summary));
    value
}

async fn assert_semantic_retry(
    host: &RunningHost,
    authored: Value,
    conflicting: Value,
    retry_message_id: &str,
) {
    let mut socket = connect(host).await;
    send(&mut socket, &authored).await;
    let accepted = receive(&mut socket).await;
    assert_eq!(
        accepted["command_or_event_type"],
        RECALL_POLICY_COMMAND_ACCEPTED
    );

    let retry = regenerate_envelope(authored, retry_message_id);
    send(&mut socket, &retry).await;
    assert_eq!(receive(&mut socket).await, accepted);

    send(&mut socket, &conflicting).await;
    let refusal = receive(&mut socket).await;
    assert_eq!(
        refusal["command_or_event_type"],
        RECALL_POLICY_COMMAND_REFUSED
    );
    assert!(
        refusal["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("different command body"))
    );
}

#[tokio::test]
async fn regenerated_envelopes_replay_only_semantically_identical_recall_commands() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let mut snapshot_socket = connect(&host).await;
    send(
        &mut snapshot_socket,
        &command(
            RECALL_POLICY_SUBSCRIBE,
            "semantic-sub",
            "semantic-sub-key",
            false,
        ),
    )
    .await;
    let base_version = receive(&mut snapshot_socket).await["version"]
        .as_u64()
        .expect("snapshot version");
    drop(snapshot_socket);

    assert_semantic_retry(
        &host,
        set_command("set-authored", "set-semantic-key", base_version, "quiet"),
        set_command("set-conflict", "set-semantic-key", base_version, "work"),
        "set-retry",
    )
    .await;
    assert_semantic_retry(
        &host,
        evaluate_command(
            "evaluate-authored",
            "evaluate-semantic-key",
            "semantic-session",
            "technical_project",
            &["athanor"],
            Some("the-athanor"),
            100,
            false,
        ),
        evaluate_command(
            "evaluate-conflict",
            "evaluate-semantic-key",
            "semantic-session",
            "technical_project",
            &["athanor"],
            Some("the-athanor"),
            101,
            false,
        ),
        "evaluate-retry",
    )
    .await;
    assert_semantic_retry(
        &host,
        complete_command(
            "complete-authored",
            "complete-semantic-key",
            "semantic-session",
            &["athanor"],
        ),
        complete_command(
            "complete-conflict",
            "complete-semantic-key",
            "semantic-session",
            &["different"],
        ),
        "complete-retry",
    )
    .await;
    assert_semantic_retry(
        &host,
        fail_command(
            "fail-authored",
            "fail-semantic-key",
            "semantic-session",
            "retrieval unavailable",
        ),
        fail_command(
            "fail-conflict",
            "fail-semantic-key",
            "semantic-session",
            "different failure",
        ),
        "fail-retry",
    )
    .await;
    assert_semantic_retry(
        &host,
        invalidate_command(
            "compact-authored",
            "compact-semantic-key",
            "semantic-session",
            "summary",
        ),
        invalidate_command(
            "compact-conflict",
            "compact-semantic-key",
            "semantic-session",
            "different summary",
        ),
        "compact-retry",
    )
    .await;

    host.stop().await;
}

#[tokio::test]
async fn auth_health_and_snapshot_are_explicit() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let host = start(root.path()).await;

    let response = reqwest::get(format!("http://{}/health", host.address))
        .await
        .expect("health request");
    assert_eq!(response.status(), StatusCode::OK);
    let health: Value = response.json().await.expect("health JSON");
    assert_eq!(health["projection_id"], "recall_policy");
    assert_eq!(health["websocket_path"], "/athanor/v1/ws");
    assert_eq!(health["akasha_delivery"]["broker_status"], "disabled");
    assert_eq!(health["akasha_delivery"]["latest_event_id"], Value::Null);

    let unauthenticated = connect_async(ws_url(&host))
        .await
        .expect_err("missing bearer refused");
    let status = match unauthenticated {
        tokio_tungstenite::tungstenite::Error::Http(response) => response.status(),
        other => panic!("unexpected unauthenticated error: {other}"),
    };
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let mut socket = connect(&host).await;
    send(
        &mut socket,
        &command(RECALL_POLICY_SUBSCRIBE, "sub-1", "sub-key-1", false),
    )
    .await;
    let snapshot = receive(&mut socket).await;
    assert_eq!(snapshot["command_or_event_type"], RECALL_POLICY_SNAPSHOT);
    assert_eq!(snapshot["state"]["requestedMode"], "auto");
    assert_eq!(snapshot["state"]["resolvedMode"], "conversation");
    assert!(
        snapshot["state_hash"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64)
    );
    assert_eq!(snapshot["sender_session"], "operator-session");
    assert_eq!(snapshot["reply_target"], "house-host");
    host.stop().await;
}

#[tokio::test]
async fn context_analysis_is_owned_by_host_and_returns_typed_policy() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let mut socket = connect(&host).await;
    send(
        &mut socket,
        &context_command(
            "context-1",
            "ultraverify the database retrieval architecture before bun run dev",
        ),
    )
    .await;
    let response = receive_for(&mut socket, "context-1", CONTEXT_ANALYZED).await;
    assert_eq!(response["projection_id"], CONTEXT_PROJECTION_ID);
    assert_eq!(response["analysis"]["route"]["intent"], "technical_project");
    assert_eq!(
        response["analysis"]["keywordDirectives"][0]["keyword"],
        "ultraverify"
    );
    assert_eq!(response["analysis"]["processTrigger"], "package-script-dev");
    assert_eq!(response["analysis"]["nudge"]["band"], 1);
    host.stop().await;
}

#[tokio::test]
async fn conversation_capture_persists_the_exact_giga_source_before_ingest() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let mut socket = connect(&host).await;
    send(&mut socket, &conversation_command(root.path())).await;
    let response = receive_for(&mut socket, "conversation-1", SHELL_RESULT).await;
    assert_eq!(
        response["result"]["loggedTurns"][0]["sourceID"], "turn-1",
        "{response}"
    );

    let ledger_directory = response["result"]["sourceLedgerDirectory"]
        .as_str()
        .expect("source ledger directory");
    let ledger_path = std::fs::read_dir(ledger_directory)
        .expect("source ledger directory exists")
        .next()
        .expect("one dated source ledger")
        .expect("source ledger entry")
        .path();
    let records = std::fs::read_to_string(ledger_path).expect("source ledger contents");
    let record: Value = serde_json::from_str(records.trim()).expect("source ledger JSONL record");
    assert_eq!(record["sessionID"], "session-1");
    assert_eq!(record["messageID"], "turn-1");
    assert_eq!(record["text"], "Exact durable source.");
    host.stop().await;
}

#[tokio::test]
async fn missing_broker_is_degraded_and_receipt_projection_never_invents_content() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let mut host_config = config(root.path());
    host_config.akasha_enabled = true;
    host_config.nats_url = None;
    let host = start_with_config(host_config).await;

    let health: Value = reqwest::get(format!("http://{}/health", host.address))
        .await
        .expect("health request")
        .json()
        .await
        .expect("health JSON");
    assert_eq!(health["akasha_delivery"]["broker_status"], "degraded");
    assert_eq!(health["akasha_delivery"]["broker_configured"], false);

    let mut socket = connect(&host).await;
    send(&mut socket, &receipt_subscribe_command("receipt-subscribe")).await;
    let event = receive_for(
        &mut socket,
        "receipt-subscribe",
        PAPER_BOAT_RECEIPT_SNAPSHOT,
    )
    .await;
    assert_eq!(event["projection_id"], PAPER_BOAT_RECEIPT_PROJECTION_ID);
    assert_eq!(event["authority_class"], "delivery_receipt");
    assert_eq!(event["state"]["status"], "degraded");
    assert_eq!(event["state"]["receipt"], Value::Null);
    assert!(event.get("body").is_none());
    assert!(event.get("title").is_none());
    host.stop().await;
}

#[tokio::test]
#[ignore = "requires the test-owned NATS endpoint"]
async fn nats_receipts_are_strict_room_scoped_and_ordered_on_the_host_socket() {
    let nats_url = std::env::var("SOLARISAEL_DELIVERY_TEST_NATS_URL").expect("test-owned NATS URL");
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let mut host_config = config(root.path());
    host_config.akasha_enabled = true;
    host_config.nats_url = Some(nats_url.clone());
    let host = start_with_config(host_config).await;
    let mut socket = connect(&host).await;
    send(&mut socket, &receipt_subscribe_command("receipt-subscribe")).await;
    let initial = receive_for(
        &mut socket,
        "receipt-subscribe",
        PAPER_BOAT_RECEIPT_SNAPSHOT,
    )
    .await;
    assert_eq!(initial["projection_id"], PAPER_BOAT_RECEIPT_PROJECTION_ID);
    while timeout(TokioDuration::from_millis(50), socket.next())
        .await
        .is_ok()
    {}

    let client = async_nats::connect(nats_url).await.expect("NATS publisher");
    let fixture = json!({
        "schema_version": 1,
        "event_id": "8d2c04ae-ef20-4fbc-8141-d0259cbf495f",
        "record_id": "42",
        "room": "other",
        "processed_at": "2026-08-10T09:30:00Z",
        "original_stream_sequence": 8_000_000_000_u64,
        "integrity_sha256": "a".repeat(64)
    });
    client
        .publish(
            BOAT_RECEIPT_SUBJECT,
            serde_json::to_vec(&fixture).unwrap().into(),
        )
        .await
        .expect("publish foreign receipt");
    client.flush().await.expect("flush foreign receipt");
    assert!(
        timeout(TokioDuration::from_millis(250), socket.next())
            .await
            .is_err(),
        "a foreign-room receipt must not produce a Host event"
    );

    let mut private = fixture.clone();
    private["room"] = json!("kintsu");
    private["body"] = json!("private prose");
    client
        .publish(
            BOAT_RECEIPT_SUBJECT,
            serde_json::to_vec(&private).unwrap().into(),
        )
        .await
        .expect("publish private receipt");
    client.flush().await.expect("flush private receipt");
    let refused = receive(&mut socket).await;
    assert_eq!(refused["state"]["status"], "refused");
    assert_eq!(refused["state"]["receipt"], Value::Null);
    assert!(!refused.to_string().contains("private prose"));

    let mut valid = fixture;
    valid["room"] = json!("kintsu");
    client
        .publish(
            BOAT_RECEIPT_SUBJECT,
            serde_json::to_vec(&valid).unwrap().into(),
        )
        .await
        .expect("publish valid receipt");
    client.flush().await.expect("flush valid receipt");
    let delivered = receive(&mut socket).await;
    assert_eq!(delivered["state"]["status"], "delivered");
    assert_eq!(delivered["sequence"], 8_000_000_000_u64);
    assert_eq!(delivered["state"]["receipt"]["record_id"], "42");
    assert_eq!(delivered["sender_room"], "kintsu");
    assert!(delivered.get("body").is_none());
    assert!(delivered.get("title").is_none());
    host.stop().await;
}

#[tokio::test]
#[ignore = "requires the test-owned NATS endpoint"]
async fn retained_receipt_published_before_host_is_replayed_to_the_room_projection() {
    let nats_url = std::env::var("SOLARISAEL_DELIVERY_TEST_NATS_URL").expect("test-owned NATS URL");
    let broker = Broker::connect(&nats_url).await.expect("delivery broker");
    broker.configure().await.expect("exact delivery streams");
    let event_id = uuid::Uuid::new_v4();
    let original_stream_sequence =
        u64::try_from(Utc::now().timestamp_micros()).expect("positive test clock");
    let projection = BoatReceiptProjection {
        schema_version: BOAT_RECEIPT_SCHEMA_VERSION,
        event_id: event_id.to_string(),
        record_id: "424242".into(),
        room: "kintsu".into(),
        processed_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        original_stream_sequence,
        integrity_sha256: "b".repeat(64),
    };
    broker
        .publish_receipt(
            event_id,
            serde_json::to_vec(&projection).expect("serialize retained projection"),
        )
        .await
        .expect("publish retained receipt before Host starts");

    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let mut host_config = config(root.path());
    host_config.akasha_enabled = true;
    host_config.nats_url = Some(nats_url);
    let host = start_with_config(host_config).await;
    let mut socket = connect(&host).await;
    send(
        &mut socket,
        &receipt_subscribe_command("retained-receipt-subscribe"),
    )
    .await;
    let expected_event_id = event_id.to_string();
    let delivered = timeout(TokioDuration::from_secs(5), async {
        loop {
            let event = receive(&mut socket).await;
            if event["state"]["receipt"]["event_id"].as_str() == Some(expected_event_id.as_str()) {
                break event;
            }
        }
    })
    .await
    .expect("retained receipt replay timed out");
    assert_eq!(delivered["state"]["status"], "delivered");
    assert_eq!(delivered["state"]["receipt"]["record_id"], "424242");
    assert_eq!(delivered["sequence"], original_stream_sequence);
    assert!(delivered.get("body").is_none());
    assert!(delivered.get("title").is_none());
    host.stop().await;
}

#[tokio::test]
async fn set_retry_conflict_refusals_and_resync_preserve_authority() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let mut socket = connect(&host).await;
    send(
        &mut socket,
        &command(RECALL_POLICY_SUBSCRIBE, "sub-1", "sub-key-1", false),
    )
    .await;
    let initial = receive(&mut socket).await;
    let base_version = initial["version"].as_u64().expect("snapshot version");

    let authored = set_command("set-1", "durable-key", base_version, "quiet");
    send(&mut socket, &authored).await;
    let accepted = receive(&mut socket).await;
    let delta = receive(&mut socket).await;
    assert_eq!(
        accepted["command_or_event_type"],
        RECALL_POLICY_COMMAND_ACCEPTED
    );
    assert_eq!(delta["command_or_event_type"], RECALL_POLICY_DELTA);
    assert_eq!(delta["base_version"], base_version);
    assert_eq!(delta["next_version"], base_version + 1);
    let mutations = delta["mutations"].as_array().expect("typed mutations");
    for (field, value) in [
        ("requested_mode", json!("quiet")),
        ("resolved_mode", json!("quiet")),
        ("resolution_reason", json!("explicit-override")),
    ] {
        assert!(
            mutations.iter().any(|mutation| {
                mutation["mutation_type"] == "field_update"
                    && mutation["field"] == field
                    && mutation["value"] == value
            }),
            "missing {field} delta"
        );
    }
    assert_eq!(delta["source_event_ids"][0], accepted["event_id"]);

    send(&mut socket, &authored).await;
    let duplicate = receive(&mut socket).await;
    assert_eq!(duplicate, accepted);
    assert!(
        timeout(TokioDuration::from_millis(100), socket.next())
            .await
            .is_err()
    );

    let conflict = set_command("set-1", "durable-key", base_version, "work");
    send(&mut socket, &conflict).await;
    let conflict = receive(&mut socket).await;
    assert_eq!(
        conflict["command_or_event_type"],
        RECALL_POLICY_COMMAND_REFUSED
    );
    assert!(
        conflict["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("different command body"))
    );

    send(
        &mut socket,
        &set_command("stale-1", "stale-key", base_version, "work"),
    )
    .await;
    let stale = receive(&mut socket).await;
    assert_eq!(
        stale["command_or_event_type"],
        RECALL_POLICY_COMMAND_REFUSED
    );
    assert!(
        stale["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("stale base_version"))
    );

    let mut foreign = set_command("foreign-1", "foreign-key", base_version + 1, "work");
    foreign["sender_room"] = json!("other-room");
    send(&mut socket, &foreign).await;
    let foreign = receive(&mut socket).await;
    assert_eq!(
        foreign["command_or_event_type"],
        RECALL_POLICY_COMMAND_REFUSED
    );
    assert!(
        foreign["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("foreign"))
    );

    let mut malformed = command(RECALL_POLICY_RESYNC, "bad-1", "bad-key", true);
    malformed["invented_field"] = json!(true);
    send(&mut socket, &malformed).await;
    let malformed = receive(&mut socket).await;
    assert_eq!(
        malformed["command_or_event_type"],
        RECALL_POLICY_COMMAND_REFUSED
    );
    assert!(
        malformed["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("unknown field"))
    );

    let mut unknown_version = command(RECALL_POLICY_RESYNC, "version-1", "version-key", true);
    unknown_version["schema_version"] = json!(2);
    send(&mut socket, &unknown_version).await;
    let unknown_version = receive(&mut socket).await;
    assert_eq!(
        unknown_version["command_or_event_type"],
        RECALL_POLICY_COMMAND_REFUSED
    );
    assert!(
        unknown_version["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("unsupported schema_version"))
    );

    let unknown_type = command("athanor.recall_policy.invented", "type-1", "type-key", true);
    send(&mut socket, &unknown_type).await;
    let unknown_type = receive(&mut socket).await;
    assert_eq!(
        unknown_type["command_or_event_type"],
        RECALL_POLICY_COMMAND_REFUSED
    );
    assert!(
        unknown_type["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("unknown command_or_event_type"))
    );

    send(
        &mut socket,
        &command(RECALL_POLICY_RESYNC, "sync-1", "sync-key", true),
    )
    .await;
    let snapshot = receive(&mut socket).await;
    assert_eq!(snapshot["command_or_event_type"], RECALL_POLICY_SNAPSHOT);
    assert_eq!(snapshot["state"]["requestedMode"], "quiet");
    assert_eq!(snapshot["version"], base_version + 1);

    let persisted: Value = serde_json::from_slice(
        &std::fs::read(
            root.path()
                .join("room/.omp/runtime/solarisael-house-state.json"),
        )
        .expect("persisted room state"),
    )
    .expect("persisted room JSON");
    assert_eq!(persisted["customTop"]["preserve"], true);
    assert_eq!(
        persisted["recallPolicy"]["customPolicy"],
        json!(["preserve", 7])
    );
    assert_eq!(persisted["recallPolicy"]["requestedMode"], "quiet");
    assert_eq!(persisted["recallPolicy"]["resolvedMode"], "quiet");
    host.stop().await;
}

#[tokio::test]
async fn cursor_and_idempotency_receipt_survive_restart() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let host = start(root.path()).await;

    let mut socket = connect(&host).await;
    send(
        &mut socket,
        &command(RECALL_POLICY_SUBSCRIBE, "sub-1", "sub-key-1", false),
    )
    .await;
    let initial = receive(&mut socket).await;
    let authored = set_command(
        "restart-set",
        "restart-key",
        initial["version"].as_u64().expect("version"),
        "work",
    );
    send(&mut socket, &authored).await;
    let accepted = receive(&mut socket).await;
    let delta = receive(&mut socket).await;
    host.stop().await;

    let restarted = start(root.path()).await;
    let mut socket = connect(&restarted).await;
    send(
        &mut socket,
        &command(RECALL_POLICY_SUBSCRIBE, "sub-2", "sub-key-2", false),
    )
    .await;
    let snapshot = receive(&mut socket).await;
    assert_eq!(snapshot["version"], delta["next_version"]);
    assert_eq!(snapshot["sequence"], delta["sequence"]);
    assert_eq!(snapshot["state_hash"], delta["state_hash"]);
    assert_eq!(snapshot["state"]["requestedMode"], "work");

    send(
        &mut socket,
        &regenerate_envelope(authored, "restart-set-retry"),
    )
    .await;
    assert_eq!(receive(&mut socket).await, accepted);
    assert!(
        timeout(TokioDuration::from_millis(100), socket.next())
            .await
            .is_err()
    );
    restarted.stop().await;
}
#[tokio::test]
async fn resync_reloads_external_room_state_and_makes_old_versions_stale() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let mut socket = connect(&host).await;
    send(
        &mut socket,
        &command(
            RECALL_POLICY_SUBSCRIBE,
            "external-sub",
            "external-sub-key",
            false,
        ),
    )
    .await;
    let initial = receive(&mut socket).await;
    let old_version = initial["version"].as_u64().expect("initial version");

    let state_path = root
        .path()
        .join("room/.omp/runtime/solarisael-house-state.json");
    let mut persisted: Value =
        serde_json::from_slice(&std::fs::read(&state_path).expect("read external room state"))
            .expect("external room JSON");
    persisted["recallPolicy"]["requestedMode"] = json!("work");
    persisted["recallPolicy"]["resolvedMode"] = json!("work");
    persisted["recallPolicy"]["resolutionReason"] = json!("external-authority-change");
    std::fs::write(
        &state_path,
        serde_json::to_vec_pretty(&persisted).expect("serialize external room state"),
    )
    .expect("write external room state");

    send(
        &mut socket,
        &command(
            RECALL_POLICY_RESYNC,
            "external-sync",
            "external-sync-key",
            true,
        ),
    )
    .await;
    let snapshot = receive(&mut socket).await;
    assert_eq!(snapshot["command_or_event_type"], RECALL_POLICY_SNAPSHOT);
    assert_eq!(snapshot["version"], old_version + 1);
    assert_eq!(snapshot["state"]["requestedMode"], "work");
    assert_eq!(
        snapshot["state"]["resolutionReason"],
        "external-authority-change"
    );

    send(
        &mut socket,
        &set_command("external-stale", "external-stale-key", old_version, "quiet"),
    )
    .await;
    let stale = receive(&mut socket).await;
    assert_eq!(
        stale["command_or_event_type"],
        RECALL_POLICY_COMMAND_REFUSED
    );
    assert!(
        stale["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("stale base_version"))
    );
    host.stop().await;
}

#[tokio::test]
async fn graceful_shutdown_drains_upgraded_websocket_tasks() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let mut socket = connect(&host).await;
    let RunningHost { shutdown, task, .. } = host;
    shutdown.expect("shutdown sender").send(()).ok();
    timeout(TokioDuration::from_secs(2), task)
        .await
        .expect("detached upgrade task prevented shutdown drain")
        .expect("Host task panicked")
        .expect("Host shutdown failed");
    let closed = timeout(TokioDuration::from_secs(1), socket.next())
        .await
        .expect("socket did not close after cancellation");
    assert!(closed.is_none() || matches!(closed, Some(Ok(Message::Close(_)))));
}

#[tokio::test]
async fn host_resolves_modes_hysteresis_and_quiet_without_adapter_policy() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let mut socket = connect(&host).await;

    let work = evaluate_command(
        "mode-work",
        "mode-work-key",
        "mode-session",
        "technical_project",
        &["athanor", "recall", "policy"],
        Some("the-athanor"),
        100,
        false,
    );
    send(&mut socket, &work).await;
    let work = receive_for(&mut socket, "mode-work", RECALL_POLICY_COMMAND_ACCEPTED).await;
    assert_eq!(work["decision"]["refreshReason"], "active-project-change");
    assert_eq!(work["decision"]["shouldRecall"], true);

    let complete = complete_command(
        "mode-complete",
        "mode-complete-key",
        "mode-session",
        &["athanor", "recall", "policy"],
    );
    send(&mut socket, &complete).await;
    let complete = receive_for(&mut socket, "mode-complete", RECALL_POLICY_COMMAND_ACCEPTED).await;
    assert_eq!(complete["state"]["workingSetEntries"], 2);

    let first_contact = evaluate_command(
        "mode-contact-1",
        "mode-contact-key-1",
        "mode-session",
        "casual_contact",
        &["hello"],
        Some("the-athanor"),
        200,
        true,
    );
    send(&mut socket, &first_contact).await;
    let first_contact = receive_for(
        &mut socket,
        "mode-contact-1",
        RECALL_POLICY_COMMAND_ACCEPTED,
    )
    .await;
    assert_eq!(first_contact["decision"]["resolvedMode"], "mixed");

    let second_contact = evaluate_command(
        "mode-contact-2",
        "mode-contact-key-2",
        "mode-session",
        "casual_contact",
        &["cuddles"],
        Some("the-athanor"),
        300,
        false,
    );
    send(&mut socket, &second_contact).await;
    let second_contact = receive_for(
        &mut socket,
        "mode-contact-2",
        RECALL_POLICY_COMMAND_ACCEPTED,
    )
    .await;
    assert_eq!(second_contact["decision"]["resolvedMode"], "conversation");
    assert_eq!(
        second_contact["state"]["resolutionReason"],
        "conversation-hysteresis-complete"
    );

    let quiet = set_command(
        "mode-quiet",
        "mode-quiet-key",
        second_contact["version"].as_u64().expect("current version"),
        "quiet",
    );
    send(&mut socket, &quiet).await;
    receive_for(&mut socket, "mode-quiet", RECALL_POLICY_COMMAND_ACCEPTED).await;
    let quiet_lookup = evaluate_command(
        "mode-quiet-lookup",
        "mode-quiet-lookup-key",
        "mode-session",
        "memory_lookup",
        &["remember", "athanor"],
        Some("the-athanor"),
        400,
        false,
    );

    send(&mut socket, &quiet_lookup).await;
    let quiet_lookup = receive_for(
        &mut socket,
        "mode-quiet-lookup",
        RECALL_POLICY_COMMAND_ACCEPTED,
    )
    .await;
    assert_eq!(quiet_lookup["decision"]["resolvedMode"], "quiet");
    assert_eq!(quiet_lookup["decision"]["shouldRecall"], false);
    assert_eq!(quiet_lookup["decision"]["clearWorkingSet"], true);
    let conversation = set_command(
        "mode-conversation",
        "mode-conversation-key",
        quiet_lookup["version"].as_u64().expect("current version"),
        "conversation",
    );
    send(&mut socket, &conversation).await;
    receive_for(
        &mut socket,
        "mode-conversation",
        RECALL_POLICY_COMMAND_ACCEPTED,
    )
    .await;
    let conversation_lookup = evaluate_command(
        "mode-conversation-lookup",
        "mode-conversation-lookup-key",
        "mode-session",
        "memory_lookup",
        &["remember", "continuity"],
        None,
        500,
        false,
    );
    send(&mut socket, &conversation_lookup).await;
    let conversation_lookup = receive_for(
        &mut socket,
        "mode-conversation-lookup",
        RECALL_POLICY_COMMAND_ACCEPTED,
    )
    .await;
    assert_eq!(
        conversation_lookup["decision"]["resolvedMode"],
        "conversation"
    );
    assert_eq!(conversation_lookup["decision"]["shouldRecall"], true);
    host.stop().await;
}

#[tokio::test]
async fn host_resolves_work_from_tool_evidence_and_still_yields_to_an_explicit_lookup() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let mut socket = connect(&host).await;

    let casual = evaluate_command(
        "evidence-casual",
        "evidence-casual-key",
        "evidence-session",
        "casual_contact",
        &["hello"],
        None,
        100,
        false,
    );
    send(&mut socket, &casual).await;
    let casual = receive_for(
        &mut socket,
        "evidence-casual",
        RECALL_POLICY_COMMAND_ACCEPTED,
    )
    .await;
    assert_eq!(casual["decision"]["resolvedMode"], "conversation");
    assert_eq!(casual["state"]["resolutionReason"], "casual-contact");

    let evidence = with_tool_evidence(evaluate_command(
        "evidence-work",
        "evidence-work-key",
        "evidence-session",
        "casual_contact",
        &["hello", "again"],
        None,
        200,
        false,
    ));
    send(&mut socket, &evidence).await;
    let evidence = receive_for(&mut socket, "evidence-work", RECALL_POLICY_COMMAND_ACCEPTED).await;
    assert_eq!(evidence["decision"]["resolvedMode"], "work");
    assert_eq!(evidence["state"]["resolutionReason"], "tool-evidence");

    let lookup = with_tool_evidence(evaluate_command(
        "evidence-lookup",
        "evidence-lookup-key",
        "evidence-session",
        "memory_lookup",
        &["continuity"],
        None,
        300,
        false,
    ));
    send(&mut socket, &lookup).await;
    let lookup = receive_for(
        &mut socket,
        "evidence-lookup",
        RECALL_POLICY_COMMAND_ACCEPTED,
    )
    .await;
    assert_eq!(lookup["decision"]["resolvedMode"], "conversation");
    assert_eq!(lookup["state"]["resolutionReason"], "explicit-lookup");
    host.stop().await;
}

#[tokio::test]
async fn compaction_recovery_and_session_working_set_survive_host_restart() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let mut socket = connect(&host).await;
    let invalidate = invalidate_command(
        "recover-invalidate",
        "recover-invalidate-key",
        "recover-session",
        "Recall Policy compaction recovery PostgreSQL continuity",
    );
    send(&mut socket, &invalidate).await;
    let invalidated = receive_for(
        &mut socket,
        "recover-invalidate",
        RECALL_POLICY_COMMAND_ACCEPTED,
    )
    .await;
    assert_eq!(invalidated["state"]["recoveryPending"], true);
    assert_eq!(
        invalidated["state"]["lastRefreshReason"],
        "compaction-invalidated"
    );
    host.stop().await;

    let restarted = start(root.path()).await;
    let mut socket = connect(&restarted).await;
    let recover = evaluate_command(
        "recover-evaluate",
        "recover-evaluate-key",
        "recover-session",
        "memory_lookup",
        &["continuity"],
        None,
        50,
        false,
    );
    send(&mut socket, &recover).await;
    let recover = receive_for(
        &mut socket,
        "recover-evaluate",
        RECALL_POLICY_COMMAND_ACCEPTED,
    )
    .await;
    assert_eq!(
        recover["decision"]["refreshReason"],
        "post-compaction-recovery"
    );
    assert_eq!(recover["decision"]["shouldRecall"], true);
    assert!(
        recover["decision"]["query"]
            .as_str()
            .expect("query")
            .contains("postgresql")
    );

    let complete = complete_command(
        "recover-complete",
        "recover-complete-key",
        "recover-session",
        &["continuity", "postgresql"],
    );
    send(&mut socket, &complete).await;
    let complete = receive_for(
        &mut socket,
        "recover-complete",
        RECALL_POLICY_COMMAND_ACCEPTED,
    )
    .await;
    assert_eq!(complete["state"]["recoveryPending"], false);
    assert_eq!(complete["state"]["recoveryTerms"], json!([]));
    restarted.stop().await;
}

#[tokio::test]
async fn concurrent_authenticated_clients_keep_session_state_isolated() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let host = start(root.path()).await;
    let mut alpha = connect(&host).await;
    let mut beta = connect(&host).await;

    send(
        &mut alpha,
        &evaluate_command(
            "alpha-evaluate",
            "alpha-evaluate-key",
            "alpha-session",
            "technical_project",
            &["alpha", "athanor", "work"],
            Some("alpha-project"),
            100,
            false,
        ),
    )
    .await;
    send(
        &mut beta,
        &evaluate_command(
            "beta-evaluate",
            "beta-evaluate-key",
            "beta-session",
            "technical_project",
            &["beta", "athanor", "work"],
            Some("beta-project"),
            100,
            false,
        ),
    )
    .await;

    let alpha = receive_for(&mut alpha, "alpha-evaluate", RECALL_POLICY_COMMAND_ACCEPTED).await;
    let beta = receive_for(&mut beta, "beta-evaluate", RECALL_POLICY_COMMAND_ACCEPTED).await;
    assert_eq!(alpha["sender_session"], "alpha-session");
    assert_eq!(beta["sender_session"], "beta-session");
    assert_eq!(alpha["decision"]["resolvedMode"], "work");
    assert_eq!(beta["decision"]["resolvedMode"], "work");
    assert_ne!(alpha["version"], beta["version"]);
    assert!(
        alpha["state_hash"]
            .as_str()
            .is_some_and(|hash| !hash.is_empty())
    );
    assert!(
        beta["state_hash"]
            .as_str()
            .is_some_and(|hash| !hash.is_empty())
    );
    host.stop().await;
}

fn knock_command(kind: &str, message_id: &str) -> Value {
    let mut value = command(kind, message_id, message_id, true);
    value["projection_id"] = json!(HALLWAY_PROJECTION_ID);
    value
}

fn knock_settle_command(message_id: &str) -> Value {
    let mut value = knock_command(HALLWAY_KNOCK_SETTLE, message_id);
    value["hallway_knock_settle"] = json!({
        "knockId": "5af35bb5-e9a1-4e58-849b-b78b6614bc15",
        "outcome": "completed",
        "reason": null
    });
    value
}

#[tokio::test]
async fn explicitly_disabled_host_refuses_knock_claim_and_settlement_before_database_access() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    // This fixture explicitly selects Off. A refusal (not a database failure)
    // proves the authority gate runs before the Hallway pool is consulted.
    let host = start(root.path()).await;
    let mut socket = connect(&host).await;

    send(
        &mut socket,
        &knock_command(HALLWAY_KNOCK_CLAIM, "knock-claim-off"),
    )
    .await;
    let refused = receive(&mut socket).await;
    assert_eq!(
        refused["command_or_event_type"],
        HALLWAY_KNOCK_COMMAND_REFUSED
    );
    assert_eq!(refused["correlation_id"], "knock-claim-off");
    let reason = refused["reason"].as_str().expect("refusal reason");
    assert!(
        reason.contains("knock_autonomy_disabled"),
        "refusal must name the disabled autonomy: {reason}"
    );
    assert!(
        !reason.contains("DATABASE_URL"),
        "a disabled Host must never reach the database path: {reason}"
    );

    send(&mut socket, &knock_settle_command("knock-settle-off")).await;
    let refused = receive(&mut socket).await;
    assert_eq!(
        refused["command_or_event_type"],
        HALLWAY_KNOCK_COMMAND_REFUSED
    );
    assert_eq!(refused["correlation_id"], "knock-settle-off");
    let reason = refused["reason"].as_str().expect("refusal reason");
    assert!(
        reason.contains("knock_autonomy_disabled"),
        "settlement must be gated by the same authority: {reason}"
    );
    assert!(
        !reason.contains("DATABASE_URL"),
        "a disabled Host must never reach the database path: {reason}"
    );
    host.stop().await;
}

#[tokio::test]
async fn claim_autonomy_uses_the_existing_bounded_knock_path() {
    let root = TempDir::new().expect("tempdir");
    write_room_state(root.path());
    let mut configured = config(root.path());
    configured.knock_autonomy = KnockAutonomy::Claim;
    let host = start_with_config(configured).await;
    let mut socket = connect(&host).await;

    send(
        &mut socket,
        &knock_command(HALLWAY_KNOCK_CLAIM, "knock-claim-on"),
    )
    .await;
    let response = receive(&mut socket).await;
    // Claim mode reaches the pre-existing Hallway pool check and fails there
    // because this Host has no DATABASE_URL, rather than being refused by the
    // authority gate.
    assert_eq!(
        response["command_or_event_type"],
        HALLWAY_KNOCK_COMMAND_FAILED
    );
    assert_eq!(response["correlation_id"], "knock-claim-on");
    assert_eq!(
        response["reason"],
        "Hallway Knock claim requires DATABASE_URL"
    );
    host.stop().await;
}

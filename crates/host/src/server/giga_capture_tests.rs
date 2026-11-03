//! Host conversation capture against PostgreSQL. The shell capture command
//! persists GIGA turn jobs through the canonical migrations before it replies,
//! and its receipt names what became a job and what did not.

use super::{AppState, HOST_RECIPIENT, Host, process_text};
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn require(condition: bool, message: impl Into<String>) -> TestResult {
    if condition {
        Ok(())
    } else {
        Err(message.into().into())
    }
}

fn enabled(project_keys: &[&str]) -> Value {
    json!({
        "gigaEnabled": true, "hippocampusEnabled": true, "replayMode": false,
        "subagent": false, "projectKeys": project_keys,
    })
}

fn turn(role: &str, id: &str, text: &str, minute: u32) -> Value {
    json!({
        "role": role, "id": id, "text": text,
        "timestamp": format!("2026-11-02T10:{minute:02}:00.000Z"),
    })
}

struct CaptureRoom {
    state: AppState,
    pool: PgPool,
}

impl CaptureRoom {
    fn open(pool: PgPool, directory: &std::path::Path) -> TestResult<Self> {
        let config = crate::config::HostConfig {
            bind: "127.0.0.1:0".parse()?,
            bearer_token: "capture-test".into(),
            room_dir: directory.to_path_buf(),
            state_dir: directory.join("host"),
            house_id: "solarisael".into(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "capture-host-session".into(),
            database_url: None,
            nats_url: None,
            nats_auth: None,
            knock_autonomy: crate::config::KnockAutonomy::Off,
            operator: None,
        };
        std::fs::create_dir_all(config.room_state_path().parent().ok_or("room state path")?)?;
        std::fs::create_dir_all(&config.state_dir)?;
        let room_state = json!({
            "room": "kodo", "operator": "Sol", "embodiedSpirit": "Kodo",
            "recallPolicy": {
                "requestedMode": "auto", "resolvedMode": "conversation",
                "workingSetEntries": 0, "recoveryPending": false, "recoveryTerms": [],
                "activeProject": null, "resolutionReason": "test",
                "lastRefreshReason": null, "lastRefreshAt": null,
                "degraded": null, "updatedAt": null
            }
        });
        std::fs::write(config.room_state_path(), room_state.to_string())?;
        let state = Host::new(
            config,
            Some(pool.clone()),
            CancellationToken::new(),
            TaskTracker::new(),
        )?
        .state;
        // An approved classifier grant with no credential behind it.
        let marker = json!({
            "room": "kodo",
            "jevGiga": {
                "provider": "typesafe", "mode": "active",
                "grant": {
                    "purpose": "giga-classify", "allowPrivateConversationPackets": true,
                    "policyRevision": "capture-proof-r1"
                }
            }
        });
        std::fs::write(directory.join(".athanor-room.json"), marker.to_string())?;
        Ok(Self { state, pool })
    }

    async fn capture(&self, session: &str, persist: bool, giga: Value, messages: Value) -> Value {
        let config = &self.state.config;
        let message_id = Uuid::new_v4().to_string();
        let command = json!({
            "schema_version": protocol::HOST_SCHEMA_VERSION,
            "message_id": message_id, "correlation_id": message_id,
            "idempotency_key": message_id, "causation_id": "",
            "house_id": config.house_id, "sender_room": config.room,
            "sender_spirit": config.spirit, "sender_session": config.session,
            "recipient": HOST_RECIPIENT, "reply_target": config.session,
            "scope": config.scope(), "visibility": "operator", "authority_class": "room_state",
            "created_at": chrono::Utc::now().to_rfc3339(),
            "expires_at": (chrono::Utc::now() + chrono::Duration::minutes(1)).to_rfc3339(),
            "max_hops": 1, "source_record_refs": [],
            "projection_id": protocol::SHELL_PROJECTION_ID,
            "command_or_event_type": protocol::SHELL_CONVERSATION_LOG,
            "conversation_request": {
                "roomDir": config.room_dir, "sessionId": session,
                "operator": "Sol", "spirit": "Kodo", "source": "agent_end",
                "persist": persist, "messages": messages, "giga": giga,
            }
        });
        let response = process_text(&self.state, &command.to_string()).await;
        let event: Value =
            serde_json::from_str(&response.direct[0]).expect("the shell result is JSON");
        event["result"].clone()
    }

    async fn jobs(&self, session: &str) -> TestResult<i64> {
        Ok(sqlx::query_scalar(
            "SELECT count(*) FROM giga_events WHERE room = 'kodo' AND session_id = $1",
        )
        .bind(session)
        .fetch_one(&self.pool)
        .await?)
    }
}

async fn durable_capture(room: &CaptureRoom) -> TestResult {
    let session = "capture-durable";
    let window = json!([
        turn("user", "u1", "first question", 1),
        turn("assistant", "a1", "first answer", 2),
    ]);
    let first = room
        .capture(session, true, enabled(&[]), window.clone())
        .await;
    let giga = &first["giga"];
    require(
        giga["status"] == "captured",
        format!("first capture: {first}"),
    )?;
    require(
        giga["submittedTurns"] == 2 && giga["acceptedTurns"] == 2 && giga["duplicateTurns"] == 0,
        format!("first capture counts: {giga}"),
    )?;
    // The missing credential is reported beside the capture, never in its way.
    require(
        giga["classifierStatus"]["credential_required"] == true,
        format!("classifier status: {giga}"),
    )?;
    let event_ids = giga["eventIds"].clone();
    require(
        event_ids.as_array().map(Vec::len) == Some(2),
        format!("one job per turn: {giga}"),
    )?;
    require(room.jobs(session).await? == 2, "two durable turn jobs")?;

    // A lost receipt: the harness sends the identical window again.
    let resent = room.capture(session, true, enabled(&[]), window).await;
    let giga = &resent["giga"];
    require(
        giga["status"] == "captured" && giga["acceptedTurns"] == 0 && giga["duplicateTurns"] == 2,
        format!("resent capture: {giga}"),
    )?;
    require(
        giga["eventIds"] == event_ids,
        "a resend names the same jobs",
    )?;
    require(room.jobs(session).await? == 2, "a resend adds no job")?;

    // The next turn regroups the window; only the new turn becomes a job.
    let grown = json!([
        turn("user", "u1", "first question", 1),
        turn("assistant", "a1", "first answer", 2),
        turn("user", "u2", "second question", 3),
    ]);
    let next = room.capture(session, true, enabled(&[]), grown).await;
    let giga = &next["giga"];
    require(
        giga["acceptedTurns"] == 1 && giga["duplicateTurns"] == 2,
        format!("grown window: {giga}"),
    )?;
    require(room.jobs(session).await? == 3, "the new turn is a job")?;
    Ok(())
}

async fn refused_capture(room: &CaptureRoom) -> TestResult {
    let window = json!([turn("user", "o1", "not for GIGA", 1)]);
    let opted_out = |field: &str| {
        let mut giga = enabled(&[]);
        giga[field] = json!(field != "gigaEnabled");
        giga
    };
    for (giga, persist, reason) in [
        (opted_out("replayMode"), true, "replay"),
        (opted_out("subagent"), true, "subagent"),
        (opted_out("gigaEnabled"), true, "disabled"),
        (enabled(&[]), false, "not_persisted"),
    ] {
        let session = format!("capture-skip-{reason}");
        let receipt = room.capture(&session, persist, giga, window.clone()).await;
        require(
            receipt["giga"]["status"] == "skipped" && receipt["giga"]["reason"] == reason,
            format!("{reason} capture: {receipt}"),
        )?;
        require(
            room.jobs(&session).await? == 0,
            format!("{reason} adds no job"),
        )?;
    }
    let absent = room
        .capture("capture-absent", true, Value::Null, window)
        .await;
    require(absent["giga"].is_null(), format!("no GIGA asked: {absent}"))?;
    require(
        room.jobs("capture-absent").await? == 0,
        "no producer, no job",
    )?;

    // A ledger identity conflict keeps its turn out; its neighbour lands.
    let session = "capture-ledger-conflict";
    room.capture(
        session,
        true,
        enabled(&[]),
        json!([turn("user", "m1", "original words", 1)]),
    )
    .await;
    let conflicted = room
        .capture(
            session,
            true,
            enabled(&[]),
            json!([
                turn("user", "m1", "rewritten words", 1),
                turn("assistant", "m2", "a fresh reply", 2),
            ]),
        )
        .await;
    let giga = &conflicted["giga"];
    require(
        giga["status"] == "partial" && giga["ledgerFailures"] == 1 && giga["acceptedTurns"] == 1,
        format!("ledger conflict: {conflicted}"),
    )?;
    require(
        conflicted["errors"].as_array().is_some_and(|errors| {
            errors
                .iter()
                .any(|error| error["surface"] == "giga_source_ledger")
        }),
        format!("ledger conflict is named: {conflicted}"),
    )?;
    require(room.jobs(session).await? == 2, "the fresh reply is a job")?;

    // A queue identity conflict fails its atomic chunk; the chunk is resent
    // turn by turn, so only the conflicting turn stays out.
    let session = "capture-identity-conflict";
    room.capture(
        session,
        true,
        enabled(&["alpha"]),
        json!([turn("user", "p1", "scoped words", 1)]),
    )
    .await;
    let moved = room
        .capture(
            session,
            true,
            enabled(&["beta"]),
            json!([
                turn("user", "p1", "scoped words", 1),
                turn("assistant", "p2", "a new reply", 2),
            ]),
        )
        .await;
    let giga = &moved["giga"];
    require(
        giga["status"] == "partial"
            && giga["acceptedTurns"] == 1
            && giga["errors"].as_array().map(Vec::len) == Some(1)
            && giga["errors"][0]["turns"] == 1,
        format!("identity conflict: {giga}"),
    )?;
    require(room.jobs(session).await? == 2, "the new reply is a job")?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn conversation_capture_persists_giga_turn_jobs_before_its_receipt() {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("the capture proof requires a dedicated PostgreSQL URL");
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
        "refusing a live or production-looking database"
    );
    let options = PgConnectOptions::from_str(&url).expect("dedicated test URL must be valid");
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .expect("isolated database must be reachable");
    let schema = format!("athanor_capture_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .expect("isolated capture schema must create");
    let directory = std::env::temp_dir().join(format!("athanor-capture-{}", Uuid::new_v4()));
    let connection_schema = schema.clone();
    let outcome: TestResult = async {
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .after_connect(move |connection, _meta| {
                let schema = connection_schema.clone();
                Box::pin(async move {
                    sqlx::query(&format!("SET search_path TO {schema}, public"))
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect_with(options)
            .await?;
        akasha::migrations::run_migrations(&pool).await?;
        let room = CaptureRoom::open(pool, &directory)?;
        durable_capture(&room).await?;
        refused_capture(&room).await?;
        Ok(())
    }
    .await;
    sqlx::query(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
        .execute(&admin)
        .await
        .expect("isolated capture schema must drop");
    let _ = std::fs::remove_dir_all(&directory);
    outcome.expect("conversation capture contract");
}

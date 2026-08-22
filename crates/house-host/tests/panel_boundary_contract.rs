//! Boundary proof for the panel read door (guild-hall #172/#174 contract;
//! timeline doors from #183/#185).
//!
//! Kills: a panel route reachable without the bearer token, a query string
//! smuggled past the body contract, a panel that invents data when the
//! database is absent, or a read that leaks write authority. The panel is
//! read-only: board, inbox, evidence, memory timeline, memory read, lesson
//! timeline - nothing else.

use house_host::{Host, HostConfig, KnockAutonomy};
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

const DOCKET_MIGRATION: &str = include_str!("../../../substrate/migrations/0023_docket.sql");
const HALLWAY_MIGRATION: &str =
    include_str!("../../../substrate/migrations/0018_hallway_chatrooms.sql");
const BELL_MIGRATION: &str = include_str!("../../../substrate/migrations/0020_hallway_bell.sql");
const TOKEN: &str = "test-only-panel-host-token";

fn isolated_database_url() -> String {
    let url = std::env::var("SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL")
        .expect("Host panel proof requires a dedicated PostgreSQL URL");
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
        "refusing a live or production-looking database"
    );
    url
}

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
            "room": "test-room",
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
        house_id: "test-house".into(),
        room: "test-room".into(),
        spirit: "Prover".into(),
        session: "configured-session".into(),
        recipient: "house-host".into(),
        database_url: None,
        akasha_enabled: false,
        nats_url: None,
        knock_autonomy: KnockAutonomy::Off,
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

#[tokio::test]
async fn panel_refuses_the_unauthenticated_and_the_database_less() {
    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let host = start_with_config(config(root.path())).await;
    let client = reqwest::Client::new();

    for path in [
        "/athanor/v1/docket/board",
        "/athanor/v1/hallway/inbox",
        "/athanor/v1/docket/evidence",
        "/athanor/v1/memory/timeline",
        "/athanor/v1/memory/read",
        "/athanor/v1/lesson/timeline",
    ] {
        // No bearer: refused before anything else.
        let unauthenticated = client
            .post(endpoint(&host, path))
            .json(&json!({}))
            .send()
            .await
            .expect("request");
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

        // A query string is not part of the contract.
        let queried = client
            .post(format!("{}?probe=1", endpoint(&host, path)))
            .bearer_auth(TOKEN)
            .json(&json!({}))
            .send()
            .await
            .expect("request");
        assert_eq!(queried.status(), StatusCode::BAD_REQUEST);
    }

    // With the bearer and no database, the board says unavailable rather
    // than inventing an empty House.
    let no_database = client
        .post(endpoint(&host, "/athanor/v1/docket/board"))
        .bearer_auth(TOKEN)
        .json(&json!({}))
        .send()
        .await
        .expect("request");
    assert_eq!(no_database.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: Value = no_database.json().await.expect("error body");
    assert_eq!(body["error"], "panel_database_unavailable");

    // An unknown field refuses instead of being stripped in silence - on the
    // original door and on a timeline door, which deserializes the
    // substrate's own deny_unknown_fields params directly.
    let unknown_field = client
        .post(endpoint(&host, "/athanor/v1/docket/board"))
        .bearer_auth(TOKEN)
        .json(&json!({"assignedTo": "tuner"}))
        .send()
        .await
        .expect("request");
    assert_eq!(unknown_field.status(), StatusCode::BAD_REQUEST);
    let unknown_timeline_field = client
        .post(endpoint(&host, "/athanor/v1/memory/timeline"))
        .bearer_auth(TOKEN)
        .json(&json!({"spirit": "tuner"}))
        .send()
        .await
        .expect("request");
    assert_eq!(unknown_timeline_field.status(), StatusCode::BAD_REQUEST);

    // Database-less timeline says unavailable rather than inventing an
    // empty history.
    let no_database_timeline = client
        .post(endpoint(&host, "/athanor/v1/memory/timeline"))
        .bearer_auth(TOKEN)
        .json(&json!({}))
        .send()
        .await
        .expect("request");
    assert_eq!(
        no_database_timeline.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );

    host.stop().await;
}

// red-proof: route the board handler past the bearer layer, or return an
// empty board instead of 503 when the pool is absent.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Docket schema"]
async fn panel_reads_board_inbox_and_evidence_read_only() {
    let url = isolated_database_url();
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("connect test database");
    sqlx::query("DROP SCHEMA IF EXISTS docket CASCADE")
        .execute(&pool)
        .await
        .expect("reset docket");
    sqlx::raw_sql(DOCKET_MIGRATION)
        .execute(&pool)
        .await
        .expect("docket migration");
    sqlx::raw_sql(HALLWAY_MIGRATION)
        .execute(&pool)
        .await
        .expect("hallway migration");
    sqlx::raw_sql(BELL_MIGRATION)
        .execute(&pool)
        .await
        .expect("bell migration");
    let quest_id: String = sqlx::query_scalar(
        "INSERT INTO docket.quests (
            house_id, kind, title, body, authority_ceiling,
            posted_by_room, posted_by_spirit,
            intent_authority_principal, acceptance_policy,
            acceptance_policy_digest, review_class, state, activated_at,
            deadline_at
         ) VALUES (
            'test-house', 'maintenance', 'panel quest', 'bounded proof', 'operator',
            'test-room', 'Prover',
            'test-authority', '{\"mode\":\"contract\"}'::jsonb,
            '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
            'R1', 'offered', NOW(), NOW() + INTERVAL '1 day'
         )
         RETURNING quest_id::text",
    )
    .fetch_one(&pool)
    .await
    .expect("insert quest");

    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let mut configured = config(root.path());
    configured.database_url = Some(url);
    let host = start_with_config(configured).await;
    let client = reqwest::Client::new();

    let board = client
        .post(endpoint(&host, "/athanor/v1/docket/board"))
        .bearer_auth(TOKEN)
        .json(&json!({"states": ["offered"]}))
        .send()
        .await
        .expect("board request");
    assert_eq!(board.status(), StatusCode::OK);
    let board: Value = board.json().await.expect("board body");
    assert_eq!(board["ok"], true);
    let quests = board["quests"].as_array().expect("quests array");
    assert!(
        quests
            .iter()
            .any(|quest| quest["questId"] == Value::String(quest_id.clone())),
        "the offered quest must be on the panel board"
    );

    let inbox = client
        .post(endpoint(&host, "/athanor/v1/hallway/inbox"))
        .bearer_auth(TOKEN)
        .json(&json!({}))
        .send()
        .await
        .expect("inbox request");
    assert_eq!(inbox.status(), StatusCode::OK);
    let inbox: Value = inbox.json().await.expect("inbox body");
    assert_eq!(inbox["ok"], true);

    let evidence = client
        .post(endpoint(&host, "/athanor/v1/docket/evidence"))
        .bearer_auth(TOKEN)
        .json(&json!({"questId": quest_id}))
        .send()
        .await
        .expect("evidence request");
    assert_eq!(evidence.status(), StatusCode::OK);
    let evidence: Value = evidence.json().await.expect("evidence body");
    assert_eq!(evidence["ok"], true);
    assert!(evidence["receipts"].as_array().is_some());

    // A missing quest surfaces the substrate's own refusal, not an invention.
    let missing = client
        .post(endpoint(&host, "/athanor/v1/docket/evidence"))
        .bearer_auth(TOKEN)
        .json(&json!({"questId": "00000000-0000-0000-0000-000000000009"}))
        .send()
        .await
        .expect("missing evidence request");
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);

    host.stop().await;
}

// red-proof: point a timeline route at the wrong table, return the excerpt
// as the body, or invent a memory for an unknown id and this goes red.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL with base migrations applied (memories, lessons); cleans up its own rows"]
async fn panel_reads_memory_and_lesson_timelines_read_only() {
    let url = isolated_database_url();
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("connect test database");
    let proof_room = format!(
        "panel-proof-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .subsec_nanos()
    );
    let memory_id: i64 = sqlx::query_scalar(
        "INSERT INTO memories (room,type,title,source_path,body,threads)
         VALUES ($1,'conversa','panel proof',$2,$3,'{\"panel proof thread\"}')
         RETURNING id",
    )
    .bind(&proof_room)
    .bind(format!("proof/{proof_room}.md"))
    .bind("the panel reads this exact body")
    .fetch_one(&pool)
    .await
    .expect("insert proof memory");
    let lesson_id: i64 = sqlx::query_scalar(
        "INSERT INTO lessons (lesson_key,title,lesson)
         VALUES ('coding','panel proof lesson','the registry timeline reads this row')
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert proof lesson");

    let root = TempDir::new().expect("temporary Host root");
    write_room_state(root.path());
    let mut configured = config(root.path());
    configured.database_url = Some(url);
    let host = start_with_config(configured).await;
    let client = reqwest::Client::new();

    let proof = async {
        let timeline = client
            .post(endpoint(&host, "/athanor/v1/memory/timeline"))
            .bearer_auth(TOKEN)
            .json(&json!({ "room": proof_room }))
            .send()
            .await
            .expect("timeline request");
        assert_eq!(timeline.status(), StatusCode::OK);
        let timeline: Value = timeline.json().await.expect("timeline body");
        assert_eq!(timeline["ok"], true);
        let memories = timeline["memories"].as_array().expect("memories array");
        assert_eq!(memories.len(), 1, "exactly the proof room's one memory");
        assert_eq!(memories[0]["id"], json!(memory_id));
        assert_eq!(memories[0]["title"], "panel proof");
        assert_eq!(memories[0]["excerpt"], "the panel reads this exact body");

        let read = client
            .post(endpoint(&host, "/athanor/v1/memory/read"))
            .bearer_auth(TOKEN)
            .json(&json!({ "id": memory_id }))
            .send()
            .await
            .expect("read request");
        assert_eq!(read.status(), StatusCode::OK);
        let read: Value = read.json().await.expect("read body");
        assert_eq!(read["memory"]["body"], "the panel reads this exact body");
        assert_eq!(read["memory"]["threads"][0], "panel proof thread");

        let lessons = client
            .post(endpoint(&host, "/athanor/v1/lesson/timeline"))
            .bearer_auth(TOKEN)
            .json(&json!({ "type": "coding", "limit": 50 }))
            .send()
            .await
            .expect("lesson timeline request");
        assert_eq!(lessons.status(), StatusCode::OK);
        let lessons: Value = lessons.json().await.expect("lesson body");
        assert_eq!(lessons["ok"], true);
        assert!(
            lessons["lessons"]
                .as_array()
                .expect("lessons array")
                .iter()
                .any(|lesson| lesson["id"] == json!(lesson_id)),
            "the fresh proof lesson must lead the coding timeline page"
        );
    };
    proof.await;

    sqlx::query("DELETE FROM memories WHERE id=$1")
        .bind(memory_id)
        .execute(&pool)
        .await
        .expect("clean proof memory");
    sqlx::query("DELETE FROM lessons WHERE lesson_key='coding' AND id=$1")
        .bind(lesson_id)
        .execute(&pool)
        .await
        .expect("clean proof lesson");

    // After cleanup the same id is a typed refusal, never an invention.
    let missing = client
        .post(endpoint(&host, "/athanor/v1/memory/read"))
        .bearer_auth(TOKEN)
        .json(&json!({ "id": memory_id }))
        .send()
        .await
        .expect("missing memory request");
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);

    host.stop().await;
}

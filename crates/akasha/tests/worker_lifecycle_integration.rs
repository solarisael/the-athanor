//! PostgreSQL contract for `worker_lifecycle`: row locking, idempotent retry
//! under concurrency, append-only entries, token hashing, and disposal.
//! Each test uses its own house id, so tests share one migrated schema.

use akasha::AppError;
use akasha::worker_lifecycle::worker_lifecycle;
use hearth::worker_lifecycle::Call;
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const MIGRATION: &str = include_str!("../../../substrate/migrations/0034_worker_lifecycle.sql");

fn isolated_database_url() -> String {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("worker lifecycle proof requires a dedicated PostgreSQL URL");
    let options: sqlx::postgres::PgConnectOptions = url.parse().expect("valid test URL");
    let database = options
        .get_database()
        .expect("explicit test database")
        .to_ascii_lowercase();
    assert!(
        database.contains("test") && !database.contains("solarisael"),
        "refusing a non-test or live database, including percent-encoded names"
    );
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
        "refusing a live or production-looking database"
    );
    url
}

/// Apply 0034 twice under a session advisory lock: re-application must be a
/// no-op, and parallel tests must not race catalog creation.
async fn migrated() -> TestResult<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&isolated_database_url())
        .await?;
    let mut connection = pool.acquire().await?;
    sqlx::query("SELECT pg_advisory_lock(hashtext('worker_lifecycle_integration'))")
        .execute(&mut *connection)
        .await?;
    let applied = async {
        sqlx::raw_sql(MIGRATION).execute(&mut *connection).await?;
        sqlx::raw_sql(MIGRATION).execute(&mut *connection).await
    }
    .await;
    sqlx::query("SELECT pg_advisory_unlock(hashtext('worker_lifecycle_integration'))")
        .execute(&mut *connection)
        .await?;
    applied?;
    drop(connection);
    Ok(pool)
}

struct Room {
    house: String,
}

impl Room {
    fn new() -> Self {
        Self {
            house: format!("test-{}", Uuid::new_v4().simple()),
        }
    }

    async fn call(
        &self,
        pool: &PgPool,
        session: &str,
        request_id: &str,
        params: Value,
    ) -> Result<Value, AppError> {
        let Value::Object(mut map) = params else {
            panic!("params object")
        };
        map.insert("houseId".into(), json!(self.house));
        map.insert("room".into(), json!("kodo"));
        map.insert("spirit".into(), json!("Kodo"));
        map.insert("session".into(), json!(session));
        map.insert("requestId".into(), json!(request_id));
        let call = Call::from_params(Value::Object(map)).map_err(AppError::Invalid)?;
        worker_lifecycle(pool, call).await
    }
}

fn token(room: &Room, seed: char) -> String {
    format!("{}-{seed}", room.house)
}

fn refusal(result: Result<Value, AppError>) -> &'static str {
    match result {
        Err(AppError::Refusal { code, .. }) => code,
        other => panic!("expected refusal, got {other:?}"),
    }
}

fn prepare(room: &Room, slot: &str, seed: char) -> Value {
    json!({"action": "prepare", "swarmKey": "toolcall-1", "slotKey": slot, "agentName": "task", "task": "raise the wall", "token": token(room, seed)})
}

fn open(room: &Room, seed: char) -> Value {
    json!({"action": "open", "token": token(room, seed), "agentId": "0-Task", "agentName": "task"})
}

// Kills: a retried prepare minting a second book, or two first prepares of one
// native call racing into two swarms.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; uses its own house id"]
async fn concurrent_retries_commit_one_identity() -> TestResult {
    let pool = migrated().await?;
    let room = Room::new();
    let (first, second) = tokio::join!(
        room.call(&pool, "parent", "prep-0", prepare(&room, "0", 'a')),
        room.call(&pool, "parent", "prep-0", prepare(&room, "0", 'a')),
    );
    let (first, second) = (first?, second?);
    assert_eq!(first, second);
    assert_eq!(first["token"], json!(token(&room, 'a')));
    let (a, b) = tokio::join!(
        room.call(&pool, "parent", "prep-1", prepare(&room, "1", 'b')),
        room.call(&pool, "parent", "prep-2", prepare(&room, "2", 'c')),
    );
    assert_eq!(a?["swarmId"], first["swarmId"]);
    assert_eq!(b?["swarmId"], first["swarmId"]);
    let books: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM worker_lifecycle.books b JOIN worker_lifecycle.swarms s ON s.swarm_id=b.swarm_id WHERE s.house_id=$1",
    )
    .bind(&room.house)
    .fetch_one(&pool)
    .await?;
    assert_eq!(books, 3);
    // Only the hash is stored; no row or receipt carries the token.
    let leaked: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM worker_lifecycle.requests WHERE house_id=$1 AND response::text LIKE '%' || $2 || '%')
              + (SELECT count(*) FROM worker_lifecycle.instances WHERE token_hash=$2)",
    )
    .bind(&room.house)
    .bind(token(&room, 'a'))
    .fetch_one(&pool)
    .await?;
    assert_eq!(leaked, 0);
    Ok(())
}

// Kills: two sessions both binding one reservation when their opens race.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; uses its own house id"]
async fn racing_opens_bind_exactly_one_session() -> TestResult {
    let pool = migrated().await?;
    let room = Room::new();
    room.call(&pool, "parent", "prep", prepare(&room, "0", 'a')).await?;
    let (one, two) = tokio::join!(
        room.call(&pool, "child-1", "open", open(&room, 'a')),
        room.call(&pool, "child-2", "open", open(&room, 'a')),
    );
    let outcomes = [one.is_ok(), two.is_ok()];
    assert_eq!(outcomes.iter().filter(|ok| **ok).count(), 1);
    let refused = if one.is_ok() { two } else { one };
    assert_eq!(refusal(refused), "reservation_claimed");
    Ok(())
}

// Kills: entries editable or deletable outside disposal, accepted evidence
// lost with the transient book, or disposal claiming backup erasure.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; uses its own house id"]
async fn entries_are_append_only_until_whole_swarm_disposal() -> TestResult {
    let pool = migrated().await?;
    let room = Room::new();
    let prepared = room.call(&pool, "parent", "prep", prepare(&room, "0", 'a')).await?;
    let book = prepared["bookId"].as_str().unwrap().to_owned();
    let swarm = prepared["swarmId"].as_str().unwrap().to_owned();
    room.call(&pool, "child", "open", open(&room, 'a')).await?;
    room.call(&pool, "child", "note", json!({"action": "append", "bookId": book, "kind": "note", "body": "laid stones"}))
        .await?;
    let update = sqlx::query("UPDATE worker_lifecycle.entries SET body='\"rewritten\"' WHERE book_id=$1")
        .bind(&book)
        .execute(&pool)
        .await;
    assert!(update.is_err(), "entries must refuse UPDATE");
    let delete = sqlx::query("DELETE FROM worker_lifecycle.entries WHERE book_id=$1")
        .bind(&book)
        .execute(&pool)
        .await;
    assert!(delete.is_err(), "entries must refuse DELETE before disposal");

    room.call(&pool, "child", "handoff", json!({"action": "checkpoint", "bookId": book, "report": {"done": "wall"}, "terminal": true}))
        .await?;
    room.call(&pool, "parent", "accept", json!({"action": "accept", "bookId": book, "body": {"report": "wall raised", "evidence": "photo"}}))
        .await?;
    let receipt = room
        .call(&pool, "parent", "dispose", json!({"action": "dispose", "swarmId": swarm}))
        .await?;
    assert_eq!(receipt["backupsErased"], false);
    let remaining: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM worker_lifecycle.entries WHERE swarm_id=$1)
              + (SELECT count(*) FROM worker_lifecycle.books WHERE swarm_id=$1)",
    )
    .bind(&swarm)
    .fetch_one(&pool)
    .await?;
    assert_eq!(remaining, 0);
    let retained: i64 =
        sqlx::query_scalar("SELECT count(*) FROM worker_lifecycle.retained WHERE swarm_id=$1")
            .bind(&swarm)
            .fetch_one(&pool)
            .await?;
    assert_eq!(retained, 1);
    let retained_delete = sqlx::query("DELETE FROM worker_lifecycle.retained WHERE swarm_id=$1")
        .bind(&swarm)
        .execute(&pool)
        .await;
    assert!(retained_delete.is_err(), "retained evidence is append-only");
    // The disposal receipt replays; the disposed book is gone.
    assert_eq!(
        room.call(&pool, "parent", "dispose", json!({"action": "dispose", "swarmId": swarm}))
            .await?,
        receipt
    );
    let read = room
        .call(&pool, "parent", "read", json!({"action": "read", "bookId": book}))
        .await;
    assert_eq!(refusal(read), "unknown_book");
    Ok(())
}

// Kills: a successor opening from an incomplete handoff, pending operations
// dropped across succession, or a retired worker resuming through the store.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; uses its own house id"]
async fn succession_persists_through_postgres() -> TestResult {
    let pool = migrated().await?;
    let room = Room::new();
    let prepared = room.call(&pool, "parent", "prep", prepare(&room, "0", 'a')).await?;
    let book = prepared["bookId"].as_str().unwrap().to_owned();
    let first = prepared["instanceId"].as_str().unwrap().to_owned();
    room.call(&pool, "child", "open", open(&room, 'a')).await?;
    room.call(&pool, "child", "start", json!({"action": "append", "bookId": book, "kind": "tool-start", "operationId": "call-1", "body": {"tool": "bash"}}))
        .await?;
    for (request, tokens) in [("o1", 650), ("o2", 650), ("o3", 650)] {
        let observed = room
            .call(&pool, "child", request, json!({"action": "observe", "bookId": book, "tokens": tokens, "contextWindow": 1000, "requestStarted": true}))
            .await?;
        assert_eq!(observed["state"], "handoff_only");
        assert_eq!(observed["retireRequired"], request == "o3");
    }
    room.call(&pool, "child", "settle", json!({"action": "settle", "bookId": book, "outcome": "aborted"}))
        .await?;
    assert_eq!(refusal(room.call(&pool, "child", "reopen", open(&room, 'a')).await), "instance_retired");
    let successor = json!({"action": "prepare", "swarmKey": "toolcall-2", "slotKey": "0", "agentName": "task", "task": "continue", "token": token(&room, 'b'), "predecessor": {"bookId": book, "instanceId": first}});
    assert_eq!(
        refusal(room.call(&pool, "parent", "succ-1", successor.clone()).await),
        "handoff_incomplete"
    );
    room.call(&pool, "parent", "repair", json!({"action": "repair", "bookId": book, "instanceId": first, "report": "bash may have run; check first"}))
        .await?;
    room.call(&pool, "parent", "succ-2", successor).await?;
    let opened = room.call(&pool, "heir", "open", open(&room, 'b')).await?;
    assert_eq!(opened["wake"]["kind"], "successor");
    assert_eq!(opened["pendingOperations"][0]["operationId"], "call-1");
    assert!(
        opened["wake"]["text"]
            .as_str()
            .unwrap()
            .contains("check first")
    );
    Ok(())
}

// Kills: accepted evidence stranded after disposal with no public read, a
// child or another house reading it, or a cursor from elsewhere being followed.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; uses its own house id"]
async fn retained_evidence_pages_for_its_parent_after_disposal() -> TestResult {
    let pool = migrated().await?;
    let room = Room::new();
    let prepared = room.call(&pool, "parent", "prep", prepare(&room, "0", 'a')).await?;
    let book = prepared["bookId"].as_str().unwrap().to_owned();
    let swarm = prepared["swarmId"].as_str().unwrap().to_owned();
    room.call(&pool, "child", "open", open(&room, 'a')).await?;
    room.call(&pool, "child", "note", json!({"action": "append", "bookId": book, "kind": "note", "body": "transient-only"}))
        .await?;
    room.call(&pool, "child", "done", json!({"action": "checkpoint", "bookId": book, "report": "finished", "terminal": true}))
        .await?;
    for (request, body) in [("acc-1", "kept-one"), ("acc-2", "kept-two"), ("acc-3", "kept-three")] {
        room.call(&pool, "parent", request, json!({"action": "accept", "bookId": book, "body": body}))
            .await?;
    }
    room.call(&pool, "parent", "dispose", json!({"action": "dispose", "swarmId": swarm}))
        .await?;

    let mut bodies = Vec::new();
    let mut cursor: Option<String> = None;
    for page_number in 0..3 {
        let page = room
            .call(&pool, "parent", &format!("page-{page_number}"), json!({"action": "retained", "swarmId": swarm, "after": cursor, "limit": 2}))
            .await?;
        assert!(!page.to_string().contains("transient-only"));
        for record in page["retained"].as_array().unwrap() {
            assert_eq!(record["swarmId"], json!(swarm));
            assert_eq!(record["acceptedBySession"], "parent");
            bodies.push(record["body"].as_str().unwrap().to_owned());
        }
        cursor = page["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    bodies.sort();
    assert_eq!(bodies, ["kept-one", "kept-three", "kept-two"]);

    let child = room
        .call(&pool, "child", "child-read", json!({"action": "retained", "swarmId": swarm}))
        .await;
    assert_eq!(refusal(child), "parent_only");
    let stranger = Room::new();
    let foreign = stranger
        .call(&pool, "parent", "foreign-read", json!({"action": "retained", "swarmId": swarm}))
        .await;
    assert_eq!(refusal(foreign), "unknown_swarm");

    let other = room
        .call(&pool, "parent", "prep-other", json!({"action": "prepare", "swarmKey": "toolcall-9", "slotKey": "0", "agentName": "task", "task": "other", "token": token(&room, 'z')}))
        .await?;
    room.call(&pool, "parent", "acc-other", json!({"action": "accept", "bookId": other["bookId"], "body": "other-swarm"}))
        .await?;
    let first = room
        .call(&pool, "parent", "first-page", json!({"action": "retained", "swarmId": swarm, "limit": 1}))
        .await?;
    let wrong_swarm = room
        .call(&pool, "parent", "wrong-cursor", json!({"action": "retained", "swarmId": other["swarmId"], "after": first["retained"][0]["retainedId"]}))
        .await;
    assert_eq!(refusal(wrong_swarm), "unknown_cursor");
    Ok(())
}

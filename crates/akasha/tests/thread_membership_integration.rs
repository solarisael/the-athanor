mod support;

use akasha::{AppError, Config, MemoryThreadLeaveParams, memory_thread_leave, recall, remember};
use hearth::{
    MemoryScope, OperatorKey, RecallProjection, RecallRequest, RememberMemoryDetails,
    RememberRequest, RoomKey, ThreadContinuation,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{drop_isolated_schema, isolated_database};
use uuid::Uuid;

const THREAD: &str = "harbor / ledger";

/// One memory through the real write path, in `thread`, continuing `previous`.
async fn write(
    pool: &PgPool,
    cfg: &Config,
    room: &str,
    thread: &str,
    writer: Option<(&str, MemoryScope)>,
    title: &str,
    previous: Option<i64>,
) -> i64 {
    let details = RememberMemoryDetails {
        source_path: None,
        threads: vec![thread.into()],
        continues: previous
            .map(|id| ThreadContinuation {
                thread: thread.into(),
                previous_memory_id: id as u64,
            })
            .into_iter()
            .collect(),
        supersedes: vec![],
        backup: false,
    };
    let mut request = RememberRequest::new_memory(
        RoomKey::for_memory_write(room).unwrap(),
        title.into(),
        format!("The thread membership proof records {title}."),
        details,
    )
    .unwrap();

    if let Some((operator, scope)) = writer {
        request = request
            .with_operator(Some(OperatorKey::new(operator).unwrap()), Some(scope))
            .unwrap();
    }
    remember(pool, cfg, request)
        .await
        .expect("memory must commit")
        .memory_id as i64
}

/// A primary-key recall of `ids`, read as `reader`.
async fn recall_ids(
    pool: &PgPool,
    cfg: &Config,
    room: &str,
    reader: Option<&str>,
    ids: &[i64],
) -> Value {
    let query = format!(
        "memories {}",
        ids.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    let request = RecallRequest::new(RoomKey::new(room).unwrap(), query, 8, 0.4, 8, 0.3)
        .unwrap()
        .with_operator(reader.map(|key| OperatorKey::new(key).unwrap()))
        .with_projection(RecallProjection::Manual);

    serde_json::to_value(recall(pool, cfg, request, None).await.unwrap()).unwrap()
}

/// The `threads` field of the candidate for memory `id`.
fn threads_of(recalled: &Value, id: i64) -> Value {
    recalled["retrievalCandidates"]
        .as_array()
        .expect("recall must return candidates")
        .iter()
        .find(|candidate| candidate["memory_id"].as_i64() == Some(id))
        .unwrap_or_else(|| panic!("memory {id} must be a recall candidate"))["threads"]
        .clone()
}

/// Every active (predecessor, successor) memory pair of one thread.
async fn links(pool: &PgPool, room: &str, thread: &str) -> Vec<(i64, i64)> {
    sqlx::query_as(
        "SELECT previous_event.memory_id,next_event.memory_id
         FROM thread_event_links link
         JOIN threads t ON t.id=link.thread_id
         JOIN thread_events previous_event ON previous_event.id=link.previous_event_id
         JOIN thread_events next_event ON next_event.id=link.next_event_id
         WHERE t.room=$1 AND t.thread_key=$2
         ORDER BY next_event.memory_id",
    )
    .bind(room)
    .bind(thread)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn thread_keys(pool: &PgPool, id: i64) -> Vec<String> {
    sqlx::query_scalar("SELECT threads FROM memories WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn archive_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM thread_event_archive")
        .fetch_one(pool)
        .await
        .unwrap()
}

fn leave(room: &str, ids: &[i64], reason: &str) -> MemoryThreadLeaveParams {
    MemoryThreadLeaveParams {
        thread: format!("  {THREAD}  "),
        memory_ids: ids.iter().map(ToString::to_string).collect(),
        room: room.into(),
        reason: reason.into(),
        operator: Some("owner".into()),
    }
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and ATHANOR_SUBSTRATE_TEST_SCHEMA"]
async fn recall_names_each_thread_with_its_neighbors_and_hides_a_private_neighbor() {
    let (pool, cfg, schema) = isolated_database("threads_recall").await;
    let room = format!("thread-membership-{}", Uuid::new_v4());
    let first = write(&pool, &cfg, &room, THREAD, None, "first entry", None).await;
    let second = write(
        &pool,
        &cfg,
        &room,
        THREAD,
        None,
        "second entry",
        Some(first),
    )
    .await;
    let third = write(
        &pool,
        &cfg,
        &room,
        THREAD,
        None,
        "third entry",
        Some(second),
    )
    .await;
    let recalled = recall_ids(&pool, &cfg, &room, None, &[first, second, third]).await;

    assert_eq!(
        threads_of(&recalled, first),
        json!([{"key": THREAD, "previousMemoryId": null, "nextMemoryIds": [second]}])
    );
    assert_eq!(
        threads_of(&recalled, second),
        json!([{"key": THREAD, "previousMemoryId": first, "nextMemoryIds": [third]}])
    );
    assert_eq!(
        threads_of(&recalled, third),
        json!([{"key": THREAD, "previousMemoryId": second, "nextMemoryIds": []}])
    );

    sqlx::query(
        "INSERT INTO units (path, display_name) VALUES
           ('acme'::ltree, 'Acme'), ('acme.eng'::ltree, 'Engineering')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO operators (key, display_name, unit, rank) VALUES
           ('staff', 'Staff', 'acme.eng'::ltree, 1),
           ('peer', 'Peer', 'acme.eng'::ltree, 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let private_thread = "harbor / private";
    let staff = |scope| Some(("staff", scope));
    let open_first = write(
        &pool,
        &cfg,
        &room,
        private_thread,
        staff(MemoryScope::Company),
        "open first",
        None,
    )
    .await;
    let hidden = write(
        &pool,
        &cfg,
        &room,
        private_thread,
        staff(MemoryScope::Private),
        "private middle",
        Some(open_first),
    )
    .await;
    let open_last = write(
        &pool,
        &cfg,
        &room,
        private_thread,
        staff(MemoryScope::Company),
        "open last",
        Some(hidden),
    )
    .await;
    let as_peer = recall_ids(&pool, &cfg, &room, Some("peer"), &[open_first, open_last]).await;
    let as_staff = recall_ids(&pool, &cfg, &room, Some("staff"), &[open_first, open_last]).await;

    assert_eq!(
        threads_of(&as_peer, open_first),
        json!([{"key": private_thread, "previousMemoryId": null, "nextMemoryIds": []}]),
        "a reader never sees the ID of a neighbor it cannot read"
    );
    assert_eq!(
        threads_of(&as_peer, open_last),
        json!([{"key": private_thread, "previousMemoryId": null, "nextMemoryIds": []}])
    );
    assert_eq!(
        threads_of(&as_staff, open_first),
        json!([{"key": private_thread, "previousMemoryId": null, "nextMemoryIds": [hidden]}])
    );
    assert_eq!(
        threads_of(&as_staff, open_last),
        json!([{"key": private_thread, "previousMemoryId": hidden, "nextMemoryIds": []}])
    );

    drop_isolated_schema(pool, &schema).await;
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and ATHANOR_SUBSTRATE_TEST_SCHEMA"]
async fn unthreading_the_middle_member_relinks_the_chain_and_keeps_its_history() {
    let (pool, cfg, schema) = isolated_database("threads_leave").await;
    let room = format!("thread-membership-{}", Uuid::new_v4());
    let first = write(&pool, &cfg, &room, THREAD, None, "first entry", None).await;
    let second = write(
        &pool,
        &cfg,
        &room,
        THREAD,
        None,
        "second entry",
        Some(first),
    )
    .await;
    let third = write(
        &pool,
        &cfg,
        &room,
        THREAD,
        None,
        "third entry",
        Some(second),
    )
    .await;
    let before: (String, String) = sqlx::query_as("SELECT title,body FROM memories WHERE id=$1")
        .bind(second)
        .fetch_one(&pool)
        .await
        .unwrap();
    let receipt = memory_thread_leave(&pool, leave(&room, &[second], "  joined by mistake  "))
        .await
        .expect("the middle member must leave the thread");

    assert_eq!(
        serde_json::to_value(&receipt).unwrap(),
        json!({
            "ok": true,
            "room": room,
            "thread": THREAD,
            "reason": "joined by mistake",
            "memories": [{
                "id": second,
                "title": "second entry",
                "wasMember": true,
                "relinkedTo": first,
                "relinkedSuccessors": [third],
            }],
        })
    );
    assert_eq!(links(&pool, &room, THREAD).await, vec![(first, third)]);

    let archived: (i64, Option<i64>, Vec<i64>, Option<String>, String, i32) = sqlx::query_as(
        "SELECT memory_id,previous_memory_id,next_memory_ids,archived_by,reason,
                jsonb_array_length(refs)
         FROM thread_event_archive WHERE memory_id=$1",
    )
    .bind(second)
    .fetch_one(&pool)
    .await
    .expect("the archive must keep the membership");
    let live_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM thread_events e
         LEFT JOIN memory_thread_refs r ON r.event_id=e.id
         WHERE e.memory_id=$1",
    )
    .bind(second)
    .fetch_one(&pool)
    .await
    .unwrap();
    let after: (String, String) = sqlx::query_as("SELECT title,body FROM memories WHERE id=$1")
        .bind(second)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(
        archived,
        (
            second,
            Some(first),
            vec![third],
            Some("owner".into()),
            "joined by mistake".into(),
            1
        )
    );
    assert_eq!(live_rows, 0, "the thread lane loses the membership");
    assert_eq!(after, before, "the leave never edits the memory text");
    assert!(thread_keys(&pool, second).await.is_empty());
    assert_eq!(thread_keys(&pool, first).await, vec![THREAD.to_owned()]);
    assert_eq!(thread_keys(&pool, third).await, vec![THREAD.to_owned()]);

    let recalled = recall_ids(&pool, &cfg, &room, None, &[first, second, third]).await;

    assert_eq!(
        threads_of(&recalled, first),
        json!([{"key": THREAD, "previousMemoryId": null, "nextMemoryIds": [third]}])
    );
    assert_eq!(threads_of(&recalled, second), json!([]));
    assert_eq!(
        threads_of(&recalled, third),
        json!([{"key": THREAD, "previousMemoryId": first, "nextMemoryIds": []}])
    );

    let repeated = memory_thread_leave(&pool, leave(&room, &[second], "joined by mistake"))
        .await
        .expect("a repeated leave must succeed");

    assert_eq!(
        serde_json::to_value(&repeated).unwrap()["memories"],
        json!([{
            "id": second,
            "title": "second entry",
            "wasMember": false,
            "relinkedSuccessors": [],
        }])
    );
    assert_eq!(archive_count(&pool).await, 1);
    assert_eq!(links(&pool, &room, THREAD).await, vec![(first, third)]);

    let root = memory_thread_leave(&pool, leave(&room, &[first], "the thread starts later"))
        .await
        .expect("the first member must leave the thread");

    assert_eq!(
        serde_json::to_value(&root).unwrap()["memories"],
        json!([{
            "id": first,
            "title": "first entry",
            "wasMember": true,
            "relinkedSuccessors": [third],
        }])
    );
    assert!(links(&pool, &room, THREAD).await.is_empty());
    assert_eq!(archive_count(&pool).await, 2);
    assert_eq!(
        threads_of(&recall_ids(&pool, &cfg, &room, None, &[third]).await, third),
        json!([{"key": THREAD, "previousMemoryId": null, "nextMemoryIds": []}])
    );

    drop_isolated_schema(pool, &schema).await;
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and ATHANOR_SUBSTRATE_TEST_SCHEMA"]
async fn one_bad_id_refuses_the_whole_unthread() {
    let (pool, cfg, schema) = isolated_database("threads_refusal").await;
    let room = format!("thread-membership-{}", Uuid::new_v4());
    let foreign_room = format!("thread-membership-{}", Uuid::new_v4());
    let first = write(&pool, &cfg, &room, THREAD, None, "first entry", None).await;
    let second = write(
        &pool,
        &cfg,
        &room,
        THREAD,
        None,
        "second entry",
        Some(first),
    )
    .await;
    let foreign = write(
        &pool,
        &cfg,
        &foreign_room,
        THREAD,
        None,
        "foreign entry",
        None,
    )
    .await;
    let refused =
        memory_thread_leave(&pool, leave(&room, &[second, foreign], "wrong thread")).await;

    match refused {
        Err(AppError::Invalid(message)) => assert!(message.contains(&foreign.to_string())),
        _ => panic!("a foreign-room ID must refuse the entire leave"),
    }

    let blank = memory_thread_leave(&pool, leave(&room, &[second], "   ")).await;

    assert!(
        matches!(blank, Err(AppError::Invalid(message)) if message.contains("reason")),
        "a leave without a reason must refuse"
    );
    assert_eq!(links(&pool, &room, THREAD).await, vec![(first, second)]);
    assert_eq!(archive_count(&pool).await, 0);
    assert_eq!(thread_keys(&pool, second).await, vec![THREAD.to_owned()]);

    drop_isolated_schema(pool, &schema).await;
}

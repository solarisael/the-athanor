mod support;

use akasha::{AppError, MemoryThreadJoinParams, memory_thread_join, remember};
use hearth::{RememberMemoryDetails, RememberRequest, RoomKey, ThreadContinuation};
use sqlx::PgPool;
use support::{drop_isolated_schema, isolated_database};
use uuid::Uuid;

fn memory_request(room: &str, title: &str, details: RememberMemoryDetails) -> RememberRequest {
    RememberRequest::new_memory(
        RoomKey::for_memory_write(room).unwrap(),
        title.into(),
        format!("The thread join integration records {title}."),
        details,
    )
    .unwrap()
}

fn unthreaded() -> RememberMemoryDetails {
    RememberMemoryDetails {
        source_path: None,
        threads: vec![],
        continues: vec![],
        supersedes: vec![],
        backup: false,
    }
}

fn in_thread(thread: &str, supersedes: Vec<u64>) -> RememberMemoryDetails {
    RememberMemoryDetails {
        source_path: None,
        threads: vec![thread.into()],
        continues: vec![],
        supersedes,
        backup: false,
    }
}

/// Every (predecessor, successor) memory pair of one thread.
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

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and ATHANOR_SUBSTRATE_TEST_SCHEMA"]
async fn joining_old_memories_enables_continuations_and_is_idempotent() {
    let (pool, cfg, schema) = isolated_database("join").await;
    let room = format!("thread-join-{}", Uuid::new_v4());
    let thread = "work / integration";
    let first = remember(
        &pool,
        &cfg,
        memory_request(&room, "first decision", unthreaded()),
    )
    .await
    .expect("first unthreaded memory must commit");
    let second = remember(
        &pool,
        &cfg,
        memory_request(&room, "second decision", unthreaded()),
    )
    .await
    .expect("second unthreaded memory must commit");
    let ids = vec![first.memory_id as i64, second.memory_id as i64];
    let successor = || {
        memory_request(
            &room,
            "continued decision",
            RememberMemoryDetails {
                source_path: None,
                threads: vec![thread.into()],
                continues: vec![ThreadContinuation {
                    thread: thread.into(),
                    previous_memory_id: first.memory_id,
                }],
                supersedes: vec![],
                backup: false,
            },
        )
    };
    let refused = remember(&pool, &cfg, successor()).await;

    assert!(
        matches!(refused, Err(AppError::Invalid(_))),
        "a predecessor without membership must refuse"
    );

    let joined = memory_thread_join(
        &pool,
        MemoryThreadJoinParams {
            thread: format!("  {thread}  "),
            memory_ids: vec![
                second.memory_id.to_string(),
                first.memory_id.to_string(),
                first.memory_id.to_string(),
            ],
            room: room.clone(),
            operator: None,
        },
    )
    .await
    .expect("both old memories must join");

    assert!(joined.ok);
    assert_eq!(joined.room, room);
    assert_eq!(joined.thread, thread);
    assert_eq!(
        joined
            .memories
            .iter()
            .map(|memory| memory.id)
            .collect::<Vec<_>>(),
        ids
    );
    assert!(joined.memories.iter().all(|memory| !memory.already_member));
    assert_eq!(joined.memories[0].title, "first decision");
    assert_eq!(joined.memories[1].title, "second decision");

    let events: Vec<i64> = sqlx::query_scalar(
        "SELECT e.memory_id FROM thread_events e JOIN threads t ON t.id=e.thread_id
         WHERE t.room=$1 AND t.thread_key=$2 ORDER BY e.memory_id",
    )
    .bind(&room)
    .bind(thread)
    .fetch_all(&pool)
    .await
    .unwrap();
    let arrays: Vec<(i64, Vec<String>)> =
        sqlx::query_as("SELECT id,threads FROM memories WHERE id=ANY($1) ORDER BY id")
            .bind(&ids)
            .fetch_all(&pool)
            .await
            .unwrap();

    assert_eq!(events, ids);
    assert_eq!(arrays.len(), 2);
    assert!(
        arrays
            .iter()
            .all(|(_, threads)| threads == &[thread.to_owned()])
    );

    let repeated = memory_thread_join(
        &pool,
        MemoryThreadJoinParams {
            thread: thread.into(),
            memory_ids: ids.iter().map(ToString::to_string).collect(),
            room: room.clone(),
            operator: None,
        },
    )
    .await
    .expect("a repeated join must succeed");
    let event_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM thread_events e JOIN threads t ON t.id=e.thread_id
         WHERE t.room=$1 AND t.thread_key=$2",
    )
    .bind(&room)
    .bind(thread)
    .fetch_one(&pool)
    .await
    .unwrap();
    let ref_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory_thread_refs r
         JOIN thread_events e ON e.id=r.event_id JOIN threads t ON t.id=e.thread_id
         WHERE t.room=$1 AND t.thread_key=$2",
    )
    .bind(&room)
    .bind(thread)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(repeated.memories.len(), 2);
    assert!(repeated.memories.iter().all(|memory| memory.already_member));
    assert_eq!(event_count, 2);
    assert_eq!(ref_count, 2);

    remember(&pool, &cfg, successor())
        .await
        .expect("the joined predecessor now permits continuation");

    drop_isolated_schema(pool, &schema).await;
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and ATHANOR_SUBSTRATE_TEST_SCHEMA"]
async fn a_foreign_room_id_refuses_the_whole_join() {
    let (pool, cfg, schema) = isolated_database("refusal").await;
    let room = format!("thread-join-{}", Uuid::new_v4());
    let foreign_room = format!("thread-join-{}", Uuid::new_v4());
    let thread = "work / refused integration";
    let local = remember(
        &pool,
        &cfg,
        memory_request(&room, "local decision", unthreaded()),
    )
    .await
    .expect("local memory must commit");
    let foreign = remember(
        &pool,
        &cfg,
        memory_request(&foreign_room, "foreign decision", unthreaded()),
    )
    .await
    .expect("foreign memory must commit");
    let ids = vec![local.memory_id as i64, foreign.memory_id as i64];
    let refused = memory_thread_join(
        &pool,
        MemoryThreadJoinParams {
            thread: thread.into(),
            memory_ids: ids.iter().map(ToString::to_string).collect(),
            room: room.clone(),
            operator: None,
        },
    )
    .await;

    match refused {
        Err(AppError::Invalid(message)) => {
            assert!(message.contains(&foreign.memory_id.to_string()))
        }
        _ => panic!("a foreign-room ID must refuse the entire join"),
    }

    let event_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM thread_events WHERE memory_id=ANY($1)")
            .bind(&ids)
            .fetch_one(&pool)
            .await
            .unwrap();
    let arrays: Vec<Vec<String>> =
        sqlx::query_scalar("SELECT threads FROM memories WHERE id=ANY($1)")
            .bind(&ids)
            .fetch_all(&pool)
            .await
            .unwrap();

    assert_eq!(event_count, 0);
    assert_eq!(arrays.len(), 2);
    assert!(arrays.iter().all(Vec::is_empty));

    drop_isolated_schema(pool, &schema).await;
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and ATHANOR_SUBSTRATE_TEST_SCHEMA"]
async fn members_without_a_predecessor_continue_by_date_before_id() {
    let (pool, cfg, schema) = isolated_database("order").await;
    let room = format!("thread-join-{}", Uuid::new_v4());
    let thread = "work / ordered integration";
    let mut ids = Vec::new();
    for title in ["written first", "written second", "written third"] {
        let memory = remember(&pool, &cfg, memory_request(&room, title, unthreaded()))
            .await
            .expect("unthreaded memory must commit");
        ids.push(memory.memory_id as i64);
    }
    // An import: the row written first carries the latest date.
    for (id, date) in ids.iter().zip(["2026-03-03", "2026-01-01", "2026-02-02"]) {
        sqlx::query("UPDATE memories SET date=$2::date WHERE id=$1")
            .bind(id)
            .bind(date)
            .execute(&pool)
            .await
            .unwrap();
    }
    let (latest, earliest, middle) = (ids[0], ids[1], ids[2]);

    let joined = memory_thread_join(
        &pool,
        MemoryThreadJoinParams {
            thread: thread.into(),
            memory_ids: ids.iter().map(ToString::to_string).collect(),
            room: room.clone(),
            operator: None,
        },
    )
    .await
    .expect("the dated memories must join");
    let receipt_links: Vec<(i64, Option<i64>)> = joined
        .memories
        .iter()
        .map(|memory| (memory.id, memory.linked_after))
        .collect();

    assert_eq!(
        receipt_links,
        vec![
            (latest, Some(middle)),
            (earliest, None),
            (middle, Some(earliest))
        ]
    );
    assert_eq!(
        links(&pool, &room, thread).await,
        vec![(middle, latest), (earliest, middle)]
    );

    let newest = remember(
        &pool,
        &cfg,
        memory_request(&room, "written today", in_thread(thread, vec![])),
    )
    .await
    .expect("a threaded memory with no continues must commit")
    .memory_id as i64;

    assert_eq!(
        links(&pool, &room, thread).await,
        vec![(middle, latest), (earliest, middle), (latest, newest)]
    );

    drop_isolated_schema(pool, &schema).await;
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and ATHANOR_SUBSTRATE_TEST_SCHEMA"]
async fn a_successor_inherits_the_threads_of_the_memory_it_supersedes() {
    let (pool, cfg, schema) = isolated_database("inherit").await;
    let room = format!("thread-join-{}", Uuid::new_v4());
    let thread = "work / inherited integration";
    let original = remember(
        &pool,
        &cfg,
        memory_request(&room, "original decision", in_thread(thread, vec![])),
    )
    .await
    .expect("the threaded original must commit")
    .memory_id;
    let correction = || {
        memory_request(
            &room,
            "corrected decision",
            RememberMemoryDetails {
                supersedes: vec![original],
                ..unthreaded()
            },
        )
    };
    let successor = remember(&pool, &cfg, correction())
        .await
        .expect("the correction must commit")
        .memory_id as i64;
    let retried = remember(&pool, &cfg, correction())
        .await
        .expect("the same correction must commit again")
        .memory_id as i64;
    let successor_threads: Vec<String> =
        sqlx::query_scalar("SELECT threads FROM memories WHERE id=$1")
            .bind(successor)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(retried, successor);
    assert_eq!(successor_threads, vec![thread.to_owned()]);
    assert_eq!(
        links(&pool, &room, thread).await,
        vec![(original as i64, successor)]
    );

    drop_isolated_schema(pool, &schema).await;
}

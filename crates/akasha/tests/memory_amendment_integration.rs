mod support;

use akasha::{AppError, Config, MemoryAmendParams, memory_amend, recall, remember};
use hearth::{
    MemoryAmendment, MemoryScope, OperatorKey, RecallProjection, RecallRequest,
    RememberMemoryDetails, RememberRequest, RoomKey,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{drop_isolated_schema, isolated_database};
use uuid::Uuid;

/// One memory through the real write path. The error is the House refusing
/// the write, which several proofs expect.
async fn write(
    pool: &PgPool,
    cfg: &Config,
    room: &str,
    writer: Option<(&str, MemoryScope)>,
    title: &str,
    amends: Vec<MemoryAmendment>,
) -> Result<i64, AppError> {
    let mut request = RememberRequest::new_memory(
        RoomKey::for_memory_write(room).unwrap(),
        title.into(),
        format!("The amendment proof records {title}."),
        RememberMemoryDetails {
            source_path: None,
            threads: vec![],
            continues: vec![],
            supersedes: vec![],
            backup: false,
        },
    )
    .unwrap();

    if let Some((operator, scope)) = writer {
        request = request
            .with_operator(Some(OperatorKey::new(operator).unwrap()), Some(scope))
            .unwrap();
    }
    let request = request.with_amends(amends).unwrap();

    remember(pool, cfg, request)
        .await
        .map(|receipt| receipt.memory_id as i64)
}

async fn set_date(pool: &PgPool, id: i64, date: &str) {
    sqlx::query("UPDATE memories SET date=$2::date WHERE id=$1")
        .bind(id)
        .bind(date)
        .execute(pool)
        .await
        .unwrap();
}

/// The fields that an amendment must never change.
async fn row_state(pool: &PgPool, id: i64) -> (String, String, Option<i64>, bool) {
    sqlx::query_as(
        "SELECT coalesce(title,''),body,superseded_by,archived_at IS NOT NULL
         FROM memories WHERE id=$1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn amendment_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM memory_amendments")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn recall_as(
    pool: &PgPool,
    cfg: &Config,
    room: &str,
    reader: Option<&str>,
    query: &str,
) -> Value {
    let request = RecallRequest::new(RoomKey::new(room).unwrap(), query.into(), 8, 0.4, 8, 0.3)
        .unwrap()
        .with_operator(reader.map(|key| OperatorKey::new(key).unwrap()))
        .with_projection(RecallProjection::Manual);

    serde_json::to_value(recall(pool, cfg, request, None).await.unwrap()).unwrap()
}

fn candidate(recalled: &Value, id: i64) -> Value {
    recalled["retrievalCandidates"]
        .as_array()
        .expect("recall must return candidates")
        .iter()
        .find(|candidate| candidate["memory_id"].as_i64() == Some(id))
        .unwrap_or_else(|| panic!("memory {id} must be a recall candidate"))
        .clone()
}

fn candidate_ids(recalled: &Value) -> Vec<i64> {
    recalled["retrievalCandidates"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|candidate| candidate["memory_id"].as_i64())
        .collect()
}

fn amend(
    room: &str,
    older: i64,
    newer: i64,
    changed: &str,
    keeps: Option<&str>,
    operator: Option<&str>,
) -> MemoryAmendParams {
    MemoryAmendParams {
        older_memory_id: older.to_string(),
        newer_memory_id: newer.to_string(),
        changed: changed.into(),
        keeps: keeps.map(str::to_owned),
        room: room.into(),
        operator: operator.map(str::to_owned),
    }
}

fn refusal<T>(result: Result<T, AppError>) -> String {
    match result {
        Err(AppError::Invalid(message)) => message,
        Err(other) => panic!("expected a refusal, found {other}"),
        Ok(_) => panic!("expected a refusal, found success"),
    }
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and ATHANOR_SUBSTRATE_TEST_SCHEMA"]
async fn an_amendment_shows_both_directions_and_never_changes_ranking() {
    let (pool, cfg, schema) = isolated_database("amend_recall").await;
    let room = format!("memory-amend-{}", Uuid::new_v4());
    let older = write(&pool, &cfg, &room, None, "quillmarch launch plan", vec![])
        .await
        .unwrap();
    let newer = write(&pool, &cfg, &room, None, "quillmarch launch update", vec![])
        .await
        .unwrap();
    set_date(&pool, older, "2026-01-05").await;
    set_date(&pool, newer, "2026-02-10").await;
    let older_row = row_state(&pool, older).await;
    let newer_row = row_state(&pool, newer).await;
    let before = recall_as(&pool, &cfg, &room, None, "quillmarch launch").await;
    let receipt = memory_amend(
        &pool,
        amend(
            &room,
            older,
            newer,
            "  The launch moved to March.  ",
            Some("The original budget."),
            None,
        ),
    )
    .await
    .expect("the newer memory must amend the older memory");

    assert_eq!(
        serde_json::to_value(&receipt).unwrap(),
        json!({
            "ok": true,
            "room": room,
            "older": {"id": older, "title": "quillmarch launch plan"},
            "newer": {"id": newer, "title": "quillmarch launch update"},
            "changed": "The launch moved to March.",
            "keeps": "The original budget.",
            "alreadyAmended": false,
        })
    );

    let after = recall_as(&pool, &cfg, &room, None, "quillmarch launch").await;

    assert_eq!(
        candidate_ids(&after),
        candidate_ids(&before),
        "an amendment never changes the order"
    );
    assert!(
        candidate(&before, older)["score"].is_number(),
        "the ranked lanes must score the older memory"
    );
    assert_eq!(
        candidate(&after, older)["score"],
        candidate(&before, older)["score"],
        "an amendment never lowers the older memory"
    );
    assert_eq!(
        candidate(&after, older)["amendedBy"],
        json!([{"memoryId": newer, "changed": "The launch moved to March."}])
    );
    assert_eq!(candidate(&after, older)["amends"], json!([]));
    assert_eq!(
        candidate(&after, newer)["amends"],
        json!([{
            "memoryId": older,
            "changed": "The launch moved to March.",
            "keeps": "The original budget.",
        }])
    );
    assert_eq!(candidate(&after, newer)["amendedBy"], json!([]));
    assert_eq!(row_state(&pool, older).await, older_row);
    assert_eq!(row_state(&pool, newer).await, newer_row);

    drop_isolated_schema(pool, &schema).await;
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and ATHANOR_SUBSTRATE_TEST_SCHEMA"]
async fn a_repeat_is_safe_and_other_text_date_order_or_a_bad_id_refuses() {
    let (pool, cfg, schema) = isolated_database("amend_rules").await;
    let room = format!("memory-amend-{}", Uuid::new_v4());
    let foreign_room = format!("memory-amend-{}", Uuid::new_v4());
    let older = write(&pool, &cfg, &room, None, "older owner note", vec![])
        .await
        .unwrap();
    let newer = write(&pool, &cfg, &room, None, "newer owner note", vec![])
        .await
        .unwrap();
    let same_day = write(&pool, &cfg, &room, None, "same day owner note", vec![])
        .await
        .unwrap();
    let foreign = write(
        &pool,
        &cfg,
        &foreign_room,
        None,
        "foreign owner note",
        vec![],
    )
    .await
    .unwrap();
    set_date(&pool, older, "2026-01-05").await;
    set_date(&pool, newer, "2026-02-10").await;
    set_date(&pool, same_day, "2026-02-10").await;
    let first = memory_amend(
        &pool,
        amend(&room, older, newer, "The owner changed.", None, None),
    )
    .await
    .expect("the first amendment must commit");
    let repeated = memory_amend(
        &pool,
        amend(&room, older, newer, "The owner changed.", Some("  "), None),
    )
    .await
    .expect("a repeated amendment must succeed");

    assert!(!first.already_amended);
    assert!(repeated.already_amended);
    assert_eq!(amendment_count(&pool).await, 1);

    let other_text = memory_amend(
        &pool,
        amend(&room, older, newer, "The owner left.", None, None),
    )
    .await;
    let other_keeps = memory_amend(
        &pool,
        amend(
            &room,
            older,
            newer,
            "The owner changed.",
            Some("The office."),
            None,
        ),
    )
    .await;

    assert!(refusal(other_text).contains("other text"));
    assert!(refusal(other_keeps).contains("other text"));

    let stored: (String, Option<String>) = sqlx::query_as(
        "SELECT changed,keeps FROM memory_amendments WHERE older_id=$1 AND newer_id=$2",
    )
    .bind(older)
    .bind(newer)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(stored, ("The owner changed.".into(), None));

    let backwards = memory_amend(
        &pool,
        amend(&room, newer, older, "The owner moved back.", None, None),
    )
    .await;
    let message = refusal(backwards);

    assert!(
        message.contains("2026-01-05 is earlier than 2026-02-10"),
        "{message}"
    );
    memory_amend(
        &pool,
        amend(&room, newer, same_day, "The same day changed.", None, None),
    )
    .await
    .expect("a newer memory with the same date can amend");
    let reverse = memory_amend(
        &pool,
        amend(&room, same_day, newer, "The loop.", None, None),
    )
    .await;

    assert!(refusal(reverse).contains("loop"));
    assert!(
        refusal(memory_amend(&pool, amend(&room, older, older, "Itself.", None, None)).await)
            .contains("itself")
    );

    let foreign_refusal = refusal(
        memory_amend(
            &pool,
            amend(&room, older, foreign, "Across rooms.", None, None),
        )
        .await,
    );

    assert!(foreign_refusal.contains("unavailable"));
    assert!(foreign_refusal.contains(&foreign.to_string()));
    assert!(
        refusal(memory_amend(&pool, amend(&room, older, same_day, "   ", None, None)).await)
            .contains("changed")
    );
    assert_eq!(amendment_count(&pool).await, 2);

    drop_isolated_schema(pool, &schema).await;
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and ATHANOR_SUBSTRATE_TEST_SCHEMA"]
async fn an_unreadable_newer_memory_stays_hidden() {
    let (pool, cfg, schema) = isolated_database("amend_scope").await;
    let room = format!("memory-amend-{}", Uuid::new_v4());
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
    let older = write(
        &pool,
        &cfg,
        &room,
        Some(("staff", MemoryScope::Company)),
        "harbor plan",
        vec![],
    )
    .await
    .unwrap();
    let hidden = write(
        &pool,
        &cfg,
        &room,
        Some(("staff", MemoryScope::Private)),
        "private harbor update",
        vec![],
    )
    .await
    .unwrap();
    set_date(&pool, older, "2026-01-05").await;
    set_date(&pool, hidden, "2026-02-10").await;
    memory_amend(
        &pool,
        amend(
            &room,
            older,
            hidden,
            "The private update.",
            None,
            Some("staff"),
        ),
    )
    .await
    .expect("staff can amend with its own private memory");
    let reference = format!("memory {older}");
    let as_peer = recall_as(&pool, &cfg, &room, Some("peer"), &reference).await;
    let as_staff = recall_as(&pool, &cfg, &room, Some("staff"), &reference).await;

    assert_eq!(
        candidate(&as_peer, older)["amendedBy"],
        json!([]),
        "a reader never sees the ID of an amendment it cannot read"
    );
    assert_eq!(
        candidate(&as_staff, older)["amendedBy"],
        json!([{"memoryId": hidden, "changed": "The private update."}])
    );

    let peer_refusal = refusal(
        memory_amend(
            &pool,
            amend(&room, older, hidden, "Peer guess.", None, Some("peer")),
        )
        .await,
    );

    assert!(peer_refusal.contains(&hidden.to_string()));
    assert_eq!(amendment_count(&pool).await, 1);

    drop_isolated_schema(pool, &schema).await;
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and ATHANOR_SUBSTRATE_TEST_SCHEMA"]
async fn remember_with_amends_records_the_amendment_in_the_same_write() {
    let (pool, cfg, schema) = isolated_database("amend_remember").await;
    let room = format!("memory-amend-{}", Uuid::new_v4());
    let older = write(&pool, &cfg, &room, None, "tide plan", vec![])
        .await
        .unwrap();
    set_date(&pool, older, "2000-01-05").await;
    let amends = || {
        vec![
            MemoryAmendment::new(
                older as u64,
                "The tide table changed.",
                Some("The harbor map."),
            )
            .unwrap(),
        ]
    };
    let newer = write(&pool, &cfg, &room, None, "tide update", amends())
        .await
        .expect("remember must record the amendment");
    let stored: (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT changed,keeps,created_by FROM memory_amendments
         WHERE older_id=$1 AND newer_id=$2",
    )
    .bind(older)
    .bind(newer)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        stored,
        (
            "The tide table changed.".into(),
            Some("The harbor map.".into()),
            None
        )
    );

    let recalled = recall_as(
        &pool,
        &cfg,
        &room,
        None,
        &format!("memories {older}, {newer}"),
    )
    .await;

    assert_eq!(
        candidate(&recalled, older)["amendedBy"],
        json!([{"memoryId": newer, "changed": "The tide table changed."}])
    );
    assert_eq!(
        candidate(&recalled, newer)["amends"],
        json!([{"memoryId": older, "changed": "The tide table changed.", "keeps": "The harbor map."}])
    );

    let retried = write(&pool, &cfg, &room, None, "tide update", amends())
        .await
        .expect("a retried write with the same amendment must succeed");

    assert_eq!(retried, newer);
    assert_eq!(amendment_count(&pool).await, 1);

    let later = write(&pool, &cfg, &room, None, "tide forecast", vec![])
        .await
        .unwrap();
    set_date(&pool, later, "2999-01-05").await;
    let refused = write(
        &pool,
        &cfg,
        &room,
        None,
        "tide late note",
        vec![MemoryAmendment::new(later as u64, "The forecast changed.", None).unwrap()],
    )
    .await;
    let message = refusal(refused);
    let written: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memories WHERE title='tide late note'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(message.contains("is earlier than 2999-01-05"), "{message}");
    assert_eq!(written, 0, "a refused amendment refuses the whole write");
    assert_eq!(amendment_count(&pool).await, 1);

    drop_isolated_schema(pool, &schema).await;
}

//! PostgreSQL proof for the Pulse panel timeline reads (guild-hall #183/#185).
//!
//! Runs the exact parameterized statements through the driver with the exact
//! Rust objects the panel binds (coding lesson #251: a psql literal proves
//! nothing about parameter encoding). Tables are session-temporary shadows,
//! so the shared test database keeps no residue.

use athanor_substrate::{
    LessonTimelineParams, MemoryReadParams, MemoryTimelineParams, lesson_timeline, memory_read,
    memory_timeline,
};
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn isolated_database_url() -> String {
    let url = std::env::var("SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL")
        .expect("dedicated test database URL must be configured when this proof is run");
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory"),
        "refusing the live/default database"
    );
    assert!(
        !lower.contains("solarisael-house"),
        "refusing a production-looking database"
    );
    url
}

async fn temp_timeline_pool() -> TestResult<PgPool> {
    let options = PgConnectOptions::from_str(&isolated_database_url())?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                sqlx::query("SELECT set_config('search_path', 'pg_temp', false)")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await?;
    sqlx::query(
        "CREATE TEMP TABLE memories (
            id BIGINT NOT NULL, room TEXT NOT NULL, type TEXT NOT NULL,
            date DATE, title TEXT, source_path TEXT NOT NULL, body TEXT NOT NULL,
            threads TEXT[] NOT NULL DEFAULT '{}', superseded_by BIGINT,
            archived_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL
        )",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "CREATE TEMP TABLE lessons (
            lesson_key TEXT NOT NULL, kind_path TEXT NOT NULL,
            id BIGINT NOT NULL, title TEXT NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL
        )",
    )
    .execute(&pool)
    .await?;
    Ok(pool)
}

#[allow(clippy::too_many_arguments)]
async fn insert_memory(
    pool: &PgPool,
    id: i64,
    room: &str,
    kind: &str,
    title: Option<&str>,
    body: &str,
    superseded_by: Option<i64>,
    archived_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
) -> TestResult {
    sqlx::query(
        "INSERT INTO memories
           (id,room,type,title,source_path,body,threads,superseded_by,archived_at,created_at,updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10)",
    )
    .bind(id)
    .bind(room)
    .bind(kind)
    .bind(title)
    .bind(format!("proof/memory-{id}.md"))
    .bind(body)
    .bind(vec!["proof thread".to_owned()])
    .bind(superseded_by)
    .bind(archived_at)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(())
}

fn timeline_params(value: serde_json::Value) -> MemoryTimelineParams {
    serde_json::from_value(value).expect("panel-shaped request must deserialize")
}

// red-proof: drop any one WHERE arm (archived, superseded, paper-boat, room
// filter, cursor) or flip the ORDER BY and an assertion below goes red.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; tables are session-temporary"]
async fn memory_timeline_scrolls_active_rows_newest_first_with_keyset() -> TestResult {
    let pool = temp_timeline_pool().await?;
    let base = Utc::now() - Duration::hours(10);
    let at = |hours: i64| base + Duration::hours(hours);

    insert_memory(
        &pool,
        1,
        "room-a",
        "conversa",
        Some("oldest"),
        "b1",
        None,
        None,
        at(1),
    )
    .await?;
    insert_memory(
        &pool,
        2,
        "room-a",
        "conversa",
        None,
        "b2",
        None,
        None,
        at(2),
    )
    .await?;
    insert_memory(
        &pool,
        3,
        "room-b",
        "projeto",
        Some("other room"),
        "b3",
        None,
        None,
        at(3),
    )
    .await?;
    // Excluded rows: a paper boat, a superseded record, an archived record.
    insert_memory(
        &pool,
        4,
        "room-a",
        origami::boats::MEMORY_KIND,
        Some("boat"),
        "b4",
        None,
        None,
        at(4),
    )
    .await?;
    insert_memory(
        &pool,
        5,
        "room-a",
        "conversa",
        Some("superseded"),
        "b5",
        Some(2),
        None,
        at(5),
    )
    .await?;
    insert_memory(
        &pool,
        6,
        "room-a",
        "conversa",
        Some("archived"),
        "b6",
        None,
        Some(at(5)),
        at(6),
    )
    .await?;
    // The excerpt fence: 600 chars of body, 500 in the row.
    insert_memory(
        &pool,
        7,
        "room-a",
        "conversa",
        Some("long"),
        &"x".repeat(600),
        None,
        None,
        at(7),
    )
    .await?;

    let all = memory_timeline(&pool, timeline_params(json!({}))).await?;
    assert!(all.ok);
    let ids: Vec<i64> = all.memories.iter().map(|m| m.id).collect();
    assert_eq!(ids, vec![7, 3, 2, 1], "newest first, exclusions applied");
    assert_eq!(all.memories[0].excerpt.chars().count(), 500);
    assert_eq!(all.memories[2].title, "untitled");

    let one_room = memory_timeline(
        &pool,
        timeline_params(json!({ "room": "room-b", "limit": 10 })),
    )
    .await?;
    assert_eq!(
        one_room.memories.iter().map(|m| m.id).collect::<Vec<_>>(),
        vec![3]
    );

    // Keyset cursor: exact chrono object through the driver, never a literal.
    let newest = &all.memories[0];
    let older = memory_timeline(
        &pool,
        timeline_params(json!({
            "before": { "createdAt": newest.created_at.to_rfc3339(), "id": newest.id }
        })),
    )
    .await?;
    assert_eq!(
        older.memories.iter().map(|m| m.id).collect::<Vec<_>>(),
        vec![3, 2, 1],
        "the cursor page starts strictly after the cursor row"
    );

    for refused in [
        json!({ "limit": 0 }),
        json!({ "limit": 51 }),
        json!({ "room": "Bad Room" }),
    ] {
        let params = timeline_params(refused.clone());
        assert!(
            params.validate().is_err(),
            "{refused} must refuse before any SQL runs"
        );
    }
    Ok(())
}

// red-proof: return the excerpt instead of the body, hide superseded_by, or
// invent a row for an unknown id and this proof goes red.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; tables are session-temporary"]
async fn memory_read_returns_history_with_authority_visible() -> TestResult {
    let pool = temp_timeline_pool().await?;
    let base = Utc::now() - Duration::hours(2);
    insert_memory(
        &pool,
        10,
        "room-a",
        "conversa",
        Some("current"),
        &"y".repeat(700),
        None,
        None,
        base,
    )
    .await?;
    insert_memory(
        &pool,
        11,
        "room-a",
        "conversa",
        Some("old truth"),
        "corrected later",
        Some(10),
        None,
        base,
    )
    .await?;

    let current = memory_read(&pool, MemoryReadParams { id: 10 }).await?;
    assert!(current.ok);
    assert_eq!(
        current.memory.body.len(),
        700,
        "full body, never the excerpt"
    );
    assert_eq!(current.memory.threads, vec!["proof thread".to_owned()]);
    assert_eq!(current.memory.superseded_by, None);

    let history = memory_read(&pool, MemoryReadParams { id: 11 }).await?;
    assert_eq!(
        history.memory.superseded_by,
        Some(10),
        "a superseded memory reads as history with its authority visible"
    );

    let missing = memory_read(&pool, MemoryReadParams { id: 999_999 }).await;
    let refusal = missing.expect_err("an unknown id must refuse, not invent");
    assert!(
        refusal.to_string().contains("unknown_memory") || refusal.to_string().contains("no memory"),
        "typed refusal, got: {refusal}"
    );

    assert!(MemoryReadParams { id: 0 }.validate().is_err());
    Ok(())
}

// red-proof: order by created_at (lessons do not honestly carry one), drop
// the lesson_key filter, or accept an unknown family and this goes red.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; tables are session-temporary"]
async fn lesson_timeline_orders_by_updated_at_with_family_filter() -> TestResult {
    let pool = temp_timeline_pool().await?;
    let base = Utc::now() - Duration::hours(5);
    let insert = async |key: &str, id: i64, hours: i64| -> TestResult {
        sqlx::query(
            "INSERT INTO lessons (lesson_key,kind_path,id,title,updated_at)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(key)
        .bind(format!("{key}/general"))
        .bind(id)
        .bind(format!("{key} lesson {id}"))
        .bind(base + Duration::hours(hours))
        .execute(&pool)
        .await?;
        Ok(())
    };
    insert("coding", 1, 1).await?;
    insert("writing", 2, 2).await?;
    insert("coding", 3, 3).await?;

    let all: athanor_substrate::LessonTimelineResult =
        lesson_timeline(&pool, serde_json::from_value(json!({}))?).await?;
    assert!(all.ok);
    assert_eq!(
        all.lessons.iter().map(|l| l.id).collect::<Vec<_>>(),
        vec![3, 2, 1],
        "most recently updated first"
    );

    let coding =
        lesson_timeline(&pool, serde_json::from_value(json!({ "type": "coding" }))?).await?;
    assert_eq!(
        coding.lessons.iter().map(|l| l.id).collect::<Vec<_>>(),
        vec![3, 1]
    );
    assert!(coding.lessons[0].kind_path.starts_with("coding/"));

    let newest = &all.lessons[0];
    let older = lesson_timeline(
        &pool,
        serde_json::from_value(json!({
            "before": { "updatedAt": newest.updated_at.to_rfc3339(), "id": newest.id }
        }))?,
    )
    .await?;
    assert_eq!(
        older.lessons.iter().map(|l| l.id).collect::<Vec<_>>(),
        vec![2, 1]
    );

    let unknown: LessonTimelineParams = serde_json::from_value(json!({ "type": "alchemy" }))?;
    assert!(unknown.validate().is_err(), "unknown families refuse");
    Ok(())
}

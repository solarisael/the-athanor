//! PostgreSQL proof for the lesson_query routing filters.

use akasha::{LessonQueryParams, LessonQueryResult, lesson_query};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Row};
use std::str::FromStr;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn isolated_database_url() -> String {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
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

async fn temp_lesson_pool() -> TestResult<PgPool> {
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
        "CREATE TEMP TABLE lessons (
            lesson_key TEXT NOT NULL, kind_path TEXT NOT NULL DEFAULT 'coding/general',
            id BIGINT NOT NULL, scope TEXT NOT NULL DEFAULT 'house', project TEXT,
            voice TEXT, register TEXT[] NOT NULL DEFAULT '{}', shape TEXT,
            stage TEXT[] NOT NULL DEFAULT '{}', title TEXT NOT NULL, lesson TEXT NOT NULL,
            trigger_context TEXT, proof_pattern TEXT, example_text TEXT, example_cmd TEXT,
            writers TEXT[] NOT NULL DEFAULT '{}', tools TEXT[] NOT NULL DEFAULT '{}',
            negation_of BIGINT, language_keys TEXT[] NOT NULL DEFAULT '{}',
            technology_keys TEXT[] NOT NULL DEFAULT '{}', tags TEXT[] NOT NULL DEFAULT '{}',
            thread_keys TEXT[] NOT NULL DEFAULT '{}', always_on BOOLEAN NOT NULL DEFAULT FALSE,
            condition TEXT[] NOT NULL DEFAULT '{}', ast_condition TEXT[] NOT NULL DEFAULT '{}',
            trigger_scope TEXT[] NOT NULL DEFAULT '{}', interrupt_mode TEXT,
            repeat_cooldown_secs INTEGER, lesson_tsv TSVECTOR NOT NULL DEFAULT ''::tsvector,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), PRIMARY KEY (lesson_key, id)
        )",
    )
    .execute(&pool)
    .await?;
    // RoomSettings::load runs inside lesson_query since 0028; with search_path
    // locked to pg_temp the public table is invisible, so the fixture mirrors it.
    sqlx::query("CREATE TEMP TABLE room_settings (LIKE public.room_settings INCLUDING ALL)")
        .execute(&pool)
        .await?;
    Ok(pool)
}

async fn insert_lesson(pool: &PgPool, id: i64, always_on: bool) -> TestResult {
    sqlx::query(
        "INSERT INTO lessons (lesson_key,id,title,lesson,always_on) VALUES ('coding',$1,$2,$3,$4)",
    )
    .bind(id)
    .bind(format!("lesson {id}"))
    .bind(format!("body {id}"))
    .bind(always_on)
    .execute(pool)
    .await?;
    Ok(())
}
async fn insert_trigger_lesson(
    pool: &PgPool,
    id: i64,
    condition: &[&str],
    ast_condition: &[&str],
    tags: &[&str],
) -> TestResult {
    sqlx::query(
        "INSERT INTO lessons (lesson_key,id,title,lesson,condition,ast_condition,tags)
         VALUES ('coding',$1,$2,$3,$4,$5,$6)",
    )
    .bind(id)
    .bind(format!("lesson {id}"))
    .bind(format!("body {id}"))
    .bind(
        condition
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    )
    .bind(
        ast_condition
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    )
    .bind(
        tags.iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; the lessons table is session-temporary"]
async fn lesson_query_always_on_filter_returns_only_flagged_rows_and_absence_is_unchanged()
-> TestResult {
    // Kills: deleting the `AND always_on` predicate or applying it when absent.
    // red-proof: remove `if params.always_on { qb.push(\" AND always_on\") }`.
    let pool = temp_lesson_pool().await?;
    insert_lesson(&pool, 1, true).await?;
    insert_lesson(&pool, 2, false).await?;

    let flagged: LessonQueryParams = serde_json::from_value(serde_json::json!({
        "room": "kintsu", "type": "coding", "alwaysOn": true
    }))?;
    let flagged = lesson_query(&pool, flagged).await?;
    assert_eq!(
        flagged
            .lessons
            .iter()
            .map(|lesson| lesson.id)
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert!(flagged.filters.always_on);

    let unfiltered: LessonQueryParams = serde_json::from_value(serde_json::json!({
        "room": "kintsu", "type": "coding"
    }))?;
    let unfiltered = lesson_query(&pool, unfiltered).await?;
    assert_eq!(
        unfiltered
            .lessons
            .iter()
            .map(|lesson| lesson.id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(!unfiltered.filters.always_on);
    let count: i64 = sqlx::query("SELECT COUNT(*) AS count FROM lessons")
        .fetch_one(&pool)
        .await?
        .try_get("count")?;
    assert_eq!(count, 2);
    Ok(())
}
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; the lessons table is session-temporary"]
async fn lesson_query_trigger_only_returns_regex_and_ast_guards() -> TestResult {
    // Kills: dropping either trigger column from the routing predicate.
    let pool = temp_lesson_pool().await?;
    insert_lesson(&pool, 1, false).await?;
    insert_trigger_lesson(&pool, 2, &["unsafe"], &[], &["ttsr-approved"]).await?;
    insert_trigger_lesson(
        &pool,
        3,
        &[],
        &["try { $$$BODY } catch ($ERR) { }"],
        &["ttsr-approved"],
    )
    .await?;
    insert_trigger_lesson(&pool, 4, &["unapproved"], &[], &[]).await?;

    let routed: LessonQueryParams = serde_json::from_value(serde_json::json!({
        "room": "kodo", "type": "coding",
        "tag": "ttsr-approved", "triggerOnly": true
    }))?;
    let routed = lesson_query(&pool, routed).await?;
    assert_eq!(ids_of(&routed), vec![2, 3]);
    assert!(routed.filters.trigger_only);
    assert_eq!(routed.filters.tag.as_deref(), Some("ttsr-approved"));

    let bare: LessonQueryParams = serde_json::from_value(serde_json::json!({
        "room": "kodo", "type": "coding"
    }))?;
    assert_eq!(ids_of(&lesson_query(&pool, bare).await?), vec![1, 2, 3, 4]);
    Ok(())
}

async fn insert_keyed_lesson(pool: &PgPool, id: i64, technology: &[&str]) -> TestResult {
    sqlx::query(
        "INSERT INTO lessons (lesson_key,id,title,lesson,technology_keys) VALUES ('coding',$1,$2,$3,$4)",
    )
    .bind(id)
    .bind(format!("lesson {id}"))
    .bind(format!("body {id}"))
    .bind(technology.iter().map(|v| v.to_string()).collect::<Vec<_>>())
    .execute(pool)
    .await?;
    Ok(())
}

fn ids_of(result: &LessonQueryResult) -> Vec<i64> {
    let mut ids: Vec<i64> = result.lessons.iter().map(|lesson| lesson.id).collect();
    ids.sort_unstable();
    ids
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; the lessons table is session-temporary"]
async fn lesson_query_without_keys_sees_keyed_rows_and_keys_only_narrow() -> TestResult {
    // Kills: restoring `AND cardinality(technology_keys) = 0` for an empty key
    // list, or dropping the `cardinality = 0 OR &&` branch for a supplied list.
    let pool = temp_lesson_pool().await?;
    insert_lesson(&pool, 1, false).await?;
    insert_keyed_lesson(&pool, 2, &["react"]).await?;
    insert_keyed_lesson(&pool, 3, &["gdscript"]).await?;

    let bare: LessonQueryParams = serde_json::from_value(serde_json::json!({
        "room": "kodo", "type": "coding"
    }))?;
    assert_eq!(ids_of(&lesson_query(&pool, bare).await?), vec![1, 2, 3]);

    let react: LessonQueryParams = serde_json::from_value(serde_json::json!({
        "room": "kodo", "type": "coding", "technologyKeys": ["react"]
    }))?;
    assert_eq!(ids_of(&lesson_query(&pool, react).await?), vec![1, 2]);
    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; the lessons table is session-temporary"]
async fn lesson_query_ids_return_exactly_the_named_rows() -> TestResult {
    // Kills: dropping the `id = ANY` predicate, or letting eligibility keys
    // still gate a direct lookup.
    let pool = temp_lesson_pool().await?;
    insert_lesson(&pool, 1, false).await?;
    insert_keyed_lesson(&pool, 2, &["react"]).await?;
    insert_keyed_lesson(&pool, 3, &["gdscript"]).await?;

    let direct: LessonQueryParams = serde_json::from_value(serde_json::json!({
        "room": "kodo", "type": "coding", "ids": [3], "technologyKeys": ["react"]
    }))?;
    let result = lesson_query(&pool, direct).await?;
    assert_eq!(ids_of(&result), vec![3]);
    assert_eq!(result.filters.ids, vec![3]);
    Ok(())
}

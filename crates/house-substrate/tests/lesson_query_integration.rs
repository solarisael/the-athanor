//! PostgreSQL proof for the lesson_query alwaysOn wire filter.

use athanor_substrate::{LessonQueryParams, lesson_query};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Row};
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

#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; the lessons table is session-temporary"]
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

use athanor_substrate::{LessonMutationReceipt, LessonUpdateParams, lesson_update};
use serde_json::{Value, json};
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
            lesson_key TEXT NOT NULL,
            id BIGINT NOT NULL,
            title TEXT NOT NULL,
            project TEXT,
            always_on BOOLEAN NOT NULL DEFAULT FALSE,
            PRIMARY KEY (lesson_key, id)
        )",
    )
    .execute(&pool)
    .await?;
    Ok(pool)
}

async fn apply(
    pool: &PgPool,
    kind: &str,
    id: i64,
    expected_title: &str,
    patch: Value,
) -> TestResult<LessonMutationReceipt> {
    Ok(lesson_update(
        pool,
        LessonUpdateParams {
            kind: kind.into(),
            id,
            expected_title: expected_title.into(),
            patch,
        },
    )
    .await?)
}

async fn lesson_state(
    pool: &PgPool,
    lesson_key: &str,
    id: i64,
) -> TestResult<(String, bool, Option<String>)> {
    let row = sqlx::query(
        "SELECT title, always_on, project FROM lessons WHERE lesson_key=$1 AND id=$2",
    )
    .bind(lesson_key)
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok((
        row.try_get("title")?,
        row.try_get("always_on")?,
        row.try_get("project")?,
    ))
}

#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; the lessons table is session-temporary"]
async fn guarded_lesson_update_preserves_identity_and_refuses_without_mutation() -> TestResult {
    let pool = temp_lesson_pool().await?;
    sqlx::query(
        "INSERT INTO lessons (lesson_key,id,title,project,always_on) VALUES
         ('coding',41,'Guarded coding lesson','the-athanor',TRUE),
         ('writing',42,'Guarded writing lesson',NULL,FALSE)",
    )
    .execute(&pool)
    .await?;

    let demoted = apply(
        &pool,
        "coding-lesson",
        41,
        "Guarded coding lesson",
        json!({ "alwaysOn": false }),
    )
    .await?;
    assert!(demoted.ok);
    assert_eq!(demoted.id, 41);
    assert_eq!(demoted.title.as_deref(), Some("Guarded coding lesson"));
    assert_eq!(demoted.always_on, Some(false));
    assert_eq!(demoted.project, Some(Some("the-athanor".into())));
    assert_eq!(
        lesson_state(&pool, "coding", 41).await?,
        ("Guarded coding lesson".into(), false, Some("the-athanor".into()))
    );

    let cleared = apply(
        &pool,
        "coding-lesson",
        41,
        "Guarded coding lesson",
        json!({ "clearProject": true }),
    )
    .await?;
    assert!(cleared.ok);
    assert_eq!(cleared.id, 41);
    assert_eq!(cleared.always_on, Some(false));
    assert_eq!(cleared.project, Some(None));
    let cleared_json = serde_json::to_value(&cleared)?;
    assert_eq!(cleared_json["alwaysOn"], json!(false));
    assert_eq!(cleared_json["project"], Value::Null);
    assert_eq!(
        lesson_state(&pool, "coding", 41).await?,
        ("Guarded coding lesson".into(), false, None)
    );

    let stable_coding = lesson_state(&pool, "coding", 41).await?;
    let wrong_title = apply(
        &pool,
        "coding-lesson",
        41,
        "Wrong expected title",
        json!({ "alwaysOn": true }),
    )
    .await?;
    assert!(!wrong_title.ok);
    assert_eq!(wrong_title.error.as_deref(), Some("title mismatch"));
    assert_eq!(
        wrong_title.actual_title.as_deref(),
        Some("Guarded coding lesson")
    );
    assert_eq!(lesson_state(&pool, "coding", 41).await?, stable_coding);

    let conflicting_project = apply(
        &pool,
        "coding-lesson",
        41,
        "Guarded coding lesson",
        json!({ "project": "replacement", "clearProject": true }),
    )
    .await?;
    assert!(!conflicting_project.ok);
    assert_eq!(
        conflicting_project.error.as_deref(),
        Some("project and clearProject are mutually exclusive")
    );
    assert_eq!(lesson_state(&pool, "coding", 41).await?, stable_coding);

    let stable_writing = lesson_state(&pool, "writing", 42).await?;
    let wrong_kind = apply(
        &pool,
        "writing-lesson",
        42,
        "Guarded writing lesson",
        json!({ "clearProject": true }),
    )
    .await?;
    assert!(!wrong_kind.ok);
    assert_eq!(
        wrong_kind.error.as_deref(),
        Some("clearProject is not allowed for writing-lesson")
    );
    assert_eq!(lesson_state(&pool, "writing", 42).await?, stable_writing);

    let missing = apply(
        &pool,
        "coding-lesson",
        999,
        "Absent lesson",
        json!({ "alwaysOn": true }),
    )
    .await?;
    assert!(!missing.ok);
    assert_eq!(missing.error.as_deref(), Some("lesson not found"));
    assert_eq!(lesson_state(&pool, "coding", 41).await?, stable_coding);

    pool.close().await;
    Ok(())
}

use akasha::{
    LessonMutationKind, LessonMutationReceipt, LessonUpdateParams, lesson_update,
};
use serde_json::{Value, json};
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
            lesson_key TEXT NOT NULL,
            id BIGINT NOT NULL,
            title TEXT NOT NULL,
            project TEXT,
            always_on BOOLEAN NOT NULL DEFAULT FALSE,
            language_keys TEXT[] NOT NULL DEFAULT '{}',
            technology_keys TEXT[] NOT NULL DEFAULT '{}',
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
    let row =
        sqlx::query("SELECT title, always_on, project FROM lessons WHERE lesson_key=$1 AND id=$2")
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
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; the lessons table is session-temporary"]
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
    let LessonMutationReceipt::Updated {
        id,
        title,
        always_on,
        project,
        ..
    } = &demoted
    else {
        panic!("successful update must return an updated receipt");
    };
    assert_eq!(*id, 41);
    assert_eq!(title, "Guarded coding lesson");
    assert!(!*always_on);
    assert_eq!(project.as_deref(), Some("the-athanor"));
    assert_eq!(
        lesson_state(&pool, "coding", 41).await?,
        (
            "Guarded coding lesson".into(),
            false,
            Some("the-athanor".into())
        )
    );

    let cleared = apply(
        &pool,
        "coding-lesson",
        41,
        "Guarded coding lesson",
        json!({ "clearProject": true }),
    )
    .await?;
    let LessonMutationReceipt::Updated {
        id,
        always_on,
        project,
        ..
    } = &cleared
    else {
        panic!("successful update must return an updated receipt");
    };
    assert_eq!(*id, 41);
    assert!(!*always_on);
    assert!(project.is_none());
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
    let LessonMutationReceipt::Refused {
        mutation,
        error,
        actual_title,
        ..
    } = &wrong_title
    else {
        panic!("stale expected title must refuse");
    };
    assert_eq!(*mutation, LessonMutationKind::Update);
    assert_eq!(error, "title mismatch");
    assert_eq!(actual_title.as_deref(), Some("Guarded coding lesson"));
    assert_eq!(lesson_state(&pool, "coding", 41).await?, stable_coding);

    let conflicting_project = apply(
        &pool,
        "coding-lesson",
        41,
        "Guarded coding lesson",
        json!({ "project": "replacement", "clearProject": true }),
    )
    .await?;
    let LessonMutationReceipt::Refused {
        mutation, error, ..
    } = &conflicting_project
    else {
        panic!("invalid patch must refuse");
    };
    assert_eq!(*mutation, LessonMutationKind::Update);
    assert_eq!(error, "project and clearProject are mutually exclusive");
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
    let LessonMutationReceipt::Refused {
        mutation, error, ..
    } = &wrong_kind
    else {
        panic!("invalid patch must refuse");
    };
    assert_eq!(*mutation, LessonMutationKind::Update);
    assert_eq!(error, "clearProject is not allowed for writing-lesson");
    assert_eq!(lesson_state(&pool, "writing", 42).await?, stable_writing);

    let missing = apply(
        &pool,
        "coding-lesson",
        999,
        "Absent lesson",
        json!({ "alwaysOn": true }),
    )
    .await?;
    let LessonMutationReceipt::Refused {
        mutation, error, ..
    } = &missing
    else {
        panic!("missing lesson must refuse");
    };
    assert_eq!(*mutation, LessonMutationKind::Update);
    assert_eq!(error, "lesson not found");
    assert_eq!(lesson_state(&pool, "coding", 41).await?, stable_coding);

    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; the lessons table is session-temporary"]
async fn writing_and_design_lessons_accept_routing_eligibility_fields() -> TestResult {
    let pool = temp_lesson_pool().await?;
    sqlx::query(
        "INSERT INTO lessons (lesson_key,id,title) VALUES
         ('writing',51,'Figma flow lesson'),
         ('design',52,'Shelf design lesson')",
    )
    .execute(&pool)
    .await?;

    let routed_writing = apply(
        &pool,
        "writing-lesson",
        51,
        "Figma flow lesson",
        json!({ "alwaysOn": true, "technologyKeys": ["figma"], "languageKeys": [] }),
    )
    .await?;
    let LessonMutationReceipt::Updated { always_on, .. } = &routed_writing else {
        panic!("writing routing fields must update: {routed_writing:?}");
    };
    assert!(*always_on);
    let row = sqlx::query(
        "SELECT always_on, language_keys, technology_keys FROM lessons
         WHERE lesson_key='writing' AND id=51",
    )
    .fetch_one(&pool)
    .await?;
    assert!(row.try_get::<bool, _>("always_on")?);
    assert_eq!(row.try_get::<Vec<String>, _>("language_keys")?, Vec::<String>::new());
    assert_eq!(
        row.try_get::<Vec<String>, _>("technology_keys")?,
        vec!["figma".to_string()]
    );

    let shelved_design = apply(
        &pool,
        "design-lesson",
        52,
        "Shelf design lesson",
        json!({ "alwaysOn": true }),
    )
    .await?;
    let LessonMutationReceipt::Updated { always_on, .. } = &shelved_design else {
        panic!("design alwaysOn must update: {shelved_design:?}");
    };
    assert!(*always_on);

    // The gate itself stays alive: a coding-only field still refuses for writing,
    // and the refusal leaves the routed row untouched.
    let still_gated = apply(
        &pool,
        "writing-lesson",
        51,
        "Figma flow lesson",
        json!({ "scope": "tool:edit" }),
    )
    .await?;
    let LessonMutationReceipt::Refused { error, .. } = &still_gated else {
        panic!("cross-store field must still refuse: {still_gated:?}");
    };
    assert_eq!(error, "field not allowed for writing-lesson: scope");
    let row = sqlx::query(
        "SELECT always_on, technology_keys FROM lessons WHERE lesson_key='writing' AND id=51",
    )
    .fetch_one(&pool)
    .await?;
    assert!(row.try_get::<bool, _>("always_on")?);
    assert_eq!(
        row.try_get::<Vec<String>, _>("technology_keys")?,
        vec!["figma".to_string()]
    );

    pool.close().await;
    Ok(())
}

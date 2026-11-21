use akasha::migrations::run_migrations;
use akasha::spirit_texts::{
    SpiritTextError, SpiritTextKind, append_spirit_text, current_spirit_text,
};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::ops::AsyncFnOnce;
use std::str::FromStr;

use SpiritTextKind::{Identity, Presence};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// One connection keeps the search path inside the isolated schema.
async fn empty_schema() -> TestResult<(PgPool, String)> {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")?;
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
        "refusing a live or production-looking database"
    );
    let schema = format!("athanor_spirit_texts_{}", uuid::Uuid::new_v4().simple());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(PgConnectOptions::from_str(&url)?)
        .await?;

    for statement in [
        format!("CREATE SCHEMA {schema}"),
        format!("SET search_path TO {schema}, public"),
    ] {
        sqlx::query(&statement).execute(&pool).await?;
    }

    Ok((pool, schema))
}

async fn drop_schema(pool: PgPool, schema: &str) -> TestResult {
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await?;
    pool.close().await;

    Ok(())
}

async fn in_schema<T>(observe: impl AsyncFnOnce(&PgPool) -> TestResult<T>) -> TestResult<T> {
    let (pool, schema) = empty_schema().await?;
    let result = async {
        run_migrations(&pool).await?;
        observe(&pool).await
    }
    .await;
    drop_schema(pool, &schema).await?;

    result
}

async fn count(pool: &PgPool) -> TestResult<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM spirit_texts")
        .fetch_one(pool)
        .await?)
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn first_append_returns_current_with_body_digest() -> TestResult {
    let (added, current) = in_schema(async |pool| {
        let added =
            append_spirit_text(pool, "den", "Drake", Identity, "dragon\n", "Ash", 0).await?;
        let current = current_spirit_text(pool, "den", "Drake", Identity).await?;

        Ok((added, current))
    })
    .await?;

    assert_eq!(added.revision, 1);
    assert_eq!(
        added.sha256,
        "24e11938b9091b4dbc66a5e5b4705834e5e738fa85a80cd5f8844d976026b49e"
    );
    assert_eq!(current, Some(added));
    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn changed_text_preserves_both_revisions() -> TestResult {
    let (added, rows, current) = in_schema(async |pool| {
        append_spirit_text(pool, "den", "Drake", Identity, "egg", "Ash", 0).await?;
        let added = append_spirit_text(pool, "den", "Drake", Identity, "wing", "Ash", 1).await?;
        let rows: Vec<(i32, String)> =
            sqlx::query_as("SELECT revision, body FROM spirit_texts ORDER BY revision")
                .fetch_all(pool)
                .await?;
        let current = current_spirit_text(pool, "den", "Drake", Identity).await?;

        Ok((added, rows, current))
    })
    .await?;

    assert_eq!(added.revision, 2);
    assert_eq!(rows, [(1, "egg".into()), (2, "wing".into())]);
    assert_eq!(current, Some(added));
    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn stale_append_reports_current_without_a_write() -> TestResult {
    let (before, stale, after) = in_schema(async |pool| {
        append_spirit_text(pool, "den", "Drake", Identity, "egg", "Ash", 0).await?;
        let before = count(pool).await?;
        let stale = append_spirit_text(pool, "den", "Drake", Identity, "wing", "Ash", 0).await;

        Ok((before, stale, count(pool).await?))
    })
    .await?;

    assert!(matches!(
        stale,
        Err(SpiritTextError::StaleRevision { current: 1 })
    ));
    assert_eq!(after, before);
    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn identical_append_returns_the_original_row() -> TestResult {
    let (first, before, second, after) = in_schema(async |pool| {
        let first = append_spirit_text(pool, "den", "Drake", Identity, "egg", "Ash", 0).await?;
        let before = count(pool).await?;
        let second = append_spirit_text(pool, "den", "Drake", Identity, "egg", "Elm", 1).await?;

        Ok((first, before, second, count(pool).await?))
    })
    .await?;

    assert_eq!(second, first);
    assert_eq!(after, before);
    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn keys_have_independent_current_texts() -> TestResult {
    let keys = [
        ("den", "Ash", Identity),
        ("den", "Elm", Presence),
        ("nest", "Ash", Presence),
    ];
    let pairs = in_schema(async |pool| {
        let mut added = Vec::new();
        for (room, spirit, kind) in keys {
            let body = format!("{room}/{spirit}/{}", kind.as_str());
            added.push(append_spirit_text(pool, room, spirit, kind, &body, "Oak", 0).await?);
        }

        let mut current = Vec::new();
        for (room, spirit, kind) in keys {
            current.push(current_spirit_text(pool, room, spirit, kind).await?);
        }

        Ok((added, current))
    })
    .await?;

    for (added, current) in pairs.0.into_iter().zip(pairs.1) {
        assert_eq!(added.revision, 1);
        assert_eq!(current, Some(added));
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn body_bytes_round_trip() -> TestResult {
    let bodies = ["dragon\n", "dragon", r"é ' \\ $$ 🐉"];
    let actual = in_schema(async |pool| {
        let mut actual = Vec::new();
        for (revision, body) in bodies.into_iter().enumerate() {
            append_spirit_text(pool, "den", "Drake", Identity, body, "Ash", revision as i32)
                .await?;
            actual.push(
                current_spirit_text(pool, "den", "Drake", Identity)
                    .await?
                    .map(|row| row.body),
            );
        }

        Ok(actual)
    })
    .await?;

    for (actual, expected) in actual.iter().zip(bodies) {
        assert_eq!(actual.as_deref(), Some(expected));
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn invalid_input_writes_nothing() -> TestResult {
    let (empty, room, nul, rows) = in_schema(async |pool| {
        let empty = append_spirit_text(pool, "den", "Drake", Identity, "", "Ash", 0).await;
        let room = append_spirit_text(pool, "Bad Room", "Drake", Identity, "egg", "Ash", 0).await;
        let nul = append_spirit_text(pool, "den", "Drake", Identity, "dragon\0", "Ash", 0).await;

        Ok((empty, room, nul, count(pool).await?))
    })
    .await?;

    assert!(matches!(empty, Err(SpiritTextError::Invalid(_))));
    assert!(matches!(room, Err(SpiritTextError::Invalid(_))));
    assert!(matches!(
        nul,
        Err(SpiritTextError::Invalid("body contains U+0000"))
    ));
    assert_eq!(rows, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn migration_is_reapplicable() -> TestResult {
    in_schema(async |pool| {
        // The runner skips registered migrations. Exercise the SQL guard too.
        sqlx::raw_sql(include_str!(
            "../../../substrate/migrations/0050_spirit_texts.sql"
        ))
        .execute(pool)
        .await?;
        run_migrations(pool).await?;

        Ok(())
    })
    .await
}

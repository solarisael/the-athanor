use akasha::migrations::{
    STARTER_HOUSE_SPELLBOOK, run_migrations, run_migrations_with_starter_pages,
};
use serde_json::{Value, json};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::str::FromStr;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// One empty schema of its own on the dedicated test database. One
/// connection keeps the search path.
async fn empty_schema() -> TestResult<(PgPool, String)> {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")?;
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
        "refusing a live or production-looking database"
    );
    let schema = format!("athanor_spellbook_seed_{}", uuid::Uuid::new_v4().simple());
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

async fn house_row(pool: &PgPool) -> TestResult<Option<(String, String)>> {
    Ok(sqlx::query_as(
        "SELECT page::text, updated_at::text FROM spellbook_pages WHERE scope = 'house'",
    )
    .fetch_optional(pool)
    .await?)
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn a_fresh_migration_run_seeds_the_starter_house_page() -> TestResult {
    let (pool, schema) = empty_schema().await?;
    // Observe inside, assert after the drop: a failed assertion must not leak the schema.
    let observed: TestResult<Option<Value>> = async {
        run_migrations_with_starter_pages(&pool).await?;

        let page: Option<sqlx::types::Json<Value>> =
            sqlx::query_scalar("SELECT page FROM spellbook_pages WHERE scope = 'house'")
                .fetch_optional(&pool)
                .await?;
        Ok(page.map(|page| page.0))
    }
    .await;
    drop_schema(pool, &schema).await?;

    let starter: Value = serde_json::from_str(STARTER_HOUSE_SPELLBOOK)?;
    assert_eq!(observed?, Some(starter), "house row is the starter page");
    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn a_rerun_leaves_an_existing_house_page_untouched() -> TestResult {
    let (pool, schema) = empty_schema().await?;
    let observed: TestResult<(Option<(String, String)>, Option<(String, String)>)> = async {
        run_migrations(&pool).await?;
        sqlx::query("INSERT INTO spellbook_pages (scope, page) VALUES ('house', $1)")
            .bind(sqlx::types::Json(json!({"familiars": []})))
            .execute(&pool)
            .await?;
        let before = house_row(&pool).await?;

        run_migrations_with_starter_pages(&pool).await?;
        run_migrations_with_starter_pages(&pool).await?;
        Ok((before, house_row(&pool).await?))
    }
    .await;
    drop_schema(pool, &schema).await?;

    let (before, after) = observed?;
    assert!(before.is_some(), "the operator's house row exists");
    assert_eq!(after, before, "house row is byte-identical");
    Ok(())
}

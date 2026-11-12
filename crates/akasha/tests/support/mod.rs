use akasha::migrations::run_migrations;
use akasha::{Config, EmbeddingMode};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::str::FromStr;

/// A migrated schema of its own, because the shared test databases can lag
/// behind the canonical migrations. One connection keeps the search path.
pub async fn isolated_database(suffix: &str) -> (PgPool, Config, String) {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("dedicated test database URL must be configured when this proof is run");
    let lower = url.to_ascii_lowercase();
    let base = std::env::var("ATHANOR_SUBSTRATE_TEST_SCHEMA")
        .expect("this proof requires ATHANOR_SUBSTRATE_TEST_SCHEMA");
    let schema = format!("{base}_{suffix}");

    assert!(
        !lower.contains("solarisael_memory"),
        "refusing the live/default database"
    );
    assert!(
        !lower.contains("solarisael-house"),
        "refusing a production-looking database"
    );
    assert!(base.starts_with("solarisael_tuner_test_"));
    assert!(
        schema
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    );

    let options = PgConnectOptions::from_str(&url).expect("dedicated test URL must be valid");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("isolated database must be reachable");
    for statement in [
        format!("DROP SCHEMA IF EXISTS {schema} CASCADE"),
        format!("CREATE SCHEMA {schema}"),
        format!("SET search_path TO {schema}, public"),
    ] {
        sqlx::query(&statement)
            .execute(&pool)
            .await
            .expect("isolated schema must be created");
    }
    run_migrations(&pool)
        .await
        .expect("canonical migrations must apply to the isolated schema");
    let cfg = Config {
        database_url: url,
        embed_url: None,
        embed_model: "disabled".into(),
        embed_dimension: 2048,
        embedding_mode: EmbeddingMode::DisabledForTest,
        giga_source_ledger_dir: None,
        giga_source_room: None,
        house_tz: "America/Sao_Paulo".into(),
        nats_url: None,
    };

    (pool, cfg, schema)
}

pub async fn drop_isolated_schema(pool: PgPool, schema: &str) {
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await
        .expect("isolated schema must be dropped");
    pool.close().await;
}

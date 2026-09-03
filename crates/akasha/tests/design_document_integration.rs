use akasha::{DesignDocumentWriteParams, design_document_write};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::str::FromStr;

macro_rules! migration {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../substrate/migrations/",
            $name
        ))
    };
}

const MIGRATIONS: &[&str] = &[
    migration!("0001_initial.sql"),
    migration!("0002_nemotron_2048.sql"),
    migration!("0003_giga.sql"),
    migration!("0004_giga_runtime.sql"),
    migration!("0005_giga_resonance.sql"),
    migration!("0006_memory_thread_graph.sql"),
    migration!("0007_giga_source_ordinal.sql"),
    migration!("0008_unified_lessons.sql"),
    migration!("0009_bm25f_memory_search.sql"),
    migration!("0010_semantic_vocabulary.sql"),
    migration!("0011_design_lessons.sql"),
    migration!("0012_design_documents.sql"),
];

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

async fn isolated_schema() -> TestResult<(String, PgPool)> {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("dedicated test database URL must be configured when this proof is run");
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory"),
        "refusing the live/default database"
    );
    let schema = std::env::var("ATHANOR_SUBSTRATE_TEST_SCHEMA")
        .expect("design proof requires ATHANOR_SUBSTRATE_TEST_SCHEMA");
    assert!(schema.starts_with("solarisael_tuner_test_"));
    assert!(
        schema
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    );
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(PgConnectOptions::from_str(&url)?)
        .await?;
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&pool)
        .await?;
    sqlx::query(&format!("SET search_path TO {schema}, public"))
        .execute(&pool)
        .await?;
    for migration in MIGRATIONS {
        sqlx::raw_sql(migration).execute(&pool).await?;
    }
    Ok((schema, pool))
}

fn token(body: &str, supersedes: Option<i64>) -> DesignDocumentWriteParams {
    serde_json::from_value(serde_json::json!({
        "system": "design-proof",
        "docType": "token",
        "name": "reliquary-palette",
        "group": "palette",
        "values": {"--fg-gold": "oklch(73.5% 0.075 80)"},
        "body": body,
        "provenance": {"test": "design_document_integration"},
        "tags": ["palette"],
        "supersedes": supersedes,
    }))
    .unwrap()
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn same_identity_supersession_keeps_one_current_row_and_full_history() -> TestResult {
    let (schema, pool) = isolated_schema().await?;

    let first = design_document_write(&pool, token("first assertion", None)).await?;
    assert!(first.ok, "{:?}", first.error);
    let first_id = first.id.expect("first write returns an id");

    // Same system, doc_type, and name. Before the ordering fix this insert collided
    // with design_documents_current_identity_uidx while the old row was still current.
    let second = design_document_write(&pool, token("corrected assertion", Some(first_id))).await?;
    assert!(
        second.ok,
        "same-identity supersession refused: {:?}",
        second.error
    );
    let second_id = second.id.expect("second write returns an id");
    assert_eq!(second.superseded, vec![first_id]);

    let old_pointer: Option<i64> =
        sqlx::query_scalar("SELECT superseded_by FROM design_documents WHERE id=$1")
            .bind(first_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        old_pointer,
        Some(second_id),
        "the retired row points at its real successor, never at itself"
    );

    let current: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM design_documents WHERE system='design-proof' AND doc_type='token' AND name='reliquary-palette' AND superseded_by IS NULL",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(current, 1, "exactly one current row per identity");

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM design_documents WHERE system='design-proof' AND name='reliquary-palette'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(total, 2, "history is retained");

    let twice = design_document_write(&pool, token("third assertion", Some(first_id))).await?;
    assert!(
        !twice.ok,
        "an already-superseded row cannot be superseded again"
    );

    sqlx::query("SET search_path TO public")
        .execute(&pool)
        .await?;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}

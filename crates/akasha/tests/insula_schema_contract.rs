use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const INSULA_MIGRATION: &str = include_str!("../../../substrate/migrations/0022_insula.sql");

fn isolated_database_url() -> String {
    let url = std::env::var("SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL")
        .expect("Insula proof requires a dedicated PostgreSQL URL");
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
        "refusing a live or production-looking database"
    );
    url
}

async fn fresh_insula() -> TestResult<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&isolated_database_url())
        .await?;
    // This proof owns only the dedicated test database and deliberately starts
    // each migration assertion from the same pre-Insula state.
    sqlx::query("DROP SCHEMA IF EXISTS insula CASCADE")
        .execute(&pool)
        .await?;
    sqlx::raw_sql(INSULA_MIGRATION).execute(&pool).await?;
    Ok(pool)
}

async fn column_names(pool: &PgPool, table: &str) -> TestResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT column_name
         FROM information_schema.columns
         WHERE table_schema = 'insula' AND table_name = $1
         ORDER BY ordinal_position",
    )
    .bind(table)
    .fetch_all(pool)
    .await?)
}

#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Insula schema"]
async fn insula_migration_refuses_bodyful_partial_state_and_keeps_raw_rows_bounded() -> TestResult {
    let pool = fresh_insula().await?;

    let columns = column_names(&pool, "log").await?;
    for forbidden in ["body", "payload", "content", "message", "prompt", "detail"] {
        assert!(
            !columns.iter().any(|column| column == forbidden),
            "raw observations must not acquire a {forbidden} column"
        );
    }

    let expiry: String = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid)
         FROM pg_constraint
         WHERE conname = 'insula_log_expiry_check'",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        expiry.contains("expires_at =") && expiry.contains("14 days"),
        "every raw observation must expire exactly fourteen days after observation"
    );

    let session_rollup_columns = column_names(&pool, "vitals_minute").await?;
    assert!(
        !session_rollup_columns
            .iter()
            .any(|column| column == "session_id"),
        "session is raw-window-only and must never become a permanent Vitals dimension"
    );
    for permanent_dimension in ["house_id", "room", "spirit"] {
        assert!(
            session_rollup_columns
                .iter()
                .any(|column| column == permanent_dimension),
            "Vitals must retain the permanent {permanent_dimension} dimension"
        );
    }
    for forbidden_reference in ["quest_id", "attempt_id"] {
        assert!(
            !columns.iter().any(|column| column == forbidden_reference),
            "Insula observations must not acquire {forbidden_reference} or Docket authority"
        );
    }

    sqlx::query("ALTER TABLE insula.log ADD COLUMN body TEXT")
        .execute(&pool)
        .await?;
    let rejected = sqlx::raw_sql(INSULA_MIGRATION).execute(&pool).await;
    assert!(
        rejected.is_err(),
        "a bodyful partial schema must fail loudly, never silently claim healing"
    );
    assert!(
        rejected
            .expect_err("checked above")
            .to_string()
            .contains("insula.log must stay body free"),
        "the partial-schema refusal must identify the body-free invariant"
    );
    sqlx::query("ROLLBACK").execute(&pool).await?;

    sqlx::query("DROP SCHEMA insula CASCADE")
        .execute(&pool)
        .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Insula schema"]
async fn insula_retention_has_deterministic_same_house_coverage_proof() -> TestResult {
    let pool = fresh_insula().await?;

    let receipt_indexes: Vec<String> = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes
         WHERE schemaname = 'insula' AND tablename = 'retention_receipts'",
    )
    .fetch_all(&pool)
    .await?;
    assert!(
        receipt_indexes.iter().any(|index| {
            index.starts_with("CREATE UNIQUE INDEX")
                && index.contains("house_id")
                && index.contains("sweep_version")
                && index.contains("sweep_key")
        }),
        "concurrent or replayed retention sweeps need one versioned deterministic sweep identity"
    );
    let retention_days: String = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid)
         FROM pg_constraint
         WHERE conname = 'insula_retention_receipts_retention_days_check'",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        retention_days.contains("retention_days = 14"),
        "Insula v1 retention receipts must describe the fixed fourteen-day raw window"
    );

    let tombstone_columns = column_names(&pool, "log_tombstones").await?;
    assert!(
        tombstone_columns
            .iter()
            .any(|column| column == "coverage_hash"),
        "a writer sequence range alone hides gaps; tombstones require stable deleted-set coverage"
    );

    let same_house_guard: Vec<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid)
         FROM pg_constraint
         WHERE conrelid = 'insula.log_tombstones'::regclass",
    )
    .fetch_all(&pool)
    .await?;
    assert!(
        same_house_guard
            .iter()
            .any(|definition| definition.contains("house_id")),
        "a tombstone must be constrained to the same house as its retention receipt"
    );

    let vitals_columns = column_names(&pool, "vitals_minute").await?;
    for recomputation_metadata in [
        "source_first_sequence",
        "source_last_sequence",
        "source_coverage_hash",
    ] {
        assert!(
            vitals_columns
                .iter()
                .any(|column| column == recomputation_metadata),
            "permanent Vitals must preserve {recomputation_metadata} before raw retention deletes its source"
        );
    }

    sqlx::query("DROP SCHEMA insula CASCADE")
        .execute(&pool)
        .await?;
    Ok(())
}

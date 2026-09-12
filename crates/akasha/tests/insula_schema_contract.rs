use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const INSULA_MIGRATION: &str = include_str!("../../../substrate/migrations/0022_insula.sql");

fn isolated_database_url() -> String {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("Insula proof requires a dedicated PostgreSQL URL");
    let options: sqlx::postgres::PgConnectOptions = url.parse().expect("valid test URL");
    let database = options
        .get_database()
        .expect("explicit test database")
        .to_ascii_lowercase();
    assert!(
        database.contains("test") && !database.contains("solarisael"),
        "refusing a non-test or live database, including percent-encoded names"
    );
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
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Insula schema"]
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
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Insula schema"]
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

// Kills destructive upgrades, hash rewrites, historical receipt relabeling,
// and calendar-day expiry arithmetic across a daylight-saving transition.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; resets its dedicated Insula schema"]
async fn seven_day_upgrade_preserves_evidence_and_uses_elapsed_hours_across_dst() -> TestResult {
    let pool = fresh_insula().await?;
    // One connection keeps the deliberately non-UTC session timezone in scope.
    sqlx::query("SET TIME ZONE 'America/New_York'")
        .execute(&pool)
        .await?;
    sqlx::raw_sql(
        "INSERT INTO insula.log (
            event_id, span_id, trace_id, writer_id, writer_sequence,
            house_id, room, spirit, session_id, component, layer, operation,
            phase, observed_at, outcome_class, idempotency_scope,
            idempotency_key, semantic_hash, expires_at
         )
         SELECT gen_random_uuid(), gen_random_uuid(), gen_random_uuid(), gen_random_uuid(), 1,
                'solarisael', 'upgrade', 'Proof', 'upgrade-session', 'substrate', 'domain',
                'upgrade', 'point', observed, 'ok', 'trace_span',
                encode(sha256(observed::text::bytea),'hex'), repeat('a',64),
                observed + INTERVAL '14 days'
         FROM (VALUES ('2026-03-04 12:00:00-05'::timestamptz), (NOW())) source(observed);
         INSERT INTO insula.vitals_minute (
            query_name, query_version, minute, house_id, room, spirit, component,
            layer, operation, phase, outcome_class, event_count,
            source_first_sequence, source_last_sequence, source_first_observed_at,
            source_last_observed_at, source_coverage_hash
         )
         SELECT 'insula.vitals.minute', 1, date_trunc('minute', observed_at),
                house_id, room, spirit, component, layer, operation, phase, outcome_class, 1,
                writer_sequence, writer_sequence, observed_at, observed_at,
                encode(sha256((event_id::text || ':' || semantic_hash)::bytea),'hex')
         FROM insula.log;
         INSERT INTO insula.retention_receipts (
            receipt_id, receipt_kind, receipt_version, house_id, sweep_version, sweep_key,
            retention_days, swept_through, window_start, window_end, event_count, writer_count,
            duplicate_count_sum, drop_count_sum, coverage_version, coverage_hash,
            rollup_query_name, rollup_query_version, rollup_watermark
         ) VALUES (
            gen_random_uuid(), 'insula.retention.raw_delete', 1, 'solarisael', 1, repeat('b',64),
            14, NOW(), NOW()-INTERVAL '30 days', NOW()-INTERVAL '30 days', 1, 1,
            0, 0, 1, repeat('c',64), 'insula.vitals.minute', 1, NOW()
         );",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO insula.log_tombstones (
            tombstone_id, receipt_id, receipt_kind, house_id, writer_id,
            first_writer_sequence, last_writer_sequence, first_observed_at, last_observed_at,
            event_count, room_count, spirit_count, session_count, duplicate_count_sum,
            drop_count_sum, coverage_version, coverage_hash
         )
         SELECT gen_random_uuid(), receipt_id, receipt_kind, house_id, gen_random_uuid(),
                1, 1, window_start, window_end, 1, 1, 1, 1, 0, 0, 1, coverage_hash
         FROM insula.retention_receipts",
    )
    .execute(&pool)
    .await?;
    let tombstones_before: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(t) ORDER BY tombstone_id) FROM insula.log_tombstones t",
    )
    .fetch_one(&pool)
    .await?;
    let raw_before: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(l)-'expires_at' ORDER BY event_id) FROM insula.log l",
    )
    .fetch_one(&pool)
    .await?;
    let summaries_before: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(v) ORDER BY minute) FROM insula.vitals_minute v",
    )
    .fetch_one(&pool)
    .await?;
    let receipts_before: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(r) ORDER BY receipt_id) FROM insula.retention_receipts r",
    )
    .fetch_one(&pool)
    .await?;
    let migration =
        include_str!("../../../substrate/migrations/0032_insula_seven_day_retention.sql");
    sqlx::raw_sql(migration).execute(&pool).await?;
    sqlx::raw_sql(migration).execute(&pool).await?;
    let raw_after: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(l)-'expires_at' ORDER BY event_id) FROM insula.log l",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        raw_after, raw_before,
        "upgrade and replay preserve every raw field except expiry"
    );
    let summaries_after: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(v) ORDER BY minute) FROM insula.vitals_minute v",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(summaries_after, summaries_before);
    let receipts_after: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(r) ORDER BY receipt_id) FROM insula.retention_receipts r",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        receipts_after, receipts_before,
        "historical fourteen-day proof must stay truthful"
    );
    let tombstones_after: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(t) ORDER BY tombstone_id) FROM insula.log_tombstones t",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(tombstones_after, tombstones_before);
    let exact_expiry: bool = sqlx::query_scalar(
        "SELECT bool_and(expires_at-observed_at = INTERVAL '168 hours') FROM insula.log",
    )
    .fetch_one(&pool)
    .await?;
    assert!(exact_expiry);
    let invalid_expiry = sqlx::query(
        "UPDATE insula.log SET expires_at=observed_at+INTERVAL '7 days'
         WHERE observed_at='2026-03-04 12:00:00-05'::timestamptz",
    )
    .execute(&pool)
    .await
    .expect_err("DST-shortened calendar week must violate exact expiry");
    assert_eq!(
        invalid_expiry
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23514")
    );
    sqlx::query("SET TIME ZONE 'UTC'").execute(&pool).await?;
    sqlx::query("DROP SCHEMA insula CASCADE")
        .execute(&pool)
        .await?;
    Ok(())
}

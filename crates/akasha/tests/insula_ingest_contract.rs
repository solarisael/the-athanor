use akasha::{
    INSULA_MAX_TRACE_ROWS, IngestBatch, ObservationEvent, TraceScope, TrustedBinding, VitalsQuery,
    derive_idempotency_key_v1, derive_semantic_hash_v1, ingest_batch, query_trace, query_vitals,
    run_retention,
};
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const INSULA_MIGRATION: &str = include_str!("../../../substrate/migrations/0022_insula.sql");
const SEVEN_DAY_MIGRATION: &str =
    include_str!("../../../substrate/migrations/0032_insula_seven_day_retention.sql");

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
        .max_connections(2)
        .connect(&isolated_database_url())
        .await?;
    sqlx::query("DROP SCHEMA IF EXISTS insula CASCADE")
        .execute(&pool)
        .await?;
    sqlx::raw_sql(INSULA_MIGRATION).execute(&pool).await?;
    sqlx::raw_sql(SEVEN_DAY_MIGRATION).execute(&pool).await?;
    Ok(pool)
}

fn binding(session_id: &str) -> TrustedBinding {
    TrustedBinding {
        house_id: "solarisael".into(),
        room: "kintsu".into(),
        spirit: "Kintsu".into(),
        session_id: session_id.into(),
    }
}

fn event(writer_id: Uuid, writer_sequence: i64) -> ObservationEvent {
    serde_json::from_value(json!({
        "eventId": Uuid::new_v4().to_string(),
        "spanId": Uuid::new_v4().to_string(),
        "traceId": Uuid::new_v4().to_string(),
        "parentSpanId": null,
        "writerId": writer_id.to_string(),
        "writerSequence": writer_sequence,
        "component": "omp_adapter",
        "layer": "adapter",
        "operation": "tool_call",
        "phase": "point",
        "observedAt": Utc::now().to_rfc3339(),
        "durationUs": 100,
        "outcomeClass": "ok",
        "errorClass": null,
        "bytesIn": 8,
        "bytesOut": 13,
        "tokensIn": 2,
        "tokensOut": 3,
        "toolCallId": null,
        "providerRequestId": null,
        "idempotencyScope": "room_operation",
        "receiptKind": "test_receipt",
        "receiptId": format!("receipt-{writer_id}-{writer_sequence}"),
        "dropCount": 0
    }))
    .expect("test observation must satisfy the strict public DTO")
}

fn seal(binding: &TrustedBinding, mut event: ObservationEvent) -> ObservationEvent {
    event.idempotency_key =
        derive_idempotency_key_v1(binding, &event).expect("test idempotency key");
    event.semantic_hash = derive_semantic_hash_v1(binding, &event).expect("test semantic hash");
    event
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Insula schema"]
async fn ingest_collapses_only_identical_cross_session_redelivery_and_reports_corruption()
-> TestResult {
    let pool = fresh_insula().await?;
    let first_binding = binding("writer-session-a");
    let second_binding = binding("writer-session-b");

    let first = seal(&first_binding, event(Uuid::new_v4(), 1));
    let accepted = ingest_batch(
        &pool,
        &first_binding,
        IngestBatch {
            events: vec![first.clone()],
        },
    )
    .await?;
    assert_eq!(accepted.accepted_count, 1);
    assert_eq!(accepted.duplicate_count, 0);
    assert!(accepted.conflicts.is_empty());

    let mut redelivery = first.clone();
    redelivery.event_id = Uuid::new_v4().to_string();
    redelivery.writer_id = Uuid::new_v4().to_string();
    redelivery.writer_sequence = 1;
    redelivery.span_id = Uuid::new_v4().to_string();
    redelivery.observed_at = first.observed_at + Duration::seconds(1);
    redelivery.semantic_hash =
        derive_semantic_hash_v1(&second_binding, &redelivery).expect("redelivery semantic hash");
    redelivery.idempotency_key =
        derive_idempotency_key_v1(&second_binding, &redelivery).expect("redelivery key");
    let duplicate = ingest_batch(
        &pool,
        &second_binding,
        IngestBatch {
            events: vec![redelivery.clone()],
        },
    )
    .await?;
    assert_eq!(
        duplicate.accepted_count, 0,
        "identical redelivery is not another observation"
    );
    assert_eq!(
        duplicate.duplicate_count, 1,
        "identical redelivery across writer/session collapses"
    );
    assert!(duplicate.conflicts.is_empty());

    let persisted_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM insula.log")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        persisted_rows, 1,
        "duplicate redelivery must not produce a second raw row"
    );
    let rollup_count: i64 = sqlx::query_scalar("SELECT event_count FROM insula.vitals_minute")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        rollup_count, 1,
        "duplicate redelivery must not double-count Vitals"
    );

    let default_expiry_is_bounded: bool = sqlx::query_scalar(
        "SELECT expires_at = observed_at + INTERVAL '168 hours'
         FROM insula.log",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        default_expiry_is_bounded,
        "omitted expiresAt must become exactly seven days after observation"
    );
    let trace = query_trace(
        &pool,
        &TraceScope {
            house_id: first_binding.house_id.clone(),
            room: Some(first_binding.room.clone()),
            spirit: Some(first_binding.spirit.clone()),
            session_id: Some(first_binding.session_id.clone()),
        },
        &first.trace_id,
        100,
    )
    .await?;
    let trace_json = serde_json::to_value(trace)?;
    assert!(
        trace_json["queryName"].is_string(),
        "trace result names its versioned mechanical query"
    );
    assert_eq!(trace_json["queryVersion"], 1);
    assert_eq!(
        trace_json["rows"]
            .as_array()
            .expect("versioned trace rows")
            .len(),
        1
    );
    assert!(
        trace_json["rows"][0].get("body").is_none(),
        "trace DTOs must remain body-free"
    );
    assert!(
        query_trace(
            &pool,
            &TraceScope {
                house_id: first_binding.house_id.clone(),
                room: None,
                spirit: None,
                session_id: None,
            },
            &first.trace_id,
            INSULA_MAX_TRACE_ROWS + 1,
        )
        .await
        .is_err(),
        "trace drilldown must bound its raw-session window"
    );
    let vitals = query_vitals(
        &pool,
        &VitalsQuery {
            house_id: first_binding.house_id.clone(),
            room: Some(first_binding.room.clone()),
            spirit: Some(first_binding.spirit.clone()),
            component: None,
            layer: None,
            operation: None,
            phase: None,
            outcome_class: None,
            start: Utc::now() - Duration::hours(1),
            end: Utc::now() + Duration::hours(1),
            limit: 100,
        },
    )
    .await?;
    let vitals_json = serde_json::to_value(vitals)?;
    assert_eq!(vitals_json["queryName"], "insula.vitals.minute");
    assert_eq!(vitals_json["queryVersion"], 1);
    assert_eq!(
        vitals_json["rows"]
            .as_array()
            .expect("versioned Vitals rows")
            .len(),
        1
    );
    let mut semantic_conflict = redelivery.clone();
    semantic_conflict.event_id = Uuid::new_v4().to_string();
    semantic_conflict.writer_id = Uuid::new_v4().to_string();
    semantic_conflict.writer_sequence = 1;
    semantic_conflict.duration_us = Some(101);

    // Ingest re-derives keys and hashes server-side, so corruption means a
    // reused transport identity carrying different logical content, never a
    // hand-mangled key.
    let mut event_id_corruption = first.clone();
    event_id_corruption.writer_id = Uuid::new_v4().to_string();
    event_id_corruption.receipt_id = Some("receipt-eventid-corruption".into());

    let mut writer_sequence_corruption = first.clone();
    writer_sequence_corruption.event_id = Uuid::new_v4().to_string();
    writer_sequence_corruption.receipt_id = Some("receipt-writerseq-corruption".into());

    let conflicts = ingest_batch(
        &pool,
        &second_binding,
        IngestBatch {
            events: vec![
                semantic_conflict,
                event_id_corruption,
                writer_sequence_corruption,
            ],
        },
    )
    .await?;
    assert_eq!(conflicts.accepted_count, 0);
    assert_eq!(conflicts.duplicate_count, 0);
    assert_eq!(
        conflicts.conflicts.len(),
        3,
        "semantic reuse and transport corruption are distinct typed conflicts"
    );
    let conflict_kinds: Vec<String> = conflicts
        .conflicts
        .iter()
        .map(|conflict| {
            serde_json::to_value(conflict).expect("serialize typed conflict")["kind"]
                .as_str()
                .expect("conflict kind is a stable mechanical string")
                .to_owned()
        })
        .collect();
    assert_eq!(
        conflict_kinds,
        vec![
            "logical_key_reuse",
            "event_id_reuse",
            "writer_sequence_reuse"
        ],
    );

    let final_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM insula.log")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        final_rows, 1,
        "every conflict must leave the original observation intact"
    );

    sqlx::query("DROP SCHEMA insula CASCADE")
        .execute(&pool)
        .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Insula schema"]
async fn trace_projection_orders_cross_writer_parent_before_clock_skewed_child() -> TestResult {
    let pool = fresh_insula().await?;
    let trusted = binding("causal-trace-session");
    let parent = event(Uuid::new_v4(), 1);
    let mut child = event(Uuid::new_v4(), 1);
    child.trace_id = parent.trace_id.clone();
    child.parent_span_id = Some(parent.span_id.clone());
    child.receipt_id = Some("receipt-child".into());
    child.observed_at = parent.observed_at - Duration::hours(1);

    let parent_id = parent.event_id.clone();
    let child_id = child.event_id.clone();
    let trace_id = parent.trace_id.clone();
    let receipt = ingest_batch(
        &pool,
        &trusted,
        IngestBatch {
            events: vec![seal(&trusted, child), seal(&trusted, parent)],
        },
    )
    .await?;
    assert_eq!(receipt.accepted_count, 2);

    let trace = query_trace(&pool, &TraceScope::from(&trusted), &trace_id, 10).await?;
    let event_ids: Vec<String> = trace.rows.into_iter().map(|row| row.event_id).collect();
    assert_eq!(
        event_ids,
        vec![parent_id, child_id],
        "parent links outrank skewed wall time and lexical writer identity"
    );

    sqlx::query("DROP SCHEMA insula CASCADE")
        .execute(&pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn ingest_rejects_future_observation_time_before_database_access() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://localhost/insula-validation-only")
        .expect("syntactically valid lazy validation pool");
    let trusted = binding("future-skew-session");
    let mut future = event(Uuid::new_v4(), 1);
    future.observed_at = Utc::now() + Duration::days(1);

    let result = ingest_batch(
        &pool,
        &trusted,
        IngestBatch {
            events: vec![future],
        },
    )
    .await;
    assert!(matches!(
        result,
        Err(akasha::InsulaError::Validation {
            field: "observedAt",
            ..
        })
    ));
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Insula schema"]
async fn retention_is_replay_safe_keeps_coverage_and_preserves_recomputable_vitals() -> TestResult {
    let pool = fresh_insula().await?;
    let binding = binding("raw-window-session");

    let first_writer_id = Uuid::new_v4();
    let second_writer_id = Uuid::new_v4();
    let observed_at = Utc::now() - Duration::days(8);
    let mut first = event(first_writer_id, 1);
    first.observed_at = observed_at;
    let mut second = event(second_writer_id, 1);
    second.observed_at = observed_at;
    let receipt = ingest_batch(
        &pool,
        &binding,
        IngestBatch {
            events: vec![seal(&binding, first), seal(&binding, second)],
        },
    )
    .await?;
    assert_eq!(receipt.accepted_count, 2);
    let expected_coverage: String = sqlx::query_scalar(
        "SELECT encode(
             digest(
                 convert_to(
                     string_agg(event_id::text||':'||semantic_hash,E'\\n' ORDER BY event_id),
                     'UTF8'
                 ),
                 'sha256'
             ),
             'hex'
         )
         FROM insula.log",
    )
    .fetch_one(&pool)
    .await?;
    let expected_writer_coverage: Vec<(String, String)> = sqlx::query_as(
        "SELECT writer_id::text,
                encode(
                    digest(
                        convert_to(
                            string_agg(event_id::text||':'||semantic_hash,E'\\n' ORDER BY event_id),
                            'UTF8'
                        ),
                        'sha256'
                    ),
                    'hex'
                )
         FROM insula.log
         GROUP BY writer_id
         ORDER BY writer_id",
    )
    .fetch_all(&pool)
    .await?;

    let cutoff = Utc::now() - Duration::hours(12);
    let (left, right) = tokio::join!(
        run_retention(&pool, "solarisael", cutoff, 7),
        run_retention(&pool, "solarisael", cutoff, 7),
    );
    let left = left?;
    let right = right?;
    let statuses: Vec<String> = [left.status, right.status]
        .into_iter()
        .map(|status| {
            serde_json::to_value(status)
                .expect("serialize typed retention status")
                .as_str()
                .expect("retention status is stable")
                .to_owned()
        })
        .collect();
    assert!(statuses.contains(&"deleted".into()));
    assert!(statuses.contains(&"replayed".into()));

    let receipt_count: i64 = sqlx::query_scalar("SELECT count(*) FROM insula.retention_receipts")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        receipt_count, 1,
        "concurrent/replayed sweep must share one deterministic receipt"
    );
    let receipt_coverage: String =
        sqlx::query_scalar("SELECT coverage_hash FROM insula.retention_receipts")
            .fetch_one(&pool)
            .await?;
    let tombstone_coverage: Vec<(String, String)> = sqlx::query_as(
        "SELECT writer_id::text, coverage_hash
         FROM insula.log_tombstones
         ORDER BY writer_id",
    )
    .fetch_all(&pool)
    .await?;
    let vitals_coverage: String =
        sqlx::query_scalar("SELECT source_coverage_hash FROM insula.vitals_minute")
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        receipt_coverage, expected_coverage,
        "retention receipt must prove its exact deleted raw set"
    );
    assert_eq!(
        tombstone_coverage, expected_writer_coverage,
        "each writer tombstone must prove its exact deleted writer set"
    );
    assert_eq!(
        vitals_coverage, expected_coverage,
        "permanent Vitals must retain canonical raw-source coverage before deletion"
    );
    let raw_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM insula.log")
        .fetch_one(&pool)
        .await?;
    assert_eq!(raw_rows, 0);

    let replay = run_retention(&pool, "solarisael", cutoff, 7).await?;
    assert_eq!(
        serde_json::to_value(replay.status)?,
        "replayed",
        "the same deterministic sweep must replay its durable receipt"
    );
    assert!(replay.receipt_id.is_some());

    let noop = run_retention(&pool, "solarisael", cutoff + Duration::minutes(1), 7).await?;
    assert_eq!(
        serde_json::to_value(noop.status)?,
        "noop",
        "a later empty sweep is a versioned explicit outcome"
    );
    assert!(
        noop.receipt_id.is_none(),
        "no-op must not manufacture an empty delete receipt"
    );

    sqlx::query("DROP SCHEMA insula CASCADE")
        .execute(&pool)
        .await?;
    Ok(())
}

// Kills a coverage guard that trusts only counts, and a policy argument that
// silently writes a false fourteen-day receipt for seven-day raw rows.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; resets its dedicated Insula schema"]
async fn retention_refuses_incomplete_coverage_and_unsupported_policy_atomically() -> TestResult {
    let pool = fresh_insula().await?;
    let trusted = binding("coverage-refusal");
    let mut expired = event(Uuid::new_v4(), 1);
    expired.observed_at = Utc::now() - Duration::days(8);
    ingest_batch(
        &pool,
        &trusted,
        IngestBatch {
            events: vec![expired],
        },
    )
    .await?;
    let cutoff = Utc::now() - Duration::minutes(1);
    assert!(matches!(
        run_retention(&pool, "solarisael", cutoff, 14).await,
        Err(akasha::InsulaError::Validation { .. })
    ));
    sqlx::query("UPDATE insula.vitals_minute SET source_coverage_hash = repeat('0', 64)")
        .execute(&pool)
        .await?;
    assert!(matches!(
        run_retention(&pool, "solarisael", cutoff, 7).await,
        Err(akasha::InsulaError::Invariant(_))
    ));
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM insula.log),
                (SELECT count(*) FROM insula.retention_receipts),
                (SELECT count(*) FROM insula.log_tombstones)",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        counts,
        (1, 0, 0),
        "refusal leaves neither deletion nor partial proof"
    );
    sqlx::query("DROP SCHEMA insula CASCADE")
        .execute(&pool)
        .await?;
    Ok(())
}

// Kills a disabled scheduler, a second age subtraction at the cutoff, and
// accidental deletion of recent raw data or permanent summaries.
#[tokio::test]
#[ignore = "dedicated athanor_insula_retention_test_* database; runs the real 300s binary timer"]
async fn automatic_retention_fires_after_real_startup_delay_and_preserves_recent_rows() -> TestResult
{
    use std::process::Stdio;
    use std::str::FromStr;
    use std::time::{Duration as WallDuration, Instant};

    let url = isolated_database_url();
    let options = sqlx::postgres::PgConnectOptions::from_str(&url)?;
    let database = options
        .get_database()
        .ok_or("explicit test database required")?
        .to_owned();
    assert!(
        database
            .strip_prefix("athanor_insula_retention_test_")
            .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())),
        "scheduler proof refuses anything except its explicitly named disposable database"
    );
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await?;
    let actual_database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    assert_eq!(actual_database, database);
    // The disposable template supplies the real runtime prerequisites. Its
    // historical migration ledger is not this scheduler proof's authority.
    let embedding_shape: Option<String> = sqlx::query_scalar(
        "SELECT format_type(a.atttypid, a.atttypmod)
         FROM pg_attribute a
         WHERE a.attrelid=to_regclass('memory_chunks')
           AND a.attname='body_embedding' AND NOT a.attisdropped",
    )
    .fetch_optional(&pool)
    .await?;
    assert_eq!(
        embedding_shape.as_deref(),
        Some("vector(2048)"),
        "scheduler proof requires an authentic migrated vector template"
    );
    let has_settings: bool = sqlx::query_scalar("SELECT to_regclass('room_settings') IS NOT NULL")
        .fetch_one(&pool)
        .await?;
    assert!(
        has_settings,
        "scheduler proof requires authentic room_settings"
    );
    sqlx::query("DROP SCHEMA IF EXISTS insula CASCADE")
        .execute(&pool)
        .await?;
    sqlx::raw_sql(INSULA_MIGRATION).execute(&pool).await?;
    sqlx::raw_sql(SEVEN_DAY_MIGRATION).execute(&pool).await?;
    akasha::RoomSettings::load(&pool, "house").await?;
    let trusted = binding("automatic-retention-proof");
    let mut expired = event(Uuid::new_v4(), 1);
    expired.observed_at = Utc::now() - Duration::days(8);
    let mut recent = event(Uuid::new_v4(), 1);
    recent.observed_at = Utc::now() - Duration::days(6);
    let recent_id = recent.event_id.clone();
    ingest_batch(
        &pool,
        &trusted,
        IngestBatch {
            events: vec![expired, recent],
        },
    )
    .await?;
    let summaries_before: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(v) ORDER BY minute) FROM insula.vitals_minute v",
    )
    .fetch_one(&pool)
    .await?;
    let recent_before: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(l) FROM insula.log l WHERE event_id=$1::uuid")
            .bind(&recent_id)
            .fetch_one(&pool)
            .await?;
    let isolated_dir = std::env::temp_dir().join(format!("insula-scheduler-{}", Uuid::new_v4()));
    std::fs::create_dir(&isolated_dir)?;
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_athanor-substrate"));
    command
        .env_clear()
        .env("DATABASE_URL", &url)
        .env("ATHANOR_SUBSTRATE_TEST_DATABASE_URL", &url)
        .env(
            "ATHANOR_SUBSTRATE_DOTENV_PATH",
            isolated_dir.join("absent.env"),
        )
        .env("ATHANOR_DISABLE_EMBEDDING", "1")
        .current_dir(&isolated_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    // Windows needs its system directory even with every House variable stripped.
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    let started = Instant::now();
    let mut child = command.spawn()?;
    let proof = tokio::time::timeout(WallDuration::from_secs(420), async {
        loop {
            if let Some(status) = child.try_wait()? {
                return Err(format!("substrate exited before automatic receipt: {status}").into());
            }
            let receipt: Option<(i16, i64, i64)> = sqlx::query_as(
                "SELECT retention_days, event_count, writer_count FROM insula.retention_receipts",
            )
            .fetch_optional(&pool)
            .await?;
            if let Some(receipt) = receipt {
                assert!(
                    started.elapsed() >= WallDuration::from_secs(300),
                    "the real startup timer must not fire early"
                );
                assert_eq!(receipt, (7, 1, 1));
                break;
            }
            assert!(
                started.elapsed() < WallDuration::from_secs(420),
                "automatic retention did not produce a receipt within 420 seconds"
            );
            tokio::time::sleep(WallDuration::from_secs(1)).await;
        }
        let remaining: Vec<String> = sqlx::query_scalar("SELECT event_id::text FROM insula.log")
            .fetch_all(&pool)
            .await?;
        assert_eq!(remaining, vec![recent_id.clone()]);
        let recent_after: serde_json::Value =
            sqlx::query_scalar("SELECT to_jsonb(l) FROM insula.log l WHERE event_id=$1::uuid")
                .bind(&recent_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(recent_after, recent_before);
        let summaries_after: serde_json::Value = sqlx::query_scalar(
            "SELECT jsonb_agg(to_jsonb(v) ORDER BY minute) FROM insula.vitals_minute v",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(summaries_after, summaries_before);
        let tombstone_count: i64 =
            sqlx::query_scalar("SELECT sum(event_count)::bigint FROM insula.log_tombstones")
                .fetch_one(&pool)
                .await?;
        assert_eq!(tombstone_count, 1);
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await;
    child.kill().await?;
    child.wait().await?;
    std::fs::remove_dir(&isolated_dir)?;
    proof?
}

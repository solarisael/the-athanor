use akasha::{Config, EmbeddingMode, paper_boat_sleep, paper_boat_wake};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use summoning::{PaperBoatSleepRequest, PaperBoatWakeRequest};
use uuid::Uuid;

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
    migration!("0013_lesson_eligibility_keys.sql"),
    migration!("0014_lesson_threads.sql"),
    migration!("0015_canon_authority.sql"),
    migration!("0016_boat_ready_delivery.sql"),
    migration!("0017_crane_delivery.sql"),
];

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

#[tokio::test]
#[ignore = "requires a PostgreSQL database where the test may create and drop a schema"]
async fn sleep_commit_is_idempotent_and_wake_is_room_scoped_with_stale_detection() -> TestResult {
    let url = isolated_database_url();
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let pgcrypto_schema: Option<String> = sqlx::query_scalar(
        "SELECT namespace.nspname
         FROM pg_extension AS extension
         JOIN pg_namespace AS namespace ON namespace.oid = extension.extnamespace
         WHERE extension.extname = 'pgcrypto'",
    )
    .fetch_optional(&admin)
    .await?;
    let schema = format!("solarisael_boat_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await?;

    let connection_schema = schema.clone();
    let connection_pgcrypto_schema = pgcrypto_schema.clone();
    let proof = async {
        let options = PgConnectOptions::from_str(&url)?;
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .after_connect(move |connection, _metadata| {
                let schema = connection_schema.clone();
                let pgcrypto_schema = connection_pgcrypto_schema.clone();
                Box::pin(async move {
                    let search_path = match pgcrypto_schema {
                        Some(extension_schema) => format!("{schema}, {extension_schema}, public"),
                        None => format!("{schema}, public"),
                    };
                    sqlx::query("SELECT set_config('search_path', $1, false)")
                        .bind(search_path)
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect_with(options)
            .await?;
        let result = apply_migrations_and_run(&pool, &url).await;
        pool.close().await;
        result
    }
    .await;

    let cleanup = sqlx::query(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
        .execute(&admin)
        .await;
    admin.close().await;
    match (proof, cleanup) {
        (Ok(()), Ok(_)) => Ok(()),
        (Err(error), Ok(_)) => Err(error),
        (Ok(()), Err(error)) => Err(error.into()),
        (Err(proof_error), Err(cleanup_error)) => Err(format!(
            "paper boat proof failed: {proof_error}; schema cleanup failed: {cleanup_error}"
        )
        .into()),
    }
}

async fn apply_migrations_and_run(pool: &sqlx::PgPool, url: &str) -> TestResult {
    for migration in MIGRATIONS {
        sqlx::raw_sql(migration).execute(pool).await?;
    }
    // Reapplication is proved in migration order: 0016 recreates the boat trio and
    // 0017 folds it back into the crane trio, exactly as the runner would.
    for index in [15, 16] {
        sqlx::raw_sql(MIGRATIONS[index]).execute(pool).await?;
    }
    run_contract(pool, url).await
}

async fn run_contract(pool: &sqlx::PgPool, url: &str) -> TestResult {
    let cfg = Config {
        database_url: url.into(),
        embed_url: None,
        embed_model: "disabled".into(),
        embed_dimension: 2048,
        embedding_mode: EmbeddingMode::DisabledForTest,
        giga_source_ledger_dir: None,
        giga_source_room: None,
        house_tz: "America/Sao_Paulo".into(),
    };
    let suffix = Uuid::new_v4().simple();
    let room = format!("boat-test-{suffix}");
    let other_room = format!("boat-other-{suffix}");
    let empty = paper_boat_wake(pool, PaperBoatWakeRequest::new(room.clone())?).await?;
    assert!(
        empty.boat().is_none(),
        "wake must report no boat explicitly"
    );

    let body = "Tomorrow, begin with the transaction-coupled outbox proof.";
    let first = paper_boat_sleep(
        pool,
        &cfg,
        PaperBoatSleepRequest::new(room.clone(), body.into(), false)?,
    )
    .await?;
    assert!(first.inserted());
    assert!(first.durable());

    let duplicate = paper_boat_sleep(
        pool,
        &cfg,
        PaperBoatSleepRequest::new(room.clone(), body.into(), false)?,
    )
    .await?;
    assert!(!duplicate.inserted());
    assert_eq!(duplicate.memory_id(), first.memory_id());
    assert_eq!(duplicate.outbox_event_id(), first.outbox_event_id());

    let memory_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memories WHERE room=$1 AND type='paper-boat'")
            .bind(&room)
            .fetch_one(pool)
            .await?;
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM crane_outbox WHERE aggregate_id=$1 AND event_kind='boat.ready'",
    )
    .bind(i64::try_from(first.memory_id())?)
    .fetch_one(pool)
    .await?;
    let chunk_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory_chunks WHERE memory_id=$1")
            .bind(i64::try_from(first.memory_id())?)
            .fetch_one(pool)
            .await?;
    assert_eq!(memory_count, 1);
    assert_eq!(outbox_count, 1);
    assert!(
        chunk_count > 0,
        "sleep must write bounded retrieval chunks in its transaction"
    );

    let later_id: i64 = sqlx::query_scalar(
        "INSERT INTO memories (room,type,title,source_path,body)
         VALUES ($1,'memory','later memory',$2,'written after sleep') RETURNING id",
    )
    .bind(&room)
    .bind(format!("boat-test/{suffix}/later"))
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO memories (room,type,title,source_path,body)
         VALUES ($1,'memory','other memory',$2,'must not cross rooms')",
    )
    .bind(&other_room)
    .bind(format!("boat-test/{suffix}/other"))
    .execute(pool)
    .await?;

    let wake = paper_boat_wake(pool, PaperBoatWakeRequest::new(room.clone())?).await?;
    let boat = wake.boat().expect("latest room boat must be returned");
    assert_eq!(boat.id, first.memory_id());
    assert_eq!(boat.body, body);
    assert_eq!(boat.unboated.len(), 1);
    assert_eq!(boat.unboated[0].id, u64::try_from(later_id)?);
    assert_eq!(boat.unboated[0].title, "later memory");

    let second = paper_boat_sleep(
        pool,
        &cfg,
        PaperBoatSleepRequest::new(room.clone(), "newer letter".into(), false)?,
    )
    .await?;
    let newest = paper_boat_wake(pool, PaperBoatWakeRequest::new(room.clone())?).await?;
    let newest_boat = newest
        .boat()
        .expect("newest room boat must win deterministic ordering");
    assert_eq!(newest_boat.id, second.memory_id());
    assert_eq!(newest_boat.body, "newer letter");
    assert!(newest_boat.unboated.is_empty());

    let isolated = paper_boat_wake(pool, PaperBoatWakeRequest::new(other_room.clone())?).await?;
    assert!(
        isolated.boat().is_none(),
        "a boat from another room must never wake here"
    );
    Ok(())
}

use athanor_house_delivery::{
    ConsumeOutcome, DeliveryService, PublishOutcome,
    broker::Broker,
    model::{BoatReadyEvent, EVENT_KIND},
    store::Store,
};
use futures_util::StreamExt;
use house_protocol::{BOAT_RECEIPT_STREAM_NAME, BOAT_RECEIPT_SUBJECT, BoatReceiptProjection};
use serde_json::{Value, json};
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use std::{collections::BTreeSet, time::Duration};
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
];

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn isolated_database_url() -> String {
    let url = std::env::var("SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL")
        .expect("dedicated test database URL is required");
    let lower = url.to_ascii_lowercase();
    assert!(!lower.contains("solarisael_memory"));
    assert!(!lower.contains("solarisael-house"));
    url
}

fn nats_url() -> String {
    std::env::var("SOLARISAEL_DELIVERY_TEST_NATS_URL")
        .expect("a test-owned NATS 2.14.4 JetStream endpoint is required")
}

async fn insert_memory(
    pool: &sqlx::PgPool,
    memory_type: &str,
    room: &str,
    body: &str,
) -> TestResult<i64> {
    let source = format!("delivery-integration/{}", Uuid::new_v4());
    Ok(sqlx::query_scalar(
        "INSERT INTO memories (room, type, source_path, body)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(room)
    .bind(memory_type)
    .bind(source)
    .bind(body)
    .fetch_one(pool)
    .await?)
}

async fn next_outbox(pool: &sqlx::PgPool, memory_id: i64) -> TestResult<(Uuid, Value)> {
    let row =
        sqlx::query("SELECT event_id, payload FROM boat_ready_outbox WHERE aggregate_id = $1")
            .bind(memory_id)
            .fetch_one(pool)
            .await?;
    Ok((row.try_get("event_id")?, row.try_get("payload")?))
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database and test-owned NATS 2.14.4 JetStream process"]
async fn boat_ready_outbox_restart_dedup_receipt_replay_and_poison_contract() -> TestResult {
    let database_url = isolated_database_url();
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;
    let pgcrypto_schema: Option<String> = sqlx::query_scalar(
        "SELECT namespace.nspname
         FROM pg_extension AS extension
         JOIN pg_namespace AS namespace ON namespace.oid = extension.extnamespace
         WHERE extension.extname = 'pgcrypto'",
    )
    .fetch_optional(&admin)
    .await?;
    let schema = format!("solarisael_delivery_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await?;

    let connection_schema = schema.clone();
    let connection_pgcrypto_schema = pgcrypto_schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(8)
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
        .connect(&database_url)
        .await;

    let proof = match pool {
        Ok(pool) => {
            let result = apply_migrations_and_run(&pool).await;
            pool.close().await;
            result
        }
        Err(error) => Err(error.into()),
    };
    let cleanup = sqlx::query(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
        .execute(&admin)
        .await;
    admin.close().await;

    match (proof, cleanup) {
        (Ok(()), Ok(_)) => Ok(()),
        (Err(proof), Ok(_)) => Err(proof),
        (Ok(()), Err(cleanup)) => Err(cleanup.into()),
        (Err(proof), Err(cleanup)) => {
            Err(format!("delivery proof failed: {proof}; schema cleanup failed: {cleanup}").into())
        }
    }
}

async fn apply_migrations_and_run(pool: &PgPool) -> TestResult {
    for migration in MIGRATIONS {
        sqlx::raw_sql(migration).execute(pool).await?;
    }
    run_contract(pool).await
}

async fn run_contract(pool: &PgPool) -> TestResult {
    let unrelated = insert_memory(pool, "memory", "courier-proof", "not a boat").await?;
    let unrelated_events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM boat_ready_outbox WHERE aggregate_id = $1")
            .bind(unrelated)
            .fetch_one(pool)
            .await?;
    assert_eq!(unrelated_events, 0, "only exact paper-boat inserts enqueue");

    sqlx::raw_sql(MIGRATIONS[15]).execute(pool).await?;
    let unrelated_survived: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM memories WHERE id = $1)")
            .bind(unrelated)
            .fetch_one(pool)
            .await?;
    assert!(
        unrelated_survived,
        "reapplying migration preserves unknown rows"
    );

    let rolled_back_source = format!("delivery-integration/rollback-{}", Uuid::new_v4());
    let mut transaction = pool.begin().await?;
    let rolled_back_id: i64 = sqlx::query_scalar(
        "INSERT INTO memories (room, type, source_path, body)
         VALUES ('courier-proof', 'paper-boat', $1, 'rolled back body')
         RETURNING id",
    )
    .bind(&rolled_back_source)
    .fetch_one(&mut *transaction)
    .await?;
    let transactional_event: i64 =
        sqlx::query_scalar("SELECT count(*) FROM boat_ready_outbox WHERE aggregate_id = $1")
            .bind(rolled_back_id)
            .fetch_one(&mut *transaction)
            .await?;
    assert_eq!(transactional_event, 1);
    transaction.rollback().await?;
    let rollback_survived: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM memories WHERE source_path = $1)")
            .bind(&rolled_back_source)
            .fetch_one(pool)
            .await?;
    assert!(
        !rollback_survived,
        "memory and outbox event roll back together"
    );

    let memory_id = insert_memory(
        pool,
        "paper-boat",
        "courier-proof",
        "private body never leaves PostgreSQL",
    )
    .await?;
    let (event_id, payload) = next_outbox(pool, memory_id).await?;
    let keys: BTreeSet<_> = payload
        .as_object()
        .expect("outbox payload is an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "created_at",
            "event_id",
            "event_kind",
            "integrity_sha256",
            "record_id",
            "room",
            "schema_version",
        ])
    );
    let encoded = serde_json::to_string(&payload)?;
    assert!(!encoded.contains("private body"));
    assert!(!encoded.contains("title"));
    assert_eq!(payload["event_id"], event_id.to_string());
    assert_eq!(payload["event_kind"], EVENT_KIND);
    assert_eq!(payload["record_id"], memory_id.to_string());

    let broker = Broker::connect(&nats_url()).await?;
    let receipt_client = async_nats::connect(nats_url()).await?;
    let mut receipt_messages = receipt_client.subscribe(BOAT_RECEIPT_SUBJECT).await?;
    broker.configure().await?;
    let first_owner = Uuid::new_v4();
    let first = DeliveryService::new(Store::from_pool(pool.clone()), broker.clone(), first_owner);
    assert_eq!(first.publish_once().await?, PublishOutcome::Published);
    assert_eq!(
        first.consume_once(Duration::from_secs(2)).await?,
        ConsumeOutcome::Received
    );
    let receipt_message = tokio::time::timeout(Duration::from_secs(2), receipt_messages.next())
        .await?
        .expect("committed receipt projection must be published");
    let projection: BoatReceiptProjection = serde_json::from_slice(&receipt_message.payload)?;
    assert_eq!(projection.event_id, event_id.to_string());
    assert_eq!(projection.record_id, memory_id.to_string());
    assert_eq!(projection.room, "courier-proof");
    assert!(projection.original_stream_sequence > 0);
    assert_eq!(
        projection.integrity_sha256,
        payload["integrity_sha256"].as_str().unwrap()
    );
    let projection_json = serde_json::to_value(&projection)?;
    assert!(projection_json.get("body").is_none());
    assert!(projection_json.get("title").is_none());
    let receipt_context = async_nats::jetstream::new(receipt_client.clone());
    let mut receipt_stream = receipt_context.get_stream(BOAT_RECEIPT_STREAM_NAME).await?;
    let receipt_messages_before_replay = receipt_stream.info().await?.state.messages;

    broker
        .publish(Uuid::new_v4(), serde_json::to_vec(&payload)?)
        .await?;
    assert_eq!(
        first.consume_once(Duration::from_secs(2)).await?,
        ConsumeOutcome::ReceiptReplay
    );
    let receipt_messages_after_replay = receipt_stream.info().await?.state.messages;
    assert_eq!(
        receipt_messages_after_replay, receipt_messages_before_replay,
        "receipt replay must converge to one retained JetStream message"
    );
    let receipts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM boat_ready_receipts WHERE event_id = $1")
            .bind(event_id)
            .fetch_one(pool)
            .await?;
    assert_eq!(receipts, 1);

    let restart_memory = insert_memory(pool, "paper-boat", "courier-proof", "restart body").await?;
    let (restart_event_id, restart_payload) = next_outbox(pool, restart_memory).await?;
    let claimed = first
        .store()
        .claim_next(first_owner)
        .await?
        .expect("restart event is claimable");
    assert_eq!(claimed.event_id, restart_event_id);
    let first_sequence = broker
        .publish(restart_event_id, serde_json::to_vec(&restart_payload)?)
        .await?;
    sqlx::query(
        "UPDATE boat_ready_outbox SET lease_expires_at = NOW() - interval '1 second'
         WHERE event_id = $1",
    )
    .bind(restart_event_id)
    .execute(pool)
    .await?;

    let restarted = DeliveryService::new(
        Store::from_pool(pool.clone()),
        broker.clone(),
        Uuid::new_v4(),
    );
    assert_eq!(restarted.publish_once().await?, PublishOutcome::Published);
    let duplicate_sequence = broker
        .publish(restart_event_id, serde_json::to_vec(&restart_payload)?)
        .await?;
    assert_eq!(
        first_sequence, duplicate_sequence,
        "Nats-Msg-Id deduplicates replay"
    );
    assert_eq!(
        restarted.consume_once(Duration::from_secs(2)).await?,
        ConsumeOutcome::Received
    );

    let private = json!({
        "schema_version": 1,
        "event_id": Uuid::new_v4(),
        "event_kind": "boat.ready",
        "record_id": memory_id.to_string(),
        "room": "courier-proof",
        "created_at": "2026-08-10T12:00:00Z",
        "integrity_sha256": "a".repeat(64),
        "body": "must not cross this boundary"
    });
    broker
        .publish(Uuid::new_v4(), serde_json::to_vec(&private)?)
        .await?;
    assert_eq!(
        restarted.consume_once(Duration::from_secs(2)).await?,
        ConsumeOutcome::DeadLettered
    );
    let dead = sqlx::query(
        "SELECT reason_code, payload_bytes FROM boat_ready_dead_letters
         WHERE source = 'consumer' ORDER BY observed_at DESC LIMIT 1",
    )
    .fetch_one(pool)
    .await?;
    assert_eq!(dead.try_get::<String, _>("reason_code")?, "private_payload");
    assert!(dead.try_get::<i32, _>("payload_bytes")? > 0);

    let _ = insert_memory(pool, "paper-boat", "courier-proof", "one lease").await?;
    let owner_a = Uuid::new_v4();
    let owner_b = Uuid::new_v4();
    let store_a = Store::from_pool(pool.clone());
    let store_b = Store::from_pool(pool.clone());
    let (a, b) = tokio::join!(store_a.claim_next(owner_a), store_b.claim_next(owner_b));
    let claimed = [a?.is_some(), b?.is_some()]
        .into_iter()
        .filter(|claimed| *claimed)
        .count();
    assert_eq!(claimed, 1);

    let parsed = BoatReadyEvent::parse(&serde_json::to_vec(&payload)?)?;
    assert_eq!(parsed.record_id_i64(), memory_id);
    Ok(())
}

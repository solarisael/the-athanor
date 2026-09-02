use futures_util::StreamExt;
use origami::cranes::{
    broker::{
        BOAT_READY_CONSUMER_NAME, BoatReceiptProjection, Broker, CRANE_CONSUMER_NAME,
        RECEIPT_STREAM_NAME, RECEIPT_SUBJECT,
    },
    delivery::{ConsumeOutcome, DeliveryService, PublishOutcome},
    envelope::{BOAT_READY_EVENT_KIND, CraneEvent},
    outbox::Store,
};
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
    migration!("0017_crane_delivery.sql"),
];
const BOAT_READY_MIGRATION: usize = 15;
const CRANE_MIGRATION: usize = 16;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn isolated_database_url() -> String {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("dedicated test database URL is required");
    let lower = url.to_ascii_lowercase();
    assert!(!lower.contains("solarisael_memory"));
    assert!(!lower.contains("solarisael-house"));
    url
}

fn nats_url() -> String {
    std::env::var("ATHANOR_DELIVERY_TEST_NATS_URL")
        .expect("a test-owned NATS 2.14.4 JetStream endpoint is required")
}

/// Runs `proof` against a private schema of the dedicated test database, dropping
/// the schema afterwards whether the proof succeeded or failed.
async fn in_isolated_schema<F, Fut>(proof: F) -> TestResult
where
    F: FnOnce(PgPool) -> Fut,
    Fut: Future<Output = TestResult>,
{
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

    let outcome = match pool {
        Ok(pool) => {
            let result = proof(pool.clone()).await;
            pool.close().await;
            result
        }
        Err(error) => Err(error.into()),
    };
    let cleanup = sqlx::query(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
        .execute(&admin)
        .await;
    admin.close().await;

    match (outcome, cleanup) {
        (Ok(()), Ok(_)) => Ok(()),
        (Err(proof), Ok(_)) => Err(proof),
        (Ok(()), Err(cleanup)) => Err(cleanup.into()),
        (Err(proof), Err(cleanup)) => {
            Err(format!("delivery proof failed: {proof}; schema cleanup failed: {cleanup}").into())
        }
    }
}

async fn apply_migrations(pool: &PgPool) -> TestResult {
    for migration in MIGRATIONS {
        sqlx::raw_sql(migration).execute(pool).await?;
    }
    Ok(())
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
    let row = sqlx::query("SELECT event_id, payload FROM crane_outbox WHERE aggregate_id = $1")
        .bind(memory_id)
        .fetch_one(pool)
        .await?;
    Ok((row.try_get("event_id")?, row.try_get("payload")?))
}

/// Enqueues an addressed Crane exactly as a future room-addressed producer would:
/// PostgreSQL columns and the strict envelope agree, and the row is unclaimed.
async fn enqueue_addressed_crane(
    pool: &PgPool,
    aggregate_id: i64,
    room: &str,
    recipient_kind: &str,
    recipient_key: &str,
    expires_at: Option<&str>,
) -> TestResult<Uuid> {
    let event_id = Uuid::new_v4();
    let mut payload = json!({
        "schema_version": 1,
        "event_id": event_id.to_string(),
        "event_kind": "crane.letter",
        "record_id": aggregate_id.to_string(),
        "room": room,
        "created_at": "2026-08-14T12:00:00.000000Z",
        "integrity_sha256": "a".repeat(64),
        "crease_pattern": "letter.v1",
        "recipient_kind": recipient_kind,
        "recipient_key": recipient_key,
    });
    if let Some(expires_at) = expires_at {
        payload["expires_at"] = json!(expires_at);
    }
    sqlx::query(
        "INSERT INTO crane_outbox (
           event_id, idempotency_key, event_kind, aggregate_kind, aggregate_id, room,
           integrity_sha256, crease_pattern, recipient_kind, recipient_key, expires_at, payload
         ) VALUES ($1, $2, 'crane.letter', 'memory', $3, $4, $5, 'letter.v1', $6, $7, $8, $9)",
    )
    .bind(event_id)
    .bind(format!(
        "crane.letter:{recipient_kind}:{recipient_key}:{event_id}"
    ))
    .bind(aggregate_id)
    .bind(room)
    .bind("a".repeat(64))
    .bind(recipient_kind)
    .bind(recipient_key)
    .bind(expires_at.map(|value| {
        value
            .parse::<chrono::DateTime<chrono::Utc>>()
            .expect("rfc3339")
    }))
    .bind(&payload)
    .execute(pool)
    .await?;
    Ok(event_id)
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database"]
async fn crane_migration_applies_twice_and_preserves_the_boat_trio() -> TestResult {
    in_isolated_schema(|pool| async move {
        apply_migrations(&pool).await?;

        let memory_id =
            insert_memory(&pool, "paper-boat", "courier-proof", "pavement body").await?;
        let before = sqlx::query(
            "SELECT event_id, idempotency_key, event_kind, aggregate_kind, room,
                    integrity_sha256, crease_pattern, payload, state
             FROM crane_outbox WHERE aggregate_id = $1",
        )
        .bind(memory_id)
        .fetch_one(&pool)
        .await?;
        let before: (
            Uuid,
            String,
            String,
            String,
            String,
            String,
            String,
            Value,
            String,
        ) = (
            before.try_get("event_id")?,
            before.try_get("idempotency_key")?,
            before.try_get("event_kind")?,
            before.try_get("aggregate_kind")?,
            before.try_get("room")?,
            before.try_get("integrity_sha256")?,
            before.try_get("crease_pattern")?,
            before.try_get("payload")?,
            before.try_get("state")?,
        );
        assert_eq!(before.1, format!("boat.ready:memory:{memory_id}"));
        assert_eq!(before.2, BOAT_READY_EVENT_KIND);
        assert_eq!(before.6, "boat.ready.v1", "existing boat rows are creased");

        // The supported replay: the delivery migrations are applied again, in order.
        for index in [BOAT_READY_MIGRATION, CRANE_MIGRATION] {
            sqlx::raw_sql(MIGRATIONS[index]).execute(&pool).await?;
        }
        sqlx::raw_sql(MIGRATIONS[CRANE_MIGRATION])
            .execute(&pool)
            .await?;

        let after = sqlx::query(
            "SELECT event_id, idempotency_key, event_kind, aggregate_kind, room,
                    integrity_sha256, crease_pattern, payload, state
             FROM crane_outbox WHERE aggregate_id = $1",
        )
        .bind(memory_id)
        .fetch_one(&pool)
        .await?;
        let after: (
            Uuid,
            String,
            String,
            String,
            String,
            String,
            String,
            Value,
            String,
        ) = (
            after.try_get("event_id")?,
            after.try_get("idempotency_key")?,
            after.try_get("event_kind")?,
            after.try_get("aggregate_kind")?,
            after.try_get("room")?,
            after.try_get("integrity_sha256")?,
            after.try_get("crease_pattern")?,
            after.try_get("payload")?,
            after.try_get("state")?,
        );
        assert_eq!(
            before, after,
            "reapplying the delivery migrations must not touch an existing boat row"
        );

        let surviving_legacy: Vec<String> = sqlx::query_scalar(
            "SELECT table_name::text FROM information_schema.tables
             WHERE table_schema = current_schema() AND table_name LIKE 'boat\\_ready\\_%'",
        )
        .fetch_all(&pool)
        .await?;
        assert!(
            surviving_legacy.is_empty(),
            "no boat_ready_* table survives the widened road: {surviving_legacy:?}"
        );
        let legacy_names: Vec<String> = sqlx::query_scalar(
            "SELECT constraint_name::text FROM information_schema.table_constraints
             WHERE constraint_schema = current_schema()
               AND constraint_name LIKE 'boat\\_ready\\_%'",
        )
        .fetch_all(&pool)
        .await?;
        assert!(
            legacy_names.is_empty(),
            "stale constraint names: {legacy_names:?}"
        );

        // A boat that sleeps after the replay still enqueues with the same shape.
        let replayed =
            insert_memory(&pool, "paper-boat", "courier-proof", "post-replay body").await?;
        let row = sqlx::query(
            "SELECT idempotency_key, event_kind, crease_pattern FROM crane_outbox
             WHERE aggregate_id = $1",
        )
        .bind(replayed)
        .fetch_one(&pool)
        .await?;
        assert_eq!(
            row.try_get::<String, _>("idempotency_key")?,
            format!("boat.ready:memory:{replayed}")
        );
        assert_eq!(
            row.try_get::<String, _>("event_kind")?,
            BOAT_READY_EVENT_KIND
        );
        assert_eq!(row.try_get::<String, _>("crease_pattern")?, "boat.ready.v1");

        // The widened dead-letter vocabulary is storable, and the boat lane keeps its
        // exact envelope while addressed lanes may carry the new optional fields.
        for reason in ["expired", "recipient_mismatch"] {
            sqlx::query(
                "INSERT INTO crane_dead_letters (
                   event_id, source, subject, reason_code, reason, payload_sha256, payload_bytes
                 ) VALUES (NULL, 'consumer', 'athanor.crane.room.kodo', $1, 'proof', $2, 7)",
            )
            .bind(reason)
            .bind("b".repeat(64))
            .execute(&pool)
            .await?;
        }
        enqueue_addressed_crane(&pool, memory_id, "courier-proof", "room", "kodo", None).await?;
        let widened_boat = sqlx::query(
            "INSERT INTO crane_outbox (
               event_id, idempotency_key, event_kind, aggregate_kind, aggregate_id, room,
               integrity_sha256, correlation_id, payload
             ) VALUES (gen_random_uuid(), 'boat.ready:memory:99999', 'boat.ready', 'memory', $1,
                       'courier-proof', $2, gen_random_uuid(), '{}'::jsonb)",
        )
        .bind(memory_id)
        .bind("a".repeat(64))
        .execute(&pool)
        .await;
        assert!(
            widened_boat.is_err(),
            "the boat.ready lane keeps its exact seven-key envelope"
        );

        // An addressed lane that names no recipient would be unroutable, so the
        // authority refuses it outright.
        let unroutable = sqlx::query(
            "INSERT INTO crane_outbox (
               event_id, idempotency_key, event_kind, aggregate_kind, aggregate_id, room,
               integrity_sha256, payload
             ) VALUES (gen_random_uuid(), 'crane.letter:unroutable', 'crane.letter', 'memory', $1,
                       'courier-proof', $2, '{}'::jsonb)",
        )
        .bind(memory_id)
        .bind("a".repeat(64))
        .execute(&pool)
        .await;
        assert!(
            unroutable.is_err(),
            "every lane past boat.ready must name its recipient"
        );

        // A refusal recorded before routing has no subject to name.
        sqlx::query(
            "INSERT INTO crane_dead_letters (
               event_id, source, subject, reason_code, reason, payload_sha256, payload_bytes
             ) VALUES (NULL, 'publisher', NULL, 'malformed_payload', 'never routed', $1, 3)",
        )
        .bind("c".repeat(64))
        .execute(&pool)
        .await?;
        Ok(())
    })
    .await
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database and test-owned NATS 2.14.4 JetStream process"]
async fn crane_outbox_restart_dedup_receipt_replay_expiry_and_poison_contract() -> TestResult {
    in_isolated_schema(|pool| async move {
        apply_migrations(&pool).await?;
        run_contract(&pool).await
    })
    .await
}

async fn run_contract(pool: &PgPool) -> TestResult {
    let unrelated = insert_memory(pool, "memory", "courier-proof", "not a boat").await?;
    let unrelated_events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM crane_outbox WHERE aggregate_id = $1")
            .bind(unrelated)
            .fetch_one(pool)
            .await?;
    assert_eq!(unrelated_events, 0, "only exact paper-boat inserts enqueue");

    for index in [BOAT_READY_MIGRATION, CRANE_MIGRATION] {
        sqlx::raw_sql(MIGRATIONS[index]).execute(pool).await?;
    }
    let unrelated_survived: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM memories WHERE id = $1)")
            .bind(unrelated)
            .fetch_one(pool)
            .await?;
    assert!(
        unrelated_survived,
        "reapplying migrations preserves unknown rows"
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
        sqlx::query_scalar("SELECT count(*) FROM crane_outbox WHERE aggregate_id = $1")
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
    assert_eq!(payload["event_kind"], BOAT_READY_EVENT_KIND);
    assert_eq!(payload["record_id"], memory_id.to_string());

    let broker = Broker::connect(&nats_url()).await?;
    let receipt_client = async_nats::connect(nats_url()).await?;
    let mut receipt_messages = receipt_client.subscribe(RECEIPT_SUBJECT).await?;
    let mut boat_subjects = receipt_client.subscribe("athanor.boat.ready").await?;
    broker.configure().await?;
    let first_owner = Uuid::new_v4();
    let first = DeliveryService::new(Store::from_pool(pool.clone()), broker.clone(), first_owner);
    assert_eq!(first.publish_once().await?, PublishOutcome::Published);
    let boat_message = tokio::time::timeout(Duration::from_secs(2), boat_subjects.next())
        .await?
        .expect("the boat lane keeps publishing to athanor.boat.ready");
    assert_eq!(boat_message.subject.as_str(), "athanor.boat.ready");
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
    let ledger_row = sqlx::query(
        "SELECT consumer_name, event_kind, aggregate_id, room, integrity_sha256,
                stream_sequence, first_delivery_count
         FROM crane_receipts WHERE event_id = $1",
    )
    .bind(event_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(
        ledger_row.try_get::<String, _>("consumer_name")?,
        BOAT_READY_CONSUMER_NAME
    );
    assert_eq!(
        ledger_row.try_get::<String, _>("event_kind")?,
        BOAT_READY_EVENT_KIND
    );
    assert_eq!(ledger_row.try_get::<i64, _>("aggregate_id")?, memory_id);
    assert_eq!(ledger_row.try_get::<String, _>("room")?, "courier-proof");
    assert!(ledger_row.try_get::<i64, _>("stream_sequence")? > 0);
    assert_eq!(ledger_row.try_get::<i32, _>("first_delivery_count")?, 1);
    let receipt_context = async_nats::jetstream::new(receipt_client.clone());
    let mut receipt_stream = receipt_context.get_stream(RECEIPT_STREAM_NAME).await?;
    let receipt_messages_before_replay = receipt_stream.info().await?.state.messages;

    broker
        .publish(
            "athanor.boat.ready".to_owned(),
            Uuid::new_v4(),
            serde_json::to_vec(&payload)?,
        )
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
        sqlx::query_scalar("SELECT count(*) FROM crane_receipts WHERE event_id = $1")
            .bind(event_id)
            .fetch_one(pool)
            .await?;
    assert_eq!(receipts, 1);

    let restart_memory = insert_memory(pool, "paper-boat", "courier-proof", "restart body").await?;
    let (restart_event_id, restart_payload) = next_outbox(pool, restart_memory).await?;
    let claimed = Store::from_pool(pool.clone())
        .claim_next(first_owner)
        .await?
        .expect("restart event is claimable");
    assert_eq!(claimed.event_id, restart_event_id);
    assert_eq!(claimed.subject().as_deref(), Some("athanor.boat.ready"));
    let first_sequence = broker
        .publish(
            "athanor.boat.ready".to_owned(),
            restart_event_id,
            serde_json::to_vec(&restart_payload)?,
        )
        .await?;
    sqlx::query(
        "UPDATE crane_outbox SET lease_expires_at = NOW() - interval '1 second'
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
        .publish(
            "athanor.boat.ready".to_owned(),
            restart_event_id,
            serde_json::to_vec(&restart_payload)?,
        )
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
        .publish(
            "athanor.boat.ready".to_owned(),
            Uuid::new_v4(),
            serde_json::to_vec(&private)?,
        )
        .await?;
    assert_eq!(
        restarted.consume_once(Duration::from_secs(2)).await?,
        ConsumeOutcome::DeadLettered
    );
    let dead = sqlx::query(
        "SELECT reason_code, subject, payload_bytes FROM crane_dead_letters
         WHERE source = 'consumer' ORDER BY observed_at DESC LIMIT 1",
    )
    .fetch_one(pool)
    .await?;
    assert_eq!(dead.try_get::<String, _>("reason_code")?, "private_payload");
    assert_eq!(
        dead.try_get::<Option<String>, _>("subject")?.as_deref(),
        Some("athanor.boat.ready")
    );
    assert!(dead.try_get::<i32, _>("payload_bytes")? > 0);

    // --- the second lane: a room-addressed Crane on the same widened road ---

    let mut addressed_subjects = receipt_client.subscribe("athanor.crane.room.kodo").await?;
    let addressed_event_id =
        enqueue_addressed_crane(pool, memory_id, "courier-proof", "room", "kodo", None).await?;
    assert_eq!(restarted.publish_once().await?, PublishOutcome::Published);
    let addressed_message = tokio::time::timeout(Duration::from_secs(2), addressed_subjects.next())
        .await?
        .expect("an addressed Crane must reach its recipient subject");
    assert_eq!(
        addressed_message.subject.as_str(),
        "athanor.crane.room.kodo"
    );
    let parsed = CraneEvent::parse(&addressed_message.payload)?;
    assert_eq!(parsed.event_id, addressed_event_id);
    assert_eq!(parsed.lane().subject(), "athanor.crane.room.kodo");
    assert_eq!(parsed.crease_pattern.as_deref(), Some("letter.v1"));

    let receipts_before_addressed = receipt_stream.info().await?.state.messages;
    assert_eq!(
        restarted.consume_once(Duration::from_secs(2)).await?,
        ConsumeOutcome::Received
    );
    let addressed_receipt = sqlx::query(
        "SELECT consumer_name, event_kind, room FROM crane_receipts WHERE event_id = $1",
    )
    .bind(addressed_event_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(
        addressed_receipt.try_get::<String, _>("consumer_name")?,
        CRANE_CONSUMER_NAME
    );
    assert_eq!(
        addressed_receipt.try_get::<String, _>("event_kind")?,
        "crane.letter"
    );
    assert_eq!(
        addressed_receipt.try_get::<String, _>("room")?,
        "courier-proof"
    );
    assert_eq!(
        receipt_stream.info().await?.state.messages,
        receipts_before_addressed,
        "addressed lanes commit receipts without touching the boat receipt projection"
    );

    // An expired Crane is dead-lettered before the ledger, never applied.
    let expired_event_id = enqueue_addressed_crane(
        pool,
        memory_id,
        "courier-proof",
        "room",
        "kodo",
        Some("2026-08-14T12:00:01Z"),
    )
    .await?;
    assert_eq!(restarted.publish_once().await?, PublishOutcome::Published);
    assert_eq!(
        restarted.consume_once(Duration::from_secs(2)).await?,
        ConsumeOutcome::DeadLettered
    );
    let expired =
        sqlx::query("SELECT reason_code, subject FROM crane_dead_letters WHERE event_id = $1")
            .bind(expired_event_id)
            .fetch_one(pool)
            .await?;
    assert_eq!(expired.try_get::<String, _>("reason_code")?, "expired");
    assert_eq!(
        expired.try_get::<Option<String>, _>("subject")?.as_deref(),
        Some("athanor.crane.room.kodo")
    );
    let expired_receipts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM crane_receipts WHERE event_id = $1")
            .bind(expired_event_id)
            .fetch_one(pool)
            .await?;
    assert_eq!(
        expired_receipts, 0,
        "an expired Crane never reaches the ledger"
    );

    // A Crane delivered on someone else's recipient subject is refused by name. It is
    // published straight onto the wire, so no unclaimed outbox row is left behind.
    let misrouted = Uuid::new_v4();
    let misrouted_payload = json!({
        "schema_version": 1,
        "event_id": misrouted.to_string(),
        "event_kind": "crane.letter",
        "record_id": memory_id.to_string(),
        "room": "courier-proof",
        "created_at": "2026-08-14T12:00:00.000000Z",
        "integrity_sha256": "a".repeat(64),
        "crease_pattern": "letter.v1",
        "recipient_kind": "room",
        "recipient_key": "elsewhere",
    });
    broker
        .publish(
            "athanor.crane.room.kodo".to_owned(),
            misrouted,
            serde_json::to_vec(&misrouted_payload)?,
        )
        .await?;
    assert_eq!(
        restarted.consume_once(Duration::from_secs(2)).await?,
        ConsumeOutcome::DeadLettered
    );
    let mismatch = sqlx::query("SELECT reason_code FROM crane_dead_letters WHERE event_id = $1")
        .bind(misrouted)
        .fetch_one(pool)
        .await?;
    assert_eq!(
        mismatch.try_get::<String, _>("reason_code")?,
        "recipient_mismatch"
    );

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

    let parsed = CraneEvent::parse(&serde_json::to_vec(&payload)?)?;
    assert_eq!(parsed.record_id_i64(), memory_id);
    Ok(())
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database and test-owned NATS 2.14.4 JetStream process"]
async fn serve_delivers_until_cancelled_and_leaves_no_lease_behind() -> TestResult {
    in_isolated_schema(|pool| async move {
        apply_migrations(&pool).await?;
        let cancellation = tokio_util::sync::CancellationToken::new();
        let cranes = tokio::spawn(DeliveryService::serve(
            Store::from_pool(pool.clone()),
            nats_url(),
            cancellation.clone(),
        ));

        let memory_id = insert_memory(&pool, "paper-boat", "courier-proof", "served").await?;
        let (event_id, _) = next_outbox(&pool, memory_id).await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            let receipts: i64 =
                sqlx::query_scalar("SELECT count(*) FROM crane_receipts WHERE event_id = $1")
                    .bind(event_id)
                    .fetch_one(&pool)
                    .await?;
            if receipts == 1 {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "serve must publish and record the boat without help"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let state: String = sqlx::query_scalar("SELECT state FROM crane_outbox WHERE event_id = $1")
            .bind(event_id)
            .fetch_one(&pool)
            .await?;
        assert_eq!(state, "published");

        cancellation.cancel();
        tokio::time::timeout(Duration::from_secs(5), cranes)
            .await
            .expect("serve exits after cancellation")?;
        let leased: i64 = sqlx::query_scalar("SELECT count(*) FROM crane_outbox WHERE state = 'leased'")
            .fetch_one(&pool)
            .await?;
        assert_eq!(leased, 0, "cancellation lands on a tick boundary, never mid-claim");
        Ok(())
    })
    .await
}

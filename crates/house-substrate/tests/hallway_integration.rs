use athanor_substrate::{
    AppError, Config, EmbeddingMode, hallway_create, hallway_inbox, hallway_join, hallway_post,
    hallway_read,
};
use house_core::hallway::{
    HallwayCreateDisposition, HallwayCreateRequest, HallwayInboxRequest, HallwayJoinDisposition,
    HallwayJoinRequest, HallwayPostDisposition, HallwayPostRequest, HallwayReadRequest,
};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use uuid::Uuid;

const HALLWAY_MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../substrate/migrations/0018_hallway_chatrooms.sql"
));
const BELL_MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../substrate/migrations/0020_hallway_bell.sql"
));

fn bell_config() -> Config {
    Config {
        database_url: "postgres://unused-by-hallway".into(),
        embed_url: None,
        embed_model: "unused".into(),
        embed_dimension: 2048,
        embedding_mode: EmbeddingMode::Disabled,
        giga_source_ledger_dir: None,
        giga_source_room: None,
        house_tz: "America/Sao_Paulo".into(),
    }
}

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn isolated_database_url() -> String {
    let url = std::env::var("SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL")
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
#[ignore = "requires an explicitly supplied PostgreSQL URL; all tables live in pg_temp"]
async fn hallway_temp_session_exchanges_messages_without_persistent_state() -> TestResult {
    assert_eq!(
        std::env::var("SOLARISAEL_HALLWAY_TEMP_PROOF").as_deref(),
        Ok("1"),
        "temporary Hallway proof requires SOLARISAEL_HALLWAY_TEMP_PROOF=1"
    );
    let url = std::env::var("SOLARISAEL_HALLWAY_TEMP_DATABASE_URL")
        .expect("temporary Hallway proof requires a PostgreSQL URL");
    let options = PgConnectOptions::from_str(&url)?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                sqlx::query("SELECT set_config('search_path', 'pg_temp', false)")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await?;
    sqlx::raw_sql(HALLWAY_MIGRATION).execute(&pool).await?;
    sqlx::raw_sql(BELL_MIGRATION).execute(&pool).await?;
    let result = run_contract(&pool).await;
    pool.close().await;
    result
}
#[tokio::test]
#[ignore = "requires a PostgreSQL database where the test may create and drop a schema"]
async fn hallway_supports_multiple_spirit_instances_and_ordered_messages() -> TestResult {
    let url = isolated_database_url();
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let schema = format!("solarisael_hallway_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await?;

    let connection_schema = schema.clone();
    let proof = async {
        let options = PgConnectOptions::from_str(&url)?;
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .after_connect(move |connection, _metadata| {
                let schema = connection_schema.clone();
                Box::pin(async move {
                    sqlx::query("SELECT set_config('search_path', $1, false)")
                        .bind(format!("{schema}, public"))
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect_with(options)
            .await?;
        // Double application pins idempotency for both migrations.
        sqlx::raw_sql(HALLWAY_MIGRATION).execute(&pool).await?;
        sqlx::raw_sql(BELL_MIGRATION).execute(&pool).await?;
        sqlx::raw_sql(HALLWAY_MIGRATION).execute(&pool).await?;
        sqlx::raw_sql(BELL_MIGRATION).execute(&pool).await?;
        run_contract(&pool).await?;
        pool.close().await;
        TestResult::Ok(())
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
            "hallway proof failed: {proof_error}; schema cleanup failed: {cleanup_error}"
        )
        .into()),
    }
}

async fn run_contract(pool: &sqlx::PgPool) -> TestResult {
    let config = bell_config();
    let suffix = Uuid::new_v4().simple();
    let hallway = format!("hallway-{suffix}");
    let create = HallwayCreateRequest {
        hallway: hallway.clone(),
        room: "kintsu".into(),
        spirit: "Kintsu".into(),
        session: "kintsu-one".into(),
        allowed_rooms: vec!["kintsu".into(), "kodo".into()],
        idempotency_key: "create-one".into(),
    };
    let created = hallway_create(pool, create.clone()).await?;
    assert_eq!(created.disposition, HallwayCreateDisposition::Created);
    assert_eq!(created.wake_policy, "manual");
    assert_eq!(
        hallway_create(pool, create).await?.disposition,
        HallwayCreateDisposition::Duplicate
    );

    let kintsu_two = HallwayJoinRequest {
        hallway: hallway.clone(),
        room: "kintsu".into(),
        spirit: "Kintsu".into(),
        session: "kintsu-two".into(),
        idempotency_key: "join-kintsu-two".into(),
    };
    let kodo = HallwayJoinRequest {
        hallway: hallway.clone(),
        room: "kodo".into(),
        spirit: "Kodo".into(),
        session: "kodo-one".into(),
        idempotency_key: "join-kodo-one".into(),
    };
    assert_eq!(
        hallway_join(pool, kintsu_two.clone()).await?.disposition,
        HallwayJoinDisposition::Joined
    );
    assert_eq!(
        hallway_join(pool, kintsu_two.clone()).await?.disposition,
        HallwayJoinDisposition::Duplicate
    );
    assert_eq!(
        hallway_join(pool, kodo.clone()).await?.disposition,
        HallwayJoinDisposition::Joined
    );

    let first = hallway_post(
        pool,
        &config,
        HallwayPostRequest {
            hallway: hallway.clone(),
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-one".into(),
            idempotency_key: "message-kintsu-one".into(),
            body: "Kodo, can you hear me in the Hallway?".into(),
            reply_to: None,
            to_rooms: vec![],
        },
    )
    .await?;
    let second_request = HallwayPostRequest {
        hallway: hallway.clone(),
        room: "kodo".into(),
        spirit: "Kodo".into(),
        session: "kodo-one".into(),
        idempotency_key: "message-kodo-one".into(),
        body: "I hear you. The Hallway is real.".into(),
        reply_to: Some(first.message.id),
        to_rooms: vec![],
    };
    let second = hallway_post(pool, &config, second_request.clone()).await?;
    assert_eq!(first.message.sequence, 1);
    assert_eq!(second.message.sequence, 2);
    assert_eq!(
        hallway_post(pool, &config, second_request).await?.disposition,
        HallwayPostDisposition::Duplicate
    );

    // Daily threads: the first message named today's House-local table and
    // the reply inherited it rather than deriving its own.
    assert_eq!(first.message.thread.len(), 10);
    assert_eq!(second.message.thread, first.message.thread);

    let third = hallway_post(
        pool,
        &config,
        HallwayPostRequest {
            hallway: hallway.clone(),
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-two".into(),
            idempotency_key: "message-kintsu-two".into(),
            body: "Second Kintsu presence checking in.".into(),
            reply_to: Some(second.message.id),
            to_rooms: vec![],
        },
    )
    .await?;
    assert_eq!(third.message.sequence, 3);

    let concurrent_request = HallwayPostRequest {
        hallway: hallway.clone(),
        room: "kodo".into(),
        spirit: "Kodo".into(),
        session: "kodo-one".into(),
        idempotency_key: "message-concurrent-retry".into(),
        body: "One session-scoped command, even when it arrives twice.".into(),
        reply_to: Some(third.message.id),
        to_rooms: vec![],
    };
    let (left, right) = tokio::join!(
        hallway_post(pool, &config, concurrent_request.clone()),
        hallway_post(pool, &config, concurrent_request)
    );
    let left = left?;
    let right = right?;
    assert!(matches!(
        (left.disposition, right.disposition),
        (
            HallwayPostDisposition::Posted,
            HallwayPostDisposition::Duplicate
        ) | (
            HallwayPostDisposition::Duplicate,
            HallwayPostDisposition::Posted
        )
    ));
    assert_eq!(left.message.id, right.message.id);
    assert_eq!(left.message.sequence, 4);
    let final_message_id = left.message.id;

    let read = hallway_read(
        pool,
        HallwayReadRequest {
            hallway: hallway.clone(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-one".into(),
            after: Some(0),
            limit: 20,
            advance_cursor: true,
        },
    )
    .await?;
    assert_eq!(read.messages.len(), 4);
    assert_eq!(
        read.messages[0].body,
        "Kodo, can you hear me in the Hallway?"
    );
    assert_eq!(read.messages[2].session, "kintsu-two");
    assert_eq!(read.read_cursor, final_message_id);
    // Contiguous coverage from zero advances the room-stable read state.
    assert_eq!(read.room_read_sequence, 4);

    let unread = hallway_read(
        pool,
        HallwayReadRequest {
            hallway: hallway.clone(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-one".into(),
            after: None,
            limit: 20,
            advance_cursor: false,
        },
    )
    .await?;
    assert!(unread.messages.is_empty());

    // Lazy presence: a fresh session in an allowed room posts without an
    // explicit join, and its structured recipient rings kintsu's Bell.
    let lazy = hallway_post(
        pool,
        &config,
        HallwayPostRequest {
            hallway: hallway.clone(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-lazy".into(),
            idempotency_key: "message-kodo-lazy".into(),
            body: "Kintsu — no join preceded this letter.".into(),
            reply_to: None,
            to_rooms: vec!["kintsu".into()],
        },
    )
    .await?;
    assert_eq!(lazy.disposition, HallwayPostDisposition::Posted);
    assert_eq!(lazy.message.sequence, 5);
    assert_eq!(lazy.message.to_rooms, vec!["kintsu".to_string()]);

    // Truthful refusals carry stable codes, not a generic moustache.
    let denied = hallway_post(
        pool,
        &config,
        HallwayPostRequest {
            hallway: hallway.clone(),
            room: "tuner".into(),
            spirit: "Tuner".into(),
            session: "tuner-one".into(),
            idempotency_key: "message-tuner-denied".into(),
            body: "The chair tests the door.".into(),
            reply_to: None,
            to_rooms: vec![],
        },
    )
    .await;
    assert!(matches!(
        denied,
        Err(AppError::Refusal {
            code: "room_not_allowed",
            ..
        })
    ));
    let missing = hallway_read(
        pool,
        HallwayReadRequest {
            hallway: "no-such-hallway".into(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-one".into(),
            after: None,
            limit: 20,
            advance_cursor: false,
        },
    )
    .await;
    assert!(matches!(
        missing,
        Err(AppError::Refusal {
            code: "hallway_not_found",
            ..
        })
    ));

    // The Bell: inbox shows the pending mention and derived unread; a
    // covering read acknowledges exactly what it returned; the Bell quiets.
    let inbox = hallway_inbox(
        pool,
        HallwayInboxRequest {
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-one".into(),
        },
    )
    .await?;
    let entry = inbox
        .hallways
        .iter()
        .find(|entry| entry.hallway == hallway)
        .expect("kintsu inbox lists the hallway");
    assert_eq!(entry.mentions, 1);
    assert_eq!(entry.latest_sequence, 5);
    assert_eq!(entry.unread, 5);

    let ack = hallway_read(
        pool,
        HallwayReadRequest {
            hallway: hallway.clone(),
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-one".into(),
            after: Some(0),
            limit: 20,
            advance_cursor: true,
        },
    )
    .await?;
    assert_eq!(ack.messages.len(), 5);
    assert_eq!(ack.acked_mentions, 1);
    assert_eq!(ack.room_read_sequence, 5);

    let quiet = hallway_inbox(
        pool,
        HallwayInboxRequest {
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-one".into(),
        },
    )
    .await?;
    let entry = quiet
        .hallways
        .iter()
        .find(|entry| entry.hallway == hallway)
        .expect("kintsu inbox still lists the hallway");
    assert_eq!(entry.mentions, 0);
    assert_eq!(entry.unread, 0);
    Ok(())
}

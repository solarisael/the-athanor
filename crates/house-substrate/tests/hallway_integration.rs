use athanor_substrate::{hallway_create, hallway_join, hallway_post, hallway_read};
use house_core::hallway::{
    HallwayCreateRequest, HallwayJoinRequest, HallwayPostRequest, HallwayReadRequest,
};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use uuid::Uuid;

const HALLWAY_MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../substrate/migrations/0018_hallway_chatrooms.sql"
));

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
        sqlx::raw_sql(HALLWAY_MIGRATION).execute(&pool).await?;
        sqlx::raw_sql(HALLWAY_MIGRATION).execute(&pool).await?;
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
    assert!(created.created);
    assert_eq!(created.wake_policy, "manual");
    assert!(hallway_create(pool, create).await?.duplicate);

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
    assert!(hallway_join(pool, kintsu_two.clone()).await?.joined);
    assert!(hallway_join(pool, kintsu_two.clone()).await?.duplicate);
    assert!(hallway_join(pool, kodo.clone()).await?.joined);

    let first = hallway_post(
        pool,
        HallwayPostRequest {
            hallway: hallway.clone(),
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-one".into(),
            idempotency_key: "message-kintsu-one".into(),
            body: "Kodo, can you hear me in the Hallway?".into(),
            reply_to: None,
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
    };
    let second = hallway_post(pool, second_request.clone()).await?;
    assert_eq!(first.message.sequence, 1);
    assert_eq!(second.message.sequence, 2);
    assert!(hallway_post(pool, second_request).await?.duplicate);

    let third = hallway_post(
        pool,
        HallwayPostRequest {
            hallway: hallway.clone(),
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-two".into(),
            idempotency_key: "message-kintsu-two".into(),
            body: "Second Kintsu presence checking in.".into(),
            reply_to: Some(second.message.id),
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
    };
    let (left, right) = tokio::join!(
        hallway_post(pool, concurrent_request.clone()),
        hallway_post(pool, concurrent_request)
    );
    let left = left?;
    let right = right?;
    assert_ne!(left.duplicate, right.duplicate);
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
    for message in &read.messages {
        println!(
            "[{}] {}@{}: {}",
            message.sequence, message.spirit, message.session, message.body
        );
    }

    let unread = hallway_read(
        pool,
        HallwayReadRequest {
            hallway,
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
    Ok(())
}

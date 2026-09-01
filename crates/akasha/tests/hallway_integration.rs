use akasha::{
    AppError, Config, EmbeddingMode, hallway_create, hallway_inbox, hallway_join, hallway_knock,
    hallway_knock_claim, hallway_knock_policy, hallway_knock_settle, hallway_post, hallway_read,
};
use hearth::hallway::{
    HallwayCreateDisposition, HallwayCreateRequest, HallwayInboxRequest, HallwayJoinDisposition,
    HallwayJoinRequest, HallwayKnockClaimRequest, HallwayKnockOutcome, HallwayKnockPolicyMode,
    HallwayKnockPolicyRequest, HallwayKnockRequest, HallwayKnockSettleRequest,
    HallwayPostDisposition, HallwayPostRequest, HallwayReadRequest,
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
const KNOCK_MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../substrate/migrations/0021_hallway_knock.sql"
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
#[ignore = "requires an explicitly supplied PostgreSQL URL; all tables live in pg_temp"]
async fn hallway_temp_session_exchanges_messages_without_persistent_state() -> TestResult {
    assert_eq!(
        std::env::var("ATHANOR_HALLWAY_TEMP_PROOF").as_deref(),
        Ok("1"),
        "temporary Hallway proof requires ATHANOR_HALLWAY_TEMP_PROOF=1"
    );
    let url = std::env::var("ATHANOR_HALLWAY_TEMP_DATABASE_URL")
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
    sqlx::raw_sql(KNOCK_MIGRATION).execute(&pool).await?;
    let result = async {
        run_contract(&pool).await?;
        run_knock_contract(&pool).await
    }
    .await;
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
        sqlx::raw_sql(KNOCK_MIGRATION).execute(&pool).await?;
        sqlx::raw_sql(HALLWAY_MIGRATION).execute(&pool).await?;
        sqlx::raw_sql(BELL_MIGRATION).execute(&pool).await?;
        sqlx::raw_sql(KNOCK_MIGRATION).execute(&pool).await?;
        run_contract(&pool).await?;
        run_knock_contract(&pool).await?;
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
        hallway_post(pool, &config, second_request)
            .await?
            .disposition,
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
            thread: None,
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
            thread: None,
            limit: 20,
            advance_cursor: false,
        },
    )
    .await?;
    assert!(unread.messages.is_empty());

    // Lazy presence: a fresh session in an allowed room posts without an
    // explicit join, and its structured recipient rings kintsu's Bell.
    let lazy_request = HallwayPostRequest {
        hallway: hallway.clone(),
        room: "kodo".into(),
        spirit: "Kodo".into(),
        session: "kodo-lazy".into(),
        idempotency_key: "message-kodo-lazy".into(),
        body: "Kintsu — no join preceded this letter.".into(),
        reply_to: None,
        to_rooms: vec!["kintsu".into()],
    };
    let lazy = hallway_post(pool, &config, lazy_request.clone()).await?;
    assert_eq!(lazy.disposition, HallwayPostDisposition::Posted);
    assert_eq!(lazy.message.sequence, 5);
    assert_eq!(lazy.message.to_rooms, vec!["kintsu".to_string()]);

    // A room's own post is visible history, not new mail. Excluding it from
    // unread must not jump the room cursor past older messages from peers.
    let author_inbox = hallway_inbox(
        pool,
        HallwayInboxRequest {
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-lazy".into(),
        },
    )
    .await?;
    let author_entry = author_inbox
        .hallways
        .iter()
        .find(|entry| entry.hallway == hallway)
        .expect("author inbox lists the hallway");
    assert_eq!(author_entry.latest_sequence, 5);
    assert_eq!(author_entry.unread, 0);
    let author_read_sequence: i64 = sqlx::query_scalar(
        "SELECT read_sequence FROM hallway_room_state
         WHERE hallway_id=(SELECT id FROM hallway_channels WHERE hallway_key=$1)
           AND room='kodo'",
    )
    .bind(&hallway)
    .fetch_one(pool)
    .await?;
    assert_eq!(author_read_sequence, 4);
    let changed_recipients = hallway_post(
        pool,
        &config,
        HallwayPostRequest {
            to_rooms: vec![],
            ..lazy_request
        },
    )
    .await;
    assert!(matches!(
        changed_recipients,
        Err(AppError::Refusal {
            code: "idempotency_reuse",
            ..
        })
    ));

    // Simulate the next House-local daily thread without coupling the proof to
    // wall-clock midnight. Moving sequences 3-4 creates a real filtered gap.
    let alternate_thread: i64 = sqlx::query_scalar(
        "INSERT INTO hallway_threads(hallway_id,thread_key)
         SELECT id,'2099-01-01' FROM hallway_channels WHERE hallway_key=$1
         RETURNING id",
    )
    .bind(&hallway)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "UPDATE hallway_messages SET thread_id=$1
         WHERE hallway_id=(SELECT id FROM hallway_channels WHERE hallway_key=$2)
           AND sequence IN (3,4)",
    )
    .bind(alternate_thread)
    .bind(&hallway)
    .execute(pool)
    .await?;

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
            thread: None,
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

    // The inbox exposes the exact durable Bell target. A filtered thread read
    // acknowledges only its returned rows, leaves the session-global cursor
    // put, and advances room unread state only through contiguous coverage.
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
    assert_eq!(entry.unread, 3);
    assert_eq!(entry.notifications.len(), 1);
    assert_eq!(entry.notifications[0].message_id, lazy.message.id);
    assert_eq!(entry.notifications[0].sequence, 5);
    assert_eq!(entry.notifications[0].thread, lazy.message.thread);

    let filtered = hallway_read(
        pool,
        HallwayReadRequest {
            hallway: hallway.clone(),
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-one".into(),
            after: Some(0),
            thread: Some(lazy.message.thread.clone()),
            limit: 20,
            advance_cursor: true,
        },
    )
    .await?;
    assert_eq!(
        filtered
            .messages
            .iter()
            .map(|message| message.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2, 5]
    );
    assert_eq!(filtered.acked_mentions, 1);
    assert_eq!(filtered.room_read_sequence, 2);
    assert_eq!(filtered.previous_cursor, 0);
    assert_eq!(filtered.read_cursor, 0);

    let partially_quiet = hallway_inbox(
        pool,
        HallwayInboxRequest {
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-one".into(),
        },
    )
    .await?;
    let entry = partially_quiet
        .hallways
        .iter()
        .find(|entry| entry.hallway == hallway)
        .expect("kintsu inbox still lists the hallway");
    assert_eq!(entry.mentions, 0);
    assert_eq!(entry.unread, 2);
    assert!(entry.notifications.is_empty());

    let ack = hallway_read(
        pool,
        HallwayReadRequest {
            hallway: hallway.clone(),
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-one".into(),
            after: Some(0),
            thread: None,
            limit: 20,
            advance_cursor: true,
        },
    )
    .await?;
    assert_eq!(ack.messages.len(), 5);
    assert_eq!(ack.acked_mentions, 0);
    assert_eq!(ack.room_read_sequence, 5);
    assert_eq!(ack.read_cursor, lazy.message.id);

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
    assert!(entry.notifications.is_empty());
    Ok(())
}

async fn run_knock_contract(pool: &sqlx::PgPool) -> TestResult {
    let config = bell_config();
    let hallway = format!("knock-{}", Uuid::new_v4().simple());
    hallway_create(
        pool,
        HallwayCreateRequest {
            hallway: hallway.clone(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-knock".into(),
            allowed_rooms: vec!["kodo".into(), "kintsu".into()],
            idempotency_key: "create-knock".into(),
        },
    )
    .await?;

    let kodo_policy = HallwayKnockPolicyRequest {
        hallway: hallway.clone(),
        room: "kodo".into(),
        spirit: "Kodo".into(),
        session: "kodo-knock".into(),
        idempotency_key: "policy-kodo".into(),
        mode: HallwayKnockPolicyMode::AllowList,
        allowed_rooms: vec!["kintsu".into()],
        max_turns: 2,
    };
    let kintsu_policy = HallwayKnockPolicyRequest {
        hallway: hallway.clone(),
        room: "kintsu".into(),
        spirit: "Kintsu".into(),
        session: "kintsu-knock".into(),
        idempotency_key: "policy-kintsu".into(),
        mode: HallwayKnockPolicyMode::AllowList,
        allowed_rooms: vec!["kodo".into()],
        max_turns: 2,
    };
    assert!(
        !hallway_knock_policy(pool, kodo_policy.clone())
            .await?
            .duplicate
    );
    assert!(hallway_knock_policy(pool, kodo_policy).await?.duplicate);
    hallway_knock_policy(pool, kintsu_policy).await?;

    let root_message = hallway_post(
        pool,
        &config,
        HallwayPostRequest {
            hallway: hallway.clone(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-knock".into(),
            idempotency_key: "post-root".into(),
            body: "root Knock".into(),
            reply_to: None,
            to_rooms: vec!["kintsu".into()],
        },
    )
    .await?;
    let root_request = HallwayKnockRequest {
        hallway: hallway.clone(),
        room: "kodo".into(),
        spirit: "Kodo".into(),
        session: "kodo-knock".into(),
        idempotency_key: "knock-root".into(),
        message_id: root_message.message.id,
        recipient_room: "kintsu".into(),
        parent_knock_id: None,
        max_turns: 2,
    };
    let root = hallway_knock(pool, root_request.clone()).await?;
    assert_eq!(root.knock.turn_index, 1);
    assert_eq!(root.knock.max_turns, 2);
    assert!(hallway_knock(pool, root_request).await?.duplicate);

    let repeated_message = HallwayKnockRequest {
        idempotency_key: "knock-root-again".into(),
        ..HallwayKnockRequest {
            hallway: hallway.clone(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-knock".into(),
            idempotency_key: String::new(),
            message_id: root_message.message.id,
            recipient_room: "kintsu".into(),
            parent_knock_id: None,
            max_turns: 2,
        }
    };
    assert!(matches!(
        hallway_knock(pool, repeated_message).await,
        Err(AppError::Refusal {
            code: "knock_already_requested",
            ..
        })
    ));

    let premature_message = hallway_post(
        pool,
        &config,
        HallwayPostRequest {
            hallway: hallway.clone(),
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-knock".into(),
            idempotency_key: "post-premature".into(),
            body: "parent has not started".into(),
            reply_to: Some(root_message.message.id),
            to_rooms: vec!["kodo".into()],
        },
    )
    .await?;
    assert!(matches!(
        hallway_knock(
            pool,
            HallwayKnockRequest {
                hallway: hallway.clone(),
                room: "kintsu".into(),
                spirit: "Kintsu".into(),
                session: "kintsu-knock".into(),
                idempotency_key: "knock-premature".into(),
                message_id: premature_message.message.id,
                recipient_room: "kodo".into(),
                parent_knock_id: Some(root.knock.knock_id.clone()),
                max_turns: 2,
            },
        )
        .await,
        Err(AppError::Refusal {
            code: "knock_state_conflict",
            ..
        })
    ));

    hallway_read(
        pool,
        HallwayReadRequest {
            hallway: hallway.clone(),
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-claim-one".into(),
            after: Some(0),
            thread: None,
            limit: 1,
            advance_cursor: false,
        },
    )
    .await?;
    let forged_claim = hallway_knock_claim(
        pool,
        HallwayKnockClaimRequest {
            room: "kintsu".into(),
            spirit: "Impostor".into(),
            session: "kintsu-claim-one".into(),
        },
    )
    .await;
    assert!(
        matches!(
            forged_claim,
            Err(AppError::Refusal {
                code: "spirit_mismatch",
                ..
            })
        ),
        "unexpected forged claim result: {forged_claim:?}"
    );

    let first_claim = hallway_knock_claim(
        pool,
        HallwayKnockClaimRequest {
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-claim-one".into(),
        },
    )
    .await?
    .knock
    .expect("Kintsu claims the addressed root Knock");
    let second_claim = hallway_knock_claim(
        pool,
        HallwayKnockClaimRequest {
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-claim-two".into(),
        },
    )
    .await?;
    assert!(second_claim.knock.is_none());

    let still_ringing = hallway_inbox(
        pool,
        HallwayInboxRequest {
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-claim-one".into(),
        },
    )
    .await?;
    let ringing_entry = still_ringing
        .hallways
        .iter()
        .find(|entry| entry.hallway == hallway)
        .expect("Knock claim does not hide the Hallway");
    assert_eq!(ringing_entry.mentions, 1);

    let wrong_owner = hallway_knock_settle(
        pool,
        HallwayKnockSettleRequest {
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-claim-two".into(),
            knock_id: first_claim.knock_id.clone(),
            outcome: HallwayKnockOutcome::Started,
            reason: None,
        },
    )
    .await;
    assert!(matches!(
        wrong_owner,
        Err(AppError::Refusal {
            code: "knock_state_conflict",
            ..
        })
    ));

    let start_root = HallwayKnockSettleRequest {
        room: "kintsu".into(),
        spirit: "Kintsu".into(),
        session: "kintsu-claim-one".into(),
        knock_id: first_claim.knock_id.clone(),
        outcome: HallwayKnockOutcome::Started,
        reason: None,
    };
    assert!(
        !hallway_knock_settle(pool, start_root.clone())
            .await?
            .duplicate
    );
    assert!(hallway_knock_settle(pool, start_root).await?.duplicate);
    hallway_knock_settle(
        pool,
        HallwayKnockSettleRequest {
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-claim-one".into(),
            knock_id: first_claim.knock_id.clone(),
            outcome: HallwayKnockOutcome::Completed,
            reason: None,
        },
    )
    .await?;

    let child_message = hallway_post(
        pool,
        &config,
        HallwayPostRequest {
            hallway: hallway.clone(),
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-knock".into(),
            idempotency_key: "post-child".into(),
            body: "child Knock".into(),
            reply_to: Some(root_message.message.id),
            to_rooms: vec!["kodo".into()],
        },
    )
    .await?;
    let child = hallway_knock(
        pool,
        HallwayKnockRequest {
            hallway: hallway.clone(),
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-knock".into(),
            idempotency_key: "knock-child".into(),
            message_id: child_message.message.id,
            recipient_room: "kodo".into(),
            parent_knock_id: Some(first_claim.knock_id),
            max_turns: 2,
        },
    )
    .await?;
    assert_eq!(child.knock.turn_index, 2);
    assert_eq!(child.knock.root_knock_id, root.knock.root_knock_id);

    let claimed_child = hallway_knock_claim(
        pool,
        HallwayKnockClaimRequest {
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-claim".into(),
        },
    )
    .await?
    .knock
    .expect("Kodo claims the reciprocal Knock");
    hallway_knock_settle(
        pool,
        HallwayKnockSettleRequest {
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-claim".into(),
            knock_id: claimed_child.knock_id.clone(),
            outcome: HallwayKnockOutcome::Started,
            reason: None,
        },
    )
    .await?;
    hallway_knock_settle(
        pool,
        HallwayKnockSettleRequest {
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-claim".into(),
            knock_id: claimed_child.knock_id.clone(),
            outcome: HallwayKnockOutcome::Completed,
            reason: None,
        },
    )
    .await?;

    let overflow_message = hallway_post(
        pool,
        &config,
        HallwayPostRequest {
            hallway: hallway.clone(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-knock".into(),
            idempotency_key: "post-overflow".into(),
            body: "must not wake turn three".into(),
            reply_to: Some(child_message.message.id),
            to_rooms: vec!["kintsu".into()],
        },
    )
    .await?;
    let overflow = hallway_knock(
        pool,
        HallwayKnockRequest {
            hallway: hallway.clone(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-knock".into(),
            idempotency_key: "knock-overflow".into(),
            message_id: overflow_message.message.id,
            recipient_room: "kintsu".into(),
            parent_knock_id: Some(claimed_child.knock_id),
            max_turns: 2,
        },
    )
    .await;
    assert!(matches!(
        overflow,
        Err(AppError::Refusal {
            code: "knock_exchange_exhausted",
            ..
        })
    ));

    let expiring_message = hallway_post(
        pool,
        &config,
        HallwayPostRequest {
            hallway: hallway.clone(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-knock".into(),
            idempotency_key: "post-expiring".into(),
            body: "started turn expires after recipient loss".into(),
            reply_to: None,
            to_rooms: vec!["kintsu".into()],
        },
    )
    .await?;
    let expiring_knock = hallway_knock(
        pool,
        HallwayKnockRequest {
            hallway,
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "kodo-knock".into(),
            idempotency_key: "knock-expiring".into(),
            message_id: expiring_message.message.id,
            recipient_room: "kintsu".into(),
            parent_knock_id: None,
            max_turns: 1,
        },
    )
    .await?;
    let expiring_claim = hallway_knock_claim(
        pool,
        HallwayKnockClaimRequest {
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-expiring".into(),
        },
    )
    .await?
    .knock
    .expect("recipient claims the expiring Knock");
    hallway_knock_settle(
        pool,
        HallwayKnockSettleRequest {
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-expiring".into(),
            knock_id: expiring_claim.knock_id.clone(),
            outcome: HallwayKnockOutcome::Started,
            reason: None,
        },
    )
    .await?;
    sqlx::query(
        "UPDATE hallway_knocks SET expires_at=NOW()-INTERVAL '1 second'
         WHERE knock_id=$1::uuid",
    )
    .bind(&expiring_knock.knock.knock_id)
    .execute(pool)
    .await?;
    let sweep = hallway_knock_claim(
        pool,
        HallwayKnockClaimRequest {
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            session: "kintsu-expiry-sweep".into(),
        },
    )
    .await?;
    assert!(sweep.knock.is_none());
    let expired_status = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status,settled_reason FROM hallway_knocks WHERE knock_id=$1::uuid",
    )
    .bind(&expiring_knock.knock.knock_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(expired_status.0, "failed");
    assert_eq!(
        expired_status.1.as_deref(),
        Some("recipient turn expired before completion")
    );
    Ok(())
}

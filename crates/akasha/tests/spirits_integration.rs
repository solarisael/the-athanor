use akasha::migrations::run_migrations;
use akasha::spirits::{
    SPIRITS_CHANNEL, SpiritChange, SpiritError, SpiritRecord, active_spirits, begin_rename,
    begin_summon, import_spirits, retire_spirit,
};
use sqlx::postgres::{PgConnectOptions, PgListener, PgPoolOptions};
use sqlx::PgPool;
use std::ops::AsyncFnOnce;
use std::str::FromStr;
use std::time::Duration;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn url() -> TestResult<String> {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")?;
    assert!(
        !url.contains("solarisael_memory") && !url.contains("solarisael-house"),
        "refusing a live or production-looking database"
    );
    Ok(url)
}

/// The production migration runner on one isolated schema. One connection
/// keeps the search path inside it.
async fn in_schema<T>(observe: impl AsyncFnOnce(&PgPool) -> TestResult<T>) -> TestResult<T> {
    let schema = format!("athanor_spirits_{}", uuid::Uuid::new_v4().simple());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(PgConnectOptions::from_str(&url()?)?)
        .await?;
    for statement in [
        format!("CREATE SCHEMA {schema}"),
        format!("SET search_path TO {schema}, public"),
    ] {
        sqlx::query(&statement).execute(&pool).await?;
    }
    let result = async {
        run_migrations(&pool).await?;
        observe(&pool).await
    }
    .await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await?;
    pool.close().await;

    result
}

fn record(room: &str, spirit: &str) -> SpiritRecord {
    SpiritRecord {
        room: room.into(),
        spirit: spirit.into(),
        operator: "sol".into(),
    }
}

async fn summon(pool: &PgPool, record: &SpiritRecord) -> Result<SpiritChange, SpiritError> {
    Ok(begin_summon(pool, record, "test").await?.commit().await?.change)
}

// Kills: a second summon that writes a row, a conflicting summon that
// overwrites, a retire that is not idempotent, and a revival that is lost.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn summon_and_retire_are_idempotent_and_refuse_conflicts() -> TestResult {
    in_schema(async |pool| {
        let skoll = record("skoll", "Sköll");
        assert_eq!(summon(pool, &skoll).await?, SpiritChange::Summoned);
        assert_eq!(summon(pool, &skoll).await?, SpiritChange::Unchanged);
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM spirits").fetch_one(pool).await?;
        assert_eq!(rows, 1);
        assert!(matches!(
            summon(pool, &record("skoll", "Hati")).await,
            Err(SpiritError::Refused(_))
        ));
        assert!(matches!(
            summon(pool, &record("house", "House")).await,
            Err(SpiritError::Invalid(_))
        ));

        assert_eq!(retire_spirit(pool, "skoll").await?.change, SpiritChange::Retired);
        assert_eq!(retire_spirit(pool, "skoll").await?.change, SpiritChange::Unchanged);
        assert!(active_spirits(pool).await?.is_empty());
        assert_eq!(summon(pool, &skoll).await?, SpiritChange::Revived);
        assert_eq!(active_spirits(pool).await?, [skoll.clone()]);
        assert_eq!(import_spirits(pool, &[record("kodo", "Kodo")], "seed").await?, 0);

        Ok(())
    })
    .await
}

// Kills: a summon whose COMMIT sends no NOTIFY to a listening Host.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn a_committed_summon_notifies_the_room_key() -> TestResult {
    let mut listener = PgListener::connect(&url()?).await?;
    listener.listen(SPIRITS_CHANNEL).await?;
    let room = format!("notify-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);

    in_schema(async |pool| {
        summon(pool, &record(&room, "Hati")).await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if listener.recv().await?.payload() == room {
                    return Ok::<_, sqlx::Error>(());
                }
            }
        })
        .await??;

        Ok(())
    })
    .await
}

// Kills: a rename that leaves a Hallway, setting, or text row on the old key,
// breaks a Hallway foreign key, or moves a room that has woken.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn rename_moves_hallway_rows_and_refuses_a_woken_room() -> TestResult {
    in_schema(async |pool| {
        summon(pool, &record("hugo", "Hugo")).await?;
        summon(pool, &record("kodo", "Kodo")).await?;
        sqlx::raw_sql(
            "WITH hall AS (
                 INSERT INTO hallway_channels (hallway_key, created_by_room, created_by_spirit,
                     created_by_session, create_idempotency_key, create_digest)
                 VALUES ('guild-hall', 'hugo', 'Hugo', 's1', 'k1', 'd') RETURNING id)
             INSERT INTO hallway_allowed_rooms (hallway_id, room)
             SELECT id, room FROM hall, (VALUES ('hugo'), ('kodo')) AS rooms(room);
             INSERT INTO hallway_presences (hallway_id, room, spirit, session_id,
                 join_idempotency_key, join_digest)
             SELECT id, 'hugo', 'Hugo', 's1', 'j1', 'd' FROM hallway_channels;
             INSERT INTO hallway_messages (hallway_id, sequence, room, spirit, session_id,
                 idempotency_key, body, body_digest, to_rooms)
             SELECT id, 1, 'hugo', 'Hugo', 's1', 'm1', 'hi', 'd', ARRAY['hugo', 'kodo']
             FROM hallway_channels;
             INSERT INTO hallway_room_state (hallway_id, room)
             SELECT id, 'hugo' FROM hallway_channels;
             INSERT INTO hallway_notifications (hallway_id, message_id, recipient_room)
             SELECT hallway_id, id, 'hugo' FROM hallway_messages;
             INSERT INTO hallway_knock_policies (hallway_id, room, spirit, session_id,
                 idempotency_key, request_digest, mode, allowed_rooms, revision)
             SELECT id, 'kodo', 'Kodo', 's2', 'p1', 'd', 'allow_list', ARRAY['hugo'], 1
             FROM hallway_channels;
             INSERT INTO room_settings (room_key, key, value) VALUES ('hugo', 'orihon', '\"on\"');",
        )
        .execute(pool)
        .await?;

        let renamed = begin_rename(pool, "hugo", "hati", Some("Hati"))
            .await?
            .commit()
            .await?;

        assert_eq!(renamed.spirit, record("hati", "Hati"));
        let left: i64 = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM spirits WHERE room = 'hugo')
                  + (SELECT count(*) FROM hallway_allowed_rooms WHERE room = 'hugo')
                  + (SELECT count(*) FROM hallway_presences WHERE room = 'hugo')
                  + (SELECT count(*) FROM hallway_messages WHERE room = 'hugo' OR 'hugo' = ANY (to_rooms))
                  + (SELECT count(*) FROM hallway_room_state WHERE room = 'hugo')
                  + (SELECT count(*) FROM hallway_notifications WHERE recipient_room = 'hugo')
                  + (SELECT count(*) FROM hallway_knock_policies WHERE 'hugo' = ANY (allowed_rooms))
                  + (SELECT count(*) FROM hallway_channels WHERE created_by_room = 'hugo')
                  + (SELECT count(*) FROM room_settings WHERE room_key = 'hugo')",
        )
        .fetch_one(pool)
        .await?;
        assert_eq!(left, 0);
        let moved: i64 = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM hallway_allowed_rooms WHERE room = 'hati')
                  + (SELECT count(*) FROM hallway_presences WHERE room = 'hati')
                  + (SELECT count(*) FROM hallway_messages WHERE room = 'hati' AND to_rooms = ARRAY['hati', 'kodo'])
                  + (SELECT count(*) FROM hallway_room_state WHERE room = 'hati')
                  + (SELECT count(*) FROM hallway_notifications WHERE recipient_room = 'hati')
                  + (SELECT count(*) FROM hallway_knock_policies WHERE allowed_rooms = ARRAY['hati'])
                  + (SELECT count(*) FROM room_settings WHERE room_key = 'hati')",
        )
        .fetch_one(pool)
        .await?;
        assert_eq!(moved, 7);

        sqlx::query("INSERT INTO threads (room, thread_key) VALUES ('hati', 'first-waking')")
            .execute(pool)
            .await?;
        let woken = begin_rename(pool, "hati", "skoll", None).await;
        assert!(
            matches!(&woken, Err(SpiritError::Refused(reason)) if reason.contains("threads")),
            "{:?}",
            woken.err()
        );
        assert!(matches!(
            begin_rename(pool, "kodo", "hati", None).await,
            Err(SpiritError::Refused(_))
        ));

        Ok(())
    })
    .await
}

// Kills: a migration that cannot run twice on the same schema.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
async fn migration_is_reapplicable() -> TestResult {
    in_schema(async |pool| {
        sqlx::raw_sql(include_str!("../../../substrate/migrations/0051_spirits.sql"))
            .execute(pool)
            .await?;
        run_migrations(pool).await?;

        Ok(())
    })
    .await
}

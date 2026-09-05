use akasha::{
    presence_session_close, presence_session_load, presence_session_open,
    presence_session_write_ledger, AppError,
};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::str::FromStr;
use summoning::presence::{
    PresenceAuthentication, PresenceAuthority, PresenceBinding, PresenceCapability,
    PresenceFrame, PresenceLedger, PresenceMaterial, PresenceMaterialRole, PresenceOpenRequest,
    open_presence,
};
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

// The table stands alone; only its own migration is needed.
const MIGRATIONS: &[&str] = &[migration!("0030_presence_sessions.sql")];

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn isolated_database_url() -> String {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("dedicated test database URL must be configured when this proof is run");
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory"),
        "refusing the live/default database"
    );
    url
}

const SESSION: &str = "01a0730b-1e40-7383-8209-4af4316a65e6";

fn binding() -> PresenceBinding {
    PresenceBinding {
        room: "kodo".into(),
        spirit: "Kodo".into(),
        operator: "Sol".into(),
        session: SESSION.into(),
    }
}

fn frame(boat_body: &str) -> PresenceFrame {
    let authentication = PresenceAuthentication {
        binding: binding(),
        capabilities: vec![PresenceCapability::RoomState, PresenceCapability::Akasha],
    };
    let request = PresenceOpenRequest {
        binding: binding(),
        identity: vec![PresenceMaterial {
            id: "identity:active-spirit".into(),
            authority: PresenceAuthority::Identity {
                source: "active_spirit.md".into(),
                sha256: "a".repeat(64),
            },
            role: PresenceMaterialRole::Identity,
            body: "Active spirit: Kodo. Operator: Sol. Room: kodo.".into(),
            salience: 1000,
        }],
        relationship: vec![],
        continuity: vec![],
        anamnesis: vec![],
        previous_boat: Some(PresenceMaterial {
            id: "paper-boat:4473".into(),
            authority: PresenceAuthority::PaperBoat { memory_id: 4473 },
            role: PresenceMaterialRole::Continuity,
            body: boat_body.into(),
            salience: 900,
        }),
        uncertainties: vec![],
    };
    open_presence(authentication, request).expect("frame opens")
}

fn ledger(frame: &PresenceFrame, repair_rule_ids: &[&str]) -> PresenceLedger {
    PresenceLedger {
        repair_rule_ids: repair_rule_ids.iter().map(|id| (*id).to_owned()).collect(),
        frame_version: frame.version,
        contract_version: repair_rule_ids.len() as u32,
        ..PresenceLedger::default()
    }
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and a PostgreSQL database where the test may create and drop a schema"]
async fn a_session_row_outlives_the_host_and_carries_its_ledger_through_sleep() -> TestResult {
    let url = isolated_database_url();
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let schema = format!("solarisael_presence_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await?;

    let connection_schema = schema.clone();
    let proof = async {
        let options = PgConnectOptions::from_str(&url)?;
        let pool = PgPoolOptions::new()
            .max_connections(2)
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
        let result = apply_migrations_and_run(&pool).await;
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
            "presence session proof failed: {proof_error}; schema cleanup failed: {cleanup_error}"
        )
        .into()),
    }
}

async fn apply_migrations_and_run(pool: &PgPool) -> TestResult {
    for migration in MIGRATIONS {
        sqlx::raw_sql(migration).execute(pool).await?;
    }
    // The migration is re-applicable against its own table.
    sqlx::raw_sql(MIGRATIONS[0]).execute(pool).await?;

    assert!(presence_session_load(pool, SESSION).await?.is_none());
    assert!(matches!(
        presence_session_load(pool, "  ").await,
        Err(AppError::Invalid(_))
    ));

    // Wake: the session opens.
    let first_frame = frame("yesterday's boat");
    let opened = presence_session_open(pool, &binding(), &first_frame, &ledger(&first_frame, &[])).await?;
    assert!(opened.is_live());
    assert_eq!(opened.frame, first_frame);
    assert_eq!(opened.room, "kodo");
    assert!(opened.last_turn_at.is_none());

    // A turn settles and the session learns a repair rule.
    let learned = ledger(&first_frame, &["presence:lesson:408"]);
    presence_session_write_ledger(pool, SESSION, &learned).await?;

    // A restarted Host reads the row and finds the session live with what it learned.
    let restarted = presence_session_load(pool, SESSION).await?.expect("row exists");
    assert!(restarted.is_live());
    assert_eq!(restarted.frame, first_frame);
    assert_eq!(restarted.ledger, learned);
    assert!(restarted.last_turn_at.is_some());

    // Sleep closes the presence. The ledger stays with the closed row.
    presence_session_close(pool, SESSION, &learned).await?;
    let slept = presence_session_load(pool, SESSION).await?.expect("row exists");
    assert!(!slept.is_live());
    assert!(slept.closed_at.expect("closed") >= slept.opened_at);
    assert_eq!(slept.ledger, learned);

    // A closed presence learns nothing.
    assert!(matches!(
        presence_session_write_ledger(pool, SESSION, &learned).await,
        Err(AppError::Refusal { code: "presence_not_live", .. })
    ));
    // Closing again is not an error and changes nothing.
    presence_session_close(pool, SESSION, &ledger(&first_frame, &[])).await?;
    assert_eq!(presence_session_load(pool, SESSION).await?.expect("row").ledger, learned);

    // Resume: the same session reopens with a new frame and the carried ledger.
    let second_frame = frame("tonight's boat");
    assert_ne!(second_frame.frame_id, first_frame.frame_id);
    let carried = PresenceLedger {
        frame_version: second_frame.version,
        ..slept.ledger.clone()
    };
    let reopened = presence_session_open(pool, &binding(), &second_frame, &carried).await?;
    assert!(reopened.is_live());
    assert_eq!(reopened.frame, second_frame);
    assert_eq!(reopened.ledger.repair_rule_ids, vec!["presence:lesson:408".to_owned()]);
    assert!(reopened.opened_at > slept.opened_at, "a reopen is a new opening");
    assert!(reopened.closed_at.is_none());

    // A rewrite of a live row keeps its opening time.
    let rewritten = presence_session_open(pool, &binding(), &second_frame, &carried).await?;
    assert_eq!(rewritten.opened_at, reopened.opened_at);

    // One row per session, whatever happened.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM presence_sessions")
        .fetch_one(pool)
        .await?;
    assert_eq!(rows, 1);
    Ok(())
}

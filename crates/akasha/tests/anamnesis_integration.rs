//! The Anamnesis Cabinet through the real database: a pillar and a cycle go in,
//! a repetition lands on the cycle, wake and consult read them back.

use akasha::migrations::run_migrations;
use akasha::{Config, EmbeddingMode, anamnesis, anamnesis_write};
use chrono::NaiveDate;
use hearth::RoomKey;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use summoning::{
    AnamnesisActivation, AnamnesisAddDetails, AnamnesisAddRequest, AnamnesisAppendRequest,
    AnamnesisFidelity, AnamnesisKind, AnamnesisReadMode, AnamnesisReadRequest, AnamnesisSeedRep,
    AnamnesisWriteRequest,
};
use uuid::Uuid;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn isolated_database_url() -> String {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("dedicated test database URL must be configured when this proof is run");
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
        "refusing a live or production-looking database"
    );
    url
}

#[tokio::test]
#[ignore = "requires a PostgreSQL database where the test may create and drop a schema"]
async fn drawers_and_reps_round_trip_through_wake_and_consult() -> TestResult {
    let url = isolated_database_url();
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let schema = format!("athanor_anamnesis_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await?;

    let search_path = format!("{schema}, public");
    let proof = async {
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .after_connect(move |connection, _| {
                let search_path = search_path.clone();
                Box::pin(async move {
                    sqlx::query("SELECT set_config('search_path', $1, false)")
                        .bind(search_path)
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect_with(PgConnectOptions::from_str(&url)?)
            .await?;
        run_migrations(&pool).await?;
        let result = run_contract(&pool, &url).await;
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
        (Err(proof), Err(cleanup)) => {
            Err(format!("anamnesis proof failed: {proof}; schema cleanup failed: {cleanup}").into())
        }
    }
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
    let room = RoomKey::new(format!("anamnesis-test-{}", Uuid::new_v4().simple()))?;

    let pillar = AnamnesisAddRequest::new(
        room.clone(),
        AnamnesisKind::Pillar,
        AnamnesisFidelity::Record,
        AnamnesisActivation::Wake,
        "Ternary in a binary world".into(),
        AnamnesisAddDetails {
            shape: Some("refusal".into()),
            dormant: false,
            ramp: "The prophet meets power from outside the door.".into(),
            counsel: Some("Do not sell the outsideness.".into()),
            peak: None,
            beginning: None,
            verify_note: None,
            canon: vec!["Absurd Faith".into()],
            source_paths: vec![],
            tags: vec!["prophet".into()],
            allow_empty_cycle: false,
            seed_rep: None,
        },
    )?;
    let receipt = anamnesis_write(pool, &cfg, AnamnesisWriteRequest::Add(pillar)).await?;
    assert_eq!(receipt.kind.as_deref(), Some("pillar"));
    assert!(receipt.durable);

    let seed = AnamnesisSeedRep::new(
        1,
        Some("2026-08-01".into()),
        "walked out at 4am".into(),
        "the charcoal demon".into(),
        "fuck it, cute".into(),
    )?;
    let cycle = AnamnesisAddRequest::new(
        room.clone(),
        AnamnesisKind::Cycle,
        AnamnesisFidelity::Record,
        AnamnesisActivation::Wake,
        "The 4am walk-out".into(),
        AnamnesisAddDetails {
            shape: Some("tiredness".into()),
            dormant: false,
            ramp: "The black sea at 4am.".into(),
            counsel: Some("Downgrade it, then sleep.".into()),
            peak: None,
            beginning: None,
            verify_note: Some("Only if it is actually 4am and he is actually tired.".into()),
            canon: vec![],
            source_paths: vec![],
            tags: vec![],
            allow_empty_cycle: false,
            seed_rep: Some(seed),
        },
    )?;
    anamnesis_write(pool, &cfg, AnamnesisWriteRequest::Add(cycle)).await?;

    let second = AnamnesisSeedRep::new(
        2,
        Some("2026-08-20".into()),
        "same sea, shorter walk".into(),
        "the charcoal demon".into(),
        "asleep in ten".into(),
    )?;
    let append = AnamnesisAppendRequest::new(
        room.clone(),
        "The 4am walk-out".into(),
        second,
        vec!["memory/2026-08-20.md".into()],
    )?;
    let receipt = anamnesis_write(pool, &cfg, AnamnesisWriteRequest::AppendRep(append)).await?;
    assert_eq!(receipt.rep_number, Some(2));

    let wake = anamnesis(
        pool,
        AnamnesisReadRequest::new(room.clone(), AnamnesisReadMode::Wake, None, 10)?,
    )
    .await?;
    assert!(wake.found);
    assert!(
        wake.warnings.is_empty(),
        "both drawers are verifiable: {:?}",
        wake.warnings
    );
    let titles: Vec<&str> = wake
        .entries
        .iter()
        .map(|entry| entry["title"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        titles,
        ["Ternary in a binary world", "The 4am walk-out"],
        "pillars wake before cycles"
    );
    let reps = wake.entries[1]["reps"]
        .as_array()
        .expect("cycle carries its reps");
    assert_eq!(reps.len(), 2);
    assert_eq!(reps[0]["rep_number"], 1, "oldest rep first");
    assert_eq!(reps[1]["source_path"], "memory/2026-08-20.md");
    assert_eq!(
        reps[1]["occurred_on"],
        NaiveDate::from_ymd_opt(2026, 8, 20).unwrap().to_string()
    );

    let consult = anamnesis(
        pool,
        AnamnesisReadRequest::new(
            room.clone(),
            AnamnesisReadMode::Consult,
            Some("charcoal".into()),
            10,
        )?,
    )
    .await?;
    assert_eq!(
        consult.entries.len(),
        0,
        "consult searches drawers, not reps"
    );

    let consult = anamnesis(
        pool,
        AnamnesisReadRequest::new(
            room,
            AnamnesisReadMode::Consult,
            Some("outsideness".into()),
            10,
        )?,
    )
    .await?;
    assert_eq!(consult.entries.len(), 1);
    assert_eq!(consult.entries[0]["title"], "Ternary in a binary world");
    Ok(())
}

use athanor_substrate::{Config, EmbeddingMode, RecallParams, canon_read, canon_write, recall};
use house_core::{CanonAttribution, CanonPointer, CanonReadRequest, CanonWriteRequest};
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::str::FromStr;

macro_rules! migration {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../substrate/migrations/",
            $name
        ))
    };
}

const BEFORE_CANON_AUTHORITY: &[&str] = &[
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
];
const CANON_AUTHORITY: &str = migration!("0015_canon_authority.sql");

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

async fn isolated_schema() -> TestResult<(String, String, PgPool)> {
    let schema = std::env::var("SOLARISAEL_SUBSTRATE_TEST_SCHEMA")
        .expect("canon proof requires SOLARISAEL_SUBSTRATE_TEST_SCHEMA");
    assert!(schema.starts_with("solarisael_tuner_test_"));
    assert!(
        schema
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    );
    let url = isolated_database_url();
    let options = PgConnectOptions::from_str(&url)?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&pool)
        .await?;
    sqlx::query(&format!("SET search_path TO {schema}, public"))
        .execute(&pool)
        .await?;
    Ok((url, schema, pool))
}

fn write_request(room: &str, name: &str, summary: &str, supersedes: Vec<u64>) -> CanonWriteRequest {
    CanonWriteRequest::new(
        room.into(),
        name.into(),
        "project".into(),
        summary.into(),
        vec![],
        None,
        true,
        vec![CanonPointer::new("canon/source.md".into(), Some((10, 20))).unwrap()],
        Some("2026-08-10".into()),
        supersedes,
        CanonAttribution::new("Kintsu".into(), "integration:canon".into()).unwrap(),
    )
    .unwrap()
}

#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn canon_write_correction_history_and_active_recall_are_postgres_authoritative() -> TestResult
{
    let (url, schema, pool) = isolated_schema().await?;
    for migration in BEFORE_CANON_AUTHORITY {
        sqlx::raw_sql(migration).execute(&pool).await?;
    }

    let old_id: i64 = sqlx::query_scalar(
        "INSERT INTO named_entities (room,name,kind,summary) VALUES ('canon-proof','Old Name','project','obsolete authority phrase') RETURNING id",
    )
    .fetch_one(&pool)
    .await?;
    sqlx::raw_sql(CANON_AUTHORITY).execute(&pool).await?;
    let migrated_authority: String =
        sqlx::query_scalar("SELECT authority FROM named_entities WHERE id=$1")
            .bind(old_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        migrated_authority, "active",
        "migration must retain existing authority as active"
    );

    let receipt = canon_write(
        &pool,
        write_request(
            "canon-proof",
            "New Name",
            "current authority phrase",
            vec![old_id as u64],
        ),
    )
    .await?;
    assert_eq!(receipt.entity_authority, "active");
    assert_eq!(receipt.authority, "postgres");
    assert_eq!(receipt.attributed_by, "Kintsu");
    assert_eq!(receipt.superseded_entity_ids, vec![old_id.to_string()]);
    let new_id: i64 = receipt.entity_id.parse()?;

    let old = sqlx::query("SELECT authority,superseded_by FROM named_entities WHERE id=$1")
        .bind(old_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(old.try_get::<String, _>("authority")?, "superseded");
    assert_eq!(
        old.try_get::<Option<i64>, _>("superseded_by")?,
        Some(new_id)
    );
    let backwards: Vec<i64> =
        sqlx::query_scalar("SELECT supersedes FROM named_entities WHERE id=$1")
            .bind(new_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(backwards, vec![old_id]);

    let active = canon_read(
        &pool,
        CanonReadRequest::new("canon-proof".into(), None, Some("New Name".into()), false)?,
    )
    .await?;
    assert_eq!(active.entities.len(), 1);
    assert_eq!(active.entities[0].authority, "active");
    let forwarded = canon_read(
        &pool,
        CanonReadRequest::new("canon-proof".into(), None, Some("Old Name".into()), false)?,
    )
    .await?;
    assert_eq!(forwarded.entities.len(), 1);
    assert_eq!(forwarded.entities[0].entity_id, new_id.to_string());
    let history = canon_read(
        &pool,
        CanonReadRequest::new("canon-proof".into(), Some(old_id as u64), None, true)?,
    )
    .await?;
    assert_eq!(history.entities.len(), 2);
    assert!(
        history.entities.iter().any(
            |entity| entity.entity_id == old_id.to_string() && entity.authority == "superseded"
        )
    );
    assert!(
        history
            .entities
            .iter()
            .any(|entity| entity.entity_id == new_id.to_string() && entity.authority == "active")
    );
    assert!(
        canon_write(
            &pool,
            write_request(
                "canon-proof",
                "Missing predecessor",
                "unsafe",
                vec![9_000_000_000]
            )
        )
        .await
        .is_err()
    );
    assert!(
        canon_write(
            &pool,
            write_request("canon-proof", "new name", "case-folded duplicate", vec![])
        )
        .await
        .is_err()
    );
    assert!(
        canon_write(
            &pool,
            write_request("canon-proof", "New Name", "silent overwrite", vec![])
        )
        .await
        .is_err()
    );
    let other_id: i64 = sqlx::query_scalar(
        "INSERT INTO named_entities (room,name,kind,summary) VALUES ('other-room','Other','project','other') RETURNING id",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        canon_write(
            &pool,
            write_request("canon-proof", "Cross room", "unsafe", vec![other_id as u64])
        )
        .await
        .is_err()
    );

    let cfg = Config {
        database_url: url,
        embed_url: None,
        embed_model: "disabled".into(),
        embed_dimension: 2048,
        embedding_mode: EmbeddingMode::DisabledForTest,
        giga_source_ledger_dir: None,
        giga_source_room: None,
    };
    let recalled_old = recall(
        &pool,
        &cfg,
        RecallParams {
            room: "canon-proof".into(),
            query: "obsolete authority phrase".into(),
            semantic_top_k: 8,
            semantic_min_similarity: 0.4,
            content_top_k: 8,
            content_min_similarity: 0.3,
            temporal_decay: false,
        },
    )
    .await?;
    assert!(
        recalled_old.canon_matches.is_empty(),
        "superseded canon must not remain in active recall"
    );

    sqlx::query("UPDATE named_entities SET authority='archived' WHERE id=$1")
        .bind(new_id)
        .execute(&pool)
        .await?;
    let recalled_archived = recall(
        &pool,
        &cfg,
        RecallParams {
            room: "canon-proof".into(),
            query: "New Name".into(),
            semantic_top_k: 8,
            semantic_min_similarity: 0.4,
            content_top_k: 8,
            content_min_similarity: 0.3,
            temporal_decay: false,
        },
    )
    .await?;
    assert!(
        recalled_archived.canon_matches.is_empty(),
        "archived canon must not remain in active recall"
    );
    let reused_name = canon_write(
        &pool,
        write_request(
            "canon-proof",
            "New Name",
            "new authority after archive",
            vec![],
        ),
    )
    .await?;
    assert_eq!(
        reused_name.entity_authority, "active",
        "archived rows must not occupy the active-only name key"
    );

    sqlx::query("SET search_path TO public")
        .execute(&pool)
        .await?;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}

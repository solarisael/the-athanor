use akasha::{Config, EmbeddingMode, remember};
use protocol::RequestEnvelope;
use serde_json::{Value, json};
use sqlx::PgPool;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

async fn write_memory(pool: &PgPool, config: &Config, params: Value) -> TestResult<Value> {
    let request = RequestEnvelope {
        protocol: protocol::PROTOCOL_VERSION,
        id: uuid::Uuid::new_v4().to_string(),
        method: "remember".into(),
        params,
    }
    .remember_request()?;
    Ok(serde_json::to_value(
        remember(pool, config, request).await?,
    )?)
}

#[tokio::test]
#[ignore = "requires a dedicated athanor_test_ database with pgvector"]
async fn supersession_refuses_foreign_and_missing_predecessors_atomically() -> TestResult {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")?;
    let mut config = Config::for_database(&url)?;
    config.embedding_mode = EmbeddingMode::DisabledForTest;
    config.embed_url = None;
    config.nats_url = None;
    let pool = akasha::migrations::migration_pool(&config).await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    assert!(
        database.starts_with("athanor_test_"),
        "refusing a non-test database"
    );
    akasha::migrations::run_migrations(&pool).await?;
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let owner = format!("owner-{nonce}");
    let other = format!("other-{nonce}");
    let original = write_memory(&pool, &config, json!({
        "room": owner, "kind": "memory", "title": "Original private memory",
        "body": "The private record must survive a foreign supersession attempt.", "backup": false
    })).await?;
    let id = original["memory_id"].as_i64().expect("committed memory id");
    let foreign = write_memory(&pool, &config, json!({
        "room": other, "kind": "memory", "title": "Foreign replacement",
        "body": "This replacement must roll back.", "supersedes": [id.to_string()], "backup": false
    })).await.unwrap_err();
    let missing = write_memory(&pool, &config, json!({
        "room": other, "kind": "memory", "title": "Missing replacement",
        "body": "This replacement must roll back too.", "supersedes": [i64::MAX.to_string()], "backup": false
    })).await.unwrap_err();
    assert_eq!(foreign.to_string(), missing.to_string());
    let predecessor: Option<i64> =
        sqlx::query_scalar("SELECT superseded_by FROM memories WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(predecessor, None);
    let leaked: i64 = sqlx::query_scalar("SELECT count(*) FROM memories WHERE room=$1")
        .bind(&other)
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        leaked, 0,
        "a refused replacement must not leave a durable row"
    );
    let replacement = write_memory(&pool, &config, json!({
        "room": owner, "kind": "memory", "title": "Authorized replacement",
        "body": "The owner may replace its own record.", "supersedes": [id.to_string()], "backup": false
    })).await?;
    let replacement_id = replacement["memory_id"].as_i64().expect("replacement id");
    let predecessor: Option<i64> =
        sqlx::query_scalar("SELECT superseded_by FROM memories WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(predecessor, Some(replacement_id));
    sqlx::query("DELETE FROM memories WHERE room=ANY($1::text[])")
        .bind(vec![owner, other])
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}

#[test]
fn coding_lessons_cannot_target_a_different_private_room() {
    let parse = |scope: &str| {
        RequestEnvelope {
            protocol: protocol::PROTOCOL_VERSION,
            id: "scope-check".into(),
            method: "remember".into(),
            params: json!({
                "room": "luciano", "kind": "coding-lesson", "title": "Scope contract",
                "body": "A coding lesson belongs to the shared House or its own room.",
                "shape": "process", "voice": "craft", "scope": scope
            }),
        }
        .remember_request()
    };
    assert!(parse("claudio").is_err());
    assert!(parse("luciano").is_ok());
    assert!(parse("house").is_ok());
}

use akasha::{
    Config, EmbeddingMode, RecallParams, RememberRequest, ThreadContinuation,
    backup::source_migrations, recall, remember,
};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
    types::Json,
};
use std::str::FromStr;
use uuid::Uuid;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Canonical migrations live at `<athanor-root>/substrate/migrations`, outside
/// this crate. Resolve them from the crate manifest so the path survives the
/// test binary being built or run from anywhere.
macro_rules! migration {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../substrate/migrations/",
            $name
        ))
    };
}

fn require(condition: bool, message: &str) -> TestResult {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

async fn insert_lexical_memory(
    pool: &PgPool,
    room: &str,
    source_path: &str,
    body: &str,
    meta: serde_json::Value,
    superseded_by: Option<i64>,
) -> TestResult<i64> {
    let memory_id: i64 = sqlx::query_scalar(
        "INSERT INTO memories (room,type,title,source_path,body,meta,superseded_by)
         VALUES ($1,'memory',$2,$2,$3,$4,$5) RETURNING id",
    )
    .bind(room)
    .bind(source_path)
    .bind(body)
    .bind(Json(meta))
    .bind(superseded_by)
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO memory_chunks (memory_id,chunk_index,body,char_start,char_end)
         VALUES ($1,0,$2,0,$3)",
    )
    .bind(memory_id)
    .bind(body)
    .bind(i32::try_from(body.len())?)
    .execute(pool)
    .await?;
    Ok(memory_id)
}

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

fn migration_database_scope() -> (String, Option<String>) {
    let Ok(schema) = std::env::var("ATHANOR_SUBSTRATE_TEST_SCHEMA") else {
        return (isolated_database_url(), None);
    };
    assert!(schema.starts_with("solarisael_tuner_test_"));
    assert!(
        schema
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    );
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("the schema proof requires a PostgreSQL database URL");
    (url, Some(schema))
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL database"]
async fn isolated_database_guard() {
    let url = isolated_database_url();
    let options = PgConnectOptions::from_str(&url).expect("dedicated test URL must be valid");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .expect("isolated database must be reachable");
    sqlx::query("SELECT 1")
        .execute(&pool)
        .await
        .expect("isolated database health check");

    let cfg = Config {
        database_url: url,
        embed_url: None,
        embed_model: "disabled".into(),
        embed_dimension: 2048,
        embedding_mode: EmbeddingMode::DisabledForTest,
        giga_source_ledger_dir: None,
        giga_source_room: None,
        house_tz: "America/Sao_Paulo".into(),
    };
    let source_path = format!("isolated-test/{}", Uuid::new_v4());
    let body = "This mutation proves the dedicated PostgreSQL authority path.";
    let receipt = remember(
        &pool,
        &cfg,
        RememberRequest {
            room: "isolated-test".into(),
            kind: "memory".into(),
            title: "isolated integration proof".into(),
            body: body.into(),
            lesson: None,
            source_path: Some(source_path.clone()),
            source_memory_path: None,
            threads: vec!["integration".into()],
            continues: vec![],
            supersedes: vec![],
            shape: None,
            voice: None,
            register: vec![],
            scope: None,
            project: None,
            proof_pattern: None,
            trigger_context: None,
            example_text: None,
            language_keys: vec![],
            technology_keys: vec![],
            thread_keys: vec![],
            tags: vec![],
            condition: vec![],
            ast_condition: vec![],
            trigger_scope: vec![],
            interrupt_mode: None,
            repeat_cooldown_secs: None,
            backup: false,
        },
    )
    .await
    .expect("remember mutation must commit");
    assert_eq!(receipt.authority, "postgres");
    assert!(receipt.durable);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memories WHERE room=$1 AND source_path=$2")
            .bind("isolated-test")
            .bind(&source_path)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    let lexical_chunks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory_chunks WHERE memory_id=$1 AND body_embedding IS NULL",
    )
    .bind(receipt.memory_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(lexical_chunks > 0);
    let recalled = recall(
        &pool,
        &cfg,
        RecallParams {
            room: "isolated-test".into(),
            query: body.into(),
            semantic_top_k: 1,
            semantic_min_similarity: 0.5,
            content_top_k: 8,
            content_min_similarity: 0.3,
            temporal_decay: false,
        },
    )
    .await
    .expect("lexical recall must succeed with embeddings disabled");
    assert!(recalled.found);
    assert!(
        recalled.content_chunks.iter().any(|chunk| {
            chunk.get("source_path").and_then(serde_json::Value::as_str)
                == Some(source_path.as_str())
                && chunk.get("body").and_then(serde_json::Value::as_str) == Some(body)
        }),
        "lexical recall must return the exact written body for the written source path"
    );
    sqlx::query("DELETE FROM memories WHERE room=$1 AND source_path=$2")
        .bind("isolated-test")
        .bind(source_path)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and migrations through 0006"]
async fn ordered_thread_write_surfaces_explicit_recall_neighbors() {
    let url = isolated_database_url();
    let options = PgConnectOptions::from_str(&url).expect("dedicated test URL must be valid");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .expect("isolated database must be reachable");
    let cfg = Config {
        database_url: url,
        embed_url: None,
        embed_model: "disabled".into(),
        embed_dimension: 2048,
        embedding_mode: EmbeddingMode::DisabledForTest,
        giga_source_ledger_dir: None,
        giga_source_room: None,
        house_tz: "America/Sao_Paulo".into(),
    };
    let suffix = Uuid::new_v4();
    let root_source = format!("thread-integration/{suffix}/root");
    let next_source = format!("thread-integration/{suffix}/next");
    let thread = "work / page";
    let root = remember(
        &pool,
        &cfg,
        RememberRequest {
            room: "thread-continuity-integration".into(),
            kind: "memory".into(),
            title: "root decision".into(),
            body: "The initial explicit work page decision.".into(),
            lesson: None,
            source_path: Some(root_source.clone()),
            source_memory_path: None,
            threads: vec![thread.into()],
            continues: vec![],
            supersedes: vec![],
            shape: None,
            voice: None,
            register: vec![],
            scope: None,
            project: None,
            proof_pattern: None,
            trigger_context: None,
            example_text: None,
            language_keys: vec![],
            technology_keys: vec![],
            thread_keys: vec![],
            tags: vec![],
            condition: vec![],
            ast_condition: vec![],
            trigger_scope: vec![],
            interrupt_mode: None,
            repeat_cooldown_secs: None,
            backup: false,
        },
    )
    .await
    .expect("root memory must commit");
    let next_body = "The successor work page decision explicitly continues the root.";
    let next = remember(
        &pool,
        &cfg,
        RememberRequest {
            room: "thread-continuity-integration".into(),
            kind: "memory".into(),
            title: "successor decision".into(),
            body: next_body.into(),
            lesson: None,
            source_path: Some(next_source.clone()),
            source_memory_path: None,
            threads: vec![thread.into()],
            continues: vec![ThreadContinuation {
                thread: thread.into(),
                previous_memory_id: root.memory_id,
            }],
            supersedes: vec![],
            shape: None,
            voice: None,
            register: vec![],
            scope: None,
            project: None,
            proof_pattern: None,
            trigger_context: None,
            example_text: None,
            language_keys: vec![],
            technology_keys: vec![],
            thread_keys: vec![],
            tags: vec![],
            condition: vec![],
            ast_condition: vec![],
            trigger_scope: vec![],
            interrupt_mode: None,
            repeat_cooldown_secs: None,
            backup: false,
        },
    )
    .await
    .expect("continuation memory must commit");

    let recalled = recall(
        &pool,
        &cfg,
        RecallParams {
            room: "thread-continuity-integration".into(),
            query: next_body.into(),
            semantic_top_k: 1,
            semantic_min_similarity: 0.5,
            content_top_k: 8,
            content_min_similarity: 0.3,
            temporal_decay: false,
        },
    )
    .await
    .expect("recall must surface the explicit continuation");
    let candidate = recalled
        .retrieval_candidates
        .iter()
        .find(|candidate| candidate["memory_id"].as_i64() == Some(next.memory_id))
        .expect("the successor must be surfaced as a fused recall candidate");
    let neighbors = candidate["thread_neighbors"]
        .as_array()
        .expect("the surfaced successor must carry thread neighbor evidence");
    assert!(neighbors.iter().any(|neighbor| {
        neighbor["thread"].as_str() == Some(thread)
            && neighbor["direction"].as_str() == Some("previous")
            && neighbor["id"].as_i64() == Some(root.memory_id)
            && neighbor["authority_state"].as_str() == Some("active")
    }));

    sqlx::query("DELETE FROM memories WHERE room=$1 AND source_path = ANY($2::text[])")
        .bind("thread-continuity-integration")
        .bind(vec![root_source, next_source])
        .execute(&pool)
        .await
        .expect("thread integration memories must clean up");
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL database"]
async fn lexical_recall_applies_durability_decay_only_when_requested() {
    let url = isolated_database_url();
    let options = PgConnectOptions::from_str(&url).expect("dedicated test URL must be valid");
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .expect("isolated database must be reachable");
    let schema = format!("solarisael_decay_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .expect("isolated decay schema must create");

    let connection_schema = schema.clone();
    let pool_result = PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |connection, _meta| {
            let schema = connection_schema.clone();
            Box::pin(async move {
                sqlx::query(&format!("SET search_path TO {schema}, public"))
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await;

    let result: TestResult = match pool_result {
        Ok(pool) => {
            let result: TestResult = async {
                sqlx::raw_sql(migration!("0001_initial.sql"))
                    .execute(&pool)
                    .await?;
                // recall's read path spans these migrations: 0006 thread graph,
                // 0009 bm25f columns, 0010 semantic vocabulary, 0015 canon
                // authority. An isolated schema missing one fails on shadowed
                // tables, not on the contract under test.
                for sql in [
                    migration!("0006_memory_thread_graph.sql"),
                    migration!("0009_bm25f_memory_search.sql"),
                    migration!("0010_semantic_vocabulary.sql"),
                    migration!("0015_canon_authority.sql"),
                ] {
                    sqlx::raw_sql(sql).execute(&pool).await?;
                }

                let room = "temporal-decay-integration";
                let body = "identical lexical durability recall evidence";
                let old_anchor = (chrono::Utc::now() - chrono::Duration::days(28)).to_rfc3339();
                let giga_meta = |durability: f64| {
                    serde_json::json!({
                        "origin": "giga-promotion",
                        "giga": {
                            "durability": durability,
                            "decay_anchor": "candidate_created_at",
                            "decay_anchor_at": old_anchor,
                        },
                    })
                };
                let replacement_id = insert_lexical_memory(
                    &pool,
                    room,
                    "z-replacement",
                    "unrelated successor sentinel",
                    serde_json::json!({}),
                    None,
                )
                .await?;
                insert_lexical_memory(&pool, room, "a-low-durability", body, giga_meta(0.0), None)
                    .await?;
                insert_lexical_memory(&pool, room, "b-high-durability", body, giga_meta(1.0), None)
                    .await?;
                insert_lexical_memory(&pool, room, "c-legacy", body, serde_json::json!({}), None)
                    .await?;
                insert_lexical_memory(
                    &pool,
                    room,
                    "d-superseded",
                    body,
                    giga_meta(1.0),
                    Some(replacement_id),
                )
                .await?;

                let cfg = Config {
                    database_url: url.clone(),
                    embed_url: None,
                    embed_model: "test-disabled".into(),
                    embed_dimension: 2_048,
                    embedding_mode: EmbeddingMode::DisabledForTest,
                    giga_source_ledger_dir: None,
                    giga_source_room: None,
                    house_tz: "America/Sao_Paulo".into(),
                };
                let params = RecallParams {
                    room: room.into(),
                    query: body.into(),
                    semantic_top_k: 1,
                    semantic_min_similarity: 0.0,
                    content_top_k: 8,
                    content_min_similarity: 0.0,
                    temporal_decay: true,
                };
                let decayed = recall(&pool, &cfg, params).await?;
                let decayed_paths = decayed
                    .retrieval_candidates
                    .iter()
                    .filter_map(|candidate| candidate["source_path"].as_str())
                    .collect::<Vec<_>>();
                let high_rank = decayed_paths
                    .iter()
                    .position(|path| *path == "b-high-durability");
                let low_rank = decayed_paths
                    .iter()
                    .position(|path| *path == "a-low-durability");
                require(
                    matches!((high_rank, low_rank), (Some(high), Some(low)) if high < low),
                    "temporal decay must rank equal-relevance high durability above low durability",
                )?;
                let legacy = decayed
                    .content_chunks
                    .iter()
                    .find(|chunk| chunk["source_path"] == "c-legacy")
                    .ok_or("legacy lexical fixture must be recalled")?;
                require(
                    legacy["temporal_weight"].as_f64() == Some(1.0)
                        && legacy["durability"].is_null(),
                    "legacy metadata must retain weight one",
                )?;
                let relevance = decayed
                    .content_chunks
                    .iter()
                    .filter(|chunk| {
                        matches!(
                            chunk["source_path"].as_str(),
                            Some("a-low-durability" | "b-high-durability" | "c-legacy")
                        )
                    })
                    .filter_map(|chunk| chunk["ws"].as_f64())
                    .collect::<Vec<_>>();
                require(
                    relevance.len() == 3
                        && relevance
                            .iter()
                            .all(|score| (score - relevance[0]).abs() < 1e-12),
                    "ranking fixtures must have equal lexical relevance",
                )?;

                let cutoff = recall(
                    &pool,
                    &cfg,
                    RecallParams {
                        room: room.into(),
                        query: body.into(),
                        semantic_top_k: 1,
                        semantic_min_similarity: 0.0,
                        content_top_k: 2,
                        content_min_similarity: 0.0,
                        temporal_decay: true,
                    },
                )
                .await?;
                let cutoff_paths = cutoff
                    .content_chunks
                    .iter()
                    .filter_map(|chunk| chunk["source_path"].as_str())
                    .collect::<Vec<_>>();
                require(
                    cutoff_paths == vec!["b-high-durability", "c-legacy"],
                    "durability reranking must happen before the requested top-K cutoff",
                )?;
                require(
                    decayed
                        .content_chunks
                        .iter()
                        .all(|chunk| chunk["source_path"].as_str() != Some("d-superseded"))
                        && decayed.retrieval_candidates.iter().all(|candidate| {
                            candidate["source_path"].as_str() != Some("d-superseded")
                        }),
                    "superseded rows must be absent from every ordinary recall surface",
                )?;

                let bypassed = recall(
                    &pool,
                    &cfg,
                    RecallParams {
                        room: room.into(),
                        query: body.into(),
                        semantic_top_k: 1,
                        semantic_min_similarity: 0.0,
                        content_top_k: 8,
                        content_min_similarity: 0.0,
                        temporal_decay: false,
                    },
                )
                .await?;
                let bypassed_paths = bypassed
                    .retrieval_candidates
                    .iter()
                    .filter_map(|candidate| candidate["source_path"].as_str())
                    .collect::<Vec<_>>();
                let bypassed_low = bypassed_paths
                    .iter()
                    .position(|path| *path == "a-low-durability");
                let bypassed_high = bypassed_paths
                    .iter()
                    .position(|path| *path == "b-high-durability");
                require(
                    matches!(
                        (bypassed_low, bypassed_high),
                        (Some(low), Some(high)) if low < high
                    ),
                    "explicit temporal bypass must restore equal-relevance lexical ordering",
                )?;
                require(
                    bypassed.content_chunks.iter().all(|chunk| {
                        chunk["temporal_weight"].as_f64() == Some(1.0)
                            && chunk["durability"].is_null()
                    }),
                    "explicit temporal bypass must remove every decay contribution",
                )?;
                require(
                    bypassed
                        .content_chunks
                        .iter()
                        .all(|chunk| chunk["source_path"].as_str() != Some("d-superseded"))
                        && bypassed.retrieval_candidates.iter().all(|candidate| {
                            candidate["source_path"].as_str() != Some("d-superseded")
                        }),
                    "temporal bypass must not weaken supersession isolation",
                )?;
                Ok(())
            }
            .await;
            pool.close().await;
            result
        }
        Err(error) => Err(error.into()),
    };

    let cleanup = sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await;
    admin.close().await;
    cleanup.expect("isolated decay schema cleanup must succeed");
    result.expect("lexical durability decay integration contract");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database or schema"]
async fn migrations_reapply_without_clearing_current_embeddings() {
    let (url, schema) = migration_database_scope();
    let options = PgConnectOptions::from_str(&url).expect("test database URL must be valid");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("test database must be reachable");
    if let Some(schema) = &schema {
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&pool)
            .await
            .expect("isolated schema must create");
        sqlx::query(&format!("SET search_path TO {schema}, public"))
            .execute(&pool)
            .await
            .expect("isolated schema must become active");
    }
    let initial = migration!("0001_initial.sql");
    let nemotron = migration!("0002_nemotron_2048.sql");
    sqlx::raw_sql(initial)
        .execute(&pool)
        .await
        .expect("initial migration must apply");
    sqlx::raw_sql(nemotron)
        .execute(&pool)
        .await
        .expect("Nemotron migration must apply");

    let source_path = format!("migration-reapply/{}", Uuid::new_v4());
    let memory_id: i64 = sqlx::query_scalar(
        "INSERT INTO memories (room,type,title,source_path,body) VALUES ('isolated-test','memory','migration reapply',$1,'sentinel') RETURNING id",
    )
    .bind(&source_path)
    .fetch_one(&pool)
    .await
    .expect("sentinel memory must insert");
    let vector = format!("[{}]", vec!["0"; 2048].join(","));
    sqlx::query(
        "INSERT INTO memory_chunks (memory_id,chunk_index,body,char_start,char_end,body_embedding,embedded_at) VALUES ($1,0,'sentinel',0,8,$2::vector,NOW())",
    )
    .bind(memory_id)
    .bind(vector)
    .execute(&pool)
    .await
    .expect("sentinel embedding must insert");

    sqlx::raw_sql(initial)
        .execute(&pool)
        .await
        .expect("initial migration must reapply");
    sqlx::raw_sql(nemotron)
        .execute(&pool)
        .await
        .expect("Nemotron migration must reapply");
    let embedded: bool = sqlx::query_scalar(
        "SELECT body_embedding IS NOT NULL FROM memory_chunks WHERE memory_id=$1",
    )
    .bind(memory_id)
    .fetch_one(&pool)
    .await
    .expect("sentinel embedding must remain queryable");
    assert!(embedded);
    let versions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM schema_migrations WHERE version IN (1,2)")
            .fetch_one(&pool)
            .await
            .expect("migration versions must remain queryable");
    assert_eq!(versions, 2);

    sqlx::query("DELETE FROM memories WHERE id=$1")
        .bind(memory_id)
        .execute(&pool)
        .await
        .expect("sentinel cleanup must succeed");
    if let Some(schema) = &schema {
        sqlx::query("SET search_path TO public")
            .execute(&pool)
            .await
            .expect("public schema must become active for cleanup");
        sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
            .execute(&pool)
            .await
            .expect("isolated schema cleanup must succeed");
    }
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL schema"]
async fn source_migrations_accepts_text_version_columns() {
    let (url, schema) = migration_database_scope();
    let schema = schema.expect("text-version proof requires ATHANOR_SUBSTRATE_TEST_SCHEMA");
    let options = PgConnectOptions::from_str(&url).expect("test database URL must be valid");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("test database must be reachable");
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&pool)
        .await
        .expect("isolated schema must create");
    sqlx::query(&format!("SET search_path TO {schema}, public"))
        .execute(&pool)
        .await
        .expect("isolated schema must become active");
    sqlx::query("CREATE TABLE schema_migrations (version TEXT PRIMARY KEY)")
        .execute(&pool)
        .await
        .expect("text migration table must create");
    sqlx::query("INSERT INTO schema_migrations(version) VALUES ('1'), ('2')")
        .execute(&pool)
        .await
        .expect("text migration versions must insert");

    assert_eq!(source_migrations(&pool).await.unwrap(), ["1", "2"]);

    sqlx::query("SET search_path TO public")
        .execute(&pool)
        .await
        .expect("public schema must become active for cleanup");
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await
        .expect("isolated schema cleanup must succeed");
    pool.close().await;
}

fn validation_request(room: &str, kind: &str) -> RememberRequest {
    serde_json::from_value(serde_json::json!({
        "room": room,
        "kind": kind,
        "title": "validation probe",
        "body": "validation body",
    }))
    .expect("request fixture must deserialize")
}

#[test]
fn validate_refuses_house_room_for_every_lesson_kind() {
    for kind in [
        "coding-lesson",
        "project-lesson",
        "writing-lesson",
        "audio-lesson",
        "design-lesson",
    ] {
        let error = validation_request("house", kind)
            .validate()
            .expect_err("house lesson writes must be refused");
        assert!(
            format!("{error:?}").contains("house accepts only memory writes"),
            "unexpected refusal for {kind}: {error:?}"
        );
    }
}


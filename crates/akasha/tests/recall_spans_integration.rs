//! Every phase of a recall lands in `insula.log` as a child span of the
//! request's `recall` span, with a duration and an outcome, so the cost of
//! recall can be read from Insula instead of guessed.
//!
//! Run with the isolated PostgreSQL pair:
//! `ATHANOR_SUBSTRATE_TEST_DATABASE_URL=... ATHANOR_SUBSTRATE_TEST_SCHEMA=solarisael_tuner_test_<you>`
//! `cargo test -p akasha --test recall_spans_integration -- --ignored --nocapture`

use akasha::insula::TrustedBinding;
use akasha::{
    Config, EmbeddingMode, end_span, flush_insula_emitter, init_insula_emitter, recall,
    start_span,
};
use hearth::{RecallRequest, RoomKey};
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Instant;

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
    migration!("0018_hallway_chatrooms.sql"),
    migration!("0019_lesson_triggers.sql"),
    migration!("0020_hallway_bell.sql"),
    migration!("0021_hallway_knock.sql"),
    migration!("0022_insula.sql"),
    migration!("0023_docket.sql"),
    migration!("0024_docket_capability.sql"),
    migration!("0025_docket_draft_abandon.sql"),
    migration!("0026_restart.sql"),
    migration!("0027_restart_successor_proof.sql"),
    migration!("0028_room_settings.sql"),
];

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const ROOM: &str = "spans-proof";
const SEEDED_MEMORIES: i64 = 200;

/// Phases a recall with the semantic lane disabled must observe. `dates`
/// needs a date in the query; `embed`, `vocabulary`, and `semantic` need an
/// embedding endpoint and are absent by construction here.
const EXPECTED_PHASES: &[&str] = &[
    "recall.settings",
    "recall.reference",
    "recall.lexical",
    "recall.semantic_lexical",
    "recall.content",
    "recall.dates",
    "recall.threads",
    "recall.fuse",
    "recall.neighbors",
    "recall.canon",
    "recall.taxonomy",
    "recall.cluster",
];

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

struct Isolated {
    schema: String,
    pool: PgPool,
    cfg: Config,
}

impl Isolated {
    /// The Insula writer drains on its own connection while recall runs, so
    /// the pool carries two connections and the schema rides in the connect
    /// options rather than a per-connection `SET`.
    async fn open(suffix: &str) -> TestResult<Self> {
        let base = std::env::var("ATHANOR_SUBSTRATE_TEST_SCHEMA")
            .expect("this proof requires ATHANOR_SUBSTRATE_TEST_SCHEMA");
        assert!(base.starts_with("solarisael_tuner_test_"));
        let schema = format!("{base}_{suffix}");
        assert!(
            schema
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        );
        let url = isolated_database_url();
        let options = PgConnectOptions::from_str(&url)?
            .options([("search_path", format!("{schema},public").as_str())]);
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await?;
        sqlx::query(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .execute(&pool)
            .await?;
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&pool)
            .await?;
        for migration in MIGRATIONS {
            sqlx::raw_sql(migration).execute(&pool).await?;
        }
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
        Ok(Self { schema, pool, cfg })
    }

    async fn close(self) -> TestResult {
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.pool)
            .await?;
        self.pool.close().await;
        Ok(())
    }

    /// A corpus of memories, each with one chunk and one thread, so every
    /// lane has rows to scan.
    async fn seed(&self) -> TestResult {
        for index in 0..SEEDED_MEMORIES {
            let body = format!(
                "Session {index}: Sol and Kintsu reviewed the substrate lattice, \
                 the tuner ledger, and the {} porch plan on 2026-08-{:02}.",
                ["shadowboxing", "notebook", "monitor", "bath"][(index % 4) as usize],
                1 + index % 28
            );
            let memory_id: i64 = sqlx::query_scalar(
                "INSERT INTO memories (room,type,title,source_path,body,date,dates,meta)
                 VALUES ($1,'memory',$2,$3,$4,$5::date,ARRAY[$5::date],'{}'::jsonb)
                 RETURNING id",
            )
            .bind(ROOM)
            .bind(format!("Session {index}"))
            .bind(format!("{ROOM}/session-{index}.md"))
            .bind(&body)
            .bind(format!("2026-08-{:02}", 1 + index % 28))
            .fetch_one(&self.pool)
            .await?;
            sqlx::query(
                "INSERT INTO memory_chunks (memory_id,chunk_index,body,char_start,char_end)
                 VALUES ($1,0,$2,0,$3)",
            )
            .bind(memory_id)
            .bind(&body)
            .bind(i32::try_from(body.len())?)
            .execute(&self.pool)
            .await?;
            let thread_id: i64 = sqlx::query_scalar(
                "INSERT INTO threads (room,thread_key) VALUES ($1,$2)
                 ON CONFLICT (room,thread_key) DO UPDATE SET thread_key=EXCLUDED.thread_key
                 RETURNING id",
            )
            .bind(ROOM)
            .bind(format!("porch-{}", index % 10))
            .fetch_one(&self.pool)
            .await?;
            let event_id: i64 = sqlx::query_scalar(
                "INSERT INTO thread_events (thread_id,memory_id) VALUES ($1,$2) RETURNING id",
            )
            .bind(thread_id)
            .bind(memory_id)
            .fetch_one(&self.pool)
            .await?;
            sqlx::query("INSERT INTO memory_thread_refs (context,event_id) VALUES ($1,$2)")
                .bind("substrate lattice review")
                .bind(event_id)
                .execute(&self.pool)
                .await?;
        }
        sqlx::query(
            "INSERT INTO named_entities (room,name,kind,summary,aliases,weighty)
             VALUES ($1,'Substrate Lattice','system','The Rust substrate under the House.',
                     ARRAY['lattice'],true)",
        )
        .bind(ROOM)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(Debug)]
struct EndRow {
    span_id: String,
    trace_id: String,
    parent_span_id: Option<String>,
    operation: String,
    duration_us: Option<i64>,
    outcome_class: String,
}

async fn end_rows(pool: &PgPool, binding: &TrustedBinding) -> TestResult<Vec<EndRow>> {
    let rows = sqlx::query(
        "SELECT span_id::text AS span_id, trace_id::text AS trace_id,
                parent_span_id::text AS parent_span_id, operation, duration_us, outcome_class
         FROM insula.log
         WHERE session_id = $1 AND phase = 'end'
         ORDER BY writer_sequence",
    )
    .bind(&binding.session_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(EndRow {
                span_id: row.try_get("span_id")?,
                trace_id: row.try_get("trace_id")?,
                parent_span_id: row.try_get("parent_span_id")?,
                operation: row.try_get("operation")?,
                duration_us: row.try_get("duration_us")?,
                outcome_class: row.try_get("outcome_class")?,
            })
        })
        .collect()
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn every_recall_phase_is_a_child_span_with_a_duration() -> TestResult {
    let db = Isolated::open("recallspans").await?;
    db.seed().await?;
    init_insula_emitter(db.pool.clone());

    let binding = TrustedBinding {
        house_id: "solarisael".into(),
        room: ROOM.into(),
        spirit: "proof".into(),
        session_id: format!("test:recall-spans:{}", uuid::Uuid::new_v4()),
    };
    let request = RecallRequest::new(
        RoomKey::new(ROOM)?,
        "substrate lattice porch plan with Kintsu on 2026-08-07".into(),
        8,
        0.4,
        8,
        0.3,
    )?;

    let started = Instant::now();
    let span = start_span(&binding, "akasha", "substrate", "recall");
    let result = recall(&db.pool, &db.cfg, request, span.as_ref()).await;
    end_span(span, akasha::OutcomeClass::Ok, None);
    let wall_us = started.elapsed().as_micros();
    let result = result?;
    assert!(result.found, "the seeded corpus must be recalled");

    flush_insula_emitter().await;
    let rows = end_rows(&db.pool, &binding).await?;

    let parents: Vec<&EndRow> = rows
        .iter()
        .filter(|row| row.operation == "recall")
        .collect();
    assert_eq!(parents.len(), 1, "exactly one recall span: {rows:?}");
    let parent = parents[0];
    assert!(parent.parent_span_id.is_none());

    let mut by_phase: BTreeMap<&str, &EndRow> = BTreeMap::new();
    for row in rows.iter().filter(|row| row.operation != "recall") {
        assert_eq!(
            row.parent_span_id.as_deref(),
            Some(parent.span_id.as_str()),
            "{} must be a child of the recall span",
            row.operation
        );
        assert_eq!(row.trace_id, parent.trace_id, "{} shares the trace", row.operation);
        assert!(
            row.duration_us.is_some(),
            "{} must end with a duration",
            row.operation
        );
        assert!(
            matches!(row.outcome_class.as_str(), "ok" | "degraded"),
            "{} ended {}",
            row.operation,
            row.outcome_class
        );
        assert!(
            by_phase.insert(row.operation.as_str(), row).is_none(),
            "{} observed twice",
            row.operation
        );
    }
    let observed: Vec<&str> = by_phase.keys().copied().collect();
    let mut expected = EXPECTED_PHASES.to_vec();
    expected.sort_unstable();
    assert_eq!(observed, expected, "phase set");

    let children_us: i64 = by_phase
        .values()
        .filter_map(|row| row.duration_us)
        .sum();
    let parent_us = parent.duration_us.expect("parent duration");
    assert!(
        children_us <= parent_us,
        "children {children_us}us exceed parent {parent_us}us"
    );

    println!("recall phase breakdown ({SEEDED_MEMORIES} memories, semantic lane disabled)");
    println!("{:<26}{:>12}  {}", "phase", "ms", "outcome");
    let mut ordered: Vec<&EndRow> = by_phase.values().copied().collect();
    ordered.sort_by(|a, b| b.duration_us.cmp(&a.duration_us));
    for row in ordered {
        println!(
            "{:<26}{:>12.3}  {}",
            row.operation,
            row.duration_us.unwrap_or(0) as f64 / 1000.0,
            row.outcome_class
        );
    }
    println!(
        "{:<26}{:>12.3}  (children {:.3} ms, wall {:.3} ms)",
        "recall",
        parent_us as f64 / 1000.0,
        children_us as f64 / 1000.0,
        wall_us as f64 / 1000.0
    );

    db.close().await
}

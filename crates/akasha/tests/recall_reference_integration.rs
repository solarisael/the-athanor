//! Recall answers an explicit memory reference by primary key inside the
//! room's scope, and hands a named canon entity back whole. A manual
//! projection reads the selected records whole; an exact-ID-only query is
//! answered by those records and their warnings alone.
//!
//! Run with the isolated PostgreSQL pair:
//! `ATHANOR_SUBSTRATE_TEST_DATABASE_URL=... ATHANOR_SUBSTRATE_TEST_SCHEMA=solarisael_tuner_test_<you>`
//! `cargo test -p akasha --test recall_reference_integration -- --ignored`

use akasha::{Config, EmbeddingMode, RecallResult, recall};
use hearth::{MANUAL_RECORD_CAP, RecallProjection, RecallRequest, RoomKey};
use sqlx::{
    PgPool,
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

const ROOM: &str = "reference-proof";
const OTHER_ROOM: &str = "reference-other";

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
    /// Each proof owns `<ATHANOR_SUBSTRATE_TEST_SCHEMA>_<suffix>` so the two
    /// proofs can run in the same process. `public` stays on the search path
    /// only for the extensions (`pg_trgm`, `vector`) the migrations rely on.
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
        let options = PgConnectOptions::from_str(&url)?;
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        sqlx::query(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
            .execute(&pool)
            .await?;
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&pool)
            .await?;
        sqlx::query(&format!("SET search_path TO {schema}, public"))
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
            nats_url: None,
        };
        Ok(Self { schema, pool, cfg })
    }

    async fn close(self) -> TestResult {
        sqlx::query("SET search_path TO public")
            .execute(&self.pool)
            .await?;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.pool)
            .await?;
        self.pool.close().await;
        Ok(())
    }

    async fn memory(&self, room: &str, title: &str, body: &str) -> TestResult<i64> {
        let source_path = format!("{room}/{}.md", title.replace(' ', "-"));
        let memory_id: i64 = sqlx::query_scalar(
            "INSERT INTO memories (room,type,title,source_path,body,meta)
             VALUES ($1,'memory',$2,$3,$4,'{}'::jsonb) RETURNING id",
        )
        .bind(room)
        .bind(title)
        .bind(&source_path)
        .bind(body)
        .fetch_one(&self.pool)
        .await?;
        sqlx::query(
            "INSERT INTO memory_chunks (memory_id,chunk_index,body,char_start,char_end)
             VALUES ($1,0,$2,0,$3)",
        )
        .bind(memory_id)
        .bind(body)
        .bind(i32::try_from(body.len())?)
        .execute(&self.pool)
        .await?;
        Ok(memory_id)
    }

    async fn recall(&self, room: &str, query: &str) -> TestResult<RecallResult> {
        self.recall_with(room, query, RecallProjection::Auto).await
    }

    async fn recall_with(
        &self,
        room: &str,
        query: &str,
        projection: RecallProjection,
    ) -> TestResult<RecallResult> {
        Ok(recall(
            &self.pool,
            &self.cfg,
            RecallRequest::new(RoomKey::new(room)?, query.into(), 8, 0.4, 8, 0.3)?
                .with_projection(projection),
            None,
        )
        .await?)
    }
}

fn memory_ids(candidates: &[serde_json::Value]) -> Vec<i64> {
    candidates
        .iter()
        .filter_map(|candidate| candidate["memory_id"].as_i64())
        .collect()
}

fn mentions_id(candidates: &[serde_json::Value], id: i64) -> bool {
    let needle = id.to_string();
    candidates.iter().any(|candidate| {
        candidate["missing_terms"]
            .as_array()
            .is_some_and(|terms| terms.iter().any(|term| term.as_str() == Some(&needle)))
    })
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn exact_memory_reference_leads_evidence_inside_room_scope() -> TestResult {
    let db = Isolated::open("memory_reference").await?;
    let target = db
        .memory(
            ROOM,
            "monitor analysis",
            "Analysis Sol made with Kintsu about the monitor and the notebook.",
        )
        .await?;
    let house = db
        .memory(
            "house",
            "house standing rule",
            "A House-scoped rule every room may read.",
        )
        .await?;
    let foreign = db
        .memory(
            OTHER_ROOM,
            "private other room",
            "Zanzibar quarantine ledger that must never leak across rooms.",
        )
        .await?;

    // `memory <id>` resolves the in-scope row first and never lists the ID as missing.
    let result = db
        .recall(
            ROOM,
            &format!("memory {target} — analysis Sol made with Kintsu"),
        )
        .await?;
    assert!(result.found);
    let first = result
        .retrieval_candidates
        .first()
        .ok_or("exact reference must produce evidence")?;
    assert_eq!(first["memory_id"].as_i64(), Some(target));
    assert_eq!(first["source"].as_str(), Some("exact_id"));
    assert_eq!(first["missing_terms"], serde_json::json!([]));
    assert!(
        first["reasons"]
            .as_array()
            .is_some_and(|reasons| reasons.iter().any(|r| r == "exact memory id")),
        "exact row must say why it leads: {first}"
    );
    assert!(
        !mentions_id(&result.retrieval_candidates, target),
        "a resolved ID must not appear under missing_terms"
    );
    assert_eq!(
        memory_ids(&result.retrieval_candidates)
            .iter()
            .filter(|id| **id == target)
            .count(),
        1,
        "the exact row owns its memory's single seat"
    );
    assert!(
        !result
            .warnings
            .iter()
            .any(|w| w.contains(&target.to_string())),
        "an in-scope resolution raises no warning: {:?}",
        result.warnings
    );

    // `#<id>` alone and a bare integer alone both resolve.
    for query in [format!("#{target}"), target.to_string()] {
        let result = db.recall(ROOM, &query).await?;
        assert_eq!(
            memory_ids(&result.retrieval_candidates).first(),
            Some(&target),
            "{query} must resolve"
        );
    }

    // The House room is inside every room's scope; the word `memory` itself
    // still ranks rows by type below the exact row.
    let result = db.recall(ROOM, &format!("memory {house}")).await?;
    assert_eq!(
        memory_ids(&result.retrieval_candidates).first(),
        Some(&house)
    );
    assert_eq!(
        result.retrieval_candidates[0]["source"].as_str(),
        Some("exact_id")
    );

    // Another room's row is refused by ID and its body never leaves the database.
    let result = db
        .recall(ROOM, &format!("memory {foreign} quarantine ledger"))
        .await?;
    assert!(
        !memory_ids(&result.retrieval_candidates).contains(&foreign),
        "out-of-scope row leaked: {:?}",
        result.retrieval_candidates
    );
    let rendered = serde_json::to_string(&result)?;
    assert!(
        !rendered.contains("Zanzibar"),
        "out-of-scope body leaked through another lane"
    );
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w == &format!("memory {foreign} refused: outside room scope")),
        "refusal must be visible: {:?}",
        result.warnings
    );

    // An unknown ID is reported, not silently dropped.
    let result = db.recall(ROOM, "memory 999999999").await?;
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w == "memory 999999999 not found")
    );

    // A query without a reference keeps ordinary ranked behavior.
    let result = db.recall(ROOM, "analysis Sol made with Kintsu").await?;
    assert!(
        result
            .retrieval_candidates
            .iter()
            .all(|candidate| candidate["source"].as_str() != Some("exact_id")),
        "ranked search must not invent an exact row"
    );
    assert!(
        memory_ids(&result.retrieval_candidates).contains(&target),
        "ranked lanes still find the row by its words"
    );

    // A date part is never mistaken for a memory ID.
    let result = db.recall(ROOM, "memory from 2026-08-28").await?;
    assert!(
        !result.warnings.iter().any(|w| w.starts_with("memory 2026")),
        "date digits must not become a reference: {:?}",
        result.warnings
    );

    db.close().await
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn manual_projection_reads_whole_records_and_exact_only_queries_skip_ranked_lanes()
-> TestResult {
    let db = Isolated::open("manual_projection").await?;
    let tail = "TAIL-MARKER-PAST-TWELVE-HUNDRED";
    let long_body = format!(
        "{}{tail}",
        "The lattice record Sol and Kodo kept about the porch plan. ".repeat(40)
    );
    assert!(long_body.chars().count() > 1200 + 100);
    let long = db.memory(ROOM, "long lattice record", &long_body).await?;
    let second = db
        .memory(
            ROOM,
            "second lattice record",
            "A shorter lattice record about the same porch.",
        )
        .await?;
    let foreign = db
        .memory(
            OTHER_ROOM,
            "foreign lattice",
            "Zanzibar lattice that must never leak.",
        )
        .await?;
    let missing = 999_999_999_999_999_999_i64;

    // Exact-ID-only, manual: the named records whole, in order, plus warnings,
    // and nothing ranked under them for the words `memories` and `and`.
    let query = format!("memories {long}, {second}, {foreign} and {missing}");
    let result = db
        .recall_with(ROOM, &query, RecallProjection::Manual)
        .await?;
    assert_eq!(result.projection, "manual");
    assert!(result.found);
    assert_eq!(memory_ids(&result.retrieval_candidates), vec![long, second]);
    assert!(
        result
            .retrieval_candidates
            .iter()
            .all(|candidate| candidate["source"].as_str() == Some("exact_id")),
        "an exact-only query carries no ranked filler: {:?}",
        result.retrieval_candidates
    );
    assert!(result.canon_matches.is_empty() && result.date_matches.is_empty());
    assert!(
        result
            .warnings
            .contains(&format!("memory {missing} not found")),
        "missing IDs are named: {:?}",
        result.warnings
    );
    assert!(
        result
            .warnings
            .contains(&format!("memory {foreign} refused: outside room scope")),
        "refused IDs are named: {:?}",
        result.warnings
    );
    let first = &result.retrieval_candidates[0];
    assert_eq!(
        first["body"].as_str(),
        Some(long_body.as_str()),
        "manual carries the whole record"
    );
    assert!(
        first["body"]
            .as_str()
            .is_some_and(|body| body.ends_with(tail))
    );
    let excerpt = first["excerpt"].as_str().unwrap_or_default();
    assert_eq!(
        excerpt.chars().count(),
        1201,
        "the excerpt stays bounded beside the body"
    );
    assert!(excerpt.ends_with('…') && !excerpt.contains(tail));
    assert!(!serde_json::to_string(&result)?.contains("Zanzibar"));

    // The same exact-only query under the auto projection: same records and
    // warnings, excerpt only, no body key at all.
    let auto = db.recall(ROOM, &query).await?;
    assert_eq!(auto.projection, "auto");
    assert_eq!(memory_ids(&auto.retrieval_candidates), vec![long, second]);
    assert!(
        auto.retrieval_candidates
            .iter()
            .all(|candidate| candidate.get("body").is_none() && candidate["source"] == "exact_id"),
        "auto stays excerpt-bounded and unranked for an exact-only query"
    );
    assert!(
        auto.warnings
            .contains(&format!("memory {missing} not found"))
    );

    // A single explicit reference is also exact-only in both projections.
    for projection in [RecallProjection::Auto, RecallProjection::Manual] {
        let result = db
            .recall_with(ROOM, &format!("memory {long}"), projection)
            .await?;
        assert_eq!(memory_ids(&result.retrieval_candidates), vec![long]);
        assert_eq!(
            result.retrieval_candidates[0].get("body").is_some(),
            projection.is_manual()
        );
    }

    // A mixed query keeps its ranked lanes: the exact row leads and the
    // natural words still rank the other record beneath it.
    let mixed = db
        .recall_with(
            ROOM,
            &format!("memory {long} lattice porch"),
            RecallProjection::Manual,
        )
        .await?;
    assert_eq!(memory_ids(&mixed.retrieval_candidates).first(), Some(&long));
    assert_eq!(mixed.retrieval_candidates[0]["source"], "exact_id");
    assert!(
        memory_ids(&mixed.retrieval_candidates).contains(&second),
        "ranked lanes still run for a mixed query: {:?}",
        mixed.retrieval_candidates
    );

    // Natural search, manual: every selected record is hydrated to its whole
    // body by primary key inside room scope; no exact row is invented.
    let natural = db
        .recall_with(ROOM, "lattice record porch", RecallProjection::Manual)
        .await?;
    assert!(natural.found);
    assert!(natural.retrieval_candidates.len() <= MANUAL_RECORD_CAP);
    assert!(
        natural
            .retrieval_candidates
            .iter()
            .all(|candidate| candidate["source"] != "exact_id")
    );
    let hydrated_long = natural
        .retrieval_candidates
        .iter()
        .find(|candidate| candidate["memory_id"].as_i64() == Some(long))
        .ok_or("natural search must select the long record by its words")?;
    assert_eq!(hydrated_long["body"].as_str(), Some(long_body.as_str()));
    assert!(
        natural
            .retrieval_candidates
            .iter()
            .filter(|candidate| candidate["memory_id"].is_i64())
            .all(|candidate| candidate["body"].is_string()),
        "every manual candidate with a memory carries its body: {:?}",
        natural.retrieval_candidates
    );
    assert!(!serde_json::to_string(&natural)?.contains("Zanzibar"));

    // Natural search, auto: the same selection stays excerpt-bounded.
    let natural_auto = db.recall(ROOM, "lattice record porch").await?;
    assert!(
        natural_auto
            .retrieval_candidates
            .iter()
            .all(|candidate| candidate.get("body").is_none()),
        "auto never carries a body: {:?}",
        natural_auto.retrieval_candidates
    );
    assert!(
        natural_auto
            .retrieval_candidates
            .iter()
            .filter(|candidate| candidate["memory_id"].as_i64() == Some(long))
            .all(|candidate| !candidate["excerpt"]
                .as_str()
                .unwrap_or_default()
                .contains(tail)),
        "the auto excerpt of the long record stops before its tail"
    );

    // The record cap trims by count and names what it dropped, for a long
    // exact list and for a wide natural selection alike.
    let mut many = vec![long, second];
    for index in 0..MANUAL_RECORD_CAP {
        many.push(
            db.memory(
                ROOM,
                &format!("lattice record {index}"),
                &format!("Lattice record number {index} about the porch plan."),
            )
            .await?,
        );
    }
    let list = many
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let capped = db
        .recall_with(ROOM, &format!("memories {list}"), RecallProjection::Manual)
        .await?;
    assert_eq!(
        memory_ids(&capped.retrieval_candidates),
        many[..MANUAL_RECORD_CAP].to_vec(),
        "reference order leads; the tail past the cap is dropped"
    );
    let dropped = many[MANUAL_RECORD_CAP..]
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    assert!(
        capped.warnings.contains(&format!(
            "manual record cap {MANUAL_RECORD_CAP}: {} selected records dropped (memories {dropped})",
            many.len() - MANUAL_RECORD_CAP
        )),
        "the cap names every dropped record: {:?}",
        capped.warnings
    );
    let uncapped_auto = db.recall(ROOM, &format!("memories {list}")).await?;
    assert_eq!(
        memory_ids(&uncapped_auto.retrieval_candidates),
        many,
        "the auto projection keeps every resolved reference; the viewport budgets it"
    );
    let wide = db
        .recall_with(ROOM, "lattice record porch plan", RecallProjection::Manual)
        .await?;
    assert_eq!(wide.retrieval_candidates.len(), MANUAL_RECORD_CAP);
    assert!(
        wide.warnings
            .iter()
            .any(|w| w.starts_with(&format!("manual record cap {MANUAL_RECORD_CAP}:"))),
        "a wide natural selection reports its trim: {:?}",
        wide.warnings
    );
    assert!(
        wide.retrieval_candidates
            .iter()
            .all(|candidate| candidate["body"].is_string()),
        "every kept record is whole"
    );

    db.close().await
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn named_weighty_canon_returns_its_complete_assertion() -> TestResult {
    let db = Isolated::open("canon_assertion").await?;
    let sentence = "The Athanor is the House platform: PostgreSQL is authoritative, canon outranks loose memory, and markdown on disk is provenance. ";
    let long_summary = sentence.repeat(20);
    assert!(long_summary.chars().count() > 1200);
    let entity_id: i64 = sqlx::query_scalar(
        "INSERT INTO named_entities (room,name,kind,summary,aliases,weighty)
         VALUES ('house','The Athanor','platform',$1,ARRAY['Athanor'],TRUE) RETURNING id",
    )
    .bind(&long_summary)
    .fetch_one(&db.pool)
    .await?;
    let hint_id: i64 = sqlx::query_scalar(
        "INSERT INTO named_entities (room,name,kind,summary,aliases,weighty)
         VALUES ('house','Provenance Ledger','concept',$1,'{}',FALSE) RETURNING id",
    )
    .bind(&long_summary)
    .fetch_one(&db.pool)
    .await?;

    // Reorientation by name inside a longer sentence.
    let result = db
        .recall(ROOM, "reorient me on The Athanor before I decide")
        .await?;
    let entry = result
        .canon_matches
        .iter()
        .find(|m| m["termKey"].as_str() == Some("The Athanor"))
        .ok_or("named entity must resolve")?;
    assert_eq!(entry["entry"]["id"].as_i64(), Some(entity_id));
    assert_eq!(entry["entry"]["exact"], serde_json::json!(true));
    assert_eq!(entry["entry"]["truncated"], serde_json::json!(false));
    assert_eq!(entry["entry"]["weighty"], serde_json::json!(true));
    assert_eq!(
        entry["entry"]["summary"].as_str(),
        Some(long_summary.as_str()),
        "an exact match carries the complete active assertion"
    );

    // Reorientation by alias, as the whole query.
    let result = db.recall(ROOM, "Athanor").await?;
    let entry = result.canon_matches.first().ok_or("alias must resolve")?;
    assert_eq!(entry["termKey"].as_str(), Some("The Athanor"));
    assert_eq!(entry["entry"]["exact"], serde_json::json!(true));
    assert_eq!(
        entry["entry"]["summary"].as_str(),
        Some(long_summary.as_str())
    );

    // A similarity-tier hit the caller did not name stays an excerpt, and the
    // cut is explicit with a deterministic full read.
    let result = db.recall(ROOM, "markdown provenance").await?;
    let hint = result
        .canon_matches
        .iter()
        .find(|m| m["entry"]["id"].as_i64() == Some(hint_id))
        .ok_or("similarity tier must still surface the unnamed row")?;
    assert_eq!(hint["entry"]["exact"], serde_json::json!(false));
    assert_eq!(hint["entry"]["truncated"], serde_json::json!(true));
    assert_eq!(
        hint["entry"]["full_read"].as_str(),
        Some(format!("canon_read {hint_id}").as_str())
    );
    let excerpt = hint["entry"]["summary"].as_str().unwrap_or_default();
    assert!(excerpt.ends_with('…'), "a cut excerpt must end visibly");
    assert!(excerpt.chars().count() < long_summary.chars().count());

    db.close().await
}

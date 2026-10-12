//! The operator axis on real PostgreSQL. A House with no operators recalls
//! exactly what it recalled before migration 0035. A House with operators
//! shows reader R a memory when R wrote it, when it is company-wide, when it
//! predates the axis, or when it travels up to R's unit at a higher rank.
//!
//! Run with the isolated PostgreSQL pair:
//! `ATHANOR_SUBSTRATE_TEST_DATABASE_URL=... ATHANOR_SUBSTRATE_TEST_SCHEMA=solarisael_tuner_test_<you>`
//! `cargo test -p akasha --test operator_scope_integration -- --ignored`

use akasha::migrations::run_migrations;
use akasha::{Config, EmbeddingMode, RecallResult, canon_read, canon_write, recall, remember};
use hearth::{
    CanonAttribution, CanonReadRequest, CanonWriteRequest, MemoryScope, OperatorKey, RecallRequest,
    RememberMemoryDetails, RememberRequest, RoomKey,
};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::collections::BTreeSet;
use std::str::FromStr;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const ROOM: &str = "operator-proof";
const TERM: &str = "lighthouse";
const UNREGISTERED: &str = "operator not registered";

fn isolated_database_url() -> String {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
        .expect("dedicated test database URL must be configured when this proof is run");
    let options: PgConnectOptions = url.parse().expect("valid test URL");
    let database = options
        .get_database()
        .expect("explicit test database")
        .to_ascii_lowercase();
    assert!(
        database.contains("test") && !database.contains("solarisael"),
        "refusing a non-test or live database, including percent-encoded names"
    );
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
        "refusing the live database"
    );
    url
}

struct Isolated {
    schema: String,
    pool: PgPool,
    cfg: Config,
}

impl Isolated {
    /// A fresh schema with no migrations applied. `public` stays on the path
    /// only for the extensions the migrations rely on.
    async fn bare(suffix: &str) -> TestResult<Self> {
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
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_with(PgConnectOptions::from_str(&url)?)
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

    /// The owned lineage through the real migration runner.
    async fn migrated(suffix: &str) -> TestResult<Self> {
        let db = Self::bare(suffix).await?;
        run_migrations(&db.pool).await?;
        Ok(db)
    }

    /// Every migration file whose version `wanted` admits, in order, as raw SQL.
    async fn apply_files(&self, wanted: impl Fn(u32) -> bool) -> TestResult {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../substrate/migrations");
        let mut files = std::fs::read_dir(&directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        files.sort();
        for path in files {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let Some(file_version) = name.get(..4).and_then(|digits| digits.parse::<u32>().ok())
            else {
                continue;
            };
            if wanted(file_version) {
                sqlx::raw_sql(&std::fs::read_to_string(&path)?)
                    .execute(&self.pool)
                    .await?;
            }
        }
        Ok(())
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

    /// owner (acme, rank 10) above staff and peer (acme.eng, rank 1 each).
    async fn seed_company(&self) -> TestResult {
        sqlx::query(
            "INSERT INTO units (path, display_name) VALUES
               ('acme'::ltree, 'Acme'), ('acme.eng'::ltree, 'Engineering')",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "INSERT INTO operators (key, display_name, unit, rank) VALUES
               ('owner', 'Owner', 'acme'::ltree, 10),
               ('staff', 'Staff', 'acme.eng'::ltree, 1),
               ('peer', 'Peer', 'acme.eng'::ltree, 1)",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn remember(
        &self,
        writer: Option<&str>,
        scope: Option<MemoryScope>,
        marker: &str,
    ) -> TestResult<i64> {
        let request = RememberRequest::new_memory(
            RoomKey::for_memory_write(ROOM)?,
            format!("{TERM} {marker}"),
            format!("The {TERM} keeper logged {marker} at dusk."),
            RememberMemoryDetails {
                source_path: None,
                threads: vec![],
                continues: vec![],
                supersedes: vec![],
                backup: false,
            },
        )?
        .with_operator(writer.map(OperatorKey::new).transpose()?, scope)?;
        let receipt = remember(&self.pool, &self.cfg, request).await?;
        Ok(i64::try_from(receipt.memory_id)?)
    }

    /// A row written before 0035 existed: the write path now names the
    /// operator column, so a pre-axis House's rows are seeded as it stored them.
    async fn pre_axis_memory(&self, marker: &str) -> TestResult<i64> {
        let body = format!("The {TERM} keeper logged {marker} at dusk.");
        let memory_id: i64 = sqlx::query_scalar(
            "INSERT INTO memories (room,type,title,source_path,body,meta)
             VALUES ($1,'memory',$2,$3,$4,'{}'::jsonb) RETURNING id",
        )
        .bind(ROOM)
        .bind(format!("{TERM} {marker}"))
        .bind(format!("pre-axis/{marker}.md"))
        .bind(&body)
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
        Ok(memory_id)
    }

    async fn recall_as(&self, reader: Option<&str>, query: &str) -> TestResult<RecallResult> {
        let request = RecallRequest::new(RoomKey::new(ROOM)?, query.into(), 8, 0.4, 8, 0.3)?
            .with_operator(reader.map(OperatorKey::new).transpose()?);
        Ok(recall(&self.pool, &self.cfg, request, None).await?)
    }

    async fn visible_to(&self, reader: Option<&str>) -> TestResult<BTreeSet<i64>> {
        let result = self.recall_as(reader, TERM).await?;
        Ok(result
            .retrieval_candidates
            .iter()
            .chain(result.content_chunks.iter())
            .filter_map(|candidate| candidate["memory_id"].as_i64())
            .collect())
    }
}

/// The memories one proof seeds, by writer and scope.
struct Seeded {
    legacy: i64,
    staff_up: i64,
    owner_up: i64,
    peer_up: i64,
    staff_private: i64,
    staff_company: i64,
}

async fn seed_memories(db: &Isolated) -> TestResult<Seeded> {
    Ok(Seeded {
        legacy: db.remember(None, None, "legacy-row").await?,
        staff_up: db
            .remember(Some("staff"), Some(MemoryScope::Up), "staff-up")
            .await?,
        owner_up: db.remember(Some("owner"), None, "owner-up").await?,
        peer_up: db
            .remember(Some("peer"), Some(MemoryScope::Up), "peer-up")
            .await?,
        staff_private: db
            .remember(Some("staff"), Some(MemoryScope::Private), "staff-private")
            .await?,
        staff_company: db
            .remember(Some("staff"), Some(MemoryScope::Company), "staff-company")
            .await?,
    })
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn empty_operators_recall_exactly_what_the_pre_axis_house_recalled() -> TestResult {
    let db = Isolated::bare("operator_baseline").await?;
    db.apply_files(|version| version < 35).await?;
    for marker in ["first-entry", "second-entry", "third-entry"] {
        db.pre_axis_memory(marker).await?;
    }
    let before = db.recall_as(None, TERM).await?;
    assert!(
        before.retrieval_candidates.len() == 3,
        "the baseline must find every seeded row: {:?}",
        before.retrieval_candidates
    );

    db.apply_files(|version| version == 35).await?;
    let operators: i64 = sqlx::query_scalar("SELECT count(*) FROM operators")
        .fetch_one(&db.pool)
        .await?;
    assert_eq!(operators, 0);
    for reader in [None, Some("sol")] {
        let after = db.recall_as(reader, TERM).await?;
        assert_eq!(after.retrieval_candidates, before.retrieval_candidates);
        assert_eq!(after.content_chunks, before.content_chunks);
        assert_eq!(after.warnings, before.warnings);
    }
    db.close().await
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn up_scope_travels_only_to_a_higher_rank_at_or_above_the_writer_unit() -> TestResult {
    let db = Isolated::migrated("operator_up").await?;
    db.seed_company().await?;
    let seeded = seed_memories(&db).await?;

    let owner = db.visible_to(Some("owner")).await?;
    assert!(owner.contains(&seeded.staff_up), "owner reads staff up");
    assert!(owner.contains(&seeded.peer_up), "owner reads peer up");

    let staff = db.visible_to(Some("staff")).await?;
    assert!(
        !staff.contains(&seeded.owner_up),
        "staff cannot read owner up"
    );
    assert!(!staff.contains(&seeded.peer_up), "peer cannot read peer up");
    assert!(
        staff.contains(&seeded.staff_up),
        "a writer reads its own up"
    );

    let peer = db.visible_to(Some("peer")).await?;
    assert!(!peer.contains(&seeded.staff_up), "peer cannot read peer up");

    let scope: String = sqlx::query_scalar("SELECT scope FROM memories WHERE id=$1")
        .bind(seeded.owner_up)
        .fetch_one(&db.pool)
        .await?;
    assert_eq!(scope, "up", "an operator's write defaults to up");
    db.close().await
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn private_memories_reach_only_their_writer_even_by_exact_id() -> TestResult {
    let db = Isolated::migrated("operator_private").await?;
    db.seed_company().await?;
    let seeded = seed_memories(&db).await?;

    assert!(
        db.visible_to(Some("staff"))
            .await?
            .contains(&seeded.staff_private)
    );
    for reader in ["owner", "peer"] {
        assert!(
            !db.visible_to(Some(reader))
                .await?
                .contains(&seeded.staff_private),
            "{reader} must not read staff private"
        );
        let exact = db
            .recall_as(Some(reader), &format!("memory {}", seeded.staff_private))
            .await?;
        assert!(exact.retrieval_candidates.is_empty());
        assert!(
            exact
                .warnings
                .contains(&format!("memory {} not found", seeded.staff_private)),
            "a hidden row reads as not found: {:?}",
            exact.warnings
        );
    }
    db.close().await
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn pre_axis_and_company_rows_reach_every_reader() -> TestResult {
    let db = Isolated::migrated("operator_commons").await?;
    db.seed_company().await?;
    let seeded = seed_memories(&db).await?;
    for reader in [
        Some("owner"),
        Some("staff"),
        Some("peer"),
        Some("ghost"),
        None,
    ] {
        let visible = db.visible_to(reader).await?;
        assert!(
            visible.contains(&seeded.legacy),
            "{reader:?} reads the pre-axis row"
        );
        assert!(
            visible.contains(&seeded.staff_company),
            "{reader:?} reads the company row"
        );
    }
    db.close().await
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn an_unregistered_reader_is_warned_and_sees_only_commons() -> TestResult {
    let db = Isolated::migrated("operator_ghost").await?;
    db.seed_company().await?;
    let seeded = seed_memories(&db).await?;
    let ghost = db.recall_as(Some("ghost"), TERM).await?;
    assert!(
        ghost.warnings.iter().any(|warning| warning == UNREGISTERED),
        "{:?}",
        ghost.warnings
    );
    let visible = db.visible_to(Some("ghost")).await?;
    assert_eq!(
        visible,
        BTreeSet::from([seeded.legacy, seeded.staff_company])
    );
    let registered = db.recall_as(Some("owner"), TERM).await?;
    assert!(
        !registered
            .warnings
            .iter()
            .any(|warning| warning == UNREGISTERED)
    );
    db.close().await
}

// ---------------------------------------------------------------------------
// Attack 6: `up` scope edges. One tree for every edge:
//   acme (owner 10) > acme.eng (dev-a 5, dev-b 5, lead 7)
//                   > acme.eng.platform (platform-head 9)
//                   > acme.ops (ops-head 9)
//   globex (outsider 100)
// ---------------------------------------------------------------------------

async fn seed_tree(db: &Isolated) -> TestResult {
    sqlx::query(
        "INSERT INTO units (path, display_name) VALUES
           ('acme'::ltree, 'Acme'), ('acme.eng'::ltree, 'Engineering'),
           ('acme.eng.platform'::ltree, 'Platform'), ('acme.ops'::ltree, 'Operations'),
           ('globex'::ltree, 'Globex')",
    )
    .execute(&db.pool)
    .await?;
    sqlx::query(
        "INSERT INTO operators (key, display_name, unit, rank) VALUES
           ('owner', 'Owner', 'acme'::ltree, 10),
           ('dev-a', 'Dev A', 'acme.eng'::ltree, 5),
           ('dev-b', 'Dev B', 'acme.eng'::ltree, 5),
           ('lead', 'Lead', 'acme.eng'::ltree, 7),
           ('platform-head', 'Platform Head', 'acme.eng.platform'::ltree, 9),
           ('ops-head', 'Ops Head', 'acme.ops'::ltree, 9),
           ('outsider', 'Outsider', 'globex'::ltree, 100)",
    )
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// dev-a's `up` memory in a tree-seeded schema, with the control that the
/// lead (same unit, higher rank) reads it, so a reader's miss is the rule.
async fn dev_a_up(db: &Isolated) -> TestResult<i64> {
    seed_tree(db).await?;
    let up = db
        .remember(Some("dev-a"), Some(MemoryScope::Up), "dev-a-up")
        .await?;
    let lead = db.visible_to(Some("lead")).await?;
    assert!(
        lead.contains(&up),
        "control: lead must read dev-a's up row {up}; lead sees {lead:?}"
    );
    Ok(up)
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_up_scope_equal_rank_in_the_same_unit_does_not_read() -> TestResult {
    let db = Isolated::migrated("operator_up_equal").await?;
    let up = dev_a_up(&db).await?;
    let dev_b = db.visible_to(Some("dev-b")).await?;
    assert!(
        !dev_b.contains(&up),
        "dev-b (acme.eng, rank 5) reads dev-a's (acme.eng, rank 5) up row {up}: {dev_b:?}"
    );
    db.close().await
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_up_scope_higher_rank_outside_the_writer_lineage_does_not_read() -> TestResult {
    let db = Isolated::migrated("operator_up_sibling").await?;
    let up = dev_a_up(&db).await?;
    for (reader, seat) in [
        ("ops-head", "sibling acme.ops, rank 9"),
        ("platform-head", "child acme.eng.platform, rank 9"),
        ("outsider", "other tree globex, rank 100"),
    ] {
        let visible = db.visible_to(Some(reader)).await?;
        assert!(
            !visible.contains(&up),
            "{reader} ({seat}) reads dev-a's (acme.eng, rank 5) up row {up}: {visible:?}"
        );
    }
    db.close().await
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_up_scope_higher_rank_at_the_root_reads() -> TestResult {
    let db = Isolated::migrated("operator_up_root").await?;
    let up = dev_a_up(&db).await?;
    let owner = db.visible_to(Some("owner")).await?;
    assert!(
        owner.contains(&up),
        "owner (acme, rank 10) cannot read dev-a's (acme.eng, rank 5) up row {up}: {owner:?}"
    );
    db.close().await
}

/// A unit is a real seat: an operator needs a unit row, and a unit in use
/// cannot be removed. A writer is Host-validated text, not a foreign key, so
/// an unregistered writer still writes; its rows miss the writer join and
/// reach the writer and, when company-wide, every reader, and nobody else.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_unregistered_writer_rows_reach_only_their_writer_unless_company_wide() -> TestResult
{
    let db = Isolated::migrated("operator_up_unitless").await?;
    let up = dev_a_up(&db).await?;

    let unitless = sqlx::query(
        "INSERT INTO operators (key, display_name, unit, rank)
         VALUES ('drifter', 'Drifter', 'acme.nowhere'::ltree, 3)",
    )
    .execute(&db.pool)
    .await;
    let unit_removed = sqlx::query("DELETE FROM units WHERE path = 'acme.eng'::ltree")
        .execute(&db.pool)
        .await;
    assert!(
        unitless.is_err(),
        "an operator was registered in a unit with no row: {unitless:?}"
    );
    assert!(
        unit_removed.is_err(),
        "acme.eng was removed while operators sit in it: {unit_removed:?}"
    );

    let nobody_up = db
        .remember(Some("nobody"), Some(MemoryScope::Up), "nobody-up")
        .await?;
    let nobody_company = db
        .remember(Some("nobody"), Some(MemoryScope::Company), "nobody-company")
        .await?;
    let stored: (Option<String>, String) =
        sqlx::query_as("SELECT operator, scope FROM memories WHERE id = $1")
            .bind(nobody_up)
            .fetch_one(&db.pool)
            .await?;
    assert_eq!(stored, (Some("nobody".to_owned()), "up".to_owned()));
    assert!(
        db.visible_to(Some("nobody")).await?.contains(&nobody_up),
        "an unregistered writer must read its own up row"
    );
    for reader in [Some("owner"), Some("dev-a"), None] {
        let visible = db.visible_to(reader).await?;
        assert!(
            !visible.contains(&nobody_up),
            "{reader:?} reads an unregistered writer's up row"
        );
        assert!(
            visible.contains(&nobody_company),
            "{reader:?} misses an unregistered writer's company row"
        );
    }

    sqlx::query("DELETE FROM operators WHERE key = 'dev-a'")
        .execute(&db.pool)
        .await?;
    let dev_a_row: (Option<String>, String) =
        sqlx::query_as("SELECT operator, scope FROM memories WHERE id = $1")
            .bind(up)
            .fetch_one(&db.pool)
            .await?;
    assert_eq!(
        dev_a_row,
        (Some("dev-a".to_owned()), "up".to_owned()),
        "removing dev-a must not rewrite its rows"
    );
    assert!(
        db.visible_to(Some("dev-a")).await?.contains(&up),
        "dev-a lost its own up row once unregistered"
    );
    assert!(
        !db.visible_to(Some("owner")).await?.contains(&up),
        "owner still reads unregistered dev-a's up row"
    );
    db.close().await
}

// ---------------------------------------------------------------------------
// Attack 7: a Host names an operator while `operators` is empty.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_empty_operators_short_circuit_reads_writer_rows_for_all_until_one_operator_exists()
-> TestResult {
    let db = Isolated::migrated("operator_short_circuit").await?;
    let operators: i64 = sqlx::query_scalar("SELECT count(*) FROM operators")
        .fetch_one(&db.pool)
        .await?;
    assert_eq!(operators, 0);

    // The Host stamps `sol` on every write; the House has not named anyone.
    let written = db
        .remember(Some("sol"), Some(MemoryScope::Private), "sol-private")
        .await;
    let stored: Vec<(i64, Option<String>, String)> =
        sqlx::query_as("SELECT id, operator, scope FROM memories WHERE title LIKE '%sol-private%'")
            .fetch_all(&db.pool)
            .await?;
    let id = match written {
        Ok(id) => id,
        Err(refusal) => panic!(
            "with zero operators the House must write exactly as before 0035, but a Host-stamped write was refused: {refusal}; rows stored: {stored:?}"
        ),
    };
    for reader in [None, Some("sol"), Some("kodo")] {
        let visible = db.visible_to(reader).await?;
        assert!(
            visible.contains(&id),
            "inactive axis: {reader:?} cannot read row {id} {stored:?}: {visible:?}"
        );
    }

    sqlx::query("INSERT INTO units (path, display_name) VALUES ('acme'::ltree, 'Acme')")
        .execute(&db.pool)
        .await?;
    sqlx::query(
        "INSERT INTO operators (key, display_name, unit, rank)
         VALUES ('kodo', 'Kodo', 'acme'::ltree, 99)",
    )
    .execute(&db.pool)
    .await?;
    for reader in [None, Some("kodo")] {
        let visible = db.visible_to(reader).await?;
        assert!(
            !visible.contains(&id),
            "active axis: {reader:?} still reads sol's private row {id} {stored:?}"
        );
    }
    assert!(
        db.visible_to(Some("sol")).await?.contains(&id),
        "active axis: sol lost its own private row {id}"
    );
    db.close().await
}

// ---------------------------------------------------------------------------
// Attack 8: canon and named entities are not memories rows.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_canon_lookup_and_named_entities_ignore_the_operator_axis() -> TestResult {
    let db = Isolated::migrated("operator_canon").await?;
    db.seed_company().await?;
    seed_memories(&db).await?;
    let written = canon_write(
        &db.pool,
        CanonWriteRequest::new(
            ROOM.into(),
            "Lighthouse Keeper".into(),
            "person".into(),
            "Keeps the lighthouse lit through the storm season.".into(),
            vec![],
            None,
            false,
            vec![],
            None,
            vec![],
            CanonAttribution::new("crisol".into(), "operator-axis-proof".into())?,
        )?,
    )
    .await?;

    let axis_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema = current_schema() AND table_name = 'named_entities'
           AND column_name IN ('operator', 'scope')",
    )
    .fetch_one(&db.pool)
    .await?;
    assert_eq!(
        axis_columns, 0,
        "named_entities gained operator-axis columns"
    );

    let baseline = db.recall_as(None, TERM).await?.canon_matches;
    assert!(
        baseline
            .iter()
            .any(|entry| entry.to_string().contains(&written.entity_id)),
        "control: canon lookup must find entity {}: {baseline:#?}",
        written.entity_id
    );
    for reader in ["owner", "staff", "peer", "ghost"] {
        let canon = db.recall_as(Some(reader), TERM).await?.canon_matches;
        assert_eq!(
            canon, baseline,
            "{reader}'s canon matches differ from the operator-less reader's"
        );
    }
    let read = canon_read(
        &db.pool,
        CanonReadRequest::new(ROOM.into(), None, Some("Lighthouse Keeper".into()), false)?,
    )
    .await?;
    assert_eq!(
        read.entities
            .iter()
            .map(|entity| entity.entity_id.as_str())
            .collect::<Vec<_>>(),
        vec![written.entity_id.as_str()],
        "canon_read takes no operator and must return the entity unchanged"
    );
    db.close().await
}

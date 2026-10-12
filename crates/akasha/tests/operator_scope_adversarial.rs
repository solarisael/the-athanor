//! Attacks on the operator axis from a second operator in the same room.
//!
//! `staff` writes; `peer` (same unit, same rank) and `owner` (root unit,
//! higher rank) attack. Every test reads the rows and the full serialized
//! recall back from real PostgreSQL. A failing test names the leaking row.
//!
//! Run with the isolated PostgreSQL pair:
//! `ATHANOR_SUBSTRATE_TEST_DATABASE_URL=... ATHANOR_SUBSTRATE_TEST_SCHEMA=solarisael_tuner_test_<you>`
//! `cargo test -p akasha --test operator_scope_adversarial -- --ignored --test-threads=1`

use akasha::migrations::run_migrations;
use akasha::{
    Config, EmbeddingMode, MemoryReadParams, MemoryTimelineParams, memory_read, memory_timeline,
    recall, remember,
};
use hearth::{
    MemoryScope, OperatorKey, RecallProjection, RecallRequest, RememberMemoryDetails,
    RememberRequest, RoomKey, ThreadContinuation,
};
use serde_json::Value;
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::collections::BTreeSet;
use std::str::FromStr;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const ROOM: &str = "operator-attack";
const TERM: &str = "lighthouse";
/// Never part of any query, so finding it in a recall is a leak of a hidden
/// title, body or source path, never an echo of the question.
const CANARY: &str = "zircon-canary";

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

#[derive(Debug)]
#[allow(dead_code)] // every field is read through Debug in assertion messages
struct MemoryRow {
    id: i64,
    operator: Option<String>,
    scope: String,
    title: Option<String>,
    source_path: String,
    body: String,
    superseded_by: Option<i64>,
}

fn no_details() -> RememberMemoryDetails {
    RememberMemoryDetails {
        source_path: None,
        threads: vec![],
        continues: vec![],
        supersedes: vec![],
        backup: false,
    }
}

struct Isolated {
    schema: String,
    pool: PgPool,
    cfg: Config,
}

impl Isolated {
    /// A migrated schema of its own with owner (acme, rank 10) above staff and
    /// peer (acme.eng, rank 1 each).
    async fn company(suffix: &str) -> TestResult<Self> {
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
        run_migrations(&pool).await?;
        sqlx::query(
            "INSERT INTO units (path, display_name) VALUES
               ('acme'::ltree, 'Acme'), ('acme.eng'::ltree, 'Engineering')",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO operators (key, display_name, unit, rank) VALUES
               ('owner', 'Owner', 'acme'::ltree, 10),
               ('staff', 'Staff', 'acme.eng'::ltree, 1),
               ('peer', 'Peer', 'acme.eng'::ltree, 1)",
        )
        .execute(&pool)
        .await?;
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

    /// One `remember` through the real write path. The outer error is the
    /// harness failing; the inner one is the House refusing the write, which
    /// several attacks expect and therefore observe instead of propagating.
    async fn write(
        &self,
        writer: &str,
        scope: Option<MemoryScope>,
        title: &str,
        body: &str,
        details: RememberMemoryDetails,
    ) -> TestResult<Result<i64, String>> {
        let request = RememberRequest::new_memory(
            RoomKey::for_memory_write(ROOM)?,
            title.into(),
            body.into(),
            details,
        )?
        .with_operator(Some(OperatorKey::new(writer)?), scope)?;
        Ok(match remember(&self.pool, &self.cfg, request).await {
            Ok(receipt) => Ok(i64::try_from(receipt.memory_id)?),
            Err(refusal) => Err(refusal.to_string()),
        })
    }

    /// Staff's private memory: the canary sits in its title, body and path.
    async fn staff_private(&self, label: &str, details: RememberMemoryDetails) -> TestResult<i64> {
        let id = self
            .write(
                "staff",
                Some(MemoryScope::Private),
                &format!("{TERM} {label} {CANARY}"),
                &format!("The {TERM} keeper's private {label} note, {CANARY}, tamarind tide."),
                details,
            )
            .await?
            .map_err(|refusal| format!("staff could not seed its private row: {refusal}"))?;
        Ok(id)
    }

    /// Peer writes into `harbor-log`, continuing `previous`.
    async fn peer_continues(&self, previous: i64, label: &str) -> TestResult<Result<i64, String>> {
        self.write(
            "peer",
            Some(MemoryScope::Company),
            &format!("{TERM} peer {label}"),
            &format!("Peer continues the harbor log ({label})."),
            RememberMemoryDetails {
                threads: vec!["harbor-log".into()],
                continues: vec![ThreadContinuation {
                    thread: "harbor-log".into(),
                    previous_memory_id: u64::try_from(previous)?,
                }],
                ..no_details()
            },
        )
        .await
    }

    async fn recall_as(
        &self,
        reader: &str,
        query: &str,
        projection: RecallProjection,
    ) -> TestResult<Value> {
        let request = RecallRequest::new(RoomKey::new(ROOM)?, query.into(), 8, 0.4, 8, 0.3)?
            .with_operator(Some(OperatorKey::new(reader)?))
            .with_projection(projection);
        Ok(serde_json::to_value(
            recall(&self.pool, &self.cfg, request, None).await?,
        )?)
    }

    async fn row(&self, id: i64) -> TestResult<MemoryRow> {
        let row = sqlx::query(
            "SELECT id,operator,scope,title,source_path,body,superseded_by
             FROM memories WHERE id=$1",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        Ok(MemoryRow {
            id: row.try_get("id")?,
            operator: row.try_get("operator")?,
            scope: row.try_get("scope")?,
            title: row.try_get("title")?,
            source_path: row.try_get("source_path")?,
            body: row.try_get("body")?,
            superseded_by: row.try_get("superseded_by")?,
        })
    }

    async fn rows_at(&self, source_path: &str) -> TestResult<Vec<MemoryRow>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM memories WHERE room=$1 AND source_path=$2 ORDER BY id",
        )
        .bind(ROOM)
        .bind(source_path)
        .fetch_all(&self.pool)
        .await?;
        let mut rows = Vec::new();
        for id in ids {
            rows.push(self.row(id).await?);
        }
        Ok(rows)
    }
}

/// Memory IDs the ranked lanes and exact references selected.
fn selected_ids(recall: &Value) -> BTreeSet<i64> {
    ["retrievalCandidates", "contentChunks", "semanticChunks"]
        .iter()
        .flat_map(|lane| recall[*lane].as_array().into_iter().flatten())
        .filter_map(|candidate| candidate["memory_id"].as_i64())
        .collect()
}

fn names_memory(text: &str, id: i64) -> bool {
    let needle = format!("memory {id}");
    text.match_indices(&needle).any(|(at, _)| {
        !text[at + needle.len()..]
            .chars()
            .next()
            .is_some_and(|next| next.is_ascii_digit())
    })
}

/// Every place in a serialized recall that carries the hidden row: the canary
/// in any string, the row's ID in any ID-shaped field, or a sentence naming
/// `memory <id>`. The echoed `query` is skipped; it is the reader's own input.
fn leaks(recall: &Value, hidden_id: i64) -> Vec<String> {
    fn walk(path: &str, value: &Value, hidden_id: i64, found: &mut Vec<String>) {
        match value {
            Value::Object(fields) => {
                for (key, field) in fields {
                    if path.is_empty() && key == "query" {
                        continue;
                    }
                    let here = format!("{path}.{key}");
                    if matches!(
                        key.as_str(),
                        "memory_id" | "id" | "superseded_by" | "candidate_memory_id"
                    ) && field.as_i64() == Some(hidden_id)
                    {
                        found.push(format!("{here} = {hidden_id}"));
                    }
                    walk(&here, field, hidden_id, found);
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    walk(&format!("{path}[{index}]"), item, hidden_id, found);
                }
            }
            Value::String(text) => {
                if text.to_ascii_lowercase().contains(CANARY)
                    || names_memory(text, hidden_id)
                    || *text == hidden_id.to_string()
                {
                    found.push(format!("{path} = {text:?}"));
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk("", recall, hidden_id, &mut found);
    found
}

// ---------------------------------------------------------------------------
// Attack 1: the same (room, source_path) written by a second operator.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_same_source_path_by_another_operator_creates_its_own_row_and_never_touches_the_other()
-> TestResult {
    let db = Isolated::company("attack_upsert").await?;
    let path = "notes/harbor-ledger.md";
    let staff_id = db
        .staff_private(
            "ledger",
            RememberMemoryDetails {
                source_path: Some(path.into()),
                ..no_details()
            },
        )
        .await?;
    let before = db.row(staff_id).await?;

    let peer_write = db
        .write(
            "peer",
            Some(MemoryScope::Company),
            "peer overwrite",
            "Peer text aimed at the staff ledger path.",
            RememberMemoryDetails {
                source_path: Some(path.into()),
                ..no_details()
            },
        )
        .await?;
    let rows = db.rows_at(path).await?;
    let after = db.row(staff_id).await?;

    assert_eq!(
        (
            &after.operator,
            &after.scope,
            &after.title,
            &after.body,
            &after.source_path,
            &after.superseded_by
        ),
        (
            &before.operator,
            &before.scope,
            &before.title,
            &before.body,
            &before.source_path,
            &before.superseded_by
        ),
        "peer's write changed staff's private row; peer received {peer_write:?}.\nbefore: {before:#?}\nafter:  {after:#?}"
    );
    let peer_id = peer_write.clone().map_err(|refusal| {
        format!("peer's write onto its own (room, peer, path) was refused ({refusal}): an existence oracle for staff's row {after:#?}")
    })?;
    assert_ne!(
        peer_id, staff_id,
        "peer's receipt names staff's private memory {staff_id}: {rows:#?}"
    );
    let peer_row = db.row(peer_id).await?;
    assert_eq!(
        (peer_row.operator.as_deref(), peer_row.scope.as_str()),
        (Some("peer"), "company"),
        "peer's row: {peer_row:#?}"
    );
    let owners = rows
        .iter()
        .map(|row| row.operator.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        (rows.len(), owners),
        (
            2,
            BTreeSet::from([Some("peer".to_owned()), Some("staff".to_owned())])
        ),
        "the path must hold one row per operator: {rows:#?}"
    );
    db.close().await
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_identical_memory_from_another_operator_collides_on_the_derived_source_path()
-> TestResult {
    let db = Isolated::company("attack_derived").await?;
    let title = format!("{TERM} shared words");
    let body = format!("The {TERM} keeper wrote these exact shared words.");
    let staff_id = db
        .write(
            "staff",
            Some(MemoryScope::Private),
            &title,
            &body,
            no_details(),
        )
        .await?
        .map_err(|refusal| format!("staff could not seed its private row: {refusal}"))?;

    // Peer independently writes the same words. Its row must be its own: a
    // refusal would tell peer that staff wrote exactly this, and a merge
    // would file peer's memory under staff's private scope.
    let peer_write = db
        .write(
            "peer",
            Some(MemoryScope::Company),
            &title,
            &body,
            no_details(),
        )
        .await?;
    let staff_row = db.row(staff_id).await?;
    assert_eq!(
        (staff_row.operator.as_deref(), staff_row.scope.as_str()),
        (Some("staff"), "private"),
        "staff's private row changed owner or scope: {staff_row:#?}"
    );
    let peer_id = peer_write.clone().map_err(|refusal| {
        format!("peer's identical write was refused ({refusal}): an existence oracle for staff's private row {staff_row:#?}")
    })?;
    assert_ne!(
        peer_id, staff_id,
        "peer's receipt names staff's private memory {staff_id}; peer's company memory was merged into it: {staff_row:#?}"
    );
    let peer_row = db.row(peer_id).await?;
    assert_eq!(
        (peer_row.operator.as_deref(), peer_row.scope.as_str()),
        (Some("peer"), "company"),
        "peer's row: {peer_row:#?}"
    );
    db.close().await
}

// ---------------------------------------------------------------------------
// Attack 2: supersession of another operator's private row.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_supersession_by_another_operator_of_a_private_row_is_refused() -> TestResult {
    let db = Isolated::company("attack_supersede").await?;
    let staff_id = db.staff_private("retired", no_details()).await?;
    assert!(
        selected_ids(&db.recall_as("staff", TERM, RecallProjection::Auto).await?)
            .contains(&staff_id),
        "control: staff must recall its own private row before the attack"
    );

    let peer_write = db
        .write(
            "peer",
            Some(MemoryScope::Company),
            &format!("{TERM} replacement"),
            "Peer claims to replace a memory it cannot read.",
            RememberMemoryDetails {
                supersedes: vec![u64::try_from(staff_id)?],
                ..no_details()
            },
        )
        .await?;
    let staff_row = db.row(staff_id).await?;
    let staff_sees = selected_ids(&db.recall_as("staff", TERM, RecallProjection::Auto).await?);

    assert_eq!(
        staff_row.superseded_by, None,
        "peer retired staff's private row (peer received {peer_write:?}): {staff_row:#?}"
    );
    assert!(
        staff_sees.contains(&staff_id),
        "staff can no longer recall its own private row {staff_id}; staff sees {staff_sees:?}"
    );
    assert!(
        peer_write.is_err(),
        "peer's supersession of a row it cannot read was accepted as memory {peer_write:?}"
    );
    db.close().await
}

// ---------------------------------------------------------------------------
// Attack 3: thread neighbours, thread keys, taxonomy, and `continues`.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_thread_neighbours_thread_lane_and_taxonomy_hide_a_private_row() -> TestResult {
    let db = Isolated::company("attack_threads").await?;
    let private_id = db
        .staff_private(
            "watch",
            RememberMemoryDetails {
                threads: vec!["harbor-log".into(), "tidewatch-qx".into()],
                ..no_details()
            },
        )
        .await?;
    let company_id = db
        .write(
            "staff",
            Some(MemoryScope::Company),
            &format!("{TERM} public follow-up"),
            &format!("The {TERM} keeper's public follow-up on the harbor."),
            RememberMemoryDetails {
                threads: vec!["harbor-log".into()],
                continues: vec![ThreadContinuation {
                    thread: "harbor-log".into(),
                    previous_memory_id: u64::try_from(private_id)?,
                }],
                ..no_details()
            },
        )
        .await?
        .map_err(|refusal| format!("staff could not continue its own thread: {refusal}"))?;
    let query = format!("{TERM} tidewatch");

    // Teeth: the writer reaches the private row as the company row's thread
    // neighbour and through the private thread key, so an empty reader-side
    // result means filtering, not absence.
    let staff = db
        .recall_as("staff", &query, RecallProjection::Auto)
        .await?;
    let staff_neighbours = staff["retrievalCandidates"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|candidate| candidate["memory_id"] == company_id)
        .flat_map(|candidate| {
            candidate["thread_neighbors"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter_map(|neighbour| neighbour["id"].as_i64())
        .collect::<BTreeSet<_>>();
    assert!(
        staff_neighbours.contains(&private_id),
        "control: staff must see its private row as the company row's neighbour: {staff_neighbours:?}"
    );
    assert!(
        staff["taxonomy"]["threadKeys"]
            .as_array()
            .is_some_and(|keys| keys.iter().any(|key| key == "tidewatch-qx")),
        "control: staff's taxonomy must list its private thread key: {}",
        staff["taxonomy"]
    );

    for reader in ["peer", "owner"] {
        let seen = db.recall_as(reader, &query, RecallProjection::Auto).await?;
        assert!(
            selected_ids(&seen).contains(&company_id),
            "{reader} must still reach staff's company follow-up {company_id}"
        );
        let found = leaks(&seen, private_id);
        assert!(
            found.is_empty(),
            "{reader} sees staff's private row {private_id} in: {found:#?}"
        );
        let thread_keys = &seen["taxonomy"]["threadKeys"];
        assert!(
            !thread_keys
                .as_array()
                .is_some_and(|keys| keys.iter().any(|key| key == "tidewatch-qx")),
            "{reader}'s taxonomy names the private-only thread key: {thread_keys}"
        );
    }
    db.close().await
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_continues_onto_another_operators_private_row_is_indistinguishable_from_missing()
-> TestResult {
    let db = Isolated::company("attack_continues").await?;
    let private_id = db
        .staff_private(
            "chain",
            RememberMemoryDetails {
                threads: vec!["harbor-log".into()],
                ..no_details()
            },
        )
        .await?;
    let missing_id: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id),0) + 1000 FROM memories")
        .fetch_one(&db.pool)
        .await?;
    let onto_missing = db.peer_continues(missing_id, "missing").await?;
    let onto_private = db.peer_continues(private_id, "private").await?;
    let grafted: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT previous_event.memory_id, next_event.memory_id
         FROM thread_event_links link
         JOIN thread_events previous_event ON previous_event.id = link.previous_event_id
         JOIN thread_events next_event ON next_event.id = link.next_event_id
         WHERE previous_event.memory_id = $1",
    )
    .bind(private_id)
    .fetch_all(&db.pool)
    .await?;

    assert!(
        onto_missing.is_err(),
        "control: continuing a missing memory must be refused, got {onto_missing:?}"
    );
    assert!(
        grafted.is_empty(),
        "peer grafted its memory after staff's private row {private_id}: links (previous, next) = {grafted:?}; peer received {onto_private:?}"
    );
    assert_eq!(
        onto_private
            .as_ref()
            .map_err(|refusal| refusal.replace(&private_id.to_string(), "<id>")),
        onto_missing
            .as_ref()
            .map_err(|refusal| refusal.replace(&missing_id.to_string(), "<id>")),
        "continuing staff's private row must read exactly like continuing a missing one"
    );
    db.close().await
}

// ---------------------------------------------------------------------------
// Attack 4: exact-ID references.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_exact_id_of_a_private_row_reads_as_not_found_in_every_field() -> TestResult {
    let db = Isolated::company("attack_exact").await?;
    let private_id = db.staff_private("exact", no_details()).await?;
    let missing_id: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id),0) + 1000 FROM memories")
        .fetch_one(&db.pool)
        .await?;

    let staff = db
        .recall_as(
            "staff",
            &format!("memory {private_id}"),
            RecallProjection::Manual,
        )
        .await?;
    assert!(
        selected_ids(&staff).contains(&private_id),
        "control: staff must read its own private row by ID"
    );

    for reader in ["peer", "owner"] {
        for projection in [RecallProjection::Auto, RecallProjection::Manual] {
            for shape in ["memory {id}", "memory {id} {TERM}"] {
                let shaped = |id: i64| {
                    shape
                        .replace("{id}", &id.to_string())
                        .replace("{TERM}", TERM)
                };
                let hidden = db
                    .recall_as(reader, &shaped(private_id), projection)
                    .await?;
                let missing = db
                    .recall_as(reader, &shaped(missing_id), projection)
                    .await?;
                let mut without_warnings = hidden.clone();
                without_warnings["warnings"] = Value::Null;
                let found = leaks(&without_warnings, private_id);
                assert!(
                    found.is_empty(),
                    "{reader} {projection:?} {shape:?}: staff's private row {private_id} leaks in {found:#?}"
                );
                let hidden_warnings = hidden["warnings"]
                    .to_string()
                    .replace(&private_id.to_string(), "<id>");
                let missing_warnings = missing["warnings"]
                    .to_string()
                    .replace(&missing_id.to_string(), "<id>");
                assert_eq!(
                    hidden_warnings, missing_warnings,
                    "{reader} {projection:?} {shape:?}: warnings for a hidden ID differ from a missing ID"
                );
                assert!(
                    !hidden_warnings.to_ascii_lowercase().contains(CANARY),
                    "{reader}: warnings carry the private title or body: {hidden_warnings}"
                );
                assert_eq!(
                    hidden["found"], missing["found"],
                    "{reader} {projection:?} {shape:?}: `found` tells a hidden row from a missing one"
                );
            }
        }
    }
    db.close().await
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_exact_id_of_a_company_row_names_its_private_successor() -> TestResult {
    let db = Isolated::company("attack_successor").await?;
    let company_id = db
        .write(
            "staff",
            Some(MemoryScope::Company),
            &format!("{TERM} public draft"),
            &format!("The {TERM} keeper's public draft."),
            no_details(),
        )
        .await?
        .map_err(|refusal| format!("staff could not seed its company row: {refusal}"))?;
    let private_id = db
        .staff_private(
            "successor",
            RememberMemoryDetails {
                supersedes: vec![u64::try_from(company_id)?],
                ..no_details()
            },
        )
        .await?;

    let seen = db
        .recall_as(
            "peer",
            &format!("memory {company_id}"),
            RecallProjection::Manual,
        )
        .await?;
    let found = leaks(&seen, private_id);
    assert!(
        found.is_empty(),
        "peer reads company memory {company_id} and learns its private successor {private_id}: {found:#?}"
    );
    db.close().await
}

// ---------------------------------------------------------------------------
// Attack 5: the date lane and the content lane.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_date_lane_and_content_lane_hide_a_matching_private_row() -> TestResult {
    let db = Isolated::company("attack_lanes").await?;
    let private_id = db
        .staff_private(
            "tidepool",
            RememberMemoryDetails {
                source_path: Some(format!("notes/2031-04-05-{CANARY}.md")),
                ..no_details()
            },
        )
        .await?;
    let query = "tamarind 2031-04-05";

    let staff = db.recall_as("staff", query, RecallProjection::Auto).await?;
    let staff_dates = staff["dateMatches"].to_string();
    assert!(
        staff_dates.contains(CANARY),
        "control: the date lane must surface staff's own row: {staff_dates}"
    );
    assert!(
        staff["contentChunks"]
            .as_array()
            .is_some_and(|chunks| chunks.iter().any(|chunk| chunk["memory_id"] == private_id)),
        "control: the content lane must surface staff's own row: {}",
        staff["contentChunks"]
    );

    for reader in ["peer", "owner"] {
        let seen = db.recall_as(reader, query, RecallProjection::Auto).await?;
        assert_eq!(
            seen["dateMatches"],
            Value::Array(vec![]),
            "{reader}'s date lane carries staff's private row {private_id}"
        );
        assert_eq!(
            seen["contentChunks"],
            Value::Array(vec![]),
            "{reader}'s content lane carries staff's private row {private_id}"
        );
        let found = leaks(&seen, private_id);
        assert!(
            found.is_empty(),
            "{reader} sees staff's private row {private_id} in: {found:#?}"
        );
    }
    db.close().await
}

// ---------------------------------------------------------------------------
// Census lane outside the brief: the Pulse timeline (timeline.rs:110-112).
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL schema"]
async fn attack_pulse_timeline_and_memory_read_serve_a_private_row_to_the_room() -> TestResult {
    let db = Isolated::company("attack_timeline").await?;
    let private_id = db.staff_private("pulse", no_details()).await?;

    // The panel names only the room: whichever operator's Host serves it, the
    // reader is never the writer for sure, so a private row must not appear.
    let timeline = memory_timeline(
        &db.pool,
        ROOM,
        MemoryTimelineParams {
            room: None,
            before: None,
            limit: 50,
        },
    )
    .await?;
    let listed = timeline
        .memories
        .iter()
        .filter(|item| item.id == private_id)
        .map(|item| format!("{} {:?} {:?}", item.id, item.title, item.excerpt))
        .collect::<Vec<_>>();
    let read = memory_read(&db.pool, ROOM, MemoryReadParams { id: private_id })
        .await
        .map(|result| {
            format!(
                "{} {:?} {:?}",
                result.memory.id, result.memory.title, result.memory.body
            )
        });

    assert!(
        listed.is_empty(),
        "memory_timeline lists staff's private row without any reader: {listed:#?}\nmemory_read returns: {read:?}"
    );
    assert!(
        read.is_err(),
        "memory_read returns staff's private row without any reader: {read:?}"
    );
    db.close().await
}

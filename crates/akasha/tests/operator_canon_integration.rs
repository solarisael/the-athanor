//! Adversarial operator-canon contracts on an owned PostgreSQL schema.
//! Each test names the plausible bug it kills; no database or embedding fake is used.
//! Run with the dedicated port-55461 database and `-- --ignored`.

use akasha::migrations::run_migrations;
use akasha::operator_registry::{OperatorRegistration, register_operator};
use akasha::{
    AppError, Config, EmbeddingMode, EntityResolveParams, RecallResult, canon_read, canon_write,
    entity_resolve, recall,
};
use hearth::{
    CanonAttribution, CanonReadRequest, CanonScope, CanonWriteRequest, DomainError, OperatorKey,
    RecallMode, RecallRequest, RoomKey,
};
use protocol::CanonReadResult;
use serde_json::Value;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::str::FromStr;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const ROOM: &str = "canon-axis-proof";
const ABSENT_ID: u64 = 9_000_000_000;
const PREDECESSOR_REFUSAL: &str =
    "every superseded canon entity must exist in the destination room";
const NAME_REFUSAL: &str =
    "an active canon entity with this room/name already exists; explicitly supersede its ID";

struct Isolated {
    schema: String,
    pool: PgPool,
    cfg: Config,
}

impl Isolated {
    async fn bare(suffix: &str) -> TestResult<Self> {
        let base = std::env::var("ATHANOR_SUBSTRATE_TEST_SCHEMA")?;
        assert!(base.starts_with("solarisael_tuner_test_"));
        let schema = format!("{base}_{suffix}");
        assert!(schema.len() <= 63);
        assert!(
            schema
                .bytes()
                .all(|byte| { byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' })
        );

        let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")?;
        let options = PgConnectOptions::from_str(&url)?;
        assert_eq!(
            options.get_host(),
            "127.0.0.1",
            "only the assigned throwaway"
        );
        assert_eq!(options.get_port(), 55461, "only the assigned throwaway");
        assert_eq!(options.get_database(), Some("athanor_canon_test"));

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        // Extensions outlive each owned schema; dropping one must not erase a sibling's types.
        let mut tx = pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext('athanor_schema_migrations'))")
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(
            "CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public;
             CREATE EXTENSION IF NOT EXISTS vector WITH SCHEMA public;
             CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;
             CREATE EXTENSION IF NOT EXISTS ltree WITH SCHEMA public;
             ALTER EXTENSION pg_trgm SET SCHEMA public;
             ALTER EXTENSION vector SET SCHEMA public;
             ALTER EXTENSION pgcrypto SET SCHEMA public;
             ALTER EXTENSION ltree SET SCHEMA public;",
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

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

    async fn migrated(suffix: &str) -> TestResult<Self> {
        let db = Self::bare(suffix).await?;
        run_migrations(&db.pool).await?;

        Ok(db)
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

    async fn register_readers(&self) -> TestResult {
        for (key, display_name) in [("sol", "Sol Meridian"), ("ana", "Ana Vale")] {
            register_operator(&self.pool, &registration(key, display_name, "proof")).await?;
        }

        Ok(())
    }

    async fn write(&self, request: CanonWriteRequest) -> TestResult<u64> {
        Ok(canon_write(&self.pool, request).await?.entity_id.parse()?)
    }

    async fn read(
        &self,
        reader: Option<&str>,
        request: CanonReadRequest,
    ) -> TestResult<CanonReadResult> {
        Ok(canon_read(
            &self.pool,
            request.with_reader(reader.map(OperatorKey::new).transpose()?),
        )
        .await?)
    }

    async fn recall_as(
        &self,
        reader: Option<&str>,
        query: &str,
        mode: RecallMode,
    ) -> TestResult<RecallResult> {
        let request = RecallRequest::new(RoomKey::new(ROOM)?, query.into(), 8, 0.4, 8, 0.3)?
            .with_operator(reader.map(OperatorKey::new).transpose()?)
            .with_mode(mode);

        Ok(recall(&self.pool, &self.cfg, request, None).await?)
    }

    async fn resolve(&self, reader: Option<&str>, query: &str) -> TestResult<Value> {
        Ok(serde_json::to_value(
            entity_resolve(
                &self.pool,
                EntityResolveParams {
                    room: ROOM.into(),
                    query: query.into(),
                    limit: 32,
                    operator: reader.map(str::to_owned),
                },
            )
            .await?,
        )?)
    }

    async fn vocabulary(&self) -> TestResult<Vec<Value>> {
        sqlx::query("SELECT substrate_refresh_semantic_vocabulary_sources()")
            .execute(&self.pool)
            .await?;

        Ok(sqlx::query_scalar(
            "SELECT jsonb_build_object('room',room,'key',source_key,'concept',concept,
                                       'terms',lexical_terms,'kind',approved_source_kind)
             FROM semantic_vocabulary WHERE source_kind='named_entity'
             ORDER BY room,source_key",
        )
        .fetch_all(&self.pool)
        .await?)
    }
}

fn registration(key: &str, display_name: &str, unit: &str) -> OperatorRegistration {
    OperatorRegistration {
        unit: unit.into(),
        unit_name: format!("Proof {unit}"),
        key: key.into(),
        display_name: display_name.into(),
    }
}

fn fact(
    room: &str,
    name: &str,
    kind: &str,
    summary: &str,
    weighty: bool,
    supersedes: Vec<u64>,
) -> TestResult<CanonWriteRequest> {
    Ok(CanonWriteRequest::new(
        room.into(),
        name.into(),
        kind.into(),
        summary.into(),
        vec![],
        None,
        weighty,
        vec![],
        None,
        supersedes,
        CanonAttribution::new("proof".into(), "integration:operator-canon".into())?,
    )?)
}

fn person_fact(subject: &str, name: &str, summary: &str) -> TestResult<CanonWriteRequest> {
    Ok(fact(ROOM, name, "role", summary, false, vec![])?.with_axis(
        Some(OperatorKey::new(subject)?),
        Some(OperatorKey::new(subject)?),
        None,
        None,
    )?)
}

fn by_id(id: u64, history: bool) -> TestResult<CanonReadRequest> {
    Ok(CanonReadRequest::new(ROOM.into(), Some(id), None, history)?)
}

fn by_name(name: &str, history: bool) -> TestResult<CanonReadRequest> {
    Ok(CanonReadRequest::new(
        ROOM.into(),
        None,
        Some(name.into()),
        history,
    )?)
}

fn by_subject(subject: &str, name: Option<&str>, history: bool) -> TestResult<CanonReadRequest> {
    Ok(CanonReadRequest::for_subject(
        ROOM.into(),
        OperatorKey::new(subject)?,
        name.map(str::to_owned),
        history,
    )?)
}

fn invalid_message(error: AppError) -> String {
    match error {
        AppError::Invalid(message) => message,
        other => panic!("expected a public invalid-request refusal, got {other:?}"),
    }
}

async fn expect_axis_refusal(
    db: &Isolated,
    attempt: Result<CanonWriteRequest, DomainError>,
    field: &str,
    message: &str,
) {
    match attempt {
        Err(DomainError::InvalidCanon {
            field: actual_field,
            message: actual_message,
        }) => {
            assert_eq!(actual_field, field);
            assert_eq!(actual_message, message);
        }
        Err(other) => panic!("wrong refusal: {other:?}"),
        Ok(request) => {
            let written = canon_write(&db.pool, request).await;
            panic!("domain admitted forbidden axis; canon_write returned {written:?}");
        }
    }
}

fn names(result: &RecallResult) -> Vec<&str> {
    result
        .canon_matches
        .iter()
        .map(|row| row["termKey"].as_str().expect("canon name"))
        .collect()
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p01_subject_default_is_private_and_subject_selectors_include_house() -> TestResult {
    // Kills the memory-style default-to-up and room-only subject lookup regressions.
    let db = Isolated::migrated("p01").await?;
    db.register_readers().await?;
    let local = db
        .write(person_fact(
            "sol",
            "route",
            "Keeps glacier routes current.",
        )?)
        .await?;
    let house = db
        .write(
            fact(
                "house",
                "pace",
                "preference",
                "Leaves a pause before a decision.",
                true,
                vec![],
            )?
            .with_axis(
                Some(OperatorKey::new("sol")?),
                Some(OperatorKey::new("sol")?),
                None,
                None,
            )?,
        )
        .await?;

    for request in [
        by_id(local, false)?,
        by_name("route", false)?,
        by_subject("sol", Some("route"), false)?,
    ] {
        let read = db.read(Some("sol"), request).await?;
        assert_eq!(read.entities.len(), 1);
        assert_eq!(read.entities[0].entity_id, local.to_string());
        assert_eq!(read.entities[0].scope, "private");
        assert_eq!(read.entities[0].subject.as_deref(), Some("sol"));
        assert_eq!(read.entities[0].summary, "Keeps glacier routes current.");
    }
    let listed = db
        .read(Some("sol"), by_subject("sol", None, false)?)
        .await?;
    assert_eq!(
        listed
            .entities
            .iter()
            .map(|row| row.entity_id.as_str())
            .collect::<Vec<_>>(),
        vec![local.to_string(), house.to_string()]
    );
    let recalled = db
        .recall_as(Some("sol"), "route", RecallMode::Mixed)
        .await?;
    assert_eq!(names(&recalled), ["route"]);
    assert_eq!(recalled.canon_matches[0]["entry"]["exact"], true);
    assert_eq!(
        db.resolve(Some("sol"), "route").await?["matches"][0]["canonicalName"],
        "route"
    );

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p02_maximum_ancestor_rank_cannot_distinguish_private_from_absent() -> TestResult {
    // Kills rank bypasses and ID/name/subject, history, recall, or resolver existence leaks.
    let db = Isolated::migrated("p02").await?;
    register_operator(&db.pool, &registration("sol", "Sol Meridian", "proof.team")).await?;
    register_operator(&db.pool, &registration("ana", "Ana Vale", "proof")).await?;
    sqlx::query("UPDATE operators SET rank=$1 WHERE key='ana'")
        .bind(i32::MAX)
        .execute(&db.pool)
        .await?;
    let hidden = db
        .write(person_fact(
            "sol",
            "sealed bearing",
            "The compass is stored under violet glass.",
        )?)
        .await?;
    let public = db
        .write(fact(
            ROOM,
            "public bearing",
            "project",
            "The compass is shared.",
            false,
            vec![],
        )?)
        .await?;

    for history in [false, true] {
        for (hidden_request, absent_request) in [
            (by_id(hidden, history)?, by_id(ABSENT_ID, history)?),
            (
                by_name("sealed bearing", history)?,
                by_name("absent bearing", history)?,
            ),
            (
                by_subject("sol", Some("sealed bearing"), history)?,
                by_subject("sol", Some("absent bearing"), history)?,
            ),
            (
                by_subject("sol", None, history)?,
                by_subject("unregistered", None, history)?,
            ),
        ] {
            let control = db.read(Some("sol"), hidden_request.clone()).await?;
            assert_eq!(
                control.entities.len(),
                1,
                "positive control for each hidden selector"
            );
            let unseen = db.read(Some("ana"), hidden_request).await?;
            let absent = db.read(Some("ana"), absent_request).await?;
            assert_eq!(serde_json::to_value(unseen)?, serde_json::to_value(absent)?);
        }
    }
    assert_eq!(
        db.read(Some("ana"), by_id(public, false)?)
            .await?
            .entities
            .len(),
        1
    );
    for mode in [
        RecallMode::Mixed,
        RecallMode::Work,
        RecallMode::Conversation,
        RecallMode::Quiet,
    ] {
        let read = db
            .recall_as(Some("ana"), "sealed bearing public bearing", mode)
            .await?;
        assert_eq!(
            names(&read),
            ["public bearing"],
            "{mode:?} must not open private canon"
        );
    }
    let resolved = db
        .resolve(Some("ana"), "sealed bearing public bearing")
        .await?;
    assert_eq!(resolved["matches"].as_array().unwrap().len(), 1);
    assert_eq!(resolved["matches"][0]["canonicalName"], "public bearing");
    assert_eq!(
        db.resolve(Some("sol"), "sealed bearing public bearing")
            .await?["matches"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p05_other_writer_cannot_write_private_but_can_write_company() -> TestResult {
    // Kills a missing subject/writer guard without banning legitimate shared attribution.
    let db = Isolated::migrated("p05").await?;
    db.register_readers().await?;
    let request = fact(
        ROOM,
        "route",
        "role",
        "Keeps glacier routes current.",
        false,
        vec![],
    )?;
    for scope in [None, Some(CanonScope::Private)] {
        expect_axis_refusal(
            &db,
            request.clone().with_axis(
                Some(OperatorKey::new("ana")?),
                Some(OperatorKey::new("sol")?),
                scope,
                None,
            ),
            "subject",
            "only the subject writes a private fact",
        )
        .await;
        assert!(
            db.read(Some("sol"), by_name("route", false)?)
                .await?
                .entities
                .is_empty()
        );
    }
    let shared = db
        .write(request.with_axis(
            Some(OperatorKey::new("ana")?),
            Some(OperatorKey::new("sol")?),
            Some(CanonScope::Company),
            None,
        )?)
        .await?;
    for reader in [None, Some("sol"), Some("ana")] {
        let read = db.read(reader, by_id(shared, false)?).await?;
        assert_eq!(read.entities.len(), 1);
        assert_eq!(read.entities[0].scope, "company");
    }

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p06_subject_is_part_of_active_name_identity_and_duplicates_do_not_overwrite() -> TestResult
{
    // Kills the old room/name uniqueness key and silent same-subject replacement.
    let db = Isolated::migrated("p06").await?;
    db.register_readers().await?;
    let mut ids = Vec::new();
    for subject in ["sol", "ana"] {
        ids.push(
            db.write(
                person_fact(subject, "role", "Maintains the survey ledger.")?.with_axis(
                    Some(OperatorKey::new(subject)?),
                    Some(OperatorKey::new(subject)?),
                    Some(CanonScope::Company),
                    None,
                )?,
            )
            .await?,
        );
    }
    let before = db.read(Some("sol"), by_name("role", false)?).await?;
    assert_eq!(before.entities.len(), 2);
    assert_eq!(
        before
            .entities
            .iter()
            .map(|row| row.entity_id.clone())
            .collect::<Vec<_>>(),
        ids.iter().map(u64::to_string).collect::<Vec<_>>()
    );
    let duplicate = person_fact("sol", "ROLE", "This must not replace the ledger.")?.with_axis(
        Some(OperatorKey::new("sol")?),
        Some(OperatorKey::new("sol")?),
        Some(CanonScope::Company),
        None,
    )?;
    let refusal = invalid_message(canon_write(&db.pool, duplicate).await.unwrap_err());
    assert_eq!(refusal, NAME_REFUSAL);
    assert_eq!(
        serde_json::to_value(db.read(Some("sol"), by_name("role", false)?).await?)?,
        serde_json::to_value(before)?
    );

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p07_fifty_hidden_facts_cannot_change_recall_resolution_or_vocabulary() -> TestResult {
    // Kills post-LIMIT filtering, hidden taxonomy/count changes, and private vocabulary bridges.
    let db = Isolated::migrated("p07").await?;
    db.register_readers().await?;
    let shared = db
        .write(fact(
            ROOM,
            "Public Beacon",
            "project",
            "The sealed beacon follows weather reports.",
            false,
            vec![],
        )?)
        .await?;
    let queries = ["beacon", "Public Beacon", "sealed beacon"];
    let mut before = Vec::new();
    for query in queries {
        let recalled = db.recall_as(Some("ana"), query, RecallMode::Mixed).await?;
        assert_eq!(
            names(&recalled),
            ["Public Beacon"],
            "nonempty same-lane control"
        );
        before.push(serde_json::to_value(recalled)?);
    }
    let resolver_before = db
        .resolve(Some("ana"), "Public Beacon Sealed Beacon 000")
        .await?;
    assert_eq!(resolver_before["matches"].as_array().unwrap().len(), 1);
    let vocabulary_before = db.vocabulary().await?;
    assert!(
        vocabulary_before
            .iter()
            .any(|row| row["key"] == shared.to_string())
    );

    for index in 0..50 {
        db.write(person_fact(
            "sol",
            &format!("Sealed Beacon {index:03}"),
            "The sealed beacon follows weather reports.",
        )?)
        .await?;
    }
    assert_eq!(
        db.read(Some("sol"), by_subject("sol", None, false)?)
            .await?
            .entities
            .len(),
        50
    );
    for (query, baseline) in queries.into_iter().zip(before) {
        let after = db.recall_as(Some("ana"), query, RecallMode::Mixed).await?;
        assert_eq!(
            serde_json::to_value(after)?,
            baseline,
            "hidden rows changed {query:?}"
        );
    }
    assert_eq!(
        db.resolve(Some("ana"), "Public Beacon Sealed Beacon 000")
            .await?,
        resolver_before
    );
    assert_eq!(
        db.resolve(Some("sol"), "Public Beacon Sealed Beacon 000")
            .await?["matches"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(db.vocabulary().await?, vocabulary_before);

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p08_incidental_role_does_not_promote_a_person_fact_without_a_subject() -> TestResult {
    // Kills the old name-token tier-1 rule for person facts while retaining exact-name lookup.
    let db = Isolated::migrated("p08").await?;
    db.register_readers().await?;
    db.write(person_fact("sol", "role", "Maintains glacier routes.")?)
        .await?;
    db.write(fact(
        ROOM,
        "role catalogue",
        "project",
        "Lists shared responsibilities.",
        false,
        vec![],
    )?)
    .await?;

    let incidental = db
        .recall_as(
            Some("sol"),
            "explain the role catalogue today",
            RecallMode::Mixed,
        )
        .await?;
    assert_eq!(names(&incidental), ["role catalogue"]);
    assert_eq!(incidental.canon_matches[0]["entry"]["exact"], true);
    let exact = db.recall_as(Some("sol"), "role", RecallMode::Mixed).await?;
    assert!(
        exact
            .canon_matches
            .iter()
            .any(|row| row["termKey"] == "role" && row["entry"]["exact"] == true)
    );
    let subject = db
        .recall_as(Some("sol"), "Sol Meridian role", RecallMode::Mixed)
        .await?;
    assert!(
        subject
            .canon_matches
            .iter()
            .any(|row| row["termKey"] == "role" && row["entry"]["exact"] == true)
    );

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn subject_sol_is_not_the_substring_in_solution() -> TestResult {
    // Kills substring subject matching; a summary match must stay in tier 2.
    let db = Isolated::migrated("word_boundary").await?;
    db.register_readers().await?;
    db.write(person_fact(
        "sol",
        "route",
        "A solution preserves the glacier survey.",
    )?)
    .await?;
    for query in ["sol", "Ask SOL, please", "Sol Meridian"] {
        let result = db.recall_as(Some("sol"), query, RecallMode::Mixed).await?;
        assert_eq!(names(&result), ["route"]);
        assert_eq!(result.canon_matches[0]["entry"]["exact"], true, "{query:?}");
    }
    let result = db
        .recall_as(Some("sol"), "solution", RecallMode::Mixed)
        .await?;
    assert_eq!(
        names(&result),
        ["route"],
        "positive control: the summary really matches"
    );
    assert_eq!(
        result.canon_matches[0]["entry"]["exact"], false,
        "solution is not the subject sol"
    );

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p09_sql_selects_twelve_visible_seats_then_work_reorders_only_that_set() -> TestResult {
    // Kills kind weights changing selection, a profile crossing exactness, and hidden seats.
    let db = Isolated::migrated("p09").await?;
    db.register_readers().await?;
    sqlx::query("INSERT INTO room_settings (room_key,key,value) VALUES ($1,'recall_mode_profiles_enabled','true'::jsonb)")
        .bind(ROOM).execute(&db.pool).await?;
    let selected_weighty = [
        ("m-family-a", "family"),
        ("m-family-b", "family"),
        ("m-medical-a", "medical"),
        ("m-medical-b", "medical"),
        ("m-preference-a", "preference"),
        ("m-preference-b", "preference"),
        ("m-preference-c", "preference"),
        ("m-role-a", "role"),
        ("m-role-b", "role"),
        ("m-role-c", "role"),
    ];
    let facts = std::iter::once(("sol".to_string(), "medical", false))
        .chain(
            selected_weighty
                .into_iter()
                .map(|(name, kind)| (name.to_string(), kind, true)),
        )
        .chain(std::iter::once((
            "a-preference".to_string(),
            "preference",
            false,
        )))
        .chain((0..28).map(|index| (format!("a-z-{index:02}"), "role", false)));

    for (name, kind, weighty) in facts {
        let request = fact(
            ROOM,
            &name,
            kind,
            "Preserves the survey record.",
            weighty,
            vec![],
        )?
        .with_axis(
            Some(OperatorKey::new("sol")?),
            Some(OperatorKey::new("sol")?),
            Some(CanonScope::Company),
            None,
        )?;
        db.write(request).await?;
    }
    assert_eq!(
        db.read(Some("ana"), by_subject("sol", None, false)?)
            .await?
            .entities
            .len(),
        40
    );
    let mixed = db.recall_as(Some("ana"), "sol", RecallMode::Mixed).await?;
    assert_eq!(
        names(&mixed),
        [
            "sol",
            "m-family-a",
            "m-family-b",
            "m-medical-a",
            "m-medical-b",
            "m-preference-a",
            "m-preference-b",
            "m-preference-c",
            "m-role-a",
            "m-role-b",
            "m-role-c",
            "a-preference",
        ],
        "exactness, then weighty, then name select the twelve seats"
    );
    let work = db.recall_as(Some("ana"), "sol", RecallMode::Work).await?;
    assert_eq!(
        names(&work),
        [
            "sol",
            "m-preference-a",
            "m-preference-b",
            "m-preference-c",
            "m-role-a",
            "m-role-b",
            "m-role-c",
            "a-preference",
            "m-family-a",
            "m-family-b",
            "m-medical-a",
            "m-medical-b",
        ],
        "inside a tier, Work kinds beat weighty; an exact medical row still leads"
    );
    assert_eq!(work.canon_matches.len(), 12);

    for index in 0..50 {
        let request = fact(
            ROOM,
            &format!("0-hidden-{index:02}"),
            "role",
            "A sealed survey record.",
            true,
            vec![],
        )?
        .with_axis(
            Some(OperatorKey::new("sol")?),
            Some(OperatorKey::new("sol")?),
            None,
            None,
        )?;
        db.write(request).await?;
    }
    assert_eq!(
        db.read(Some("sol"), by_subject("sol", None, false)?)
            .await?
            .entities
            .len(),
        90
    );
    assert_eq!(
        db.recall_as(Some("ana"), "sol", RecallMode::Mixed)
            .await?
            .canon_matches,
        mixed.canon_matches
    );
    assert_eq!(
        db.recall_as(Some("ana"), "sol", RecallMode::Work)
            .await?
            .canon_matches,
        work.canon_matches
    );

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p12_no_operator_reads_only_shared_rows_and_cannot_write_a_subject() -> TestResult {
    // Kills treating NULL reader as privileged and accepting an unstamped subject write.
    let db = Isolated::migrated("p12").await?;
    db.register_readers().await?;
    let private = db
        .write(person_fact(
            "sol",
            "sealed route",
            "Keeps the private bearing.",
        )?)
        .await?;
    let shared = db
        .write(
            person_fact("sol", "public route", "Keeps the public bearing.")?.with_axis(
                Some(OperatorKey::new("ana")?),
                Some(OperatorKey::new("sol")?),
                Some(CanonScope::Company),
                None,
            )?,
        )
        .await?;
    for history in [false, true] {
        let hidden = db.read(None, by_id(private, history)?).await?;
        let absent = db.read(None, by_id(ABSENT_ID, history)?).await?;
        assert_eq!(serde_json::to_value(hidden)?, serde_json::to_value(absent)?);
        assert_eq!(
            db.read(None, by_id(shared, history)?).await?.entities.len(),
            1
        );
    }
    assert_eq!(
        db.read(Some("sol"), by_id(private, false)?)
            .await?
            .entities
            .len(),
        1
    );
    let recalled = db.recall_as(None, "sol", RecallMode::Mixed).await?;
    assert_eq!(names(&recalled), ["public route"]);
    let resolved = db.resolve(None, "sealed route public route").await?;
    assert_eq!(resolved["matches"].as_array().unwrap().len(), 1);
    assert_eq!(resolved["matches"][0]["canonicalName"], "public route");
    for scope in [None, Some(CanonScope::Company)] {
        expect_axis_refusal(
            &db,
            fact(ROOM, "unstamped", "role", "Must not land.", false, vec![])?.with_axis(
                None,
                Some(OperatorKey::new("sol")?),
                scope,
                None,
            ),
            "subject",
            "a row with a subject needs a Host operator",
        )
        .await;
    }
    assert!(
        db.read(Some("sol"), by_name("unstamped", false)?)
            .await?
            .entities
            .is_empty()
    );

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p13_legacy_rows_keep_public_results_and_0048_reapply_is_idempotent() -> TestResult {
    // Kills migration changes to legacy visibility/order and non-idempotent 0048 DDL.
    // New public SQL needs 0048: seed historical state, then compare with explicit expectations.
    let db = Isolated::bare("p13").await?;
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../substrate/migrations");
    let mut files = std::fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    files.sort();
    for path in files {
        let Some(version) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.get(..4))
            .and_then(|digits| digits.parse::<u32>().ok())
        else {
            continue;
        };
        if version < 48 {
            sqlx::raw_sql(&std::fs::read_to_string(path)?)
                .execute(&db.pool)
                .await?;
            // Some SQL files leave ledger writes to the runner; record only files actually applied.
            sqlx::query("INSERT INTO schema_migrations (version) VALUES ($1) ON CONFLICT (version) DO NOTHING")
                .bind(i32::try_from(version)?)
                .execute(&db.pool)
                .await?;
        }
    }
    let mut ids = Vec::new();
    for (name, weighty) in [("Legacy Beacon", false), ("Legacy Ledger", true)] {
        ids.push(
            sqlx::query_scalar::<_, i64>(
                "INSERT INTO named_entities (room,name,kind,summary,weighty)
             VALUES ($1,$2,'project','Legacy stable assertion.', $3) RETURNING id",
            )
            .bind(ROOM)
            .bind(name)
            .bind(weighty)
            .fetch_one(&db.pool)
            .await?,
        );
    }
    run_migrations(&db.pool).await?;
    let mut before_reapply = Vec::new();
    for reader in [None, Some("sol"), Some("ana")] {
        for (id, name) in ids.iter().zip(["Legacy Beacon", "Legacy Ledger"]) {
            let read = db.read(reader, by_id(u64::try_from(*id)?, false)?).await?;
            assert_eq!(read.entities.len(), 1);
            assert_eq!(read.entities[0].name, name);
            assert_eq!(read.entities[0].summary, "Legacy stable assertion.");
            assert_eq!(read.entities[0].scope, "company");
            assert_eq!(read.entities[0].subject, None);
        }
        let recalled = db.recall_as(reader, "legacy", RecallMode::Mixed).await?;
        assert_eq!(names(&recalled), ["Legacy Ledger", "Legacy Beacon"]);
        let resolved = db
            .resolve(reader, "Legacy Beacon and Legacy Ledger")
            .await?;
        assert_eq!(resolved["matches"].as_array().unwrap().len(), 2);
        assert_eq!(resolved["matches"][0]["canonicalName"], "Legacy Beacon");
        assert_eq!(resolved["matches"][1]["canonicalName"], "Legacy Ledger");
        before_reapply.push((serde_json::to_value(recalled)?, resolved));
    }
    let vocabulary_before = db.vocabulary().await?;
    assert_eq!(vocabulary_before.len(), 2);
    sqlx::raw_sql(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../substrate/migrations/0048_operator_canon.sql"
    )))
    .execute(&db.pool)
    .await?;
    for (reader, (recalled, resolved)) in [None, Some("sol"), Some("ana")]
        .into_iter()
        .zip(before_reapply)
    {
        assert_eq!(
            serde_json::to_value(db.recall_as(reader, "legacy", RecallMode::Mixed).await?)?,
            recalled
        );
        assert_eq!(
            db.resolve(reader, "Legacy Beacon and Legacy Ledger")
                .await?,
            resolved
        );
    }
    assert_eq!(db.vocabulary().await?, vocabulary_before);

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p14_first_pass_refuses_up_and_min_elevation_without_writing() -> TestResult {
    // Kills accidentally enabling an unimplemented elevation-based canon grant.
    let db = Isolated::migrated("p14").await?;
    db.register_readers().await?;
    for level in [None, Some(5)] {
        expect_axis_refusal(
            &db,
            fact(ROOM, "up attempt", "role", "Must not land.", false, vec![])?.with_axis(
                Some(OperatorKey::new("sol")?),
                Some(OperatorKey::new("sol")?),
                Some(CanonScope::Up),
                level,
            ),
            "scope",
            "the scope up is not available for canon yet",
        )
        .await;
    }
    expect_axis_refusal(
        &db,
        fact(
            ROOM,
            "level attempt",
            "role",
            "Must not land.",
            false,
            vec![],
        )?
        .with_axis(
            Some(OperatorKey::new("sol")?),
            Some(OperatorKey::new("sol")?),
            Some(CanonScope::Company),
            Some(5),
        ),
        "minElevation",
        "minElevation needs the scope up",
    )
    .await;
    assert!(
        db.read(Some("sol"), by_subject("sol", None, true)?)
            .await?
            .entities
            .is_empty()
    );
    let control = db
        .write(person_fact(
            "sol",
            "private control",
            "A supported scope still writes.",
        )?)
        .await?;
    assert_eq!(
        db.read(Some("sol"), by_id(control, false)?)
            .await?
            .entities
            .len(),
        1
    );

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p15_lineage_redacts_hidden_successors_and_predecessors() -> TestResult {
    // Kills hidden IDs in projections and visibility bypasses in recursive lineage steps.
    let db = Isolated::migrated("p15").await?;
    db.register_readers().await?;
    let old = db
        .write(fact(
            ROOM,
            "open bearing",
            "project",
            "A once-shared assertion.",
            false,
            vec![],
        )?)
        .await?;
    let hidden = db
        .write(
            fact(
                ROOM,
                "sealed bearing",
                "role",
                "The new bearing is private.",
                false,
                vec![old],
            )?
            .with_axis(
                Some(OperatorKey::new("sol")?),
                Some(OperatorKey::new("sol")?),
                None,
                None,
            )?,
        )
        .await?;
    for history in [false, true] {
        let visible = db.read(Some("ana"), by_id(old, history)?).await?;
        assert_eq!(visible.entities.len(), 1);
        assert_eq!(visible.entities[0].entity_id, old.to_string());
        assert_eq!(visible.entities[0].superseded_by, None);
        assert!(!serde_json::to_string(&visible)?.contains(&format!("\"{hidden}\"")));
    }
    let owner = db.read(Some("sol"), by_id(old, true)?).await?;
    assert_eq!(owner.entities.len(), 2);
    assert_eq!(
        owner.entities[0].superseded_by.as_deref(),
        Some(hidden.to_string().as_str())
    );
    assert_eq!(
        db.read(Some("ana"), by_name("open bearing", true)?)
            .await?
            .entities
            .len(),
        1
    );

    let private_predecessor = db
        .write(person_fact(
            "sol",
            "sealed ledger",
            "A private prior assertion.",
        )?)
        .await?;
    let shared_successor = db
        .write(
            fact(
                ROOM,
                "open ledger",
                "project",
                "An intentionally shared correction.",
                false,
                vec![private_predecessor],
            )?
            .with_axis(
                Some(OperatorKey::new("sol")?),
                Some(OperatorKey::new("sol")?),
                Some(CanonScope::Company),
                None,
            )?,
        )
        .await?;
    for reader in [None, Some("ana")] {
        for history in [false, true] {
            let visible = db.read(reader, by_id(shared_successor, history)?).await?;
            assert_eq!(visible.entities.len(), 1);
            assert_eq!(visible.entities[0].entity_id, shared_successor.to_string());
            assert!(visible.entities[0].supersedes.is_empty());
        }
    }
    let owner = db.read(Some("sol"), by_id(shared_successor, true)?).await?;
    assert_eq!(owner.entities.len(), 2);
    assert!(
        owner
            .entities
            .iter()
            .any(|row| row.entity_id == shared_successor.to_string()
                && row.supersedes == [private_predecessor.to_string()])
    );

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p15_private_labels_do_not_escape_through_a_company_successor() -> TestResult {
    // Kills copying private predecessor labels into a row with a wider audience.
    let db = Isolated::migrated("p15_alias").await?;
    db.register_readers().await?;
    let private_labels = ["sealed ledger", "violet folio", "glass archive"];
    let private_request = CanonWriteRequest::new(
        ROOM.into(),
        private_labels[0].into(),
        "role".into(),
        "A private survey detail.".into(),
        private_labels[1..]
            .iter()
            .map(|label| label.to_string())
            .collect(),
        None,
        false,
        vec![],
        None,
        vec![],
        CanonAttribution::new("proof".into(), "integration:operator-canon".into())?,
    )?
    .with_axis(
        Some(OperatorKey::new("sol")?),
        Some(OperatorKey::new("sol")?),
        Some(CanonScope::Private),
        None,
    )?;
    let private_predecessor = db.write(private_request).await?;
    let company_successor = db
        .write(
            fact(
                ROOM,
                "open registry",
                "role",
                "A released survey detail.",
                false,
                vec![private_predecessor],
            )?
            .with_axis(
                Some(OperatorKey::new("sol")?),
                Some(OperatorKey::new("sol")?),
                Some(CanonScope::Company),
                None,
            )?,
        )
        .await?;

    let shared_predecessor = db
        .write(
            person_fact("sol", "public charter", "A shared survey charter.")?.with_axis(
                Some(OperatorKey::new("sol")?),
                Some(OperatorKey::new("sol")?),
                Some(CanonScope::Company),
                None,
            )?,
        )
        .await?;
    let shared_successor = db
        .write(
            fact(
                ROOM,
                "current charter",
                "role",
                "The current shared charter.",
                false,
                vec![shared_predecessor],
            )?
            .with_axis(
                Some(OperatorKey::new("sol")?),
                Some(OperatorKey::new("sol")?),
                Some(CanonScope::Company),
                None,
            )?,
        )
        .await?;
    let private_control = db
        .write(person_fact(
            "sol",
            "quiet cabin",
            "A private survey shelter.",
        )?)
        .await?;
    let private_successor = db
        .write(
            fact(
                ROOM,
                "current cabin",
                "role",
                "The current private shelter.",
                false,
                vec![private_control],
            )?
            .with_axis(
                Some(OperatorKey::new("sol")?),
                Some(OperatorKey::new("sol")?),
                None,
                None,
            )?,
        )
        .await?;
    let owner = db
        .read(Some("sol"), by_id(private_successor, false)?)
        .await?;
    assert_eq!(owner.entities.len(), 1);
    assert!(
        owner.entities[0]
            .aliases
            .iter()
            .any(|alias| alias == "quiet cabin"),
        "private-to-private supersession must retain its old name for the subject"
    );
    let private_forward = db.read(Some("sol"), by_name("quiet cabin", false)?).await?;
    assert_eq!(private_forward.entities.len(), 1);
    assert_eq!(
        private_forward.entities[0].entity_id,
        private_successor.to_string()
    );

    // Collect every boundary's result before failing, so one alias leak cannot hide the others.
    let mut leaks = Vec::new();
    for reader in [Some("ana"), None] {
        let control = db.read(reader, by_id(shared_successor, false)?).await?;
        assert_eq!(control.entities.len(), 1);
        assert!(
            control.entities[0]
                .aliases
                .iter()
                .any(|alias| alias == "public charter"),
            "company-to-company supersession must retain its old name"
        );
        let forwarded = db.read(reader, by_name("public charter", false)?).await?;
        assert_eq!(forwarded.entities.len(), 1);
        assert_eq!(
            forwarded.entities[0].entity_id,
            shared_successor.to_string()
        );
        let recalled_control = db
            .recall_as(reader, "public charter", RecallMode::Mixed)
            .await?;
        assert_eq!(names(&recalled_control), ["current charter"]);
        let resolved_control = db.resolve(reader, "public charter").await?;
        assert_eq!(resolved_control["matches"].as_array().unwrap().len(), 1);
        assert_eq!(
            resolved_control["matches"][0]["canonicalName"],
            "current charter"
        );

        let successor = db.read(reader, by_id(company_successor, false)?).await?;
        assert_eq!(
            successor.entities.len(),
            1,
            "the widened row itself remains readable"
        );
        for label in private_labels {
            if successor.entities[0]
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(label))
            {
                leaks.push(format!("{reader:?}: successor aliases expose {label:?}"));
            }
            for history in [false, true] {
                let read = db.read(reader, by_name(label, history)?).await?;
                if !read.entities.is_empty() {
                    leaks.push(format!(
                        "{reader:?}: canon_read({label:?}, history={history}) reaches {}",
                        read.entities
                            .iter()
                            .map(|row| row.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
            let recalled = db.recall_as(reader, label, RecallMode::Mixed).await?;
            if !recalled.canon_matches.is_empty() {
                leaks.push(format!(
                    "{reader:?}: recall({label:?}) reaches {:?}",
                    names(&recalled)
                ));
            }
            let resolved = db.resolve(reader, label).await?;
            if !resolved["matches"].as_array().unwrap().is_empty() {
                leaks.push(format!(
                    "{reader:?}: entity_resolve({label:?}) returns {}",
                    resolved["matches"]
                ));
            }
        }
    }
    let vocabulary = db.vocabulary().await?;
    let terms = vocabulary
        .iter()
        .flat_map(|row| row["terms"].as_array().unwrap())
        .map(|term| term.as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        terms.contains(&"public charter"),
        "the same vocabulary path retains shared old names"
    );
    for label in private_labels {
        if terms.contains(&label) {
            leaks.push(format!("vocabulary exposes private term {label:?}"));
        }
    }

    db.close().await?;
    assert!(
        leaks.is_empty(),
        "private predecessor labels escaped:\n{}",
        leaks.join("\n")
    );

    Ok(())
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p15_history_cannot_cross_or_name_a_hidden_middle_row() -> TestResult {
    // Kills recursive traversal through a hidden bridge, including labels copied past it.
    let db = Isolated::migrated("p15_bridge").await?;
    db.register_readers().await?;
    let origin = db
        .write(fact(
            ROOM,
            "open origin",
            "project",
            "The initial shared assertion.",
            false,
            vec![],
        )?)
        .await?;
    let middle = db
        .write(
            fact(
                ROOM,
                "sealed middle",
                "role",
                "An intermediate private assertion.",
                false,
                vec![origin],
            )?
            .with_axis(
                Some(OperatorKey::new("sol")?),
                Some(OperatorKey::new("sol")?),
                None,
                None,
            )?,
        )
        .await?;
    let terminus = db
        .write(
            fact(
                ROOM,
                "open terminus",
                "project",
                "The final shared assertion.",
                false,
                vec![middle],
            )?
            .with_axis(
                Some(OperatorKey::new("sol")?),
                Some(OperatorKey::new("sol")?),
                Some(CanonScope::Company),
                None,
            )?,
        )
        .await?;
    for start in [origin, terminus] {
        let owner = db.read(Some("sol"), by_id(start, true)?).await?;
        assert_eq!(
            owner
                .entities
                .iter()
                .map(|row| row.entity_id.clone())
                .collect::<Vec<_>>(),
            [origin.to_string(), middle.to_string(), terminus.to_string()],
            "positive control: the same history reader traverses all three visible rows"
        );
    }

    let mut leaks = Vec::new();
    for reader in [Some("ana"), None] {
        for start in [origin, terminus] {
            let history = db.read(reader, by_id(start, true)?).await?;
            assert_eq!(
                history
                    .entities
                    .iter()
                    .map(|row| row.entity_id.clone())
                    .collect::<Vec<_>>(),
                [start.to_string()],
                "{reader:?} crossed the hidden middle from {start}"
            );
            assert_eq!(history.entities[0].superseded_by, None);
            assert!(history.entities[0].supersedes.is_empty());
            let rendered = serde_json::to_string(&history)?;
            if rendered.contains("sealed middle") || rendered.contains(&format!("\"{middle}\"")) {
                leaks.push(format!(
                    "{reader:?}: history from {start} names hidden middle: {rendered}"
                ));
            }
        }
    }

    db.close().await?;
    assert!(
        leaks.is_empty(),
        "history names a hidden bridge:\n{}",
        leaks.join("\n")
    );

    Ok(())
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p16_hidden_predecessor_equals_absent_and_failed_batches_change_nothing() -> TestResult {
    // Kills hidden-predecessor existence oracles and partial supersession before a refusal.
    let db = Isolated::migrated("p16_predecessor").await?;
    db.register_readers().await?;
    let hidden = db
        .write(person_fact(
            "sol",
            "sealed bearing",
            "PRIVATE VIOLET GLASS",
        )?)
        .await?;
    let visible = db
        .write(fact(
            ROOM,
            "open bearing",
            "project",
            "A visible predecessor.",
            false,
            vec![],
        )?)
        .await?;
    let hidden_before = serde_json::to_value(db.read(Some("sol"), by_id(hidden, true)?).await?)?;
    let visible_before = serde_json::to_value(db.read(Some("ana"), by_id(visible, true)?).await?)?;
    for predecessors in [
        vec![hidden],
        vec![ABSENT_ID],
        vec![visible, hidden],
        vec![visible, ABSENT_ID],
    ] {
        let request = fact(
            ROOM,
            "replacement",
            "role",
            "Must not land.",
            false,
            predecessors,
        )?
        .with_axis(
            Some(OperatorKey::new("ana")?),
            Some(OperatorKey::new("ana")?),
            Some(CanonScope::Company),
            None,
        )?;
        assert_eq!(
            invalid_message(canon_write(&db.pool, request).await.unwrap_err()),
            PREDECESSOR_REFUSAL
        );
    }
    assert_eq!(
        serde_json::to_value(db.read(Some("sol"), by_id(hidden, true)?).await?)?,
        hidden_before
    );
    assert_eq!(
        serde_json::to_value(db.read(Some("ana"), by_id(visible, true)?).await?)?,
        visible_before
    );
    assert!(
        db.read(Some("ana"), by_name("replacement", true)?)
            .await?
            .entities
            .is_empty()
    );

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p16_deferred_name_existence_oracle_still_redacts_private_metadata() -> TestResult {
    // OMEGA defers refusal-versus-success leakage; this kills private metadata in the refusal.
    let db = Isolated::migrated("p16_name").await?;
    db.register_readers().await?;
    let hidden = db
        .write(person_fact(
            "sol",
            "sealed bearing",
            "PRIVATE VIOLET GLASS",
        )?)
        .await?;
    let request = fact(
        ROOM,
        "sealed bearing",
        "role",
        "A permitted company assertion.",
        false,
        vec![],
    )?
    .with_axis(
        Some(OperatorKey::new("ana")?),
        Some(OperatorKey::new("sol")?),
        Some(CanonScope::Company),
        None,
    )?;
    let refusal = invalid_message(canon_write(&db.pool, request).await.unwrap_err());
    assert_eq!(refusal, NAME_REFUSAL);
    for private_marker in [
        hidden.to_string(),
        "PRIVATE VIOLET GLASS".into(),
        "sol".into(),
    ] {
        assert!(
            !refusal.contains(&private_marker),
            "private metadata in {refusal:?}"
        );
    }
    let absent_name = fact(
        ROOM,
        "unused bearing",
        "role",
        "A permitted company assertion.",
        false,
        vec![],
    )?
    .with_axis(
        Some(OperatorKey::new("ana")?),
        Some(OperatorKey::new("sol")?),
        Some(CanonScope::Company),
        None,
    )?;
    let shared = db.write(absent_name).await?;
    assert_eq!(
        db.read(Some("ana"), by_id(shared, false)?)
            .await?
            .entities
            .len(),
        1
    );
    let owner = db.read(Some("sol"), by_id(hidden, false)?).await?;
    assert_eq!(owner.entities.len(), 1);
    assert_eq!(owner.entities[0].authority, "active");
    assert_eq!(owner.entities[0].scope, "private");

    db.close().await
}

#[tokio::test]
#[ignore = "requires the assigned isolated PostgreSQL database"]
async fn p20_registration_replay_preserves_one_unit_operator_and_existing_values() -> TestResult {
    // Kills duplicate registration, replay overwrites, and orphan units on a refused move.
    let db = Isolated::migrated("p20").await?;
    let original = registration("sol", "Sol Meridian", "proof");
    let first = register_operator(&db.pool, &original).await?;
    assert!(first.unit_created && first.operator_created);
    let mut replay = registration("sol", "REPLAY MUST NOT RENAME", "proof");
    replay.unit_name = "REPLAY MUST NOT RENAME UNIT".into();
    let second = register_operator(&db.pool, &replay).await?;
    assert!(!second.unit_created && !second.operator_created);
    let units: Vec<(String, String)> =
        sqlx::query_as("SELECT path::text,display_name FROM units ORDER BY path")
            .fetch_all(&db.pool)
            .await?;
    let operators: Vec<(String, String, String, i32)> =
        sqlx::query_as("SELECT key,display_name,unit::text,rank FROM operators ORDER BY key")
            .fetch_all(&db.pool)
            .await?;
    assert_eq!(units, [("proof".into(), "Proof proof".into())]);
    assert_eq!(
        operators,
        [("sol".into(), "Sol Meridian".into(), "proof".into(), 0)]
    );
    let moved = register_operator(&db.pool, &registration("sol", "Sol Meridian", "elsewhere"))
        .await
        .unwrap_err();
    assert_eq!(
        invalid_message(moved),
        "operator is already registered in a different unit"
    );
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM units")
        .fetch_one(&db.pool)
        .await?;
    assert_eq!(after, 1, "refused move must roll back its new unit");

    db.close().await
}

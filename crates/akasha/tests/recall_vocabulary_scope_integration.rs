use akasha::migrations::run_migrations;
use akasha::{Config, EmbeddingMode, RecallResult, recall};
use hearth::{OperatorKey, RecallRequest, RoomKey};
use serde_json::json;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::collections::BTreeSet;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use uuid::Uuid;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const QUERY: &str = "quasarprobe";
const DIMENSION: usize = 2048;

fn test_database_url() -> TestResult<String> {
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")?;
    let options: PgConnectOptions = url.parse()?;
    let database = options
        .get_database()
        .ok_or("an explicit test database is required")?;
    let database = database.to_ascii_lowercase();

    let dedicated = database.starts_with("athanor_hardening_") || database.contains("test");

    if !dedicated
        || ["solarisael", "production", "prod", "live"]
            .iter()
            .any(|name| database.contains(name))
    {
        return Err("refusing a non-test or production-looking database".into());
    }

    Ok(url)
}

fn vector(first: f32, second: f32) -> Vec<f32> {
    let mut values = vec![0.0; DIMENSION];
    values[0] = first;
    values[1] = second;

    values
}

// Only embedding inference is replaced. PostgreSQL selects concepts and ranks real memory rows.
async fn answer_embedding(socket: TcpStream, response: &[u8]) -> TestResult {
    let mut socket = BufReader::new(socket);
    let mut content_length = 0;

    loop {
        let mut line = String::new();
        let read = socket.read_line(&mut line).await?;

        if read == 0 {
            return Err("embedding request ended before its headers".into());
        }
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse::<usize>()?;
        }
    }

    let mut body = vec![0; content_length];
    socket.read_exact(&mut body).await?;
    socket.get_mut().write_all(response).await?;
    socket.get_mut().shutdown().await?;

    Ok(())
}

struct Fixture {
    pool: PgPool,
    cfg: Config,
    room: String,
    embedder: JoinHandle<TestResult>,
}

struct ThreadFixture {
    id: i64,
    memory_id: i64,
    target_id: i64,
    concept: String,
}

impl Fixture {
    async fn new() -> TestResult<Self> {
        let url = test_database_url()?;
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&url)
            .await?;
        run_migrations(&pool).await?;

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let endpoint = format!("http://{}/api/embed", listener.local_addr()?);
        let body = json!({"embeddings": [vector(1.0, 0.0)]}).to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let embedder = tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await?;
                tokio::time::timeout(
                    Duration::from_secs(2),
                    answer_embedding(socket, response.as_bytes()),
                )
                .await??;
            }
        });
        let room = format!("hardening-vocabulary-{}", Uuid::new_v4().simple());
        let cfg = Config {
            database_url: url,
            embed_url: Some(endpoint),
            embed_model: room.clone(),
            embed_dimension: DIMENSION,
            embedding_mode: EmbeddingMode::Required,
            giga_source_ledger_dir: None,
            giga_source_room: None,
            house_tz: "UTC".into(),
            nats_url: None,
        };

        Ok(Self {
            pool,
            cfg,
            room,
            embedder,
        })
    }

    fn operator(&self, role: &str) -> String {
        format!("{}-{role}", self.room)
    }

    async fn register_operators(&self) -> TestResult {
        let unit = self.room.replace('-', "_");
        sqlx::query(
            "INSERT INTO units (path, display_name) VALUES
             ($1::ltree, 'Root'), (($1 || '.team')::ltree, 'Team'),
             (($1 || '_outside')::ltree, 'Outside')",
        )
        .bind(&unit)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "INSERT INTO operators (key, display_name, unit, rank) VALUES
             ($1 || '-writer', 'Writer', ($2 || '.team')::ltree, 1),
             ($1 || '-peer', 'Peer', ($2 || '.team')::ltree, 1),
             ($1 || '-leader', 'Leader', $2::ltree, 10),
             ($1 || '-outsider', 'Outside', ($2 || '_outside')::ltree, 99)",
        )
        .bind(&self.room)
        .bind(&unit)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn memory(
        &self,
        body: &str,
        operator: Option<&str>,
        scope: &str,
        threads: &[String],
    ) -> TestResult<i64> {
        let id = sqlx::query_scalar(
            "INSERT INTO memories (room, type, title, source_path, body, operator, scope, threads)
             VALUES ($1, 'memory', $2, $3, $2, $4, $5, $6) RETURNING id",
        )
        .bind(&self.room)
        .bind(body)
        .bind(format!("hardening/{}.md", Uuid::new_v4()))
        .bind(operator)
        .bind(scope)
        .bind(threads)
        .fetch_one(&self.pool)
        .await?;
        sqlx::query(
            "INSERT INTO memory_chunks (memory_id, chunk_index, body, char_start, char_end)
             VALUES ($1, 0, $2, 0, $3)",
        )
        .bind(id)
        .bind(body)
        .bind(i32::try_from(body.chars().count())?)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    async fn thread(
        &self,
        concept: &str,
        operator: Option<&str>,
        scope: &str,
    ) -> TestResult<ThreadFixture> {
        let id = sqlx::query_scalar(
            "INSERT INTO threads (room, thread_key) VALUES ($1, $2) RETURNING id",
        )
        .bind(&self.room)
        .bind(concept)
        .fetch_one(&self.pool)
        .await?;
        let memory_id = self.member(id, concept, operator, scope).await?;
        let target_id = self.memory(concept, None, "company", &[]).await?;

        Ok(ThreadFixture {
            id,
            memory_id,
            target_id,
            concept: concept.into(),
        })
    }

    async fn member(
        &self,
        thread: i64,
        concept: &str,
        operator: Option<&str>,
        scope: &str,
    ) -> TestResult<i64> {
        let memory_id = self
            .memory("Source note", operator, scope, &[concept.into()])
            .await?;
        sqlx::query("INSERT INTO thread_events (thread_id, memory_id) VALUES ($1, $2)")
            .bind(thread)
            .bind(memory_id)
            .execute(&self.pool)
            .await?;

        Ok(memory_id)
    }

    async fn refresh_sources(&self) -> TestResult {
        sqlx::query("SELECT substrate_refresh_semantic_vocabulary_sources()")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn embed(&self, kind: &str, key: &str, first: f32, second: f32) -> TestResult {
        let changed = sqlx::query(
            "UPDATE semantic_vocabulary
             SET embedding = $1::vector, embedding_model = $2, embedding_dimension = $3,
                 embedded_at = greatest(clock_timestamp(), source_updated_at)
             WHERE source_kind = $4 AND source_key = $5",
        )
        .bind(serde_json::to_string(&vector(first, second))?)
        .bind(&self.cfg.embed_model)
        .bind(i32::try_from(DIMENSION)?)
        .bind(kind)
        .bind(key)
        .execute(&self.pool)
        .await?
        .rows_affected();

        if changed != 1 {
            return Err(
                format!("expected one canonical vocabulary source, changed {changed}").into(),
            );
        }

        Ok(())
    }

    async fn recall_as(
        &self,
        operator: Option<&str>,
        query: &str,
        bridge: bool,
    ) -> TestResult<RecallResult> {
        let request = RecallRequest::new(RoomKey::new(&self.room)?, query.into(), 8, 0.9, 8, 0.9)?
            .with_operator(operator.map(OperatorKey::new).transpose()?);
        let mut cfg = self.cfg.clone();

        if !bridge {
            cfg.embedding_mode = EmbeddingMode::DisabledForTest;
        }

        Ok(recall(&self.pool, &cfg, request, None).await?)
    }

    async fn remove_rows(&self) -> TestResult {
        let mut tx = self.pool.begin().await?;

        for statement in [
            "DELETE FROM semantic_vocabulary v
             WHERE v.room = $1 OR v.embedding_model = $1
                OR (v.source_kind = 'named_entity' AND EXISTS (
                    SELECT 1 FROM named_entities e
                    WHERE e.id::text = v.source_key AND e.meta->>'hardening_vocabulary_room' = $1))
                OR (v.source_kind = 'lesson_metadata' AND EXISTS (
                    SELECT 1 FROM lessons l
                    WHERE l.lesson_key || ':' || l.id::text = v.source_key
                      AND l.meta->>'hardening_vocabulary_room' = $1))",
            "DELETE FROM named_entities WHERE meta->>'hardening_vocabulary_room' = $1",
            "DELETE FROM lessons WHERE meta->>'hardening_vocabulary_room' = $1",
            "DELETE FROM memories WHERE room = $1",
            "DELETE FROM threads WHERE room = $1",
            "DELETE FROM operators
             WHERE key IN ($1 || '-writer', $1 || '-peer', $1 || '-leader', $1 || '-outsider')",
            "DELETE FROM units
             WHERE path IN (replace($1, '-', '_')::ltree,
                            (replace($1, '-', '_') || '.team')::ltree,
                            (replace($1, '-', '_') || '_outside')::ltree)",
        ] {
            sqlx::query(statement)
                .bind(&self.room)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        let remaining: i64 = sqlx::query_scalar(
            "SELECT
                (SELECT count(*) FROM semantic_vocabulary WHERE room = $1 OR embedding_model = $1)
              + (SELECT count(*) FROM named_entities WHERE meta->>'hardening_vocabulary_room' = $1)
              + (SELECT count(*) FROM lessons WHERE meta->>'hardening_vocabulary_room' = $1)
              + (SELECT count(*) FROM memories WHERE room = $1)
              + (SELECT count(*) FROM threads WHERE room = $1)
              + (SELECT count(*) FROM operators
                 WHERE key IN ($1 || '-writer', $1 || '-peer', $1 || '-leader', $1 || '-outsider'))
              + (SELECT count(*) FROM units
                 WHERE path IN (replace($1, '-', '_')::ltree,
                                (replace($1, '-', '_') || '.team')::ltree,
                                (replace($1, '-', '_') || '_outside')::ltree))",
        )
        .bind(&self.room)
        .fetch_one(&self.pool)
        .await?;

        if remaining != 0 {
            return Err(format!(
                "{} retains {remaining} fixture rows after cleanup",
                self.room
            )
            .into());
        }

        eprintln!(
            "recall vocabulary fixture {}: {remaining} owned rows remain",
            self.room
        );

        Ok(())
    }

    async fn close(mut self) -> TestResult {
        // Recall's content lane commits its own transaction. Cleanup must use row ownership, not an outer BEGIN.
        let cleanup = self.remove_rows().await;
        self.pool.close().await;
        self.embedder.abort();
        let stopped = (&mut self.embedder).await;
        cleanup.map_err(|error| format!("fixture cleanup failed for {}: {error}", self.room))?;

        match stopped {
            Ok(result) => result?,
            Err(error) if error.is_cancelled() => {}
            Err(error) => return Err(error.into()),
        }

        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.embedder.abort();
    }
}

async fn observe<T, F, Fut>(operation: F) -> TestResult<T>
where
    T: Send + 'static,
    F: FnOnce(Arc<Fixture>) -> Fut,
    Fut: Future<Output = TestResult<T>> + Send + 'static,
{
    let fixture = Arc::new(Fixture::new().await?);
    let observation = tokio::spawn(operation(Arc::clone(&fixture))).await;
    let fixture = Arc::try_unwrap(fixture)
        .map_err(|_| "the observation retained ownership of its fixture")?;
    fixture.close().await?;

    match observation {
        Ok(result) => result,
        Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
        Err(error) => Err(error.into()),
    }
}

fn memory_ids(result: &RecallResult) -> BTreeSet<i64> {
    result
        .retrieval_candidates
        .iter()
        .filter_map(|candidate| candidate["memory_id"].as_i64())
        .collect()
}

fn concepts(result: &RecallResult) -> BTreeSet<String> {
    result
        .retrieval_candidates
        .iter()
        .filter_map(|candidate| candidate["semantic_lexical_concepts"].as_array())
        .flatten()
        .filter_map(|concept| concept["concept"].as_str().map(str::to_owned))
        .collect()
}

fn assert_expansion(result: &RecallResult, sources: &[&ThreadFixture], context: &str) {
    for source in sources {
        assert!(
            memory_ids(result).contains(&source.target_id),
            "{context}: missing target for {}",
            source.concept
        );
    }

    let expected = sources
        .iter()
        .map(|source| source.concept.clone())
        .collect();
    assert_eq!(
        concepts(result),
        expected,
        "{context}: vocabulary crossed scope"
    );
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL pointing to an isolated test database"]
async fn hardening_thread_vocabulary_uses_operator_visibility() -> TestResult {
    let (company, private, upward, inactive, lexical_only, author, leader, restricted) =
        observe(|db| async move {
            let operators: i64 = sqlx::query_scalar("SELECT count(*) FROM operators")
                .fetch_one(&db.pool)
                .await?;

            if operators != 0 {
                return Err(
                    "the inactive-axis control requires a database with no committed operators"
                        .into(),
                );
            }

            let writer = db.operator("writer");
            let company = db.thread("cedar", Some(&writer), "company").await?;
            let private = db.thread("basalt", Some(&writer), "private").await?;
            let upward = db.thread("marble", Some(&writer), "up").await?;
            db.refresh_sources().await?;

            for source in [&company, &private, &upward] {
                db.embed("active_thread", &source.id.to_string(), 1.0, 0.0)
                    .await?;
            }

            let inactive = db.recall_as(None, QUERY, true).await?;
            db.register_operators().await?;
            let lexical_only = db.recall_as(Some(&writer), QUERY, false).await?;
            let author = db.recall_as(Some(&writer), QUERY, true).await?;
            let leader = db
                .recall_as(Some(&db.operator("leader")), QUERY, true)
                .await?;
            let mut restricted = Vec::new();

            for role in [Some("peer"), Some("outsider"), Some("unknown"), None] {
                let operator = role.map(|role| db.operator(role));
                let result = db.recall_as(operator.as_deref(), QUERY, true).await?;
                restricted.push((role, result));
            }

            Ok((
                company,
                private,
                upward,
                inactive,
                lexical_only,
                author,
                leader,
                restricted,
            ))
        })
        .await?;

    assert!(
        memory_ids(&lexical_only).is_empty(),
        "the unrelated query must need vocabulary expansion"
    );
    assert_expansion(&inactive, &[&company, &private, &upward], "inactive axis");
    assert_expansion(&author, &[&company, &private, &upward], "writer control");
    assert_expansion(&leader, &[&company, &upward], "ancestor control");
    assert!(
        !memory_ids(&leader).contains(&private.target_id),
        "rank cannot expose a private concept through a public target"
    );

    for (role, result) in restricted {
        assert_expansion(&result, &[&company], &format!("reader {role:?}"));
        assert!(
            !memory_ids(&result).contains(&private.target_id),
            "{role:?}: private vocabulary expanded a public memory"
        );
        assert!(
            !memory_ids(&result).contains(&upward.target_id),
            "{role:?}: another operator's upward vocabulary expanded a public memory"
        );
        assert!(
            !memory_ids(&result).contains(&private.memory_id),
            "{role:?}: private source memory became visible"
        );
        assert!(
            !memory_ids(&result).contains(&upward.memory_id),
            "{role:?}: upward source memory became visible"
        );
    }

    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL pointing to an isolated test database"]
async fn hardening_hidden_concepts_cannot_exhaust_ranked_vocabulary() -> TestResult {
    let (visible, hidden, before, author, after) = observe(|db| async move {
        db.register_operators().await?;
        let writer = db.operator("writer");
        let peer = db.operator("peer");
        let visible = db.thread("juniper", Some(&writer), "company").await?;
        db.refresh_sources().await?;
        db.embed("active_thread", &visible.id.to_string(), 0.6, 0.8)
            .await?;
        let before = db.recall_as(Some(&peer), QUERY, true).await?;
        let mut hidden = Vec::new();

        for concept in [
            "agate", "beryl", "cobalt", "diamond", "emerald", "feldspar", "garnet", "hematite",
        ] {
            hidden.push(db.thread(concept, Some(&writer), "private").await?);
        }

        db.refresh_sources().await?;
        db.embed("active_thread", &visible.id.to_string(), 0.6, 0.8)
            .await?;

        for source in &hidden {
            db.embed("active_thread", &source.id.to_string(), 1.0, 0.0)
                .await?;
        }

        let author = db.recall_as(Some(&writer), QUERY, true).await?;
        let after = db.recall_as(Some(&peer), QUERY, true).await?;

        Ok((visible, hidden, before, author, after))
    })
    .await?;

    assert_expansion(&before, &[&visible], "visible concept before competition");
    assert!(
        hidden
            .iter()
            .any(|source| memory_ids(&author).contains(&source.target_id)),
        "writer control must reach a high-ranked private concept"
    );
    assert!(
        memory_ids(&after).contains(&visible.target_id),
        "hidden high-ranked concepts consumed the vocabulary limit before visibility filtering"
    );
    assert_expansion(
        &after,
        &[&visible],
        "visible concept after private competition",
    );

    for source in hidden {
        assert!(
            !memory_ids(&after).contains(&source.target_id),
            "hidden competitor expanded a public target: {}",
            source.concept
        );
    }

    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL pointing to an isolated test database"]
async fn hardening_scope_changes_revoke_thread_terms_without_refresh() -> TestResult {
    let (stable, changing, before, author, revoked, restored, embedded_sources) = observe(|db| async move {
        db.register_operators().await?;
        let writer = db.operator("writer");
        let peer = db.operator("peer");
        let stable = db.thread("willow", Some(&writer), "company").await?;
        let changing = db.thread("topaz", Some(&writer), "company").await?;
        db.member(changing.id, &changing.concept, Some(&writer), "private").await?;
        db.refresh_sources().await?;

        for source in [&stable, &changing] {
            db.embed("active_thread", &source.id.to_string(), 1.0, 0.0).await?;
        }

        let before = db.recall_as(Some(&peer), QUERY, true).await?;
        sqlx::query("UPDATE memories SET scope = 'private' WHERE id = $1")
            .bind(changing.memory_id).execute(&db.pool).await?;
        let author = db.recall_as(Some(&writer), QUERY, true).await?;
        let revoked = db.recall_as(Some(&peer), QUERY, true).await?;
        let embedded_sources: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM semantic_vocabulary WHERE embedding_model = $1 AND embedding IS NOT NULL",
        ).bind(&db.cfg.embed_model).fetch_one(&db.pool).await?;
        sqlx::query("UPDATE memories SET scope = 'company' WHERE id = $1")
            .bind(changing.memory_id).execute(&db.pool).await?;
        let restored = db.recall_as(Some(&peer), QUERY, true).await?;

        Ok((stable, changing, before, author, revoked, restored, embedded_sources))
    }).await?;

    assert_eq!(
        embedded_sources, 2,
        "the test must retain both cached concept vectors across the scope change"
    );
    assert_expansion(
        &before,
        &[&stable, &changing],
        "one visible member is sufficient",
    );
    assert_expansion(
        &author,
        &[&stable, &changing],
        "writer retains both private members",
    );
    assert_expansion(
        &restored,
        &[&stable, &changing],
        "scope widened again without refresh",
    );
    assert!(
        memory_ids(&revoked).contains(&stable.target_id),
        "revocation must preserve the independent visible concept"
    );
    assert!(
        !memory_ids(&revoked).contains(&changing.target_id),
        "cached thread terms still expand a public target after the last visible member becomes private"
    );
    assert_expansion(&revoked, &[&stable], "revoked thread concept");

    Ok(())
}

#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL pointing to an isolated test database"]
async fn hardening_global_and_legacy_vocabulary_keep_canonical_sources() -> TestResult {
    let (legacy, canon, lesson, canon_target, lesson_target, archived_target, archived_indexed, results) = observe(|db| async move {
        db.register_operators().await?;
        let legacy = db.thread("alabaster", None, "private").await?;
        let canon = format!("canon{}", Uuid::new_v4().simple());
        let archived = format!("archived{}", Uuid::new_v4().simple());
        let lesson = format!("lesson{}", Uuid::new_v4().simple());
        let active_id: i64 = sqlx::query_scalar(
            "INSERT INTO named_entities (room, name, kind, summary, meta)
             VALUES ('house', $1, 'concept', 'Shared concept',
                     jsonb_build_object('hardening_vocabulary_room', $2::text)) RETURNING id",
        ).bind(&canon).bind(&db.room).fetch_one(&db.pool).await?;
        let archived_id: i64 = sqlx::query_scalar(
            "INSERT INTO named_entities (room, name, kind, summary, authority, meta)
             VALUES ('house', $1, 'concept', 'Archived concept', 'archived',
                     jsonb_build_object('hardening_vocabulary_room', $2::text)) RETURNING id",
        ).bind(&archived).bind(&db.room).fetch_one(&db.pool).await?;
        let lesson_id: i64 = sqlx::query_scalar(
            "INSERT INTO lessons (lesson_key, scope, title, lesson, meta)
             VALUES ('coding', 'house', $1, 'Shared lesson',
                     jsonb_build_object('hardening_vocabulary_room', $2::text)) RETURNING id",
        ).bind(&lesson).bind(&db.room).fetch_one(&db.pool).await?;
        let canon_target = db.memory(&canon, None, "company", &[]).await?;
        let lesson_target = db.memory(&lesson, None, "company", &[]).await?;
        let archived_target = db.memory(&archived, None, "company", &[]).await?;
        db.refresh_sources().await?;
        db.embed("active_thread", &legacy.id.to_string(), 1.0, 0.0).await?;
        db.embed("named_entity", &active_id.to_string(), 1.0, 0.0).await?;
        db.embed("lesson_metadata", &format!("coding:{lesson_id}"), 1.0, 0.0).await?;
        let archived_indexed: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM semantic_vocabulary WHERE source_kind = 'named_entity' AND source_key = $1)",
        ).bind(archived_id.to_string()).fetch_one(&db.pool).await?;
        let mut results = Vec::new();

        for role in [Some("peer"), Some("unknown"), None] {
            let operator = role.map(|role| db.operator(role));
            results.push(db.recall_as(operator.as_deref(), QUERY, true).await?);
        }

        Ok((legacy, canon, lesson, canon_target, lesson_target, archived_target, archived_indexed, results))
    }).await?;

    assert!(
        !archived_indexed,
        "the production source refresh must exclude archived canon"
    );

    for result in results {
        let ids = memory_ids(&result);
        assert!(
            ids.contains(&legacy.target_id),
            "operator-less legacy sources remain permitted"
        );
        assert!(
            ids.contains(&canon_target),
            "active global canon must remain a vocabulary source"
        );
        assert!(
            ids.contains(&lesson_target),
            "global lesson metadata must remain a vocabulary source"
        );
        assert!(
            !ids.contains(&archived_target),
            "archived canon must not expand a public target"
        );
        assert_eq!(
            concepts(&result),
            BTreeSet::from([legacy.concept.clone(), canon.clone(), lesson.clone()])
        );
        let kinds: BTreeSet<&str> = result
            .retrieval_candidates
            .iter()
            .filter_map(|candidate| candidate["semantic_lexical_concepts"].as_array())
            .flatten()
            .filter_map(|concept| concept["source_kind"].as_str())
            .collect();
        assert_eq!(
            kinds,
            BTreeSet::from(["active_thread", "named_entity", "lesson_metadata"])
        );
    }

    Ok(())
}

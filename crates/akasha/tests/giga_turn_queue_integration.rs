//! Durable per-turn GIGA queue against real PostgreSQL through the canonical
//! migration runner: capture identity, count/timer batch release, bounded
//! claims, deadline settlement, and the 0043 legacy-window cutover.

use akasha::{
    AppError, giga_conversation_ingest, giga_event_claim, giga_event_deadline_settle,
    giga_event_finish, giga_event_ingest, giga_event_replay, giga_health, giga_queue_maintenance,
    giga_turn_release, migrations::run_migrations,
};
use hearth::{
    GigaEvent, GigaEventClaimReceipt, GigaEventClaimRequest, GigaEventFinishOutcome,
    GigaEventFinishRequest, GigaEventReplayRequest, GigaEventType, GigaLifecycle,
    GigaQueueMaintenanceOperation, GigaQueueMaintenanceRequest, GigaQueueMaintenanceScope,
    GigaQueueState, GigaScope, GigaSourceRef, GigaSourceType, GigaVisibility, RoomKey,
};
use protocol::{
    GIGA_CONVERSATION_INGEST_MAX_TURNS, GigaConversationIngestParams, GigaConversationIngestResult,
    GigaConversationTurnParams, GigaEventIngestDisposition, GigaHealthParams,
};
use sha2::{Digest, Sha256};
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{error::Error, str::FromStr, time::Duration};
use uuid::Uuid;

const TURN_AT: &str = "2030-01-01T00:00:00Z";
const WINDOW_AT: &str = "2030-01-01T00:00:01Z";
const MIGRATION_0043: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../substrate/migrations/0043_giga_turn_queue.sql"
));

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn require(condition: bool, message: impl Into<String>) -> TestResult {
    if condition {
        Ok(())
    } else {
        Err(std::io::Error::other(message.into()).into())
    }
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

/// Runs `scenario` in a fresh schema built by the production migration runner,
/// then drops only that schema.
async fn in_isolated_schema<F, Fut>(context: &str, scenario: F)
where
    F: FnOnce(PgPool) -> Fut,
    Fut: std::future::Future<Output = TestResult>,
{
    let options = PgConnectOptions::from_str(&isolated_database_url())
        .expect("dedicated test URL must be valid");
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .expect("isolated database must be reachable");
    let schema = format!("solarisael_giga_turn_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .expect("isolated GIGA schema must create");
    let connection_schema = schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(8)
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
    let result = match pool {
        Ok(pool) => {
            let result = match run_migrations(&pool).await {
                Ok(_) => scenario(pool.clone()).await,
                Err(error) => Err(error.into()),
            };
            pool.close().await;
            result
        }
        Err(error) => Err(error.into()),
    };
    let cleanup = sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await;
    admin.close().await;
    cleanup.expect("isolated GIGA schema cleanup must succeed");
    result.expect(context);
}

fn content_hash(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

/// The documented canonical job id: sha256("giga_turn_v1" NUL room NUL session
/// NUL source), lower hex. Capture and migration 0043 must both produce it.
fn turn_event_id(room: &str, session: &str, source: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"giga_turn_v1\0");
    digest.update(room.as_bytes());
    digest.update(b"\0");
    digest.update(session.as_bytes());
    digest.update(b"\0");
    digest.update(source.as_bytes());
    format!("{:x}", digest.finalize())
}

fn turn(session: &str, source: &str, role: &str, content: &str) -> GigaConversationTurnParams {
    GigaConversationTurnParams {
        role: role.into(),
        source_id: source.into(),
        content_hash: content_hash(content),
        session_id: session.into(),
        timestamp: TURN_AT.into(),
        has_stable_id: true,
    }
}

fn user_turns(session: &str, sources: &[&str]) -> Vec<GigaConversationTurnParams> {
    sources
        .iter()
        .map(|source| turn(session, source, "user", source))
        .collect()
}

async fn capture(
    pool: &PgPool,
    room: &str,
    project: Option<&str>,
    turns: Vec<GigaConversationTurnParams>,
) -> Result<GigaConversationIngestResult, AppError> {
    giga_conversation_ingest(
        pool,
        GigaConversationIngestParams {
            room: room.into(),
            project_keys: project.map(|key| vec![key.to_owned()]).unwrap_or_default(),
            turns,
        },
    )
    .await
}

async fn release(
    pool: &PgPool,
    room: &str,
    batch_turns: u32,
    wait_seconds: u32,
) -> TestResult<u64> {
    Ok(giga_turn_release(pool, &RoomKey::new(room)?, batch_turns, wait_seconds).await?)
}

async fn claim(
    pool: &PgPool,
    room: &str,
    worker_id: &str,
    lease_seconds: u32,
) -> TestResult<GigaEventClaimReceipt> {
    let request = GigaEventClaimRequest::new(RoomKey::new(room)?, worker_id.into(), lease_seconds)?;
    Ok(giga_event_claim(pool, request).await?)
}

fn claimed_id(claim: &GigaEventClaimReceipt) -> Option<String> {
    claim.event().map(|event| event.event_id().to_owned())
}

async fn waiting_turns(pool: &PgPool, room: &str, session: &str) -> TestResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM giga_events
         WHERE room=$1 AND session_id=$2 AND waiting_since IS NOT NULL",
    )
    .bind(room)
    .bind(session)
    .fetch_one(pool)
    .await?)
}

fn finish_request(
    room: &str,
    event_id: &str,
    worker_id: &str,
    outcome: GigaEventFinishOutcome,
    error_class: Option<&str>,
) -> TestResult<GigaEventFinishRequest> {
    Ok(GigaEventFinishRequest::new(
        RoomKey::new(room)?,
        event_id.into(),
        worker_id.into(),
        outcome,
        0,
        error_class.map(str::to_owned),
        None,
    )?)
}

/// Count release, unclaimable waiting turns, durable release after partial
/// claims, independent conversations, and concurrent bounded claims.
async fn count_release_contract(pool: &PgPool) -> TestResult {
    let room = "turn-count";
    let first = capture(
        pool,
        room,
        None,
        user_turns("s1", &["c1", "c2", "c3", "c4", "c5", "c6", "c7"]),
    )
    .await?;
    require(
        first.accepted_turns == 7 && first.duplicate_turns == 0 && first.event_ids.len() == 7,
        "seven new turns must become seven turn jobs",
    )?;
    require(
        release(pool, room, 8, 3_600).await? == 0,
        "a group below the batch count must keep waiting",
    )?;
    require(
        claimed_id(&claim(pool, room, "count-early", 60).await?).is_none(),
        "waiting turns must not be claimable",
    )?;

    capture(pool, room, None, user_turns("s1", &["c8"])).await?;
    capture(pool, room, None, user_turns("s2", &["d1", "d2", "d3"])).await?;
    require(
        release(pool, room, 8, 3_600).await? == 8,
        "the eighth waiting turn must release its whole conversation",
    )?;
    require(
        waiting_turns(pool, room, "s1").await? == 0 && waiting_turns(pool, room, "s2").await? == 3,
        "release must not reach an independent conversation below its own count",
    )?;

    let (left, right, third, fourth) = tokio::join!(
        claim(pool, room, "count-a", 60),
        claim(pool, room, "count-b", 60),
        claim(pool, room, "count-c", 60),
        claim(pool, room, "count-d", 60)
    );
    let mut claimed = [left?, right?, third?, fourth?]
        .iter()
        .map(claimed_id)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| std::io::Error::other("every concurrent claim must win a released turn"))?;
    claimed.sort();
    claimed.dedup();
    require(
        claimed.len() == 4,
        "concurrent claims must lease distinct turns",
    )?;

    // Four of eight are leased; the group's remaining turns stay released.
    for worker in ["count-e", "count-f", "count-g", "count-h"] {
        let next = claim(pool, room, worker, 60).await?;
        require(
            next.event().is_some_and(|event| event.session_id() == "s1"),
            "a released batch must stay claimable after part of it is claimed",
        )?;
    }
    require(
        claimed_id(&claim(pool, room, "count-none", 60).await?).is_none(),
        "only released turns may be claimed",
    )?;

    capture(pool, room, None, user_turns("s1", &["c9"])).await?;
    require(
        release(pool, room, 8, 3_600).await? == 0 && waiting_turns(pool, room, "s1").await? == 1,
        "a turn arriving after release must start a new waiting group",
    )?;

    let bulk = (1..=10).map(|n| format!("b{n}")).collect::<Vec<_>>();
    let bulk = bulk.iter().map(String::as_str).collect::<Vec<_>>();
    capture(pool, room, None, user_turns("s3", &bulk)).await?;
    require(
        release(pool, room, 8, 3_600).await? == 10,
        "a conversation at or above the count releases every waiting turn",
    )?;
    require(
        giga_turn_release(pool, &RoomKey::new(room)?, 0, 120)
            .await
            .is_err()
            && giga_turn_release(pool, &RoomKey::new(room)?, 8, 3_601)
                .await
                .is_err(),
        "release must refuse settings outside their validated bounds",
    )
}

/// The oldest waiting turn sets the deadline: later arrivals never postpone it,
/// a partial batch releases on time, and another conversation keeps its own.
async fn timer_release_contract(pool: &PgPool) -> TestResult {
    let room = "turn-timer";
    capture(pool, room, None, user_turns("t1", &["first"])).await?;
    require(
        release(pool, room, 8, 4).await? == 0,
        "a fresh turn must wait",
    )?;
    tokio::time::sleep(Duration::from_millis(2_000)).await;
    capture(pool, room, None, user_turns("t1", &["second"])).await?;
    capture(pool, room, None, user_turns("t2", &["other"])).await?;
    require(
        release(pool, room, 8, 4).await? == 0,
        "no turn has reached the deadline yet",
    )?;
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    require(
        release(pool, room, 8, 4).await? == 2,
        "the oldest turn's deadline must release its partial batch, including the newer turn",
    )?;
    require(
        waiting_turns(pool, room, "t1").await? == 0 && waiting_turns(pool, room, "t2").await? == 1,
        "an independent conversation must keep its own deadline",
    )
}

/// Capture identity: changed grouping, repeats, conflicts, atomic refusal,
/// bounds, concurrent overlap, and the retired tombstone after a purge.
async fn capture_identity_contract(pool: &PgPool) -> TestResult {
    let room = "turn-capture";
    let first = capture(pool, room, None, user_turns("g", &["g1", "g2", "g3"])).await?;
    let regrouped = capture(pool, room, None, user_turns("g", &["g2", "g3", "g4"])).await?;
    require(
        first.event_ids
            == ["g1", "g2", "g3"]
                .map(|source| turn_event_id(room, "g", source))
                .to_vec(),
        "each captured turn must own its documented canonical job id",
    )?;
    require(
        regrouped.accepted_turns == 1
            && regrouped.duplicate_turns == 2
            && regrouped.event_ids
                == ["g2", "g3", "g4"]
                    .map(|source| turn_event_id(room, "g", source))
                    .to_vec(),
        "a changed grouping must keep one stable job per source",
    )?;
    let repeated = capture(pool, room, None, user_turns("g", &["g5", "g5"])).await?;
    require(
        repeated.accepted_turns == 1
            && repeated.duplicate_turns == 1
            && repeated.event_ids == vec![turn_event_id(room, "g", "g5")],
        "a turn repeated inside one capture must collapse to its one job",
    )?;
    let per_source: Vec<i64> = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM giga_event_sources s
         JOIN giga_events e ON e.event_id=s.event_id
         WHERE e.room=$1 GROUP BY s.source_id ORDER BY s.source_id",
    )
    .bind(room)
    .fetch_all(pool)
    .await?;
    require(
        per_source == vec![1; 5],
        "every captured source must appear in exactly one event",
    )?;

    let changed_hash = turn("g", "g1", "user", "edited text");
    let changed_role = turn("g", "g1", "assistant", "g1");
    for (label, project, refused) in [
        ("content hash", None, changed_hash.clone()),
        ("role", None, changed_role),
        (
            "project",
            Some("other-project"),
            user_turns("g", &["g1"]).remove(0),
        ),
    ] {
        require(
            capture(pool, room, project, vec![refused]).await.is_err(),
            format!("a repeated identity with a different {label} must be refused"),
        )?;
    }
    require(
        capture(
            pool,
            room,
            None,
            vec![turn("g", "g6", "user", "g6"), changed_hash],
        )
        .await
        .is_err(),
        "one conflicting turn must refuse its whole capture",
    )?;
    let stored_g6: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM giga_turn_identities WHERE room=$1 AND source_id='g6')
             OR EXISTS(SELECT 1 FROM giga_events WHERE event_id=$2)",
    )
    .bind(room)
    .bind(turn_event_id(room, "g", "g6"))
    .fetch_one(pool)
    .await?;
    require(!stored_g6, "a refused capture must store none of its turns")?;

    let too_many = (0..=GIGA_CONVERSATION_INGEST_MAX_TURNS)
        .map(|n| turn("bounds", &format!("n{n}"), "user", "bound"))
        .collect::<Vec<_>>();
    let mut unstable = turn("bounds", "unstable", "user", "x");
    unstable.has_stable_id = false;
    let invalid = [
        (None, Vec::new()),
        (None, too_many),
        (None, vec![unstable]),
        (None, vec![turn("bounds", "system", "system", "x")]),
    ];
    for (project, turns) in invalid {
        require(
            capture(pool, room, project, turns).await.is_err(),
            "capture must refuse empty, oversized, unstable, and non-conversation input",
        )?;
    }
    require(
        giga_conversation_ingest(
            pool,
            GigaConversationIngestParams {
                room: room.into(),
                project_keys: vec!["one".into(), "two".into()],
                turns: user_turns("bounds", &["two-projects"]),
            },
        )
        .await
        .is_err(),
        "capture must refuse more than one project key",
    )?;

    let (left, right) = tokio::join!(
        capture(pool, room, None, user_turns("race", &["p", "q", "r"])),
        capture(pool, room, None, user_turns("race", &["r", "q", "s"]))
    );
    let (left, right) = (left?, right?);
    let race_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM giga_events WHERE room=$1 AND session_id='race'",
    )
    .bind(room)
    .fetch_one(pool)
    .await?;
    require(
        left.accepted_turns + right.accepted_turns == 4
            && left.duplicate_turns + right.duplicate_turns == 2
            && race_events == 4,
        "overlapping concurrent captures must store each turn once",
    )?;

    let waiting_purge = giga_queue_maintenance(
        pool,
        GigaQueueMaintenanceRequest::new(
            RoomKey::new(room)?,
            GigaQueueMaintenanceOperation::PurgeStuck,
            GigaQueueMaintenanceScope::Room,
        ),
    )
    .await?;
    require(
        waiting_purge.eligible_events == 0 && waiting_purge.deleted_events == 0,
        "maintenance must preserve unreleased turns",
    )?;
    require(
        release(pool, room, 1, 120).await? == 9,
        "waiting turns must remain releasable",
    )?;
    let purge = giga_queue_maintenance(
        pool,
        GigaQueueMaintenanceRequest::new(
            RoomKey::new(room)?,
            GigaQueueMaintenanceOperation::PurgeStuck,
            GigaQueueMaintenanceScope::Room,
        ),
    )
    .await?;
    require(
        purge.deleted_events == 9,
        "purge must remove eligible released jobs",
    )?;
    let retired: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT state FROM giga_turn_identities WHERE room=$1")
            .bind(room)
            .fetch_all(pool)
            .await?;
    require(
        retired == vec!["retired".to_owned()],
        "purging a job must leave its identity as a retired tombstone",
    )?;
    let recaptured = capture(pool, room, None, user_turns("g", &["g1", "g2"])).await?;
    let resurrected: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM giga_events WHERE room=$1")
            .bind(room)
            .fetch_one(pool)
            .await?;
    require(
        recaptured.accepted_turns == 0
            && recaptured.duplicate_turns == 2
            && recaptured.event_ids.is_empty()
            && resurrected == 0,
        "recapturing a retired turn must acknowledge it without recreating work",
    )?;
    require(
        capture(
            pool,
            room,
            None,
            vec![turn("g", "g1", "user", "edited text")],
        )
        .await
        .is_err(),
        "a retired identity must keep refusing conflicting evidence",
    )
}

async fn malformed_event_does_not_starve_neighbors(pool: &PgPool) -> TestResult {
    let room = "turn-malformed";
    let receipt = capture(
        pool,
        room,
        None,
        user_turns("bad-session", &["bad", "healthy"]),
    )
    .await?;
    release(pool, room, 1, 120).await?;
    sqlx::query(
        "UPDATE giga_events SET event_type='task_started',available_at=NOW()-INTERVAL '2 seconds'
         WHERE event_id=$1",
    )
    .bind(&receipt.event_ids[0])
    .execute(pool)
    .await?;

    let refused = claim(pool, room, "decode-first", 60).await?;
    require(
        refused.event().is_none(),
        "malformed lifecycle must not become a worker claim",
    )?;
    let bad: (String, Option<String>) =
        sqlx::query_as("SELECT queue_state,last_error FROM giga_events WHERE event_id=$1")
            .bind(&receipt.event_ids[0])
            .fetch_one(pool)
            .await?;
    require(
        bad == ("failed".into(), Some("GigaStoredEventInvalid".into())),
        "malformed stored work must retain a visible terminal failure",
    )?;
    let next = claim(pool, room, "decode-next", 60).await?;
    require(
        claimed_id(&next) == Some(receipt.event_ids[1].clone()),
        "a malformed older row must not prevent the healthy neighbor claim",
    )
}

/// Bounded leases on turn jobs: committed results stay committed, deadline
/// settlement needs the exact live claim, stale workers cannot write, and the
/// final attempt settles as failed.
async fn lease_and_deadline_contract(pool: &PgPool) -> TestResult {
    let room = "turn-lease";
    capture(pool, room, None, user_turns("l", &["l1", "l2"])).await?;
    require(
        release(pool, room, 2, 3_600).await? == 2,
        "two turns must release",
    )?;
    let (done, slow) = tokio::join!(
        claim(pool, room, "lease-done", 60),
        claim(pool, room, "lease-slow", 60)
    );
    let (done, slow) = (done?, slow?);
    let done_id = claimed_id(&done).ok_or_else(|| std::io::Error::other("first claim"))?;
    let slow_id = claimed_id(&slow).ok_or_else(|| std::io::Error::other("second claim"))?;
    require(
        done_id != slow_id,
        "concurrent claims must lease distinct turns",
    )?;

    giga_event_finish(
        pool,
        finish_request(
            room,
            &done_id,
            "lease-done",
            GigaEventFinishOutcome::Succeeded,
            None,
        )?,
    )
    .await?;
    require(
        giga_event_deadline_settle(pool, &done).await?.is_none(),
        "a deadline after a committed result must not settle it again",
    )?;
    let done_state: String =
        sqlx::query_scalar("SELECT queue_state FROM giga_events WHERE event_id=$1")
            .bind(&done_id)
            .fetch_one(pool)
            .await?;
    require(
        done_state == "succeeded",
        "a committed result stays committed",
    )?;

    let settled = giga_event_deadline_settle(pool, &slow)
        .await?
        .ok_or_else(|| std::io::Error::other("the live claim must settle"))?;
    require(
        settled.queue_state() == GigaQueueState::Pending && settled.attempt_count() == 1,
        "a missed deadline before the final attempt must return the turn for retry",
    )?;
    let attempt = sqlx::query(
        "SELECT outcome,error_class FROM giga_event_attempts
         WHERE event_id=$1 AND attempt_count=1",
    )
    .bind(&slow_id)
    .fetch_one(pool)
    .await?;
    require(
        attempt.try_get::<Option<String>, _>("outcome")?.as_deref() == Some("retry")
            && attempt
                .try_get::<Option<String>, _>("error_class")?
                .as_deref()
                == Some("GigaWorkerDeadlineExceeded"),
        "deadline settlement must record its own error class",
    )?;
    require(
        giga_event_deadline_settle(pool, &slow).await?.is_none(),
        "a settled claim must not settle twice",
    )?;
    require(
        claimed_id(&claim(pool, room, "lease-early", 60).await?).is_none(),
        "a deadline retry must honour its delay",
    )?;

    let skip_retry_delay =
        "UPDATE giga_events SET available_at=clock_timestamp() WHERE event_id=$1";
    sqlx::query(skip_retry_delay)
        .bind(&slow_id)
        .execute(pool)
        .await?;
    let stale = claim(pool, room, "lease-stale", 60).await?;
    require(
        stale.attempt_count() == Some(2),
        "the retry must be attempt two",
    )?;
    sqlx::query(
        "UPDATE giga_events SET lease_expires_at=clock_timestamp()-INTERVAL '1 second'
         WHERE event_id=$1",
    )
    .bind(&slow_id)
    .execute(pool)
    .await?;
    let current = claim(pool, room, "lease-current", 60).await?;
    require(
        claimed_id(&current).as_deref() == Some(slow_id.as_str())
            && current.attempt_count() == Some(3),
        "an expired lease must be reclaimed as the next attempt",
    )?;
    require(
        giga_event_deadline_settle(pool, &stale).await?.is_none(),
        "a stale claim must not settle a reclaimed lease",
    )?;
    require(
        giga_event_finish(
            pool,
            finish_request(
                room,
                &slow_id,
                "lease-stale",
                GigaEventFinishOutcome::Succeeded,
                None,
            )?,
        )
        .await
        .is_err(),
        "a stale worker must not finish a reclaimed lease",
    )?;

    let mut live = current;
    for expected_attempt in [3_u32, 4] {
        let receipt = giga_event_deadline_settle(pool, &live)
            .await?
            .ok_or_else(|| std::io::Error::other("the live claim must settle"))?;
        require(
            receipt.queue_state() == GigaQueueState::Pending
                && receipt.attempt_count() == expected_attempt,
            "attempts before the last must settle as retry",
        )?;
        sqlx::query(skip_retry_delay)
            .bind(&slow_id)
            .execute(pool)
            .await?;
        live = claim(pool, room, &format!("lease-attempt-{expected_attempt}"), 60).await?;
    }
    require(
        live.attempt_count() == Some(5),
        "the final bounded attempt must be claimed",
    )?;
    let last = giga_event_deadline_settle(pool, &live)
        .await?
        .ok_or_else(|| std::io::Error::other("the final claim must settle"))?;
    require(
        last.queue_state() == GigaQueueState::Failed && last.attempt_count() == 5,
        "a missed deadline on the final attempt must fail the turn",
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL database"]
async fn turn_queue_release_capture_and_lease_contracts() {
    in_isolated_schema("per-turn queue contracts", |pool| async move {
        let tables: (bool, bool) = sqlx::query_as(
            "SELECT to_regclass('giga_turn_identities') IS NOT NULL,
                    to_regclass('giga_window_replacements') IS NOT NULL",
        )
        .fetch_one(&pool)
        .await?;
        require(
            tables == (true, true),
            "the canonical runner must create the turn tables",
        )?;
        sqlx::raw_sql(MIGRATION_0043).execute(&pool).await?;
        count_release_contract(&pool).await?;
        timer_release_contract(&pool).await?;
        capture_identity_contract(&pool).await?;
        malformed_event_does_not_starve_neighbors(&pool).await?;
        lease_and_deadline_contract(&pool).await
    })
    .await;
}

fn legacy_window(room: &str, event_id: &str, turns: &[(&str, &str)]) -> TestResult<GigaEvent> {
    let mut sources = Vec::with_capacity(turns.len());
    for (source_id, content) in turns {
        sources.push(GigaSourceRef::new(
            GigaSourceType::Turn,
            (*source_id).into(),
            "user".into(),
            TURN_AT.into(),
            content_hash(content),
            GigaScope::new(Some(room.into()), None, GigaVisibility::Private, true)?,
            None,
        )?);
    }
    Ok(GigaEvent::new(
        event_id.into(),
        GigaEventType::ConversationWindow,
        RoomKey::new(room)?,
        "legacy-session".into(),
        vec![],
        sources,
        GigaLifecycle::conversation_window(),
        WINDOW_AT.into(),
    )?)
}

/// Writes one pre-0043 window through the production event path and drives it
/// to `outcome` through real claims: `None` leaves it pending, `Some(None)`
/// leaves it running, `Some(Some(outcome))` finishes it.
async fn legacy_fixture(
    pool: &PgPool,
    room: &str,
    event_id: &str,
    turns: &[(&str, &str)],
    outcome: Option<Option<GigaEventFinishOutcome>>,
) -> TestResult {
    let ingested = giga_event_ingest(pool, legacy_window(room, event_id, turns)?).await?;
    require(
        ingested.disposition == GigaEventIngestDisposition::Accepted,
        "legacy window must be stored",
    )?;
    let Some(outcome) = outcome else {
        return Ok(());
    };
    let claimed = claim(pool, room, "legacy-worker", 300).await?;
    require(
        claimed_id(&claimed).as_deref() == Some(event_id),
        "the legacy window must be the claimed event",
    )?;
    if let Some(outcome) = outcome {
        let error_class = (outcome == GigaEventFinishOutcome::Failed).then_some("fixture_failure");
        giga_event_finish(
            pool,
            finish_request(room, event_id, "legacy-worker", outcome, error_class)?,
        )
        .await?;
    }
    Ok(())
}

async fn cutover_snapshot(pool: &PgPool) -> TestResult<Vec<String>> {
    let mut rows = Vec::new();
    for query in [
        "SELECT concat_ws('|',room,session_id,source_id,state,event_id,source_role,content_hash,
                          scope_project,origin,recorded_at)
         FROM giga_turn_identities ORDER BY 1",
        "SELECT concat_ws('|',event_id,queue_state,last_error,waiting_since,attempt_count,
                          available_at,updated_at)
         FROM giga_events ORDER BY 1",
        "SELECT concat_ws('|',event_id,source_ordinal,source_id,source_role,content_hash,
                          scope_room,scope_project,scope_visibility,publication_review_required)
         FROM giga_event_sources ORDER BY 1",
        "SELECT concat_ws('|',window_event_id,previous_state,replaced_at)
         FROM giga_window_replacements ORDER BY 1",
        "SELECT concat_ws('|',event_id,replay_count,attempt_count,outcome,error_class,finished_at)
         FROM giga_event_attempts ORDER BY 1",
    ] {
        rows.extend(
            sqlx::query_scalar::<_, String>(query)
                .fetch_all(pool)
                .await?,
        );
    }
    Ok(rows)
}

async fn identity(pool: &PgPool, room: &str, source: &str) -> TestResult<(String, Option<String>)> {
    Ok(sqlx::query_as(
        "SELECT state,event_id FROM giga_turn_identities
         WHERE room=$1 AND session_id='legacy-session' AND source_id=$2",
    )
    .bind(room)
    .bind(source)
    .fetch_one(pool)
    .await?)
}

async fn event_state(pool: &PgPool, event_id: &str) -> TestResult<(String, Option<String>, bool)> {
    Ok(sqlx::query_as(
        "SELECT queue_state,last_error,waiting_since IS NULL FROM giga_events WHERE event_id=$1",
    )
    .bind(event_id)
    .fetch_one(pool)
    .await?)
}

/// Production-shaped legacy windows written through the pre-cutover path, then
/// cut over by migration 0043 and re-applied to prove idempotency.
async fn legacy_cutover_contract(pool: &PgPool) -> TestResult {
    let room = "turn-legacy";
    let succeeded = Some(Some(GigaEventFinishOutcome::Succeeded));
    let failed = Some(Some(GigaEventFinishOutcome::Failed));
    legacy_fixture(
        pool,
        room,
        "legacy-w1",
        &[("a", "a"), ("b", "b"), ("c", "c")],
        succeeded,
    )
    .await?;
    legacy_fixture(pool, room, "legacy-w5", &[("x", "x-first")], succeeded).await?;
    legacy_fixture(
        pool,
        room,
        "legacy-w2",
        &[("b", "b"), ("c", "c"), ("d", "d")],
        failed,
    )
    .await?;
    legacy_fixture(pool, room, "legacy-w6", &[("x", "x-second")], failed).await?;
    legacy_fixture(
        pool,
        room,
        "legacy-w4",
        &[("f", "f"), ("g", "g")],
        Some(None),
    )
    .await?;
    legacy_fixture(pool, room, "legacy-w3", &[("e", "e"), ("f", "f")], None).await?;

    sqlx::raw_sql(MIGRATION_0043).execute(pool).await?;

    let w1 = Some("legacy-w1".to_owned());
    for source in ["a", "b", "c"] {
        require(
            identity(pool, room, source).await? == ("active".to_owned(), w1.clone()),
            "a turn classified by a succeeded window must stay owned by that window",
        )?;
    }
    let d_job = turn_event_id(room, "legacy-session", "d");
    require(
        identity(pool, room, "d").await? == ("active".to_owned(), Some(d_job.clone()))
            && event_state(pool, &d_job).await?
                == (
                    "failed".to_owned(),
                    Some("GigaLegacyWindowFailed".to_owned()),
                    true,
                ),
        "a turn only failed windows carried must be parked as a failed canonical job",
    )?;
    for source in ["e", "f", "g"] {
        let job = turn_event_id(room, "legacy-session", source);
        require(
            identity(pool, room, source).await? == ("active".to_owned(), Some(job.clone()))
                && event_state(pool, &job).await? == ("pending".to_owned(), None, true),
            "a turn an unfinished window carried must become a released pending job",
        )?;
    }
    require(
        identity(pool, room, "x").await? == ("conflict".to_owned(), None),
        "disagreeing historical evidence must stay a visible conflict",
    )?;
    let job_sources: Vec<(String, i64)> = sqlx::query_as(
        "SELECT event_id,COUNT(*)::bigint FROM giga_event_sources
         WHERE event_id=ANY($1) GROUP BY event_id ORDER BY event_id",
    )
    .bind(
        ["d", "e", "f", "g"]
            .map(|source| turn_event_id(room, "legacy-session", source))
            .to_vec(),
    )
    .fetch_all(pool)
    .await?;
    require(
        job_sources.len() == 4 && job_sources.iter().all(|(_, count)| *count == 1),
        "every canonical job must hold exactly its one target turn",
    )?;

    let archives: Vec<(String, String)> = sqlx::query_as(
        "SELECT window_event_id,previous_state FROM giga_window_replacements ORDER BY 1",
    )
    .fetch_all(pool)
    .await?;
    require(
        archives
            == [
                ("legacy-w2", "failed"),
                ("legacy-w3", "pending"),
                ("legacy-w4", "running"),
                ("legacy-w6", "failed"),
            ]
            .map(|(window, state)| (window.to_owned(), state.to_owned()))
            .to_vec(),
        "every legacy window that never succeeded must be archived with its prior state",
    )?;
    for (window, expected_error) in [
        ("legacy-w2", "fixture_failure"),
        ("legacy-w6", "fixture_failure"),
        ("legacy-w3", "GigaWindowReplacedByTurnJobs"),
        ("legacy-w4", "GigaWindowReplacedByTurnJobs"),
    ] {
        require(
            event_state(pool, window).await?.0 == "failed"
                && event_state(pool, window).await?.1.as_deref() == Some(expected_error),
            "archived windows must end failed, never succeeded, keeping prior failures",
        )?;
    }
    for window in ["legacy-w1", "legacy-w5"] {
        require(
            event_state(pool, window).await?.0 == "succeeded",
            "succeeded windows must stay untouched",
        )?;
    }
    let w4_attempt = sqlx::query(
        "SELECT outcome,error_class,finished_at IS NOT NULL AS finished
         FROM giga_event_attempts WHERE event_id='legacy-w4' AND attempt_count=1",
    )
    .fetch_one(pool)
    .await?;
    require(
        w4_attempt
            .try_get::<Option<String>, _>("outcome")?
            .as_deref()
            == Some("failed")
            && w4_attempt
                .try_get::<Option<String>, _>("error_class")?
                .as_deref()
                == Some("GigaWindowReplacedByTurnJobs")
            && w4_attempt.try_get::<bool, _>("finished")?,
        "the running window's open attempt must close with the replacement class",
    )?;

    let before_reapply = cutover_snapshot(pool).await?;
    sqlx::raw_sql(MIGRATION_0043).execute(pool).await?;
    require(
        cutover_snapshot(pool).await? == before_reapply,
        "re-applying 0043 must change nothing",
    )?;

    let health = giga_health(pool, GigaHealthParams { room: room.into() }.try_into()?).await?;
    require(
        health.queue_depth == 4
            && health.failed_count == 1
            && health.processed_count == 2
            && health.classifier.consecutive_failures == 0,
        "archived windows must not count as backlog, failures, or failure streaks",
    )?;
    require(
        health.identity_conflict_count == 1,
        "migration conflicts must remain visible in health",
    )?;

    let replay = |event_id: &str| {
        GigaEventReplayRequest::new(
            RoomKey::new(room).expect("valid room"),
            event_id.into(),
            "test-operator".into(),
            "deliberate cutover replay".into(),
        )
    };
    require(
        giga_event_replay(pool, replay("legacy-w2")?).await.is_err(),
        "an archived window must refuse replay",
    )?;
    let replayed = giga_event_replay(pool, replay(&d_job)?).await?;
    require(
        replayed.queue_state() == GigaQueueState::Pending
            && event_state(pool, &d_job).await?.0 == "pending"
            && event_state(pool, &d_job).await?.2,
        "a parked canonical job must recover through deliberate replay",
    )?;

    let session = "legacy-session";
    let known = capture(pool, room, None, vec![turn(session, "b", "user", "b")]).await?;
    require(
        known.accepted_turns == 0
            && known.duplicate_turns == 1
            && known.event_ids == vec!["legacy-w1".to_owned()],
        "recapturing a classified legacy turn must point at its succeeded window",
    )?;
    let parked = capture(pool, room, None, vec![turn(session, "d", "user", "d")]).await?;
    require(
        parked.duplicate_turns == 1 && parked.event_ids == vec![d_job.clone()],
        "the SQL-derived canonical id must equal the documented id",
    )?;
    let fresh = capture(pool, room, None, vec![turn(session, "n", "user", "n")]).await?;
    require(
        fresh.accepted_turns == 1 && fresh.event_ids == vec![turn_event_id(room, session, "n")],
        "the Rust-derived canonical id must equal the documented id",
    )?;
    require(
        capture(
            pool,
            room,
            None,
            vec![turn(session, "x", "user", "x-first")],
        )
        .await
        .is_err(),
        "capturing a conflicted legacy identity must be refused",
    )?;

    let purge = giga_queue_maintenance(
        pool,
        GigaQueueMaintenanceRequest::new(
            RoomKey::new(room)?,
            GigaQueueMaintenanceOperation::PurgeStuck,
            GigaQueueMaintenanceScope::Room,
        ),
    )
    .await?;
    let archived_left: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM giga_window_replacements w
         JOIN giga_events e ON e.event_id=w.window_event_id",
    )
    .fetch_one(pool)
    .await?;
    let archived_attempts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM giga_event_attempts
         WHERE event_id IN ('legacy-w2','legacy-w4','legacy-w6')",
    )
    .fetch_one(pool)
    .await?;
    require(
        purge.deleted_events == 4 && archived_left == 4 && archived_attempts == 3,
        "purge must keep waiting jobs and archived windows with their attempts",
    )?;
    let health = giga_health(pool, GigaHealthParams { room: room.into() }.try_into()?).await?;
    require(
        health.queue_depth == 1
            && health.failed_count == 0
            && health.classifier.last_error_class.0.is_none()
            && health.identity_conflict_count == 1,
        "archived windows must not surface as the latest queue error",
    )?;
    require(
        identity(pool, room, "e").await? == ("retired".to_owned(), None),
        "a purged canonical job must leave a retired identity",
    )?;
    let recaptured = capture(pool, room, None, vec![turn(session, "e", "user", "e")]).await?;
    require(
        recaptured.duplicate_turns == 1 && recaptured.event_ids.is_empty(),
        "recapturing a purged legacy turn must not recreate its job",
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL and an isolated PostgreSQL database"]
async fn turn_queue_legacy_cutover_contract() {
    in_isolated_schema("legacy window cutover contract", |pool| async move {
        legacy_cutover_contract(&pool).await
    })
    .await;
}

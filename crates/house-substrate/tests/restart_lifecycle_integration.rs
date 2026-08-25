//! The restart plane against a real PostgreSQL schema. One test, twelve phases:
//! the full lifecycle, the storm guard's counting query, the exit fence, expiry
//! of an unclaimed request, the stale-token refusal, one-verify-only, the storm
//! cap at the moment of arming, a lapsed idempotent replay, an intent that
//! crosses its TTL behind a row lock, the Insula divergence read, and the
//! capability and session authority of each door. It is one test on purpose —
//! it owns the whole `restart` schema and resets it, so it must not race a
//! sibling that does the same.
//!
//! Run it against a scratch database only:
//!   SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL=postgres://.../restart_intent_scratch \
//!     cargo test -p athanor-substrate --test restart_lifecycle_integration -- --ignored

use athanor_substrate::{
    AppError, EXITING_DEADLINE_SECS, RELAUNCHING_DEADLINE_SECS, REQUESTED_TTL_SECS,
    STORM_MAX_EXITING_PER_WINDOW, query_unverified_exit, restart_claim, restart_request,
    restart_status, restart_transition, restart_verify,
};
use house_protocol::restart::{
    RestartClaimParams, RestartConsentSource, RestartHarness, RestartMode, RestartRequestParams,
    RestartState, RestartStatusParams, RestartTransitionParams, RestartTransitionTarget,
    RestartVerifyParams,
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const RESTART_MIGRATION: &str = include_str!("../../../substrate/migrations/0026_restart.sql");
const KEEPER: &str = "omp-keeper";
const KEEPER_SECRET: &str = "keeper-capability-secret";
const ROOM: &str = "kodo";
const OTHER_ROOM: &str = "tuner";
// Harness session ids, not principal strings: the adapter presents whatever
// hostSessionIdentity yields, and the exit fence compares that value byte for
// byte (Kodo's ruling, 2026-08-25).
const ROOM_SESSION: &str = "3f1c9a6e-70b4-4a2f-9d13-5c8e2b7a4d61";
const SUCCESSOR_SESSION: &str = "b2d47f08-9c31-4e6a-8f57-1a90c3d5e742";
const OTHER_SESSION: &str = "e57a1c93-4d20-4b6f-9a81-7c3f5e2d0b84";
const REQUEST_SECRET: &str = "room-request-secret";
const EXIT_SECRET: &str = "room-exit-secret";
const VERIFY_SECRET: &str = "room-verify-secret";
const OTHER_TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn isolated_database_url() -> String {
    let url = std::env::var("SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL")
        .expect("the restart proof requires a dedicated PostgreSQL URL");
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
        "refusing a live or production-looking database"
    );
    url
}

async fn fresh_restart() -> TestResult<PgPool> {
    // More than one connection: the lock-contention phase holds a row lock on
    // one connection while a claim waits behind it on another.
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&isolated_database_url())
        .await?;
    // This proof owns only the dedicated test database and starts from the
    // same pre-restart state every run.
    sqlx::query("DROP SCHEMA IF EXISTS restart CASCADE")
        .execute(&pool)
        .await?;
    sqlx::raw_sql(RESTART_MIGRATION).execute(&pool).await?;
    // Applied twice on purpose: the migration must be re-applicable over its
    // own completed schema, and its column contract must accept it.
    sqlx::raw_sql(RESTART_MIGRATION).execute(&pool).await?;
    let residuals: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='restart' AND c.relkind='r' AND c.relname NOT IN ('intents','intent_events','principal_capabilities')",
    )
    .fetch_all(&pool)
    .await?;
    assert!(
        residuals.is_empty(),
        "the migration left residual relations: {residuals:?}"
    );
    for (principal, operation_class, secret) in [
        (KEEPER, "restart_claim", KEEPER_SECRET),
        (ROOM, "restart_request", REQUEST_SECRET),
        (ROOM, "restart_exit", EXIT_SECRET),
        (ROOM, "restart_verify", VERIFY_SECRET),
        (OTHER_ROOM, "restart_request", REQUEST_SECRET),
    ] {
        sqlx::query(
            "INSERT INTO restart.principal_capabilities (principal, operation_class, capability_hash) VALUES ($1, $2, $3)",
        )
        .bind(principal)
        .bind(operation_class)
        .bind(format!("{:x}", Sha256::digest(secret.as_bytes())))
        .execute(&pool)
        .await?;
    }
    Ok(pool)
}

fn request_params(workspace: &str, key: &str) -> RestartRequestParams {
    room_request_params(ROOM, workspace, key)
}

fn room_request_params(room: &str, workspace: &str, key: &str) -> RestartRequestParams {
    RestartRequestParams {
        harness: RestartHarness::Omp,
        workspace: workspace.into(),
        mode: RestartMode::Resume,
        session_id: Some(ROOM_SESSION.into()),
        reason: "the loader installed a newer release than this session loaded".into(),
        consent_source: RestartConsentSource::OperatorStandingPolicy,
        requester_room: room.into(),
        requester_spirit: "Kodo".into(),
        requester_session: ROOM_SESSION.into(),
        capability: REQUEST_SECRET.into(),
        idempotency_key: key.into(),
    }
}

fn claim_params(intent_id: &str, key: &str) -> RestartClaimParams {
    RestartClaimParams {
        intent_id: intent_id.into(),
        claimant: KEEPER.into(),
        capability: KEEPER_SECRET.into(),
        idempotency_key: key.into(),
    }
}

fn transition_params(
    intent_id: &str,
    to: RestartTransitionTarget,
    claim_token: Option<&str>,
) -> RestartTransitionParams {
    // The exit arm carries the room's secret and the session that asked; the
    // keeper's arms carry the minted lease and neither of those.
    let arming = to == RestartTransitionTarget::Exiting;
    RestartTransitionParams {
        intent_id: intent_id.into(),
        claim_token: claim_token.map(str::to_owned),
        requester_session: arming.then(|| ROOM_SESSION.to_owned()),
        capability: arming.then(|| EXIT_SECRET.to_owned()),
        to,
        detail: Some("the loader installed a newer release".into()),
    }
}

fn verify_params(intent_id: &str, session: &str) -> RestartVerifyParams {
    RestartVerifyParams {
        intent_id: intent_id.into(),
        successor_session: session.into(),
        room: ROOM.into(),
        spirit: "Kodo".into(),
        capability: VERIFY_SECRET.into(),
    }
}

/// Write an `exiting` event with an age. The ledger refuses UPDATE, so a past
/// arrival can only be written, never aged afterwards.
async fn aged_exit_event(pool: &PgPool, intent_id: &str, age: &str) -> TestResult {
    sqlx::query(
        "INSERT INTO restart.intent_events (intent_id,event_kind,principal,created_at) VALUES ($1::text::uuid,$2,$3,NOW()-$4::interval)",
    )
    .bind(intent_id)
    .bind(RestartState::Exiting.as_str())
    .bind("kodo:Kodo")
    .bind(age)
    .execute(pool)
    .await?;
    Ok(())
}

fn refusal_code(error: &AppError) -> &str {
    match error {
        AppError::Refusal { code, .. } => code,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

async fn stored_state(pool: &PgPool, intent_id: &str) -> TestResult<(String, Option<String>)> {
    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT state, failed_stage FROM restart.intents WHERE intent_id=$1::text::uuid",
    )
    .bind(intent_id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

async fn ledger(pool: &PgPool, intent_id: &str) -> TestResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT event_kind FROM restart.intent_events WHERE intent_id=$1::text::uuid ORDER BY created_at",
    )
    .bind(intent_id)
    .fetch_all(pool)
    .await?)
}

async fn arm_and_exit(pool: &PgPool, workspace: &str, key: &str) -> TestResult<String> {
    let intent = restart_request(pool, request_params(workspace, key)).await?;
    restart_transition(
        pool,
        transition_params(&intent.intent_id, RestartTransitionTarget::Exiting, None),
    )
    .await?;
    Ok(intent.intent_id)
}

// Kills: a lifecycle that admits a state out of the amended order, a storm
// guard whose counting query misses the workspace or the window, an exit door
// that fires from anything but requested, a lapsed request that still claims,
// a superseded token that still moves the intent, and a second verify. Then
// Kintsu's four: a prefilled request that arms past the hourly cap, a lapsed
// idempotency key that replays alive, an intent that crosses its TTL behind a
// row lock and still claims, an Insula read that hands one room another room's
// workspace, and a door that takes the public intent id for authority.
// red-proof: allow exiting from claimed in restart_transition; drop the
// workspace predicate from the storm count; remove the expiry branch from
// restart_claim; return Ok from the stale_lease branch; relax
// restart_verify's state fence; delete the storm recount from the exiting arm;
// return the replay receipt without the expiry check; read expiry with NOW()
// instead of clock_timestamp() after the lock; drop the requester_room
// predicate from query_unverified_exit; skip require_capability or
// require_requester_session on any door.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated restart schema"]
async fn restart_intent_lifecycle_holds_its_fences() -> TestResult {
    let pool = fresh_restart().await?;

    // 1. requested -> exiting -> claimed -> relaunching -> verified.
    let workspace = "D:/athanor-wt/restart-intent";
    let requested = restart_request(&pool, request_params(workspace, "request-1")).await?;
    assert_eq!(requested.state, RestartState::Requested);

    let replay = restart_request(&pool, request_params(workspace, "request-1")).await?;
    assert_eq!(
        replay.intent_id, requested.intent_id,
        "a replayed idempotency key returns the existing intent instead of a twin"
    );

    let pending = restart_status(
        &pool,
        RestartStatusParams {
            workspace: workspace.into(),
        },
    )
    .await?
    .intent
    .expect("a requested intent is pending");
    assert_eq!(pending.intent_id, requested.intent_id);
    assert_eq!(pending.state, RestartState::Requested);
    assert_eq!(pending.mode, RestartMode::Resume);
    assert_eq!(pending.session_id.as_deref(), Some(ROOM_SESSION));

    let exiting = restart_transition(
        &pool,
        transition_params(&requested.intent_id, RestartTransitionTarget::Exiting, None),
    )
    .await?;
    assert_eq!(exiting.state, RestartState::Exiting);

    let claim = restart_claim(&pool, claim_params(&requested.intent_id, "claim-1")).await?;
    assert_eq!(claim.claim_epoch, 1);
    assert_eq!(claim.stage_deadlines.requested_ttl_secs, REQUESTED_TTL_SECS);
    assert_eq!(claim.stage_deadlines.exiting_secs, EXITING_DEADLINE_SECS);
    assert_eq!(
        claim.stage_deadlines.relaunching_secs,
        RELAUNCHING_DEADLINE_SECS
    );
    assert_eq!(
        claim.claim_token.len(),
        64,
        "the minted token is 32 bytes of lowercase hex"
    );

    let relaunching = restart_transition(
        &pool,
        transition_params(
            &requested.intent_id,
            RestartTransitionTarget::Relaunching,
            Some(&claim.claim_token),
        ),
    )
    .await?;
    assert_eq!(relaunching.state, RestartState::Relaunching);

    let verified =
        restart_verify(&pool, verify_params(&requested.intent_id, SUCCESSOR_SESSION)).await?;
    assert_eq!(verified.state, RestartState::Verified);
    assert_eq!(
        stored_state(&pool, &requested.intent_id).await?,
        ("verified".to_owned(), None)
    );
    assert_eq!(
        ledger(&pool, &requested.intent_id).await?,
        vec![
            "requested".to_owned(),
            "exiting".to_owned(),
            "claimed".to_owned(),
            "relaunching".to_owned(),
            "verified".to_owned()
        ],
        "every transition writes exactly one event, in order"
    );
    assert!(
        restart_status(
            &pool,
            RestartStatusParams {
                workspace: workspace.into()
            }
        )
        .await?
        .intent
        .is_none(),
        "a verified intent is finished, not pending"
    );

    // 2. One verify per intent.
    let second = restart_verify(&pool, verify_params(&requested.intent_id, OTHER_SESSION))
        .await
        .expect_err("a verified intent cannot verify again");
    assert_eq!(refusal_code(&second), "not_verifiable");
    let successor: String = sqlx::query_scalar(
        "SELECT successor_session FROM restart.intents WHERE intent_id=$1::text::uuid",
    )
    .bind(&requested.intent_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        successor, SUCCESSOR_SESSION,
        "the refused second verify left the first successor untouched"
    );

    // 3. The exit door is legal only from requested.
    let claimed_workspace = "D:/athanor-wt/exit-fence";
    let armed = arm_and_exit(&pool, claimed_workspace, "request-fence").await?;
    restart_claim(&pool, claim_params(&armed, "claim-fence")).await?;
    let late_exit = restart_transition(
        &pool,
        transition_params(&armed, RestartTransitionTarget::Exiting, None),
    )
    .await
    .expect_err("a claimed intent cannot be armed for exit");
    assert_eq!(refusal_code(&late_exit), "exit_not_requested");

    // 4. A stale or absent token moves nothing.
    let stale = restart_transition(
        &pool,
        transition_params(
            &armed,
            RestartTransitionTarget::Relaunching,
            Some(OTHER_TOKEN),
        ),
    )
    .await
    .expect_err("a superseded token is not a lease");
    assert_eq!(refusal_code(&stale), "stale_lease");
    let tokenless = restart_transition(
        &pool,
        transition_params(&armed, RestartTransitionTarget::Relaunching, None),
    )
    .await
    .expect_err("the keeper's door demands the minted token");
    assert!(matches!(tokenless, AppError::Invalid(_)));
    assert_eq!(
        stored_state(&pool, &armed).await?.0,
        "claimed",
        "both refusals left the intent where it was"
    );

    // 5. An unclaimed request past its TTL never fires.
    let lapsed_workspace = "D:/athanor-wt/expiry";
    let lapsed = restart_request(&pool, request_params(lapsed_workspace, "request-lapsed")).await?;
    // The clock is the only thing this proof fakes: the TTL is 300 seconds and
    // the test does not sleep through it.
    sqlx::query(
        "UPDATE restart.intents SET expires_at=NOW()-INTERVAL '1 second' WHERE intent_id=$1::text::uuid",
    )
        .bind(&lapsed.intent_id)
        .execute(&pool)
        .await?;
    assert!(
        restart_status(
            &pool,
            RestartStatusParams {
                workspace: lapsed_workspace.into()
            }
        )
        .await?
        .intent
        .is_none(),
        "a lapsed request is not pending, and the read never writes"
    );
    let expired = restart_claim(&pool, claim_params(&lapsed.intent_id, "claim-lapsed"))
        .await
        .expect_err("a lapsed request cannot be claimed");
    assert_eq!(refusal_code(&expired), "intent_expired");
    assert_eq!(
        stored_state(&pool, &lapsed.intent_id).await?.0,
        "expired",
        "the claim door marked the lapsed request on its way out"
    );
    assert_eq!(
        ledger(&pool, &lapsed.intent_id).await?,
        vec!["requested".to_owned(), "expired".to_owned()]
    );
    let again = restart_claim(&pool, claim_params(&lapsed.intent_id, "claim-lapsed-2"))
        .await
        .expect_err("an expired intent stays unclaimable");
    assert_eq!(refusal_code(&again), "not_claimable");

    // 6. The storm guard counts exits per workspace inside the window.
    let storm_workspace = "D:/athanor-wt/storm";
    for index in 0..STORM_MAX_EXITING_PER_WINDOW {
        arm_and_exit(&pool, storm_workspace, &format!("storm-{index}")).await?;
    }
    let storm = restart_request(&pool, request_params(storm_workspace, "storm-next"))
        .await
        .expect_err("a full window refuses the next request");
    assert_eq!(refusal_code(&storm), "restart_storm");
    // The bound is per workspace: another workspace is untouched by this storm.
    restart_request(&pool, request_params("D:/athanor-wt/calm", "calm-1")).await?;
    // And it is per window: a full window of exits older than the hour counts
    // for nothing. The ledger refuses UPDATE, so the age is written at insert.
    let aged_workspace = "D:/athanor-wt/aged";
    for index in 0..STORM_MAX_EXITING_PER_WINDOW {
        let aged = restart_request(
            &pool,
            request_params(aged_workspace, &format!("aged-{index}")),
        )
        .await?;
        sqlx::query(
            "INSERT INTO restart.intent_events (intent_id,event_kind,principal,created_at) VALUES ($1::text::uuid,$2,$3,NOW()-INTERVAL '2 hours')",
        )
        .bind(&aged.intent_id)
        .bind(RestartState::Exiting.as_str())
        .bind("kodo:Kodo")
        .execute(&pool)
        .await?;
    }
    restart_request(&pool, request_params(aged_workspace, "aged-next")).await?;

    // 7. The cap binds arrivals at exiting, not requests: four requests minted
    // while the window was empty must not all arm.
    let prefilled_workspace = "D:/athanor-wt/prefilled";
    let mut prefilled = Vec::new();
    for index in 0..=STORM_MAX_EXITING_PER_WINDOW {
        prefilled.push(
            restart_request(
                &pool,
                request_params(prefilled_workspace, &format!("prefilled-{index}")),
            )
            .await?
            .intent_id,
        );
    }
    for intent_id in prefilled
        .iter()
        .take(STORM_MAX_EXITING_PER_WINDOW as usize)
    {
        restart_transition(
            &pool,
            transition_params(intent_id, RestartTransitionTarget::Exiting, None),
        )
        .await?;
    }
    let over_cap = prefilled.last().expect("the window plus one");
    let stormed = restart_transition(
        &pool,
        transition_params(over_cap, RestartTransitionTarget::Exiting, None),
    )
    .await
    .expect_err("a request minted before the storm must still refuse to arm");
    assert_eq!(refusal_code(&stormed), "restart_storm");
    assert_eq!(
        stored_state(&pool, over_cap).await?.0,
        "requested",
        "the refused arm left the intent where it was"
    );

    // 8. A replay of a lapsed key never hands back a live-looking receipt.
    let replay_workspace = "D:/athanor-wt/replay";
    let stale = restart_request(&pool, request_params(replay_workspace, "replay-1")).await?;
    sqlx::query(
        "UPDATE restart.intents SET expires_at=NOW()-INTERVAL '1 second' WHERE intent_id=$1::text::uuid",
    )
    .bind(&stale.intent_id)
    .execute(&pool)
    .await?;
    let replayed = restart_request(&pool, request_params(replay_workspace, "replay-1"))
        .await
        .expect_err("a lapsed idempotency key must not replay alive");
    assert_eq!(refusal_code(&replayed), "intent_expired");
    assert_eq!(
        ledger(&pool, &stale.intent_id).await?,
        vec!["requested".to_owned(), "expired".to_owned()],
        "the replay killed the row in the ledger, exactly once"
    );
    let replayed_again = restart_request(&pool, request_params(replay_workspace, "replay-1"))
        .await
        .expect_err("an expired key stays expired");
    assert_eq!(refusal_code(&replayed_again), "intent_expired");

    // 9. The lock-contention race: alive when the claim door opened its
    // transaction, dead by the time the row lock arrived.
    let racing = restart_request(&pool, request_params("D:/athanor-wt/race", "race-1")).await?;
    sqlx::query(
        "UPDATE restart.intents SET expires_at=clock_timestamp()+INTERVAL '2 seconds' WHERE intent_id=$1::text::uuid",
    )
    .bind(&racing.intent_id)
    .execute(&pool)
    .await?;
    let mut blocker = pool.begin().await?;
    sqlx::query("SELECT 1 FROM restart.intents WHERE intent_id=$1::text::uuid FOR UPDATE")
        .bind(&racing.intent_id)
        .fetch_one(&mut *blocker)
        .await?;
    let waiting_claim = tokio::spawn({
        let pool = pool.clone();
        let params = claim_params(&racing.intent_id, "race-claim");
        async move { restart_claim(&pool, params).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(4000)).await;
    blocker.rollback().await?;
    let raced = waiting_claim
        .await?
        .expect_err("an intent that lapsed behind the lock must not claim");
    assert_eq!(refusal_code(&raced), "intent_expired");
    assert_eq!(stored_state(&pool, &racing.intent_id).await?.0, "expired");

    // 10. Insula reports this room's divergence and nothing else. The clock
    // starts at the first exiting event, so an aged arrival is written aged.
    let diverged = restart_request(
        &pool,
        request_params("D:/athanor-wt/diverged", "diverged-1"),
    )
    .await?;
    sqlx::query("UPDATE restart.intents SET state=$2 WHERE intent_id=$1::text::uuid")
        .bind(&diverged.intent_id)
        .bind(RestartState::Exiting.as_str())
        .execute(&pool)
        .await?;
    aged_exit_event(&pool, &diverged.intent_id, "10 minutes").await?;
    aged_exit_event(&pool, &requested.intent_id, "10 minutes").await?;
    let foreign = restart_request(
        &pool,
        room_request_params(OTHER_ROOM, "D:/athanor-wt/tuner", "foreign-1"),
    )
    .await?;
    aged_exit_event(&pool, &foreign.intent_id, "10 minutes").await?;

    let reported = query_unverified_exit(&pool, ROOM, 20).await?;
    assert_eq!(reported.room, ROOM);
    let reported_ids: Vec<&str> = reported
        .rows
        .iter()
        .map(|row| row.intent_id.as_str())
        .collect();
    assert!(
        reported_ids.contains(&diverged.intent_id.as_str()),
        "an exit with no verified successor past the window is a divergence"
    );
    assert!(
        !reported_ids.contains(&requested.intent_id.as_str()),
        "a verified intent stays silent even with an aged exit"
    );
    assert!(
        !reported_ids.contains(&foreign.intent_id.as_str()),
        "another room's workspace and requester never ride this room's read"
    );
    let foreign_read = query_unverified_exit(&pool, OTHER_ROOM, 20).await?;
    assert!(
        foreign_read
            .rows
            .iter()
            .any(|row| row.intent_id == foreign.intent_id),
        "the other room still reads its own divergence"
    );

    // 11. The intent id is public, so every door demands its own authority.
    let fenced = restart_request(&pool, request_params("D:/athanor-wt/fence", "fence-1")).await?;
    let borrowed_id = restart_transition(
        &pool,
        RestartTransitionParams {
            capability: Some("not-the-room-secret".into()),
            ..transition_params(&fenced.intent_id, RestartTransitionTarget::Exiting, None)
        },
    )
    .await
    .expect_err("reading the intent id is not permission to arm an exit");
    assert_eq!(refusal_code(&borrowed_id), "restart_capability");
    let wrong_session = restart_transition(
        &pool,
        RestartTransitionParams {
            requester_session: Some(OTHER_SESSION.into()),
            ..transition_params(&fenced.intent_id, RestartTransitionTarget::Exiting, None)
        },
    )
    .await
    .expect_err("another session must not arm this session's exit");
    assert_eq!(refusal_code(&wrong_session), "exit_not_authorized");
    assert_eq!(
        stored_state(&pool, &fenced.intent_id).await?.0,
        "requested",
        "two refused arms moved nothing"
    );
    let unfenced_request = restart_request(
        &pool,
        RestartRequestParams {
            capability: "not-the-room-secret".into(),
            ..request_params("D:/athanor-wt/fence", "fence-2")
        },
    )
    .await
    .expect_err("a declared consent source is not authority");
    assert_eq!(refusal_code(&unfenced_request), "restart_capability");

    // 12. The successor proves itself too: the room's verify secret, a room
    // that owns the intent, and a session that is not the one that left.
    let returning = arm_and_exit(&pool, "D:/athanor-wt/successor", "successor-1").await?;
    let successor_claim = restart_claim(&pool, claim_params(&returning, "successor-claim")).await?;
    restart_transition(
        &pool,
        transition_params(
            &returning,
            RestartTransitionTarget::Relaunching,
            Some(&successor_claim.claim_token),
        ),
    )
    .await?;
    let unfenced_verify = restart_verify(
        &pool,
        RestartVerifyParams {
            capability: "not-the-room-secret".into(),
            ..verify_params(&returning, SUCCESSOR_SESSION)
        },
    )
    .await
    .expect_err("naming the intent id is not proof that the successor came back");
    assert_eq!(refusal_code(&unfenced_verify), "restart_capability");
    let foreign_verify = restart_verify(
        &pool,
        RestartVerifyParams {
            room: OTHER_ROOM.into(),
            ..verify_params(&returning, OTHER_SESSION)
        },
    )
    .await
    .expect_err("a foreign room cannot sign this room's return");
    assert_eq!(refusal_code(&foreign_verify), "verify_not_authorized");
    let self_verify = restart_verify(&pool, verify_params(&returning, ROOM_SESSION))
        .await
        .expect_err("the session that left cannot sign its own return");
    assert_eq!(refusal_code(&self_verify), "verify_not_authorized");
    assert_eq!(
        restart_verify(&pool, verify_params(&returning, SUCCESSOR_SESSION))
            .await?
            .state,
        RestartState::Verified,
        "the real successor still verifies once"
    );
    Ok(())
}

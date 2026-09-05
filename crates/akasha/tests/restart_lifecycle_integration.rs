//! The restart plane against a real PostgreSQL schema. One test, fifteen
//! phases: the full lifecycle, the storm guard's counting query, the exit
//! fence, expiry of an unclaimed request, the stale-token refusal,
//! one-verify-only, the storm cap at the moment of arming, a lapsed idempotent
//! replay, an intent that crosses its TTL behind a row lock, the Insula
//! divergence read, the capability and session authority of each door, one live
//! intent per workspace, the exact-intent read, and the armed exit no keeper
//! ever claimed. It is one test on purpose — it owns the whole `restart` schema
//! and resets it, so it must not race a sibling that does the same.
//!
//! Run it against a scratch database only:
//!   ATHANOR_SUBSTRATE_TEST_DATABASE_URL=postgres://.../restart_intent_scratch \
//!     cargo test -p akasha --test restart_lifecycle_integration -- --ignored

use akasha::{
    AppError, EXIT_UNCLAIMED_REASON, EXITING_DEADLINE_SECS, RELAUNCHING_DEADLINE_SECS,
    REQUESTED_TTL_SECS, STORM_MAX_EXITING_PER_WINDOW, query_unverified_exit, restart_claim,
    restart_request, restart_status, restart_transition, restart_verify,
};
use protocol::restart::{
    RestartClaimParams, RestartConsentSource, RestartHarness, RestartMode, RestartRequestParams,
    RestartState, RestartStatusParams, RestartTransitionParams, RestartTransitionTarget,
    RestartVerifyParams,
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const RESTART_MIGRATION: &str = include_str!("../../../substrate/migrations/0026_restart.sql");
const RESTART_PROOF_MIGRATION: &str =
    include_str!("../../../substrate/migrations/0027_restart_successor_proof.sql");
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
    let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
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
    sqlx::raw_sql(RESTART_PROOF_MIGRATION)
        .execute(&pool)
        .await?;
    // Applied twice on purpose: both restart migrations must re-apply cleanly.
    sqlx::raw_sql(RESTART_MIGRATION).execute(&pool).await?;
    sqlx::raw_sql(RESTART_PROOF_MIGRATION)
        .execute(&pool)
        .await?;
    let residuals: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='restart' AND c.relkind='r' AND c.relname NOT IN ('intents','intent_events','principal_capabilities','successor_proofs')",
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
fn fresh_request_params(workspace: &str, key: &str) -> RestartRequestParams {
    RestartRequestParams {
        mode: RestartMode::Fresh,
        session_id: None,
        ..request_params(workspace, key)
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

fn verify_params(intent_id: &str, session: &str, proof: &str) -> RestartVerifyParams {
    RestartVerifyParams {
        intent_id: intent_id.into(),
        successor_session: session.into(),
        successor_proof: proof.into(),
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

/// Push an armed exit past the deadline the House published for it. The
/// intents row is not append-only, so moving its clock is the only way a test
/// can stand where a keeperless room stands a minute after the exit.
async fn age_exiting_deadline(pool: &PgPool, intent_id: &str) -> TestResult {
    sqlx::query(
        "UPDATE restart.intents SET exiting_deadline_at=clock_timestamp()-INTERVAL '1 second' WHERE intent_id=$1::text::uuid",
    )
    .bind(intent_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// End an intent's hold on its workspace. A workspace carries one live intent,
/// so any phase that wants a second restart has to finish the first one; this
/// is the short way to say "that restart is over" without a whole lifecycle.
async fn retire(pool: &PgPool, intent_id: &str) -> TestResult {
    sqlx::query("UPDATE restart.intents SET state=$2 WHERE intent_id=$1::text::uuid")
        .bind(intent_id)
        .bind(RestartState::Expired.as_str())
        .execute(pool)
        .await?;
    Ok(())
}

/// The claim is queued behind the row lock, not merely spawned. A blocked
/// `FOR UPDATE` waits on the holding transaction, so the fact shows up as a
/// backend of this database waiting on a Lock, not as an ungranted table lock.
async fn wait_for_queued_claim(pool: &PgPool) -> TestResult {
    for _ in 0..200 {
        let queued: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock'",
        )
        .fetch_one(pool)
        .await?;
        if queued > 0 {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("no claim ever queued behind the row lock");
}

/// The TTL has really passed, whatever the machine was doing. A plain read of
/// the row does not block behind the FOR UPDATE that holds it.
async fn wait_until_lapsed(pool: &PgPool, intent_id: &str) -> TestResult {
    for _ in 0..200 {
        let lapsed: bool = sqlx::query_scalar(
            "SELECT expires_at <= clock_timestamp() FROM restart.intents WHERE intent_id=$1::text::uuid",
        )
        .bind(intent_id)
        .fetch_one(pool)
        .await?;
        if lapsed {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("the intent never passed its TTL");
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
// Cut two: a workspace that carries two live intents at once, so a fresh
// request can stand in for the successor the keeper is still waiting on, and a
// status read that can only ever speak about pending intents, so no watch can
// see its own verified.
// red-proof: drop refuse_on_live_intent from restart_request, or the partial
// unique index from 0026; answer the exact-id read from the pending query.
// Cut three: an armed exit no keeper ever claimed, which holds its workspace
// and refuses every later restart with intent_pending until somebody edits the
// database, and a sweep greedy enough to take an exit whose keeper is alive.
// red-proof: drop expire_stranded_exits_in_workspace from restart_request, or
// let its read reach a claimed row — both land on intent_pending, the first
// for the stranded exit and the second for the claimed one.
#[tokio::test]
#[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated restart schema"]
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
            intent_id: None,
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
    assert_eq!(claim.claim_token.len(), 64);

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
    let successor_proof = relaunching
        .successor_proof
        .as_deref()
        .expect("each relaunching transition returns its proof exactly once");

    let verified = restart_verify(
        &pool,
        verify_params(&requested.intent_id, ROOM_SESSION, successor_proof),
    )
    .await?;
    assert_eq!(verified.state, RestartState::Verified);
    assert_eq!(
        stored_state(&pool, &requested.intent_id).await?,
        ("verified".to_owned(), None)
    );
    let proof_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM restart.successor_proofs WHERE intent_id=$1::text::uuid",
    )
    .bind(&requested.intent_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(proof_rows, 0, "verification consumes the current proof row");
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
                workspace: workspace.into(),
                intent_id: None,
            }
        )
        .await?
        .intent
        .is_none(),
        "a verified intent is finished, not pending"
    );

    // 2. One verify per intent and no replay after proof cleanup.
    let second = restart_verify(
        &pool,
        verify_params(&requested.intent_id, ROOM_SESSION, successor_proof),
    )
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
        successor, ROOM_SESSION,
        "the resume successor keeps the exact logical session identity"
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
                workspace: lapsed_workspace.into(),
                intent_id: None,
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

    // 6. The storm guard counts exits per workspace inside the window. One
    // workspace carries one live intent, so each restart in the window has to
    // finish before the next one can be asked for.
    let storm_workspace = "D:/athanor-wt/storm";
    for index in 0..STORM_MAX_EXITING_PER_WINDOW {
        let armed = arm_and_exit(&pool, storm_workspace, &format!("storm-{index}")).await?;
        retire(&pool, &armed).await?;
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
        aged_exit_event(&pool, &aged.intent_id, "2 hours").await?;
        retire(&pool, &aged.intent_id).await?;
    }
    restart_request(&pool, request_params(aged_workspace, "aged-next")).await?;

    // 7. The cap binds arrivals at exiting, not requests: an intent asked for
    // while the window was empty must still refuse to arm once it fills. The
    // one-live fence means the window fills from finished restarts, so the
    // events are written for retired intents.
    let prefilled_workspace = "D:/athanor-wt/prefilled";
    let mut retired = Vec::new();
    for index in 0..STORM_MAX_EXITING_PER_WINDOW {
        let done = restart_request(
            &pool,
            request_params(prefilled_workspace, &format!("prefilled-done-{index}")),
        )
        .await?;
        retire(&pool, &done.intent_id).await?;
        retired.push(done.intent_id);
    }
    let over_cap = restart_request(&pool, request_params(prefilled_workspace, "prefilled-live"))
        .await?
        .intent_id;
    for intent_id in &retired {
        aged_exit_event(&pool, intent_id, "1 minute").await?;
    }
    let stormed = restart_transition(
        &pool,
        transition_params(&over_cap, RestartTransitionTarget::Exiting, None),
    )
    .await
    .expect_err("a request minted before the storm must still refuse to arm");
    assert_eq!(refusal_code(&stormed), "restart_storm");
    assert_eq!(
        stored_state(&pool, &over_cap).await?.0,
        "requested",
        "the refused arm left the intent where it was"
    );
    retire(&pool, &over_cap).await?;

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

    // 8b. Status hides an expired request. A genuinely new key must retire
    // that hidden row before the one-live fence admits its replacement.
    let rollover_workspace = "D:/athanor-wt/rollover";
    let stranded =
        restart_request(&pool, request_params(rollover_workspace, "rollover-old")).await?;
    sqlx::query(
        "UPDATE restart.intents SET expires_at=NOW()-INTERVAL '1 second' WHERE intent_id=$1::text::uuid",
    )
    .bind(&stranded.intent_id)
    .execute(&pool)
    .await?;
    assert!(
        restart_status(
            &pool,
            RestartStatusParams {
                workspace: rollover_workspace.into(),
                intent_id: None,
            },
        )
        .await?
        .intent
        .is_none(),
        "the lapsed row is already absent from the adapter's pending read"
    );
    let replacement =
        restart_request(&pool, request_params(rollover_workspace, "rollover-new")).await?;
    assert_ne!(replacement.intent_id, stranded.intent_id);
    assert_eq!(replacement.state, RestartState::Requested);
    assert_eq!(
        stored_state(&pool, &stranded.intent_id).await?.0,
        "expired",
        "the new key retired the hidden row before taking the workspace"
    );
    assert_eq!(
        ledger(&pool, &stranded.intent_id).await?,
        vec!["requested".to_owned(), "expired".to_owned()]
    );

    // 9. The lock-contention race: alive when the claim door opened its
    // transaction, dead by the time the row lock arrived. Both facts are
    // observed before the lock is released, so the phase never rests on how
    // long a sleep happened to take.
    let racing = restart_request(&pool, request_params("D:/athanor-wt/race", "race-1")).await?;
    sqlx::query(
        "UPDATE restart.intents SET expires_at=clock_timestamp()+INTERVAL '1 second' WHERE intent_id=$1::text::uuid",
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
    wait_for_queued_claim(&pool).await?;
    wait_until_lapsed(&pool, &racing.intent_id).await?;
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

    // 12. Proofs rotate per relaunch attempt and only the current proof can
    // verify. Resume accepts the recorded session; room authority stays strict.
    let returning = arm_and_exit(&pool, "D:/athanor-wt/successor", "successor-1").await?;
    let successor_claim = restart_claim(&pool, claim_params(&returning, "successor-claim")).await?;
    let first = restart_transition(
        &pool,
        transition_params(
            &returning,
            RestartTransitionTarget::Relaunching,
            Some(&successor_claim.claim_token),
        ),
    )
    .await?;
    let first_proof = first
        .successor_proof
        .as_deref()
        .expect("the first relaunch attempt returns a proof");
    let second = restart_transition(
        &pool,
        transition_params(
            &returning,
            RestartTransitionTarget::Relaunching,
            Some(&successor_claim.claim_token),
        ),
    )
    .await?;
    let second_proof = second
        .successor_proof
        .as_deref()
        .expect("the retry returns a rotated proof");
    assert_ne!(
        first_proof, second_proof,
        "retry rotates the successor proof"
    );
    let stored_hash: String = sqlx::query_scalar(
        "SELECT proof_hash FROM restart.successor_proofs WHERE intent_id=$1::text::uuid",
    )
    .bind(&returning)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        stored_hash,
        format!("{:x}", Sha256::digest(second_proof.as_bytes()))
    );
    assert_ne!(
        stored_hash, second_proof,
        "the database stores only the proof hash"
    );

    let stale_verify = restart_verify(&pool, verify_params(&returning, ROOM_SESSION, first_proof))
        .await
        .expect_err("the prior attempt proof cannot replay");
    assert_eq!(refusal_code(&stale_verify), "verify_not_authorized");
    let unfenced_verify = restart_verify(
        &pool,
        RestartVerifyParams {
            capability: "not-the-room-secret".into(),
            ..verify_params(&returning, ROOM_SESSION, second_proof)
        },
    )
    .await
    .expect_err("the proof does not replace the room capability");
    assert_eq!(refusal_code(&unfenced_verify), "restart_capability");
    let foreign_verify = restart_verify(
        &pool,
        RestartVerifyParams {
            room: OTHER_ROOM.into(),
            ..verify_params(&returning, OTHER_SESSION, second_proof)
        },
    )
    .await
    .expect_err("a foreign room cannot sign this room's return");
    assert_eq!(refusal_code(&foreign_verify), "verify_not_authorized");
    assert_eq!(
        restart_verify(&pool, verify_params(&returning, ROOM_SESSION, second_proof))
            .await?
            .state,
        RestartState::Verified,
        "resume verifies with the exact recorded session"
    );
    let proof_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM restart.successor_proofs WHERE intent_id=$1::text::uuid",
    )
    .bind(&returning)
    .fetch_one(&pool)
    .await?;
    assert_eq!(proof_rows, 0, "verification deletes the consumed proof");

    // Fresh mode requires a distinct successor session.
    let fresh = restart_request(
        &pool,
        fresh_request_params("D:/athanor-wt/fresh-successor", "fresh-successor-1"),
    )
    .await?;
    restart_transition(
        &pool,
        transition_params(&fresh.intent_id, RestartTransitionTarget::Exiting, None),
    )
    .await?;
    let fresh_claim = restart_claim(&pool, claim_params(&fresh.intent_id, "fresh-claim")).await?;
    let fresh_transition = restart_transition(
        &pool,
        transition_params(
            &fresh.intent_id,
            RestartTransitionTarget::Relaunching,
            Some(&fresh_claim.claim_token),
        ),
    )
    .await?;
    let fresh_proof = fresh_transition.successor_proof.as_deref().unwrap();
    assert_eq!(
        restart_verify(
            &pool,
            verify_params(&fresh.intent_id, SUCCESSOR_SESSION, fresh_proof),
        )
        .await?
        .state,
        RestartState::Verified
    );

    // A valid proof cannot cross the live House deadline.
    let late = arm_and_exit(&pool, "D:/athanor-wt/late-proof", "late-proof-1").await?;
    let late_claim = restart_claim(&pool, claim_params(&late, "late-proof-claim")).await?;
    let late_transition = restart_transition(
        &pool,
        transition_params(
            &late,
            RestartTransitionTarget::Relaunching,
            Some(&late_claim.claim_token),
        ),
    )
    .await?;
    let late_proof = late_transition.successor_proof.as_deref().unwrap();
    sqlx::query(
        "UPDATE restart.intents SET relaunching_deadline_at=NOW()-INTERVAL '1 second' WHERE intent_id=$1::text::uuid",
    )
    .bind(&late)
    .execute(&pool)
    .await?;
    let expired = restart_verify(&pool, verify_params(&late, ROOM_SESSION, late_proof))
        .await
        .expect_err("verification after the live relaunching deadline must fail");
    assert_eq!(refusal_code(&expired), "verify_expired");
    let failed = restart_transition(
        &pool,
        transition_params(
            &late,
            RestartTransitionTarget::Failed,
            Some(&late_claim.claim_token),
        ),
    )
    .await?;
    assert_eq!(failed.state, RestartState::Failed);
    assert!(
        failed.successor_proof.is_none(),
        "a terminal transition returns no launch credential"
    );
    let proof_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM restart.successor_proofs WHERE intent_id=$1::text::uuid",
    )
    .bind(&late)
    .fetch_one(&pool)
    .await?;
    assert_eq!(proof_rows, 0, "failed transitions delete the current proof");

    // 13. One live intent per workspace, so the keeper's newest-live read can
    // never hand it a stranger while the one it watches is still running.
    let one_live_workspace = "D:/athanor-wt/one-live";
    let watched = restart_request(&pool, request_params(one_live_workspace, "one-live-1")).await?;
    // The second ask is held, not answered, and the keeper's read is asked
    // before the refusal is inspected: a minted twin would answer this read
    // with a stranger while the watched intent is still running.
    let twin = restart_request(&pool, request_params(one_live_workspace, "one-live-2")).await;
    let pending = restart_status(
        &pool,
        RestartStatusParams {
            workspace: one_live_workspace.into(),
            intent_id: None,
        },
    )
    .await?
    .intent
    .expect("the live intent is pending");
    assert_eq!(
        pending.intent_id, watched.intent_id,
        "the pending read can only ever be the one live intent"
    );
    assert_eq!(
        refusal_code(&twin.expect_err("a workspace with a live intent mints no second one")),
        "intent_pending"
    );
    let live_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM restart.intents WHERE workspace=$1 AND state IN ('requested','exiting','claimed','relaunching')",
    )
    .bind(one_live_workspace)
    .fetch_one(&pool)
    .await?;
    assert_eq!(live_rows, 1, "one workspace, one live row");
    // Below the door as well: the partial unique index refuses the twin.
    let direct = sqlx::query(
        "INSERT INTO restart.intents (harness,workspace,mode,reason,consent_source,requester_room,requester_spirit,requester_session,idempotency_key,expires_at) VALUES ('omp',$1,'resume','a twin the index must refuse','operator-standing-policy',$2,'Kodo',$3,'one-live-direct',NOW()+INTERVAL '300 seconds')",
    )
    .bind(one_live_workspace)
    .bind(ROOM)
    .bind(ROOM_SESSION)
    .execute(&pool)
    .await;
    assert!(
        direct.is_err(),
        "a second live intent is unconstructible, not merely refused at the door"
    );
    retire(&pool, &watched.intent_id).await?;
    restart_request(&pool, request_params(one_live_workspace, "one-live-3")).await?;

    // 14. The exact-intent read: the keeper's verify watch needs a positive
    // sighting of verified, and the pending read cannot give one.
    let successor_workspace = "D:/athanor-wt/successor";
    assert!(
        restart_status(
            &pool,
            RestartStatusParams {
                workspace: successor_workspace.into(),
                intent_id: None,
            },
        )
        .await?
        .intent
        .is_none(),
        "a verified intent is finished, so it is not pending"
    );
    let exact = restart_status(
        &pool,
        RestartStatusParams {
            workspace: successor_workspace.into(),
            intent_id: Some(returning.clone()),
        },
    )
    .await?
    .intent
    .expect("the exact read answers for a finished intent");
    assert_eq!(exact.intent_id, returning);
    assert_eq!(
        exact.state,
        RestartState::Verified,
        "the watch can see its own successor arrive"
    );
    assert!(
        restart_status(
            &pool,
            RestartStatusParams {
                workspace: one_live_workspace.into(),
                intent_id: Some(returning.clone()),
            },
        )
        .await?
        .intent
        .is_none(),
        "naming an id buys no reach into another workspace's restart"
    );
    assert!(
        restart_status(
            &pool,
            RestartStatusParams {
                workspace: successor_workspace.into(),
                intent_id: Some("00000000-0000-0000-0000-000000000001".into()),
            },
        )
        .await?
        .intent
        .is_none(),
        "an unknown id reads as none, never as somebody else's intent"
    );

    // 15. The armed exit nobody claimed. A room with no keeper arms an exit
    // that no owner can ever claim; that row is dead and it holds the
    // workspace, so before this fence one request_restart from a keeperless
    // room refused every later restart with intent_pending forever.
    let keeperless = "D:/athanor-wt/keeperless";
    let stranded = arm_and_exit(&pool, keeperless, "keeperless-1").await?;
    age_exiting_deadline(&pool, &stranded).await?;
    let after = restart_request(&pool, request_params(keeperless, "keeperless-2")).await?;
    assert_eq!(
        after.state,
        RestartState::Requested,
        "the request behind a stranded exit proceeds instead of refusing forever"
    );
    assert_ne!(after.intent_id, stranded);
    assert_eq!(
        stored_state(&pool, &stranded).await?,
        ("failed".to_string(), Some("exiting".to_string())),
        "the stranded exit reaches a terminal state, and it names the stage it died in"
    );
    let (reason, actor): (Option<String>, String) = sqlx::query_as(
        "SELECT detail->>'reason', principal FROM restart.intent_events WHERE intent_id=$1::text::uuid AND event_kind=$2 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&stranded)
    .bind(RestartState::Failed.as_str())
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        reason.as_deref(),
        Some(EXIT_UNCLAIMED_REASON),
        "the ledger says why it failed, not only that it failed"
    );
    assert!(
        actor.starts_with("house:"),
        "the House expired it: no spirit was in the room to do it ({actor})"
    );
    assert!(
        ledger(&pool, &stranded).await?.contains(&"failed".to_string()),
        "the terminal move is in the append-only ledger"
    );

    // The other half: a claimed exit is a live keeper's work, whatever its
    // exiting deadline says, so the sweep must never take it and the workspace
    // stays held.
    let live_keeper = "D:/athanor-wt/live-keeper";
    let watched = arm_and_exit(&pool, live_keeper, "live-keeper-1").await?;
    restart_claim(&pool, claim_params(&watched, "live-keeper-claim")).await?;
    age_exiting_deadline(&pool, &watched).await?;
    let refused = restart_request(&pool, request_params(live_keeper, "live-keeper-2")).await;
    assert_eq!(
        refusal_code(&refused.expect_err("a claimed intent still holds its workspace")),
        "intent_pending"
    );
    assert_eq!(
        stored_state(&pool, &watched).await?,
        ("claimed".to_string(), None),
        "the sweep never touches an intent a keeper already claimed"
    );
    Ok(())
}

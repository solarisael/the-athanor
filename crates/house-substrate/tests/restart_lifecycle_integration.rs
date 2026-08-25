//! The restart plane against a real PostgreSQL schema. One test, six phases:
//! the full lifecycle, the storm guard's counting query, the tokenless exit
//! fence, expiry of an unclaimed request, the stale-token refusal, and
//! one-verify-only. It is one test on purpose — it owns the whole `restart`
//! schema and resets it, so it must not race a sibling that does the same.
//!
//! Run it against a scratch database only:
//!   SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL=postgres://.../scratch \
//!     cargo test -p athanor-substrate --test restart_lifecycle_integration -- --ignored

use athanor_substrate::{
    AppError, EXITING_DEADLINE_SECS, RELAUNCHING_DEADLINE_SECS, REQUESTED_TTL_SECS,
    STORM_MAX_EXITING_PER_WINDOW, restart_claim, restart_request, restart_status,
    restart_transition, restart_verify,
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
    let pool = PgPoolOptions::new()
        .max_connections(1)
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
    sqlx::query(
        "INSERT INTO restart.principal_capabilities (principal, operation_class, capability_hash) VALUES ($1, 'restart_claim', $2)",
    )
    .bind(KEEPER)
    .bind(format!("{:x}", Sha256::digest(KEEPER_SECRET.as_bytes())))
    .execute(&pool)
    .await?;
    Ok(pool)
}

fn request_params(workspace: &str, key: &str) -> RestartRequestParams {
    RestartRequestParams {
        harness: RestartHarness::Omp,
        workspace: workspace.into(),
        mode: RestartMode::Resume,
        session_id: Some("service:kodo".into()),
        reason: "the loader installed a newer release than this session loaded".into(),
        consent_source: RestartConsentSource::OperatorStandingPolicy,
        requester_room: "kodo".into(),
        requester_spirit: "Kodo".into(),
        requester_session: "service:kodo".into(),
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
    RestartTransitionParams {
        intent_id: intent_id.into(),
        claim_token: claim_token.map(str::to_owned),
        to,
        detail: Some(r#"{"source":"omp-adapter","session":"service:kodo"}"#.into()),
    }
}

fn verify_params(intent_id: &str, session: &str) -> RestartVerifyParams {
    RestartVerifyParams {
        intent_id: intent_id.into(),
        successor_session: session.into(),
        room: "kodo".into(),
        spirit: "Kodo".into(),
    }
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
// a superseded token that still moves the intent, and a second verify.
// red-proof: allow exiting from claimed in restart_transition; drop the
// workspace predicate from the storm count; remove the expiry branch from
// restart_claim; return Ok from the stale_lease branch; relax
// restart_verify's state fence.
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
    assert_eq!(pending.session_id.as_deref(), Some("service:kodo"));

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
        restart_verify(&pool, verify_params(&requested.intent_id, "service:kodo-2")).await?;
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
    let second = restart_verify(&pool, verify_params(&requested.intent_id, "service:kodo-3"))
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
        successor, "service:kodo-2",
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
    Ok(())
}

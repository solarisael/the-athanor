//! The restart intent plane: one leased state machine per self-restart.
//!
//! The concern: a session asks the House to restart its harness, the adapter
//! arms the exit, a keeper claims the corpse and relaunches, and the successor
//! session proves it came back. Five doors and nothing else:
//! [`restart_request`], [`restart_claim`], [`restart_transition`],
//! [`restart_verify`], [`restart_status`]. The wire vocabulary lives in
//! `protocol::restart`; no state literal is re-declared here.
//!
//! State order (contract v1 as amended 2026-08-25): requested -> exiting ->
//! claimed -> relaunching -> verified. The keeper only runs after omp is gone,
//! so it cannot claim before the exit; the adapter therefore arms `exiting`
//! tokenless from `requested`, and the keeper claims from `exiting` or, after a
//! crash exit that never armed, from `requested`.
//!
//! Why this is not the Docket: the Docket plane structurally needs 15-minute
//! leases, room capabilities, and independent review. A restart is second-scale
//! and machine-verified, so it mirrors the Docket's fences over its own schema
//! instead of borrowing its tables.
//!
//! ## Keeper principal
//! The claim door authenticates a *principal*, not a room: the keeper owns the
//! terminal and impersonates nobody. The shipped name is **`omp-keeper`** —
//! chosen because it is a lowercase hyphenated slug, so it validates under the
//! same law room keys obey (`config.rs` ROOM_KEY_RE) and under the migration's
//! `restart_principal_capabilities_principal_check`. One slug law, two kinds of
//! principal, no second convention. Its secret is provisioned offline exactly
//! like a room capability (`substrate/provision-restart-capability.ps1`, which
//! mirrors `provision-room-capability.ps1`): the ritual writes only the sha256
//! here and drops the secret in the keeper's runtime file. Caller-supplied
//! claimant text mints nothing; without a provisioned row a claim refuses.
//!
//! ## Authority
//! Who may move an intent is its own concern and lives in [`authority`]: the
//! intent id proves nothing here, because `restart_status` hands it out with no
//! capability at all.
//!
//! `requester_session` is one identifier kind and one only: the **harness
//! session id**, the exact string the adapter's `hostSessionIdentity` yields.
//! Resume keeps that logical session id while the keeper creates a new process;
//! fresh mode creates a different session. An attempt-scoped proof, minted only
//! after the predecessor exits, distinguishes the process incarnation.
//!
//! ## Expiry is lazy
//! An unclaimed request past its TTL is dead, and this plane runs no clock
//! service. Whichever write door touches a lapsed request kills it and refuses;
//! the status read never reports one. A request nobody ever touches stays a dead
//! `requested` row — the adapter arms within one turn and the keeper polls
//! within seconds of an exit, so a sweep would have no work. enough: a clock
//! sweep is the upgrade path if the ledger ever needs an `expired` event for a
//! row no door touched.

mod authority;
mod proof;

use crate::config::{AppError, ROOM_KEY_RE};
use authority::{
    EXIT_CLASS, KEEPER_CLAIM_CLASS, REQUEST_CLASS, VERIFY_CLASS, require_capability,
    require_requester_session, require_successor_identity,
};
use chrono::{DateTime, Utc};
use protocol::restart::{
    RestartClaimParams, RestartClaimReceipt, RestartMode, RestartRequestParams,
    RestartRequestReceipt, RestartStageDeadlines, RestartState, RestartStatusDeadlines,
    RestartStatusIntent, RestartStatusParams, RestartStatusReceipt, RestartTransitionParams,
    RestartTransitionReceipt, RestartTransitionTarget, RestartVerifyParams, RestartVerifyReceipt,
};
use proof::{
    clear as clear_successor_proof, require_current as require_successor_proof,
    rotate as rotate_successor_proof,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

// Stage deadlines and the storm bound: one const block, one authority. The
// claim receipt hands these to the keeper, so the keeper carries no second
// copy. They are the contract's unmeasured v1 hypotheses — change them only
// from observed durations.
/// An unclaimed request older than this becomes `expired` and never fires.
pub const REQUESTED_TTL_SECS: i64 = 300;
/// The keeper escalates to kill when an `exiting` intent passes this.
pub const EXITING_DEADLINE_SECS: i64 = 60;
/// A relaunch that has not verified inside this window is late.
pub const RELAUNCHING_DEADLINE_SECS: i64 = 120;
/// The first launch and one retry. The attempt that would exceed this budget
/// lands on `failed:relaunching` instead.
pub const RELAUNCH_ATTEMPT_LIMIT: i32 = 2;
/// The storm window: one hour.
pub const STORM_WINDOW_SECS: i64 = 3600;
/// More than this many intents reaching `exiting` inside one window for one
/// workspace is a restart storm.
pub const STORM_MAX_EXITING_PER_WINDOW: i64 = 3;

/// The principal for acts the House performs itself, with no spirit in the
/// room: lazy expiry is the only one today.
const HOUSE_PRINCIPAL: &str = "house:restart";

fn refusal(code: &'static str, message: &'static str) -> AppError {
    AppError::Refusal { code, message }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let longest = left.len().max(right.len());
    for index in 0..longest {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(a ^ b);
    }
    difference == 0
}

fn principal(room: &str, spirit: &str) -> String {
    format!("{room}:{spirit}")
}

/// One slug law for every principal name here, room or keeper.
fn validate_slug(value: &str, field: &'static str) -> Result<(), AppError> {
    if ROOM_KEY_RE.is_match(value) {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "{field} must be a lowercase slug"
        )))
    }
}

fn validate_intent_id(value: &str) -> Result<(), AppError> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| AppError::Invalid("intentId must be a UUID".into()))
}

/// Storage and this binary hold the same state list. A value outside it means
/// the two have drifted, which is a build fault and not a caller's problem.
fn state_of(value: &str) -> Result<RestartState, AppError> {
    RestartState::from_str(value).ok_or_else(|| {
        AppError::Config(format!(
            "restart intent carries a state this build does not know: {value}"
        ))
    })
}

fn mode_of(value: &str) -> Result<RestartMode, AppError> {
    RestartMode::from_str(value).ok_or_else(|| {
        AppError::Config(format!(
            "restart intent carries a mode this build does not know: {value}"
        ))
    })
}

fn stage_deadlines() -> RestartStageDeadlines {
    RestartStageDeadlines {
        requested_ttl_secs: REQUESTED_TTL_SECS,
        exiting_secs: EXITING_DEADLINE_SECS,
        relaunching_secs: RELAUNCHING_DEADLINE_SECS,
        relaunch_attempt_limit: RELAUNCH_ATTEMPT_LIMIT,
    }
}

/// The states that hold a workspace, built from the vocabulary and never from
/// literals. The same four sit in migration 0026's partial unique index and in
/// the pending read the adapter arms against — one list, three readers.
fn live_states() -> [&'static str; 4] {
    [
        RestartState::Requested.as_str(),
        RestartState::Exiting.as_str(),
        RestartState::Claimed.as_str(),
        RestartState::Relaunching.as_str(),
    ]
}

/// One live intent per workspace. The keeper acts on the newest live intent it
/// can see, so a second one lets a fresh request pass for an unverified
/// successor (Kintsu's keeper P1, 2026-08-25). The unique index refuses the
/// twin structurally; this is the same refusal with a name the caller can read.
async fn refuse_on_live_intent(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
) -> Result<(), AppError> {
    let live: Option<String> = sqlx::query_scalar(
        "SELECT intent_id::text FROM restart.intents WHERE workspace=$1 AND state = ANY($2)",
    )
    .bind(workspace)
    .bind(&live_states()[..])
    .fetch_optional(&mut **tx)
    .await?;
    if live.is_some() {
        return Err(refusal(
            "intent_pending",
            "this workspace already has a live restart intent",
        ));
    }
    Ok(())
}

/// The guard bounds arrivals at `exiting`, not requests: a request minted
/// before the window filled must still refuse to arm, so both the request door
/// and the exit door count and refuse.
fn refuse_on_storm(reached_exiting_in_window: i64) -> Result<(), AppError> {
    if reached_exiting_in_window >= STORM_MAX_EXITING_PER_WINDOW {
        return Err(refusal(
            "restart_storm",
            "too many restarts reached exiting for this workspace inside the storm window",
        ));
    }
    Ok(())
}

/// How many intents reached `exiting` for this workspace inside the window. The
/// caller holds the workspace advisory lock, so the count and the decision it
/// feeds are one decision.
async fn reached_exiting_in_window(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
) -> Result<i64, AppError> {
    let reached: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT event.intent_id) FROM restart.intent_events AS event JOIN restart.intents AS intent USING (intent_id) WHERE intent.workspace=$1 AND event.event_kind=$2 AND event.created_at > clock_timestamp() - ($3 * INTERVAL '1 second')",
    )
    .bind(workspace)
    .bind(RestartState::Exiting.as_str())
    .bind(STORM_WINDOW_SECS)
    .fetch_one(&mut **tx)
    .await?;
    Ok(reached)
}

async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    intent_id: &str,
    claim_epoch: Option<i64>,
    event_kind: &str,
    principal: &str,
    detail: Value,
    idempotency_key: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO restart.intent_events (intent_id,claim_epoch,event_kind,principal,detail,idempotency_key) VALUES ($1::text::uuid,$2,$3,$4,$5,$6)",
    )
    .bind(intent_id)
    .bind(claim_epoch)
    .bind(event_kind)
    .bind(principal)
    .bind(detail)
    .bind(idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Whichever write door touches a lapsed request is the one that kills it, and
/// it dies in the ledger before that door's refusal returns.
async fn expire_lapsed_request(
    tx: &mut Transaction<'_, Postgres>,
    intent_id: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE restart.intents SET state=$2,updated_at=NOW() WHERE intent_id=$1::text::uuid",
    )
    .bind(intent_id)
    .bind(RestartState::Expired.as_str())
    .execute(&mut **tx)
    .await?;
    insert_event(
        tx,
        intent_id,
        None,
        RestartState::Expired.as_str(),
        HOUSE_PRINCIPAL,
        json!({"ttlSecs": REQUESTED_TTL_SECS}),
        None,
    )
    .await
}

/// Read the expiry decision after the row lock is held. NOW() is the
/// transaction's start, and a caller can sit behind `FOR UPDATE` long enough to
/// cross the TTL while it waits, so this is the only clock the write doors may
/// believe.
async fn lapsed_after_lock(
    tx: &mut Transaction<'_, Postgres>,
    intent_id: &str,
) -> Result<bool, AppError> {
    let lapsed: bool = sqlx::query_scalar(
        "SELECT expires_at <= clock_timestamp() FROM restart.intents WHERE intent_id=$1::text::uuid",
    )
    .bind(intent_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(lapsed)
}
/// The workspace lock makes this sweep and the replacement insert one act.
/// Status hides a lapsed request, so a new key must retire that row before the
/// one-live fence can decide whether the workspace is occupied.
async fn expire_lapsed_requests_in_workspace(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
) -> Result<(), AppError> {
    let intent_ids: Vec<String> = sqlx::query_scalar(
        "SELECT intent_id::text FROM restart.intents WHERE workspace=$1 AND state=$2 FOR UPDATE",
    )
    .bind(workspace)
    .bind(RestartState::Requested.as_str())
    .fetch_all(&mut **tx)
    .await?;
    for intent_id in intent_ids {
        if lapsed_after_lock(tx, &intent_id).await? {
            expire_lapsed_request(tx, &intent_id).await?;
        }
    }
    Ok(())
}

fn refuse_expired() -> AppError {
    refusal(
        "intent_expired",
        "the intent passed its unclaimed deadline and never fires",
    )
}

/// Request one restart. Idempotent by `(workspace, idempotencyKey)`: a replay
/// returns the existing intent instead of minting a twin. The workspace lock
/// makes that replay door and the storm guard one decision, so two concurrent
/// requests cannot both pass a full window.
pub async fn restart_request(
    pool: &PgPool,
    request: RestartRequestParams,
) -> Result<RestartRequestReceipt, AppError> {
    request.validate().map_err(AppError::Invalid)?;
    validate_slug(&request.requester_room, "requesterRoom")?;
    require_capability(
        pool,
        &request.requester_room,
        REQUEST_CLASS,
        &request.capability,
    )
    .await?;

    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(&request.workspace)
        .execute(&mut *tx)
        .await?;

    if let Some(row) = sqlx::query(
        "SELECT intent_id::text AS intent_id,state,expires_at FROM restart.intents WHERE workspace=$1 AND idempotency_key=$2 FOR UPDATE",
    )
    .bind(&request.workspace)
    .bind(&request.idempotency_key)
    .fetch_optional(&mut *tx)
    .await?
    {
        let intent_id: String = row.try_get("intent_id")?;
        let state = state_of(&row.try_get::<String, _>("state")?)?;
        let expires_at: DateTime<Utc> = row.try_get("expires_at")?;
        // The adapter and the keeper both act on the state this replay returns,
        // so a lapsed request must die here instead of reading alive again.
        let lapsed = lapsed_after_lock(&mut tx, &intent_id).await?;
        if state == RestartState::Expired || (state == RestartState::Requested && lapsed) {
            if state == RestartState::Requested {
                expire_lapsed_request(&mut tx, &intent_id).await?;
            }
            tx.commit().await?;
            return Err(refuse_expired());
        }
        let receipt = RestartRequestReceipt {
            intent_id,
            state,
            expires_at: expires_at.to_rfc3339(),
        };
        tx.commit().await?;
        return Ok(receipt);
    }
    expire_lapsed_requests_in_workspace(&mut tx, &request.workspace).await?;

    refuse_on_live_intent(&mut tx, &request.workspace).await?;
    refuse_on_storm(reached_exiting_in_window(&mut tx, &request.workspace).await?)?;

    let row = sqlx::query(
        "INSERT INTO restart.intents (harness,workspace,mode,session_id,reason,consent_source,requester_room,requester_spirit,requester_session,idempotency_key,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,NOW()+($11 * INTERVAL '1 second')) RETURNING intent_id::text AS intent_id,state,expires_at",
    )
    .bind(request.harness.as_str())
    .bind(&request.workspace)
    .bind(request.mode.as_str())
    .bind(request.session_id.as_deref())
    .bind(&request.reason)
    .bind(request.consent_source.as_str())
    .bind(&request.requester_room)
    .bind(&request.requester_spirit)
    .bind(&request.requester_session)
    .bind(&request.idempotency_key)
    .bind(REQUESTED_TTL_SECS)
    .fetch_one(&mut *tx)
    .await?;
    let intent_id: String = row.try_get("intent_id")?;
    let state: String = row.try_get("state")?;
    let expires_at: DateTime<Utc> = row.try_get("expires_at")?;

    insert_event(
        &mut tx,
        &intent_id,
        None,
        RestartState::Requested.as_str(),
        &principal(&request.requester_room, &request.requester_spirit),
        json!({
            "harness": request.harness.as_str(),
            "mode": request.mode.as_str(),
            "consentSource": request.consent_source.as_str(),
            "requesterSession": request.requester_session,
            "sessionId": request.session_id,
        }),
        Some(&request.idempotency_key),
    )
    .await?;
    tx.commit().await?;

    Ok(RestartRequestReceipt {
        intent_id,
        state: state_of(&state)?,
        expires_at: expires_at.to_rfc3339(),
    })
}

/// Claim one intent for the keeper. Legal from `exiting` (the ordinary path,
/// after the adapter armed and omp died) and from `requested` (a crash exit
/// that never armed). The token is minted once and shown once: only its sha256
/// is stored, and the state fence already refuses a second claim, so a replay
/// never needs the secret back.
pub async fn restart_claim(
    pool: &PgPool,
    request: RestartClaimParams,
) -> Result<RestartClaimReceipt, AppError> {
    request.validate().map_err(AppError::Invalid)?;
    validate_intent_id(&request.intent_id)?;
    validate_slug(&request.claimant, "claimant")?;
    require_capability(
        pool,
        &request.claimant,
        KEEPER_CLAIM_CLASS,
        &request.capability,
    )
    .await?;

    let mut tx = pool.begin().await?;
    let intent = sqlx::query(
        "SELECT state,claim_epoch FROM restart.intents WHERE intent_id=$1::text::uuid FOR UPDATE",
    )
    .bind(&request.intent_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| {
        refusal(
            "unknown_intent",
            "the requested restart intent does not exist",
        )
    })?;
    let state = state_of(&intent.try_get::<String, _>("state")?)?;
    let lapsed = lapsed_after_lock(&mut tx, &request.intent_id).await?;

    if state == RestartState::Requested && lapsed {
        expire_lapsed_request(&mut tx, &request.intent_id).await?;
        tx.commit().await?;
        return Err(refuse_expired());
    }
    if !matches!(state, RestartState::Requested | RestartState::Exiting) {
        return Err(refusal(
            "not_claimable",
            "only a requested or exiting restart intent can be claimed",
        ));
    }

    let claim_epoch: i64 = intent.try_get::<i64, _>("claim_epoch")? + 1;
    let claim_token: String = sqlx::query_scalar("SELECT encode(gen_random_bytes(32),'hex')")
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE restart.intents SET state=$2,claim_epoch=$3,claimant=$4,claim_idempotency_key=$5,lease_token_hash=$6,claimed_at=NOW(),updated_at=NOW() WHERE intent_id=$1::text::uuid",
    )
    .bind(&request.intent_id)
    .bind(RestartState::Claimed.as_str())
    .bind(claim_epoch)
    .bind(&request.claimant)
    .bind(&request.idempotency_key)
    .bind(sha256_hex(claim_token.as_bytes()))
    .execute(&mut *tx)
    .await?;
    insert_event(
        &mut tx,
        &request.intent_id,
        Some(claim_epoch),
        RestartState::Claimed.as_str(),
        &request.claimant,
        json!({"claimEpoch": claim_epoch, "from": state.as_str()}),
        Some(&request.idempotency_key),
    )
    .await?;
    tx.commit().await?;

    Ok(RestartClaimReceipt {
        claim_token,
        claim_epoch,
        stage_deadlines: stage_deadlines(),
    })
}

/// Move one intent. Two doors share this method because they share the ledger:
/// the adapter's `exiting` out of `requested`, fenced by the requester's own
/// session and room secret, and the keeper's token-fenced `relaunching` or
/// `failed`. A refused exit must be sharp and named — the adapter stands down on
/// it and keeps the session alive — so `exit_not_requested` never hides inside
/// the lease refusal.
pub async fn restart_transition(
    pool: &PgPool,
    request: RestartTransitionParams,
) -> Result<RestartTransitionReceipt, AppError> {
    request.validate().map_err(AppError::Invalid)?;
    validate_intent_id(&request.intent_id)?;

    let mut tx = pool.begin().await?;
    // The exit arm is the storm-bearing one, so it decides under the same
    // workspace lock restart_request takes: two arms racing cannot both pass a
    // window with room for one.
    if request.to == RestartTransitionTarget::Exiting {
        let workspace: String = sqlx::query_scalar(
            "SELECT workspace FROM restart.intents WHERE intent_id=$1::text::uuid",
        )
        .bind(&request.intent_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            refusal(
                "unknown_intent",
                "the requested restart intent does not exist",
            )
        })?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(&workspace)
            .execute(&mut *tx)
            .await?;
    }

    let intent = sqlx::query(
        "SELECT state,claim_epoch,claimant,workspace,requester_room,requester_spirit,requester_session,lease_token_hash,relaunch_attempts FROM restart.intents WHERE intent_id=$1::text::uuid FOR UPDATE",
    )
    .bind(&request.intent_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| refusal("unknown_intent", "the requested restart intent does not exist"))?;
    let state = state_of(&intent.try_get::<String, _>("state")?)?;
    let lapsed = lapsed_after_lock(&mut tx, &request.intent_id).await?;

    if state == RestartState::Requested && lapsed {
        expire_lapsed_request(&mut tx, &request.intent_id).await?;
        tx.commit().await?;
        return Err(refuse_expired());
    }

    let claim_epoch: i64 = intent.try_get("claim_epoch")?;
    let relaunch_attempts: i32 = intent.try_get("relaunch_attempts")?;
    let (reached, failed_stage, actor) = match request.to {
        RestartTransitionTarget::Exiting => {
            let room: String = intent.try_get("requester_room")?;
            let spirit: String = intent.try_get("requester_spirit")?;
            let requester_session: String = intent.try_get("requester_session")?;
            let workspace: String = intent.try_get("workspace")?;
            // The room comes off the locked row and never off the caller, so
            // reading an intent id buys no choice of which secret to hold.
            require_capability(
                &mut *tx,
                &room,
                EXIT_CLASS,
                request.capability.as_deref().unwrap_or_default(),
            )
            .await?;
            require_requester_session(
                &requester_session,
                request.requester_session.as_deref().unwrap_or_default(),
            )?;
            if state != RestartState::Requested {
                return Err(refusal(
                    "exit_not_requested",
                    "only a requested restart intent can be armed for exit",
                ));
            }
            refuse_on_storm(reached_exiting_in_window(&mut tx, &workspace).await?)?;
            sqlx::query(
                "UPDATE restart.intents SET state=$2,exiting_deadline_at=NOW()+($3 * INTERVAL '1 second'),updated_at=NOW() WHERE intent_id=$1::text::uuid",
            )
            .bind(&request.intent_id)
            .bind(RestartState::Exiting.as_str())
            .bind(EXITING_DEADLINE_SECS)
            .execute(&mut *tx)
            .await?;
            (RestartState::Exiting, None, principal(&room, &spirit))
        }
        RestartTransitionTarget::Relaunching | RestartTransitionTarget::Failed => {
            // Mirrors docket.rs:981-988: expired, superseded, stale, and
            // invalid are one refusal that never says which. An absent token
            // reaches here only from an unclaimed intent, which is the same
            // fact: this caller holds no lease.
            let expected_hash: Option<String> = intent.try_get("lease_token_hash")?;
            let supplied_hash = sha256_hex(
                request
                    .claim_token
                    .as_deref()
                    .unwrap_or_default()
                    .as_bytes(),
            );
            let fenced = expected_hash
                .as_deref()
                .is_some_and(|hash| constant_time_equal(supplied_hash.as_bytes(), hash.as_bytes()));
            if !fenced {
                return Err(refusal(
                    "stale_lease",
                    "the lease is expired, superseded, stale, or invalid",
                ));
            }
            let claimant: String = intent.try_get("claimant")?;
            let (reached, failed_stage) =
                keeper_move(&mut tx, &request, state, relaunch_attempts).await?;
            (reached, failed_stage, claimant)
        }
    };
    let successor_proof = match reached {
        RestartState::Relaunching => Some(
            rotate_successor_proof(
                &mut tx,
                &request.intent_id,
                claim_epoch,
                relaunch_attempts + 1,
            )
            .await?,
        ),
        RestartState::Failed => {
            clear_successor_proof(&mut tx, &request.intent_id).await?;
            None
        }
        _ => None,
    };

    insert_event(
        &mut tx,
        &request.intent_id,
        Some(claim_epoch),
        reached.as_str(),
        &actor,
        json!({
            "from": state.as_str(),
            "failedStage": failed_stage.map(RestartState::as_str),
            "detail": request.detail,
        }),
        None,
    )
    .await?;
    tx.commit().await?;

    Ok(RestartTransitionReceipt {
        state: reached,
        successor_proof,
    })
}

/// The keeper's half of a transition, after the lease fence passed. It writes
/// the intent row and reports which state the intent actually reached, which is
/// not always the one the keeper named: the attempt past the retry budget lands
/// on `failed` instead of a third relaunch.
async fn keeper_move(
    tx: &mut Transaction<'_, Postgres>,
    request: &RestartTransitionParams,
    state: RestartState,
    relaunch_attempts: i32,
) -> Result<(RestartState, Option<RestartState>), AppError> {
    let failed_stage = match (state, request.to) {
        (RestartState::Claimed, RestartTransitionTarget::Relaunching) => None,
        // The contract's one retry. The attempt past the budget lands on
        // failed:relaunching instead of refusing a keeper out of tries.
        (RestartState::Relaunching, RestartTransitionTarget::Relaunching) => {
            if relaunch_attempts >= RELAUNCH_ATTEMPT_LIMIT {
                Some(RestartState::Relaunching)
            } else {
                None
            }
        }
        (RestartState::Claimed | RestartState::Relaunching, RestartTransitionTarget::Failed) => {
            Some(state)
        }
        _ => {
            return Err(refusal(
                "illegal_transition",
                "that transition is not legal from the intent's current state",
            ));
        }
    };

    let Some(stage) = failed_stage else {
        sqlx::query(
            "UPDATE restart.intents SET state=$2,relaunching_deadline_at=NOW()+($3 * INTERVAL '1 second'),relaunch_attempts=relaunch_attempts+1,updated_at=NOW() WHERE intent_id=$1::text::uuid",
        )
        .bind(&request.intent_id)
        .bind(RestartState::Relaunching.as_str())
        .bind(RELAUNCHING_DEADLINE_SECS)
        .execute(&mut **tx)
        .await?;
        return Ok((RestartState::Relaunching, None));
    };
    sqlx::query(
        "UPDATE restart.intents SET state=$2,failed_stage=$3,updated_at=NOW() WHERE intent_id=$1::text::uuid",
    )
    .bind(&request.intent_id)
    .bind(RestartState::Failed.as_str())
    .bind(stage.as_str())
    .execute(&mut **tx)
    .await?;
    Ok((RestartState::Failed, Some(stage)))
}

/// The successor proves both parts of its return: the room capability names the
/// room, and the current relaunch attempt's keeper-minted proof names the new
/// process incarnation. Resume may therefore keep the logical session id
/// without letting the predecessor sign its own return.
pub async fn restart_verify(
    pool: &PgPool,
    request: RestartVerifyParams,
) -> Result<RestartVerifyReceipt, AppError> {
    request.validate().map_err(AppError::Invalid)?;
    validate_intent_id(&request.intent_id)?;
    validate_slug(&request.room, "room")?;

    let mut tx = pool.begin().await?;
    let intent = sqlx::query(
        "SELECT state,claim_epoch,requester_room,requester_session,mode,session_id,relaunch_attempts,relaunching_deadline_at \
         FROM restart.intents WHERE intent_id=$1::text::uuid FOR UPDATE",
    )
    .bind(&request.intent_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| {
        refusal(
            "unknown_intent",
            "the requested restart intent does not exist",
        )
    })?;
    let state = state_of(&intent.try_get::<String, _>("state")?)?;
    let claim_epoch: i64 = intent.try_get("claim_epoch")?;
    let requester_room: String = intent.try_get("requester_room")?;
    let requester_session: String = intent.try_get("requester_session")?;
    let mode = mode_of(&intent.try_get::<String, _>("mode")?)?;
    let recorded_session: Option<String> = intent.try_get("session_id")?;
    let relaunch_attempt: i32 = intent.try_get("relaunch_attempts")?;
    let relaunching_deadline: Option<DateTime<Utc>> = intent.try_get("relaunching_deadline_at")?;

    require_capability(&mut *tx, &requester_room, VERIFY_CLASS, &request.capability).await?;
    if state != RestartState::Relaunching {
        return Err(refusal(
            "not_verifiable",
            "only a relaunching restart intent can be verified",
        ));
    }
    let observed_at: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut *tx)
        .await?;
    if relaunching_deadline.is_none_or(|deadline| observed_at >= deadline) {
        return Err(refusal(
            "verify_expired",
            "the relaunching window ended before the successor verified",
        ));
    }
    require_successor_identity(
        mode,
        &requester_room,
        &requester_session,
        recorded_session.as_deref(),
        &request.room,
        &request.successor_session,
    )?;
    require_successor_proof(
        &mut tx,
        &request.intent_id,
        claim_epoch,
        relaunch_attempt,
        &request.successor_proof,
    )
    .await?;

    sqlx::query(
        "UPDATE restart.intents SET state=$2,successor_session=$3,successor_room=$4,successor_spirit=$5,verified_at=NOW(),updated_at=NOW() WHERE intent_id=$1::text::uuid",
    )
    .bind(&request.intent_id)
    .bind(RestartState::Verified.as_str())
    .bind(&request.successor_session)
    .bind(&request.room)
    .bind(&request.spirit)
    .execute(&mut *tx)
    .await?;
    clear_successor_proof(&mut tx, &request.intent_id).await?;
    insert_event(
        &mut tx,
        &request.intent_id,
        Some(claim_epoch),
        RestartState::Verified.as_str(),
        &principal(&request.room, &request.spirit),
        json!({"successorSession": request.successor_session}),
        None,
    )
    .await?;
    tx.commit().await?;

    Ok(RestartVerifyReceipt {
        state: RestartState::Verified,
    })
}

/// The columns one status row carries. Both reads below project exactly this,
/// so the receipt cannot depend on which question was asked.
const STATUS_COLUMNS: &str = "intent_id::text AS intent_id,state,mode,session_id,expires_at,exiting_deadline_at,relaunching_deadline_at";

/// Read one intent, and no capability either way: the id this hands out
/// authorizes nothing (see [`authority`]).
///
/// Two questions, two reads. Without an id: the workspace's pending intent,
/// which is what the adapter arms against, and a lapsed unclaimed request is
/// not pending. With an id: that exact intent in whatever state it reached,
/// terminal included, because the keeper's verify watch needs a positive
/// sighting of `verified` and the pending read can structurally never show one
/// (Kintsu's keeper P1, 2026-08-25).
pub async fn restart_status(
    pool: &PgPool,
    request: RestartStatusParams,
) -> Result<RestartStatusReceipt, AppError> {
    request.validate().map_err(AppError::Invalid)?;

    let row = match request.intent_id.as_deref() {
        Some(intent_id) => {
            validate_intent_id(intent_id)?;
            // The workspace still scopes the read, so naming an id buys no
            // reach into another workspace's restart.
            sqlx::query(&format!(
                "SELECT {STATUS_COLUMNS} FROM restart.intents WHERE workspace=$1 AND intent_id=$2::text::uuid"
            ))
            .bind(&request.workspace)
            .bind(intent_id)
            .fetch_optional(pool)
            .await?
        }
        None => {
            sqlx::query(&format!(
                "SELECT {STATUS_COLUMNS} FROM restart.intents WHERE workspace=$1 AND state = ANY($2) AND (state<>$3 OR expires_at>clock_timestamp()) ORDER BY created_at DESC LIMIT 1"
            ))
            .bind(&request.workspace)
            .bind(&live_states()[..])
            .bind(RestartState::Requested.as_str())
            .fetch_optional(pool)
            .await?
        }
    };

    let Some(row) = row else {
        return Ok(RestartStatusReceipt {
            workspace: request.workspace,
            intent: None,
        });
    };
    let state: String = row.try_get("state")?;
    let mode: String = row.try_get("mode")?;
    let expires_at: DateTime<Utc> = row.try_get("expires_at")?;
    let exiting_deadline_at: Option<DateTime<Utc>> = row.try_get("exiting_deadline_at")?;
    let relaunching_deadline_at: Option<DateTime<Utc>> = row.try_get("relaunching_deadline_at")?;
    Ok(RestartStatusReceipt {
        workspace: request.workspace,
        intent: Some(RestartStatusIntent {
            intent_id: row.try_get("intent_id")?,
            state: state_of(&state)?,
            mode: mode_of(&mode)?,
            session_id: row.try_get("session_id")?,
            deadlines: RestartStatusDeadlines {
                expires_at: expires_at.to_rfc3339(),
                exiting_deadline_at: exiting_deadline_at.map(|at| at.to_rfc3339()),
                relaunching_deadline_at: relaunching_deadline_at.map(|at| at.to_rfc3339()),
            },
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Kills: a storm guard that fires one grant too late, so a fourth restart
    // reaches exiting inside the window.
    // red-proof: change `>=` to `>` in refuse_on_storm.
    #[test]
    fn storm_guard_refuses_the_grant_that_would_break_the_bound() {
        for observed in 0..STORM_MAX_EXITING_PER_WINDOW {
            assert!(
                refuse_on_storm(observed).is_ok(),
                "{observed} exits inside the window still leaves room"
            );
        }
        for observed in [
            STORM_MAX_EXITING_PER_WINDOW,
            STORM_MAX_EXITING_PER_WINDOW + 1,
        ] {
            let error = refuse_on_storm(observed).expect_err("the window is full");
            assert!(
                matches!(
                    error,
                    AppError::Refusal {
                        code: "restart_storm",
                        ..
                    }
                ),
                "the full window must refuse by name"
            );
        }
    }

    // Kills: a claimant or requester room admitted without the slug law, which
    // is how a capability lookup would be handed arbitrary caller text.
    // red-proof: drop the validate_slug call from restart_claim.
    #[test]
    fn principal_names_obey_one_slug_law() {
        validate_slug("omp-keeper", "claimant").unwrap();
        validate_slug("kodo", "requesterRoom").unwrap();
        for rejected in ["OmpKeeper", "omp keeper", "omp_keeper", "-keeper", ""] {
            assert!(
                validate_slug(rejected, "claimant").is_err(),
                "{rejected} is not a slug"
            );
        }
    }

    // Kills: a state or mode read back from storage by guesswork instead of the
    // one shared table, and an intent id accepted without being a UUID.
    // red-proof: make state_of fall back to a default instead of refusing.
    #[test]
    fn storage_values_are_read_through_the_shared_vocabulary() {
        assert_eq!(state_of("relaunching").unwrap(), RestartState::Relaunching);
        assert!(state_of("exited").is_err());
        assert_eq!(mode_of("fresh").unwrap(), RestartMode::Fresh);
        assert!(mode_of("resumed").is_err());
        validate_intent_id("00000000-0000-0000-0000-000000000001").unwrap();
        assert!(validate_intent_id("00000000-0000-0000-0000-00000000000").is_err());
    }

    // Kills: a stage-deadline receipt that stops matching the const block, so
    // the keeper would obey a number the House does not hold.
    // red-proof: change one number in stage_deadlines() only.
    #[test]
    fn stage_deadlines_come_from_the_const_block() {
        let deadlines = stage_deadlines();
        assert_eq!(deadlines.requested_ttl_secs, REQUESTED_TTL_SECS);
        assert_eq!(deadlines.exiting_secs, EXITING_DEADLINE_SECS);
        assert_eq!(deadlines.relaunching_secs, RELAUNCHING_DEADLINE_SECS);
        assert_eq!(deadlines.relaunch_attempt_limit, RELAUNCH_ATTEMPT_LIMIT);
    }

    // Kills: a token comparison that stops comparing content, or one that
    // treats an absent token as a match.
    // red-proof: replace constant_time_equal's body with a length comparison.
    #[test]
    fn token_comparison_needs_the_whole_secret() {
        let hash = sha256_hex(b"claim-token");
        assert!(constant_time_equal(
            sha256_hex(b"claim-token").as_bytes(),
            hash.as_bytes()
        ));
        assert!(!constant_time_equal(
            sha256_hex(b"claim-tokem").as_bytes(),
            hash.as_bytes()
        ));
        assert!(!constant_time_equal(b"", hash.as_bytes()));
    }
}

//! Host-owned restart of one exact portable instance.
//!
//! Authority stays in the native restart intents (`akasha::restart_*`): the
//! Host presents the room's provisioned secrets. The intent binds both the
//! conversation and its live instance, so a sibling in the same workspace
//! cannot be armed, claimed, or relaunched by this path.
//! The Host never kills: a predecessor that does not leave fails the intent.
//! The claim and the verify run only after the predecessor is gone, so their
//! secrets meet their doors' fence before the request mints an intent and
//! again before the exit is armed, while the predecessor still runs.

use super::{LauncherKind, Refusal, Registry, RestartRecord, RestartStage, Terminal};
use crate::server::AppState;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use protocol::restart::{
    RestartClaimParams, RestartConsentSource, RestartHarness, RestartMode, RestartRequestParams,
    RestartStatusParams, RestartTransitionParams, RestartTransitionTarget, RestartVerifyParams,
};
use serde_json::Value;
use sqlx::PgPool;
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

const CLAIMANT: &str = "omp-keeper";
const CLAIM_SECRET: &str = "restart-capability";
const REQUEST_SECRET: &str = "restart-request-capability";
const EXIT_SECRET: &str = "restart-exit-capability";
const VERIFY_SECRET: &str = "restart-verify-capability";
const WATCH_TICK: Duration = Duration::from_millis(250);

fn unavailable(message: impl Into<String>) -> Refusal {
    Refusal::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "restart_authority_unavailable",
        message,
    )
}

/// Read one provisioned holder file. The secret stays inside the Host.
fn secret(room_dir: &Path, file: &str) -> Result<String, Refusal> {
    let text = std::fs::read_to_string(room_dir.join(".omp").join("runtime").join(file)).map_err(
        |error| unavailable(format!("restart capability {file} is unreadable: {error}")),
    )?;
    let secret = text.trim();
    if secret.is_empty() {
        return Err(unavailable(format!("restart capability {file} is empty")));
    }
    Ok(secret.to_owned())
}

fn pool(state: &AppState) -> Result<&PgPool, Refusal> {
    state
        .hallway_pool
        .as_ref()
        .ok_or_else(|| unavailable("this Host runs without PostgreSQL restart authority"))
}

fn refused(error: akasha::AppError) -> Refusal {
    match error {
        akasha::AppError::Refusal { code, .. } => {
            Refusal::new(StatusCode::CONFLICT, code, error.to_string())
        }
        other => Refusal::internal(format!("restart authority failed: {other}")),
    }
}

/// A secret the claim or the verify door refuses after the exit strands the
/// restart: the exit is armed, nothing claims it, no successor verifies. Both
/// secrets meet their doors' own fence here instead, while the predecessor
/// still runs. The keeper launcher presents the same holders: provisioning
/// names this claim file as the keeper's capabilityPath, and the successor
/// verifies with this room's verify file.
pub(super) async fn require_relaunch_authority(state: &AppState) -> Result<(), Refusal> {
    let pool = pool(state)?;
    for (door, principal, file, stranded) in [
        (
            akasha::RestartPreflightDoor::Claim,
            CLAIMANT,
            CLAIM_SECRET,
            "nothing could claim the restart",
        ),
        (
            akasha::RestartPreflightDoor::Verify,
            state.config.room.as_str(),
            VERIFY_SECRET,
            "the successor could not verify",
        ),
    ] {
        let capability = secret(&state.config.room_dir, file)?;
        akasha::restart_preflight(pool, door, principal, &capability)
            .await
            .map_err(|error| match error {
                akasha::AppError::Refusal { code, message } => Refusal::new(
                    StatusCode::CONFLICT,
                    code,
                    format!(
                        "restart capability {file} is refused for {principal} ({message}); after the exit {stranded}, so the session stays running"
                    ),
                ),
                other => refused(other),
            })?;
    }
    Ok(())
}

/// The keeper principal's claim. The relaunch and the failure path present the
/// same secret under the same idempotency key.
async fn claim(state: &AppState, pool: &PgPool, intent_id: &str) -> Result<String, Refusal> {
    let receipt = akasha::restart_claim(
        pool,
        RestartClaimParams {
            intent_id: intent_id.to_owned(),
            claimant: CLAIMANT.into(),
            capability: secret(&state.config.room_dir, CLAIM_SECRET)?,
            idempotency_key: format!("athanor-host:claim:{intent_id}"),
        },
    )
    .await
    .map_err(refused)?;
    Ok(receipt.claim_token)
}

/// Record a requested intent for the exact target. Nothing is armed yet.
/// requesterSession is the logical session (conversation) the native authority
/// compares on verify; requesterInstance names the exact process.
#[allow(clippy::too_many_arguments)]
pub(super) async fn request(
    state: &AppState,
    command_id: Uuid,
    target: Uuid,
    conversation_id: &str,
    workspace: &str,
    reason: &str,
    fresh: bool,
) -> Result<String, Refusal> {
    // restart_request runs the native expiry sweep and the one-live-intent
    // fence itself; a pre-read here would refuse rows the sweep would clear.
    let pool = pool(state)?;
    let receipt = akasha::restart_request(
        pool,
        RestartRequestParams {
            harness: RestartHarness::Omp,
            workspace: workspace.to_owned(),
            mode: if fresh {
                RestartMode::Fresh
            } else {
                RestartMode::Resume
            },
            session_id: (!fresh).then(|| conversation_id.to_owned()),
            reason: reason.to_owned(),
            consent_source: RestartConsentSource::OperatorStandingPolicy,
            requester_room: state.config.room.clone(),
            requester_spirit: state.config.spirit.clone(),
            requester_session: conversation_id.to_owned(),
            requester_instance: Some(target.to_string()),
            capability: secret(&state.config.room_dir, REQUEST_SECRET)?,
            idempotency_key: format!("athanor-host:request:{command_id}"),
        },
    )
    .await
    .map_err(refused)?;
    Ok(receipt.intent_id)
}

/// Resume reopens a file; OMP writes it only after a real turn persists, so an
/// unsaved session would vanish into a failed successor.
pub(super) fn require_saved_session(session_file: &str) -> Result<(), Refusal> {
    match std::fs::File::open(session_file).and_then(|file| file.metadata()) {
        Ok(metadata) if metadata.is_file() && metadata.len() > 0 => Ok(()),
        _ => Err(Refusal::new(
            StatusCode::CONFLICT,
            "session_unsaved",
            "the session file is not on disk yet; resume needs one persisted turn",
        )),
    }
}
fn require_live_exit_deadline(deadline: Option<&str>) -> Result<(), Refusal> {
    if deadline
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .is_some_and(|deadline| deadline > Utc::now())
    {
        return Ok(());
    }
    Err(Refusal::new(
        StatusCode::CONFLICT,
        "restart_exit_expired",
        "the restart no longer authorizes this process to exit",
    ))
}

pub(super) async fn confirm_exit(state: &AppState, record: &RestartRecord) -> Result<(), Refusal> {
    let status = akasha::restart_status(
        pool(state)?,
        RestartStatusParams {
            workspace: record.workspace.clone(),
            intent_id: Some(record.intent_id.clone()),
        },
    )
    .await
    .map_err(refused)?;
    let intent = status
        .intent
        .filter(|intent| intent.state == protocol::restart::RestartState::Exiting)
        .ok_or_else(|| {
            Refusal::new(
                StatusCode::CONFLICT,
                "restart_not_exiting",
                "the restart no longer awaits this process exit",
            )
        })?;
    require_live_exit_deadline(intent.deadlines.exiting_deadline_at.as_deref())
}

/// Arm the exit after the target's own guards accepted the restart. A resume
/// re-checks the session file here, the last moment before the adapter exits.
pub(super) async fn arm(state: &AppState, record: &RestartRecord) -> Result<(), Refusal> {
    if !record.fresh {
        require_saved_session(&record.session_file)?;
    }
    // The last moment before the adapter exits: a secret rotated since the
    // request refuses here, before this process is told it may leave.
    require_relaunch_authority(state).await?;
    // A replayed acknowledge after a lost answer finds the arm already done.
    let current = akasha::restart_status(
        pool(state)?,
        RestartStatusParams {
            workspace: record.workspace.clone(),
            intent_id: Some(record.intent_id.clone()),
        },
    )
    .await
    .map_err(refused)?;
    if let Some(intent) = current
        .intent
        .filter(|intent| intent.state == protocol::restart::RestartState::Exiting)
    {
        require_live_exit_deadline(intent.deadlines.exiting_deadline_at.as_deref())?;
    } else {
        akasha::restart_transition(
            pool(state)?,
            RestartTransitionParams {
                intent_id: record.intent_id.clone(),
                claim_token: None,
                requester_session: Some(record.conversation_id.clone()),
                capability: Some(secret(&state.config.room_dir, EXIT_SECRET)?),
                to: RestartTransitionTarget::Exiting,
                detail: Some(record.reason.clone()),
            },
        )
        .await
        .map_err(refused)?;
    }
    if record.launcher == LauncherKind::Tern {
        let terminal = record
            .terminal
            .clone()
            .ok_or_else(|| Refusal::internal("the restart has no predecessor terminal"))?;
        tokio::task::spawn_blocking(move || super::launch::preserve_predecessor(&terminal))
            .await
            .map_err(|error| {
                Refusal::internal(format!("the pane-preservation task failed: {error}"))
            })??;
    }
    confirm_exit(state, record).await
}

/// A requested intent whose target refused never arms; claiming it lets the
/// Host close it as failed instead of leaving it to lapse silently.
pub(super) async fn fail(state: &AppState, record: &RestartRecord, detail: &str) {
    let Ok(pool) = pool(state) else { return };
    let token = match record.claim_token.clone() {
        Some(token) => token,
        None => match claim(state, pool, &record.intent_id).await {
            Ok(token) => token,
            Err(refusal) => {
                // Without a claim token nothing can fail the intent. Only a later
                // restart_request for this workspace sweeps it, and only once the
                // intent is past its stage deadline.
                tracing::warn!(
                    intent = %record.intent_id,
                    code = refusal.code,
                    error = %refusal.message,
                    "restart intent could not be claimed to fail it; it holds its workspace until a later restart request sweeps it past its stage deadline"
                );
                return;
            }
        },
    };
    if let Err(error) = akasha::restart_transition(
        pool,
        RestartTransitionParams {
            intent_id: record.intent_id.clone(),
            claim_token: Some(token),
            requester_session: None,
            capability: None,
            to: RestartTransitionTarget::Failed,
            detail: Some(detail.chars().take(512).collect()),
        },
    )
    .await
    {
        tracing::warn!(intent = %record.intent_id, %error, "restart intent could not be failed");
    }
}

async fn deadline(state: &AppState, record: &RestartRecord, relaunching: bool) -> DateTime<Utc> {
    let fallback = Utc::now()
        + chrono::Duration::seconds(if relaunching {
            akasha::RELAUNCHING_DEADLINE_SECS
        } else {
            akasha::EXITING_DEADLINE_SECS
        });
    let Ok(pool) = pool(state) else {
        return fallback;
    };
    let Ok(status) = akasha::restart_status(
        pool,
        RestartStatusParams {
            workspace: record.workspace.clone(),
            intent_id: Some(record.intent_id.clone()),
        },
    )
    .await
    else {
        return fallback;
    };
    status
        .intent
        .and_then(|intent| {
            if relaunching {
                intent.deadlines.relaunching_deadline_at
            } else {
                intent.deadlines.exiting_deadline_at
            }
        })
        .and_then(|at| DateTime::parse_from_rfc3339(&at).ok())
        .map(|at| at.with_timezone(&Utc))
        .unwrap_or(fallback)
}

async fn update(
    state: &AppState,
    restart_id: Uuid,
    change: impl FnOnce(&mut RestartRecord),
) -> Option<RestartRecord> {
    let mut registry = state.portable.lock().await;
    let record = registry.restarts.get_mut(&restart_id)?;
    change(record);
    record.updated_at = Utc::now();
    let record = record.clone();
    // A stage the journal cannot record must not drive further side effects.
    if let Err(error) = super::ledger::save(&state.config.state_dir, &registry) {
        tracing::error!(%error, "portable restart record could not be persisted; restart stopped");
        return None;
    }
    Some(record)
}

/// After the exit is armed: wait for the observed predecessor exit, then hand
/// the launch to the registered launcher. Keeper launchers relaunch themselves.
pub(super) async fn drive(state: AppState, restart_id: Uuid) {
    // The acknowledge committed AwaitingExit before spawning this task. Any
    // other stage means another driver, a settled record, or a Host that
    // restarted and loaded the record as failed: nothing to drive.
    let Some(record) = snapshot(&state, restart_id).await else {
        return;
    };
    if record.stage != RestartStage::AwaitingExit {
        return;
    }
    let exit_by = deadline(&state, &record, false).await;
    loop {
        let exited = {
            let registry = state.portable.lock().await;
            registry.process_exited(record.target)
        };
        match exited {
            Some(true) => break,
            // Unknown is not exited: keep looking until the exiting deadline.
            Some(false) | None => {}
        }
        if Utc::now() >= exit_by {
            finish_failed(
                &state,
                restart_id,
                "the predecessor did not leave by the exiting deadline; nothing was killed",
            )
            .await;
            return;
        }
        if state.cancellation.is_cancelled() {
            return;
        }
        tokio::time::sleep(WATCH_TICK).await;
    }
    {
        let mut registry = state.portable.lock().await;
        registry.retire(record.target, "restarted");
    }
    if record.launcher != LauncherKind::Tern {
        // The integrated keeper saw the armed 87 exit and owns claim/relaunch/verify.
        update(&state, restart_id, |record| {
            record.stage = RestartStage::HandedToKeeper;
        })
        .await;
        return;
    }
    if let Err(refusal) = relaunch(&state, restart_id).await {
        finish_failed(&state, restart_id, &refusal.message).await;
    }
}

/// Every external step here follows a committed checkpoint. A checkpoint the
/// journal refuses ends the restart before the next step: the claim before the
/// relaunching transition, the relaunching record before the tab opens.
async fn relaunch(state: &AppState, restart_id: Uuid) -> Result<(), Refusal> {
    let record = snapshot(state, restart_id)
        .await
        .ok_or_else(|| Refusal::internal("restart record vanished"))?;
    let pool = pool(state)?;
    let token = claim(state, pool, &record.intent_id).await?;
    update(state, restart_id, |record| {
        record.claim_token = Some(token.clone())
    })
    .await
    .ok_or_else(|| Refusal::internal("the journal could not record the claim; no relaunch"))?;
    let relaunching = akasha::restart_transition(
        pool,
        RestartTransitionParams {
            intent_id: record.intent_id.clone(),
            claim_token: Some(token),
            requester_session: None,
            capability: None,
            to: RestartTransitionTarget::Relaunching,
            detail: None,
        },
    )
    .await
    .map_err(refused)?;
    let proof = relaunching
        .successor_proof
        .ok_or_else(|| Refusal::internal("relaunching minted no successor proof"))?;
    let terminal = record
        .terminal
        .clone()
        .ok_or_else(|| Refusal::internal("tern restart has no terminal identity"))?;
    let launch = record
        .launch
        .clone()
        .ok_or_else(|| Refusal::internal("tern restart has no launch command"))?;
    let relaunch_by = deadline(state, &record, true).await;
    // Committed before the tab opens: an uncommitted relaunch opens nothing.
    update(state, restart_id, |record| {
        record.stage = RestartStage::Relaunching;
        record.successor_proof = Some(proof);
        record.relaunch_by = Some(relaunch_by);
    })
    .await
    .ok_or_else(|| {
        Refusal::internal("the journal could not record the relaunch; no tab was opened")
    })?;
    let workspace = record.workspace.clone();
    // Fresh starts a new conversation; resume reopens the exact session file.
    let resume = (!record.fresh).then(|| record.session_file.clone());
    let config = state.config.clone();
    let opened = tokio::task::spawn_blocking(move || {
        super::launch::new_tab(&config, &terminal, &workspace, &launch, resume.as_deref())
    })
    .await
    .map_err(|error| Refusal::internal(format!("tern launch task failed: {error}")))??;
    // The successor is verified only against the exact pane Tern returned.
    let block = opened.block.clone();
    update(state, restart_id, |record| {
        record.expected_pane = Some(block);
    })
    .await
    .ok_or_else(|| {
        Refusal::internal(format!(
            "the journal could not record the opened pane (tern session {} tab {} block {}); the successor is not verified",
            opened.session, opened.tab, opened.block
        ))
    })?;
    loop {
        tokio::time::sleep(WATCH_TICK).await;
        let Some(current) = snapshot(state, restart_id).await else {
            return Ok(());
        };
        if current.stage != RestartStage::Relaunching {
            return Ok(());
        }
        // The successor may have attached before the launch answer arrived.
        {
            let mut registry = state.portable.lock().await;
            let early = registry
                .instances
                .iter()
                .find(|(id, instance)| {
                    instance.terminal.as_ref().is_some_and(|terminal| {
                        successor_matches(
                            &current,
                            **id,
                            &instance.conversation_id,
                            &instance.session_file,
                            &instance.workspace,
                            terminal,
                        )
                    })
                })
                .map(|(id, instance)| {
                    (
                        *id,
                        instance.conversation_id.clone(),
                        instance.session_file.clone(),
                        instance.workspace.clone(),
                        instance.terminal.clone(),
                    )
                });
            if let Some((id, conversation, file, workspace, terminal)) = early {
                verify_successor(
                    state,
                    &mut registry,
                    id,
                    &conversation,
                    &file,
                    &workspace,
                    terminal.as_ref(),
                )
                .await?;
                if let Some(predecessor) = registry
                    .restarts
                    .get(&restart_id)
                    .map(|record| record.target)
                {
                    registry.retire(predecessor, "restarted");
                }
                return Ok(());
            }
        }
        if Utc::now() >= relaunch_by {
            return Err(Refusal::internal(
                "no successor attached from the new Tern tab before the relaunching deadline",
            ));
        }
        if state.cancellation.is_cancelled() {
            return Ok(());
        }
    }
}

async fn snapshot(state: &AppState, restart_id: Uuid) -> Option<RestartRecord> {
    state
        .portable
        .lock()
        .await
        .restarts
        .get(&restart_id)
        .cloned()
}

/// Tern prints ids as JSON numbers today; both spellings read as the same text.
pub(super) fn id_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) if number.is_u64() => Some(number.to_string()),
        _ => None,
    }
}

/// The record fails in the journal and the intent fails in the authority. The
/// journal may be the very thing that broke, so the intent settles either way:
/// failing it opens nothing, and it frees the workspace now instead of at the
/// stage deadline.
async fn finish_failed(state: &AppState, restart_id: Uuid, detail: &str) {
    let committed = update(state, restart_id, |record| {
        record.stage = RestartStage::Failed;
        record.detail = Some(detail.to_owned());
    })
    .await;
    let record = match committed {
        Some(record) => Some(record),
        None => snapshot(state, restart_id).await,
    };
    if let Some(record) = record {
        fail(state, &record, detail).await;
    }
}

/// The successor is exactly the process Tern started in the new pane: a new
/// instance, same workspace and Tern session, pane equal to the returned
/// block. Resume also keeps the conversation and file; fresh must not.
pub(super) fn successor_matches(
    record: &RestartRecord,
    successor: Uuid,
    conversation_id: &str,
    session_file: &str,
    workspace: &str,
    terminal: &Terminal,
) -> bool {
    let conversation = if record.fresh {
        record.conversation_id != conversation_id
    } else {
        record.conversation_id == conversation_id && record.session_file == session_file
    };
    record.stage == RestartStage::Relaunching
        && conversation
        && record.workspace == workspace
        && record.target != successor
        && record
            .terminal
            .as_ref()
            .is_some_and(|own| own.session == terminal.session)
        && record.expected_pane.as_deref() == Some(terminal.pane.as_str())
}

/// Verify against the native authority with the successor's logical session.
pub(super) async fn verify_successor(
    state: &AppState,
    registry: &mut Registry,
    successor: Uuid,
    conversation_id: &str,
    session_file: &str,
    workspace: &str,
    terminal: Option<&Terminal>,
) -> Result<Option<Value>, Refusal> {
    let Some(terminal) = terminal else {
        return Ok(None);
    };
    let Some((restart_id, record)) = registry.restarts.iter().find(|(_, record)| {
        successor_matches(
            record,
            successor,
            conversation_id,
            session_file,
            workspace,
            terminal,
        )
    }) else {
        return Ok(None);
    };
    let restart_id = *restart_id;
    let record = record.clone();
    let proof = record
        .successor_proof
        .clone()
        .ok_or_else(|| Refusal::internal("relaunching restart holds no successor proof"))?;
    akasha::restart_verify(
        pool(state)?,
        RestartVerifyParams {
            intent_id: record.intent_id.clone(),
            successor_session: conversation_id.to_owned(),
            successor_proof: proof,
            room: state.config.room.clone(),
            spirit: state.config.spirit.clone(),
            capability: secret(&state.config.room_dir, VERIFY_SECRET)?,
        },
    )
    .await
    .map_err(refused)?;
    if let Some(stored) = registry.restarts.get_mut(&restart_id) {
        stored.stage = RestartStage::Verified;
        stored.successor = Some(successor);
        stored.successor_proof = None;
        stored.updated_at = Utc::now();
    }
    Ok(Some(successor_notice(restart_id, &record)))
}

pub(super) fn successor_notice(restart_id: Uuid, record: &RestartRecord) -> Value {
    serde_json::json!({
        "restartId": restart_id,
        "reason": record.reason,
        "predecessorInstanceId": record.target,
        "mode": if record.fresh { "fresh" } else { "resume" },
    })
}

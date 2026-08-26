use crate::clock::{Deadline, deadline, house_deadline};
use crate::config::KeeperConfig;
use crate::decide::{
    ExitingAction, RelaunchAction, StatusStep, VerifyWatch, armed_exit_hint, exiting_action,
    relaunch_action, status_step, verify_watch,
};
use crate::protocol::{
    EXITING_DEADLINE_SECS, METHOD_RESTART_CLAIM, METHOD_RESTART_STATUS, METHOD_RESTART_TRANSITION,
    ProtocolErrorBody, RestartClaimParams, RestartClaimReceipt, RestartMode, RestartState,
    RestartStatusIntent, RestartStatusParams, RestartStatusReceipt, RestartTransitionParams,
    RestartTransitionReceipt, RestartTransitionTarget,
};
use crate::resolve::resolve_substrate_exe;
use crate::session::{Answer, SubstrateSession};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const CHILD_POLL: Duration = Duration::from_millis(200);
/// The relaunching stage is seconds-scale by contract, so the keeper looks for
/// the successor's verify often, on the substrate session it already holds open.
const VERIFY_POLL: Duration = Duration::from_secs(1);
const UNKNOWN_EXIT_CODE: i32 = -1;
const RESTART_INTENT_ENV: &str = "ATHANOR_RESTART_INTENT_ID";
const RESTART_SUCCESSOR_PROOF_ENV: &str = "ATHANOR_RESTART_SUCCESSOR_PROOF";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Stopped { exit_code: i32 },
    Refused { message: String },
    Failed { message: String },
}

/// Where one relaunch stage ended.
enum Relaunched {
    /// The successor is running and the House saw it verify.
    Verified(Child),
    /// The loop is over: a refusal, or the retry budget spent.
    Stopped(Outcome),
}

/// Why one relaunch attempt produced no verified successor.
enum Attempt {
    Verified(Child),
    Failed(String),
}
struct RestartLaunch<'a> {
    intent: &'a RestartStatusIntent,
    successor_proof: &'a str,
}

pub fn run(config: &KeeperConfig) -> Result<Outcome> {
    config.validate()?;
    let mut child = spawn_omp(config, None).context("omp could not start")?;
    loop {
        let exit_code = watch_child(config, &mut child)?;
        report_exit(exit_code);

        // # enough: one substrate child for each ask; it keeps the release pointer fresh (census 1.9)
        let executable = resolve_substrate_exe(&config.program_root)
            .context("the current substrate could not be resolved")?;
        let mut session = SubstrateSession::start(&executable, &config.state_root)?;

        let pending = match ask_status(&mut session, config, None)? {
            Ok(pending) => pending,
            Err(refusal) => {
                session.close()?;
                return Ok(refusal_outcome(&refusal));
            }
        };
        let pending = match pending {
            Some(pending) if status_step(Some(pending.state)) == StatusStep::Claim => pending,
            Some(pending) => {
                session.close()?;
                println!(
                    "omp-keeper: intent {} is {} for {}; nothing to relaunch",
                    pending.intent_id,
                    pending.state.as_str(),
                    config.workspace
                );
                return Ok(Outcome::Stopped { exit_code });
            }
            None => {
                session.close()?;
                println!(
                    "omp-keeper: no restart intent for {}; the keeper exits",
                    config.workspace
                );
                return Ok(Outcome::Stopped { exit_code });
            }
        };

        let capability = config
            .read_capability()
            .context("the keeper restart_claim capability could not be read")?;
        let claim: RestartClaimReceipt = match session.call(
            METHOD_RESTART_CLAIM,
            &RestartClaimParams {
                intent_id: pending.intent_id.clone(),
                claimant: config.claimant.clone(),
                capability: capability.expose().to_string(),
                idempotency_key: claim_key(&pending.intent_id, &config.claimant),
            },
        )? {
            Answer::Ok(result) => result,
            Answer::Refused(refusal) => {
                session.close()?;
                return Ok(refusal_outcome(&refusal));
            }
        };
        println!(
            "omp-keeper: claimed intent {} at epoch {}",
            pending.intent_id, claim.claim_epoch
        );

        match relaunch(config, &mut session, &pending, &claim)? {
            Relaunched::Verified(successor) => {
                adopt_verified_successor(&mut child, successor, session.close());
            }
            Relaunched::Stopped(outcome) => {
                session.close()?;
                return Ok(outcome);
            }
        }
    }
}

/// Bring omp back and hold the stage open until the House has seen the successor
/// verify. An attempt fails for either reason — omp would not start, or it
/// started and never proved itself inside the window — and both spend from the
/// same budget the claim handed over.
fn relaunch(
    config: &KeeperConfig,
    session: &mut SubstrateSession,
    pending: &RestartStatusIntent,
    claim: &RestartClaimReceipt,
) -> Result<Relaunched> {
    let mut attempts = 0;
    // The House's own relaunching deadline, carried across attempts so a read
    // that fails cannot quietly widen the window a silent successor gets.
    let mut last_window: Option<Deadline> = None;
    loop {
        attempts += 1;
        // One relaunching transition per attempt. The intent row counts
        // relaunch_attempts and mints a fresh relaunching_deadline_at on every
        // relaunching transition (house-substrate/src/restart/mod.rs:518-545),
        // so a retry runs inside the House's own new window instead of a second
        // clock invented here.
        let detail = (attempts > 1).then(|| format!("relaunch attempt {attempts}"));
        let successor_proof = match transition(
            session,
            pending,
            &claim.claim_token,
            RestartTransitionTarget::Relaunching,
            detail,
        )? {
            Answer::Ok(receipt) => match receipt.state {
                RestartState::Relaunching => {
                    println!(
                        "omp-keeper: intent {} is relaunching (attempt {attempts})",
                        pending.intent_id
                    );
                    receipt.successor_proof
                }
                // the House spends the budget itself when a keeper asks once too often
                RestartState::Failed => {
                    return Ok(Relaunched::Stopped(Outcome::Failed {
                        message: format!(
                            "omp-keeper: the House spent the relaunch budget for intent {}; it is failed:relaunching and omp is not running",
                            pending.intent_id
                        ),
                    }));
                }
                other => bail!(
                    "a relaunching transition answered {}, which the keeper cannot act on",
                    other.as_str()
                ),
            },
            Answer::Refused(refusal) => {
                return Ok(Relaunched::Stopped(refusal_outcome(&refusal)));
            }
        };

        let failure = match successor_proof {
            Some(ref proof) => {
                match attempt_relaunch(config, session, pending, proof, &mut last_window)? {
                    Attempt::Verified(child) => {
                        println!(
                            "omp-keeper: the House saw the successor verify intent {}",
                            pending.intent_id
                        );
                        return Ok(Relaunched::Verified(child));
                    }
                    Attempt::Failed(failure) => failure,
                }
            }
            None => "the relaunching transition returned no successor proof".to_owned(),
        };
        eprintln!("omp-keeper: relaunch attempt {attempts} failed: {failure}");

        if relaunch_action(attempts, claim.stage_deadlines.relaunch_attempt_limit)
            == RelaunchAction::Fail
        {
            let detail = format!("{attempts} relaunch attempts failed; the last: {failure}");
            let answer = transition(
                session,
                pending,
                &claim.claim_token,
                RestartTransitionTarget::Failed,
                Some(detail.clone()),
            )?;
            if let Answer::Refused(refusal) = answer {
                return Ok(Relaunched::Stopped(refusal_outcome(&refusal)));
            }
            return Ok(Relaunched::Stopped(Outcome::Failed {
                message: format!(
                    "omp-keeper: {detail}; intent {} is failed:relaunching and omp is not running",
                    pending.intent_id
                ),
            }));
        }
    }
}

/// What one watch of a relaunched child concluded.
enum Watched {
    Verified,
    /// The child is running or gone, but the House never proved it. Carries the
    /// sentence the operator and the intent's `failed` detail both get.
    Unproven(String),
}

/// One attempt: start omp, then hold it against the House's relaunching window
/// until the successor verifies. A successor that runs but never verifies is not
/// the session Sol asked for, so it does not outlive its deadline.
fn attempt_relaunch(
    config: &KeeperConfig,
    session: &mut SubstrateSession,
    pending: &RestartStatusIntent,
    successor_proof: &str,
    last_window: &mut Option<Deadline>,
) -> Result<Attempt> {
    let mut child = match spawn_omp(
        config,
        Some(RestartLaunch {
            intent: pending,
            successor_proof,
        }),
    ) {
        Ok(child) => child,
        Err(error) => return Ok(Attempt::Failed(format!("{error:#}"))),
    };
    // Past this line the keeper owns a live omp, so no error may leave this
    // function. An escaping `?` drops the Child without killing it: on Windows
    // that leaves Sol's omp running with nothing watching it and the intent
    // stuck in relaunching, which is the one shape the House cannot clean up.
    // Every sad path below reaches the same kill and reports a failed attempt.
    match watch_relaunched(session, config, pending, &mut child, last_window) {
        Ok(Watched::Verified) => Ok(Attempt::Verified(child)),
        Ok(Watched::Unproven(reason)) => {
            leave_no_child(&mut child);
            Ok(Attempt::Failed(reason))
        }
        Err(error) => {
            leave_no_child(&mut child);
            Ok(Attempt::Failed(format!(
                "the keeper lost the House mid-relaunch: {error:#}"
            )))
        }
    }
}

fn watch_relaunched(
    session: &mut SubstrateSession,
    config: &KeeperConfig,
    pending: &RestartStatusIntent,
    child: &mut Child,
    last_window: &mut Option<Deadline>,
) -> Result<Watched> {
    let Some(window) = relaunching_window(session, config, pending, last_window)? else {
        return Ok(Watched::Unproven(format!(
            "the House has named no relaunching deadline for intent {}, so there is no window to wait inside",
            pending.intent_id
        )));
    };
    loop {
        match observe(session, config, pending)? {
            VerifyWatch::Verified => return Ok(Watched::Verified),
            // Finished, but the House never said verified. Which end it reached
            // is not ours to guess, so it counts as no verify at all.
            VerifyWatch::Terminal => {
                return Ok(Watched::Unproven(format!(
                    "intent {} is finished without a verify the House would confirm",
                    pending.intent_id
                )));
            }
            VerifyWatch::Waiting => {}
        }
        if window.has_passed(Utc::now()) {
            return Ok(Watched::Unproven(format!(
                "the successor did not verify by {} ({})",
                window.at().to_rfc3339(),
                window.source()
            )));
        }
        if child
            .try_wait()
            .context("the relaunched omp child could not be inspected")?
            .is_some()
        {
            // verified and exited in one breath is still verified, so ask once more
            return Ok(match observe(session, config, pending)? {
                VerifyWatch::Verified => Watched::Verified,
                _ => Watched::Unproven("the successor exited before it verified".to_string()),
            });
        }
        std::thread::sleep(VERIFY_POLL);
    }
}

/// The window this attempt may wait inside. Only the House sets one. When the
/// read fails or carries no instant, the deadline the House last published
/// stands: minting a fresh one here would hand a silent successor more time than
/// the House ever allowed, and that is the one direction this stage must not
/// fail. `claim.stageDeadlines.relaunchingSecs` is deliberately not a fallback.
fn relaunching_window(
    session: &mut SubstrateSession,
    config: &KeeperConfig,
    pending: &RestartStatusIntent,
    last_window: &mut Option<Deadline>,
) -> Result<Option<Deadline>> {
    let published = match ask_status(session, config, Some(&pending.intent_id)) {
        Ok(Ok(Some(intent))) if intent.intent_id == pending.intent_id => {
            intent.deadlines.relaunching_deadline_at
        }
        Ok(Ok(_)) => None,
        Ok(Err(refusal)) => {
            eprintln!(
                "omp-keeper: the House refused the relaunching window read ({}: {}); the deadline it last published stands",
                refusal.code, refusal.message
            );
            None
        }
        Err(error) => {
            eprintln!(
                "omp-keeper: the relaunching window read failed ({error:#}); the deadline it last published stands"
            );
            None
        }
    };
    match published {
        Some(published) => {
            let window = house_deadline(&published)?;
            *last_window = Some(window);
            Ok(Some(window))
        }
        None => Ok(*last_window),
    }
}

/// One look at our own intent, by id. A refused read decides nothing: it is
/// neither a verify nor a terminal sighting, so the window is left to end the
/// wait rather than this answer.
fn observe(
    session: &mut SubstrateSession,
    config: &KeeperConfig,
    pending: &RestartStatusIntent,
) -> Result<VerifyWatch> {
    match ask_status(session, config, Some(&pending.intent_id))? {
        Ok(observed) => Ok(verify_watch(&pending.intent_id, observed.as_ref())),
        Err(refusal) => {
            eprintln!(
                "omp-keeper: the House refused a verification read ({}: {})",
                refusal.code, refusal.message
            );
            Ok(VerifyWatch::Waiting)
        }
    }
}

fn report_exit(exit_code: i32) {
    if armed_exit_hint(Some(exit_code)) {
        println!("omp-keeper: omp exited {exit_code} (armed exit hint); asking the House");
    } else {
        println!("omp-keeper: omp exited {exit_code}; asking the House");
    }
}

fn refusal_outcome(refusal: &ProtocolErrorBody) -> Outcome {
    if refusal.is_storm_refusal() {
        return Outcome::Refused {
            message: format!(
                "omp-keeper: the House refused another restart for now ({}). omp is not running; start it yourself when you want it back.",
                refusal.message
            ),
        };
    }
    Outcome::Refused {
        message: format!(
            "omp-keeper: the House refused the restart ({}: {}). omp is not running; start it yourself when you want it back.",
            refusal.code, refusal.message
        ),
    }
}

fn claim_key(intent_id: &str, claimant: &str) -> String {
    format!("{claimant}:claim:{intent_id}")
}

fn transition(
    session: &mut SubstrateSession,
    pending: &RestartStatusIntent,
    claim_token: &str,
    to: RestartTransitionTarget,
    detail: Option<String>,
) -> Result<Answer<RestartTransitionReceipt>> {
    session.call(
        METHOD_RESTART_TRANSITION,
        &RestartTransitionParams {
            intent_id: pending.intent_id.clone(),
            // the keeper's transitions always carry the minted token; only the adapter's exit is tokenless
            claim_token: Some(claim_token.to_string()),
            // exiting-arm fields; the keeper never uses that door
            requester_session: None,
            capability: None,
            to,
            detail,
        },
    )
}

/// `intent_id` absent asks the workspace question — is there anything pending —
/// which is the live-states-only read. `intent_id` present asks about that one
/// intent in whatever state it reached, which is the only read that can show a
/// `verified` successor.
fn ask_status(
    session: &mut SubstrateSession,
    config: &KeeperConfig,
    intent_id: Option<&str>,
) -> Result<std::result::Result<Option<RestartStatusIntent>, ProtocolErrorBody>> {
    let params = RestartStatusParams {
        workspace: config.workspace.clone(),
        intent_id: intent_id.map(str::to_string),
    };
    match session.call::<_, RestartStatusReceipt>(METHOD_RESTART_STATUS, &params)? {
        Answer::Ok(receipt) => Ok(Ok(receipt.intent)),
        Answer::Refused(refusal) => Ok(Err(refusal)),
    }
}

fn spawn_omp(config: &KeeperConfig, restart: Option<RestartLaunch<'_>>) -> Result<Child> {
    // Console-inheriting on purpose: Sol watches this child, so no detach and no
    // hidden window. The initial child drops stale restart state. A successor
    // receives the exact attempt proof, intent, and session selector the House
    // recorded.
    let mut command = Command::new(config.program());
    command
        .env_remove(RESTART_INTENT_ENV)
        .env_remove(RESTART_SUCCESSOR_PROOF_ENV)
        .args(config.program_args())
        .current_dir(&config.workspace)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some(restart) = restart {
        command
            .env(RESTART_INTENT_ENV, &restart.intent.intent_id)
            .env(RESTART_SUCCESSOR_PROOF_ENV, restart.successor_proof);
        if restart.intent.mode == RestartMode::Resume {
            let session_id = restart.intent.session_id.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "resume intent {} carries no session id",
                    restart.intent.intent_id
                )
            })?;
            command.arg("--resume").arg(session_id);
        }
    }
    command
        .spawn()
        .with_context(|| format!("omp could not start: {}", config.program()))
}

fn watch_child(config: &KeeperConfig, child: &mut Child) -> Result<i32> {
    let mut last_poll = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .context("omp child could not be inspected")?
        {
            return Ok(status.code().unwrap_or(UNKNOWN_EXIT_CODE));
        }
        std::thread::sleep(CHILD_POLL);
        if config.watch_interval_secs == 0
            || last_poll.elapsed() < Duration::from_secs(config.watch_interval_secs)
        {
            continue;
        }
        last_poll = Instant::now();
        let watched = match watch_status(config) {
            Ok(watched) => watched,
            Err(error) => {
                // a failed poll never touches Sol's live session
                eprintln!("omp-keeper: restart_status poll failed: {error:#}");
                continue;
            }
        };
        // The intent carries the instant it must be gone by, so the keeper reads
        // that instant instead of timing the stage itself. A keeper that started
        // after the adapter armed is already late on this first look.
        let state = watched.as_ref().map(|pending| pending.state);
        let exiting_deadline = match watched.as_ref() {
            Some(pending) if pending.state == RestartState::Exiting => Some(deadline(
                pending.deadlines.exiting_deadline_at.as_deref(),
                Utc::now(),
                EXITING_DEADLINE_SECS,
            )?),
            _ => None,
        };
        if exiting_action(state, exiting_deadline, Utc::now()) == ExitingAction::Kill {
            let passed = exiting_deadline.expect("a kill decision carries the deadline it read");
            println!(
                "omp-keeper: omp did not leave by {} ({}); killing it",
                passed.at().to_rfc3339(),
                passed.source()
            );
            return kill_child(child);
        }
    }
}

/// Kill and reap. An unreaped child is a handle the keeper would hold forever,
/// and a child that died between the look and the kill is already what we wanted.
fn kill_child(child: &mut Child) -> Result<i32> {
    if let Err(error) = child.kill() {
        if child
            .try_wait()
            .context("omp child could not be inspected")?
            .is_none()
        {
            return Err(error).context("omp child could not be killed");
        }
    }
    let status = child
        .wait()
        .context("killed omp child could not be reaped")?;
    Ok(status.code().unwrap_or(UNKNOWN_EXIT_CODE))
}

/// Make sure no omp survives a failed attempt. This is already the sad path, so
/// a kill the keeper cannot even report must not stop it from reaching `failed`:
/// that transition is what the House and the next keeper are waiting on.
fn leave_no_child(child: &mut Child) {
    if let Err(error) = kill_child(child) {
        eprintln!("omp-keeper: the relaunched omp child could not be put down: {error:#}");
    }
}

/// Transfer the verified child before cleaning up its disposable substrate.
/// Cleanup failure stays visible without breaking the keeper's ownership.
fn adopt_verified_successor<T>(current: &mut T, successor: T, close_result: Result<()>) {
    *current = successor;
    if let Err(error) = close_result {
        eprintln!(
            "omp-keeper: the restart is verified, but its substrate session did not close cleanly; continuing to supervise omp: {error:#}"
        );
    }
}

fn watch_status(config: &KeeperConfig) -> Result<Option<RestartStatusIntent>> {
    let executable = resolve_substrate_exe(&config.program_root)?;
    let mut session = SubstrateSession::start(&executable, &config.state_root)?;
    let answer = ask_status(&mut session, config, None);
    session.close()?;
    match answer? {
        Ok(pending) => Ok(pending),
        Err(refusal) => bail!("{}: {}", refusal.code, refusal.message),
    }
}

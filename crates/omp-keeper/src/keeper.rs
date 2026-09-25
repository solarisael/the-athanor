use crate::clock::{Deadline, deadline, house_deadline};
use crate::config::{ConsoleMode, KeeperConfig};
use crate::control::{KeeperObserver, NoopObserver, StopControl};
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
use crate::session::{Answer, HouseLine};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use interactive_process::{InteractiveChild, InteractiveCommand};
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const CHILD_POLL: Duration = Duration::from_millis(200);
/// The relaunching stage is seconds-scale by contract, so the keeper looks for
/// the successor's verify often, on the substrate session it already holds open.
const VERIFY_POLL: Duration = Duration::from_secs(1);
/// How long the keeper rests between asks while the House cannot answer.
const HOUSE_RETRY: Duration = Duration::from_secs(2);
/// How often a long silence is told again on the console.
const SILENCE_RETELL: Duration = Duration::from_secs(60);
const UNKNOWN_EXIT_CODE: i32 = -1;
const RESTART_INTENT_ENV: &str = "ATHANOR_RESTART_INTENT_ID";
const RESTART_SUCCESSOR_PROOF_ENV: &str = "ATHANOR_RESTART_SUCCESSOR_PROOF";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Stopped { exit_code: i32 },
    Refused { message: String },
    Failed { message: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum ControlledOutcome {
    Completed(Outcome),
    Stopped,
}

struct Runtime<'a> {
    control: &'a StopControl,
    observer: &'a dyn KeeperObserver,
    console: ConsoleMode,
}

enum OmpChild {
    Inherited(Child),
    Interactive(InteractiveChild),
}

impl OmpChild {
    fn id(&self) -> u32 {
        match self {
            Self::Inherited(child) => child.id(),
            Self::Interactive(child) => child.id(),
        }
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match self {
            Self::Inherited(child) => child.try_wait(),
            Self::Interactive(child) => child.try_wait(),
        }
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        match self {
            Self::Inherited(child) => child.wait(),
            Self::Interactive(child) => child.wait(),
        }
    }

    fn kill(&mut self) -> io::Result<()> {
        match self {
            Self::Inherited(child) => child.kill(),
            Self::Interactive(child) => child.terminate(),
        }
    }
}

enum Relaunched {
    /// The successor is running and the House saw it verify.
    Verified(OmpChild),
    /// The loop is over: a refusal, or the retry budget spent.
    Completed(Outcome),
    Stopped,
}

enum Attempt {
    Verified(OmpChild),
    Failed(String),
    Stopped,
}
struct RestartLaunch<'a> {
    intent: &'a RestartStatusIntent,
    successor_proof: &'a str,
}

pub fn lock_path(config_path: &Path) -> PathBuf {
    config_path.with_file_name("omp-keeper.lock")
}

#[cfg(windows)]
pub fn try_hold_lock(config_path: &Path) -> io::Result<Option<File>> {
    use std::os::windows::fs::OpenOptionsExt;

    match OpenOptions::new()
        .create(true)
        .write(true)
        .share_mode(0)
        .open(lock_path(config_path))
    {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.raw_os_error() == Some(32) => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(not(windows))]
pub fn try_hold_lock(config_path: &Path) -> io::Result<Option<File>> {
    // Keeper exclusion uses Windows sharing rules; other platforms only create the file.
    OpenOptions::new()
        .create(true)
        .write(true)
        .open(lock_path(config_path))
        .map(Some)
}

pub fn run(config: &KeeperConfig, config_path: &Path) -> Result<Outcome> {
    let Some(_lock) = try_hold_lock(config_path)
        .with_context(|| format!("hold keeper lock {}", lock_path(config_path).display()))?
    else {
        return Ok(Outcome::Refused {
            message: format!("another keeper already holds {}", lock_path(config_path).display()),
        });
    };
    let control = StopControl::new();
    let observer = NoopObserver;
    match run_controlled(config, ConsoleMode::Inherit, &control, &observer)? {
        ControlledOutcome::Completed(outcome) => Ok(outcome),
        ControlledOutcome::Stopped => unreachable!("the standalone keeper has no stop requester"),
    }
}

#[cfg(all(test, windows))]
mod lock_tests {
    use super::*;

    #[test]
    fn lock_excludes_another_handle_until_dropped() {
        let temp = tempfile::tempdir().expect("temp dir");
        let config_path = temp.path().join("omp-keeper.json");
        let held = try_hold_lock(&config_path).unwrap().unwrap();
        assert!(try_hold_lock(&config_path).unwrap().is_none());
        let config: KeeperConfig = serde_json::from_value(serde_json::json!({
            "ompLaunch": ["must-not-spawn.exe"],
            "workspace": temp.path(),
            "programRoot": temp.path(),
            "stateRoot": temp.path()
        })).unwrap();
        assert!(matches!(run(&config, &config_path).unwrap(), Outcome::Refused { .. }));
        drop(held);
        assert!(try_hold_lock(&config_path).unwrap().is_some());
    }
}

pub fn run_controlled(
    config: &KeeperConfig,
    console: ConsoleMode,
    control: &StopControl,
    observer: &dyn KeeperObserver,
) -> Result<ControlledOutcome> {
    config.validate()?;
    let runtime = Runtime {
        control,
        observer,
        console,
    };
    let mut child = spawn_omp(config, None, &runtime).context("omp could not start")?;
    loop {
        let exit_code = match watch_child(config, &mut child, &runtime)? {
            Some(exit_code) => exit_code,
            None => return Ok(ControlledOutcome::Stopped),
        };
        report_exit(exit_code);
        if control.is_stop_requested() {
            return Ok(ControlledOutcome::Stopped);
        }

        // # enough: one substrate child for each restart; one whose line breaks is replaced (census 1.9)
        let mut house = HouseLine::new(&config.program_root, &config.state_root);

        let heard = if armed_exit_hint(Some(exit_code)) {
            // An armed exit is a restart the House already agreed to. A House
            // that cannot be asked right now has not changed its mind, and the
            // House alone decides when the intent is dead: it refuses a lapsed
            // one when asked. So the keeper waits here instead of walking away.
            until_answered(&mut house, &runtime, |house| ask_status(house, config, None))?
        } else {
            match ask_status(&mut house, config, None)? {
                Answer::Ok(pending) => Heard::Answered(pending),
                Answer::Refused(refusal) => Heard::Refused(refusal),
                Answer::Unreachable(reason) => {
                    house.close()?;
                    return Ok(ControlledOutcome::Completed(Outcome::Failed {
                        message: format!(
                            "omp-keeper: omp exited {exit_code} without arming a restart, and the House could not be asked whether one was pending ({reason}); nothing was relaunched"
                        ),
                    }));
                }
            }
        };
        let pending = match heard {
            Heard::Answered(pending) => pending,
            Heard::Refused(refusal) => {
                house.close()?;
                return Ok(ControlledOutcome::Completed(refusal_outcome(&refusal)));
            }
            Heard::Stopped => {
                house.close()?;
                return Ok(ControlledOutcome::Stopped);
            }
        };
        let pending = match pending {
            Some(pending) if status_step(Some(pending.state)) == StatusStep::Claim => pending,
            Some(pending) => {
                house.close()?;
                println!(
                    "omp-keeper: intent {} is {} for {}; nothing to relaunch",
                    pending.intent_id,
                    pending.state.as_str(),
                    config.workspace
                );
                return Ok(ControlledOutcome::Completed(Outcome::Stopped { exit_code }));
            }
            None => {
                house.close()?;
                println!(
                    "omp-keeper: no restart intent for {}; the keeper exits",
                    config.workspace
                );
                return Ok(ControlledOutcome::Completed(Outcome::Stopped { exit_code }));
            }
        };
        if control.is_stop_requested() {
            house.close()?;
            return Ok(ControlledOutcome::Stopped);
        }

        let capability = config
            .read_capability()
            .context("the keeper restart_claim capability could not be read")?;
        let claim_params = RestartClaimParams {
            intent_id: pending.intent_id.clone(),
            claimant: config.claimant.clone(),
            capability: capability.expose().to_string(),
            idempotency_key: claim_key(&pending.intent_id, &config.claimant),
        };
        // The claim carries its idempotency key, so asking again after silence
        // cannot mint a second claim.
        let claim: RestartClaimReceipt = match until_answered(&mut house, &runtime, |house| {
            house.call(METHOD_RESTART_CLAIM, &claim_params)
        })? {
            Heard::Answered(claim) => claim,
            Heard::Refused(refusal) => {
                house.close()?;
                return Ok(ControlledOutcome::Completed(refusal_outcome(&refusal)));
            }
            Heard::Stopped => {
                house.close()?;
                return Ok(ControlledOutcome::Stopped);
            }
        };
        println!(
            "omp-keeper: claimed intent {} at epoch {}",
            pending.intent_id, claim.claim_epoch
        );

        match relaunch(config, &mut house, &pending, &claim, &runtime)? {
            Relaunched::Verified(successor) => {
                adopt_verified_successor(&mut child, successor, house.close());
            }
            Relaunched::Completed(outcome) => {
                house.close()?;
                return Ok(ControlledOutcome::Completed(outcome));
            }
            Relaunched::Stopped => {
                house.close()?;
                return Ok(ControlledOutcome::Stopped);
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
    house: &mut HouseLine<'_>,
    pending: &RestartStatusIntent,
    claim: &RestartClaimReceipt,
    runtime: &Runtime<'_>,
) -> Result<Relaunched> {
    let mut attempts = 0;
    // The House's own relaunching deadline, carried across attempts so a read
    // that fails cannot quietly widen the window a silent successor gets.
    let mut last_window: Option<Deadline> = None;
    loop {
        if runtime.control.is_stop_requested() {
            return Ok(Relaunched::Stopped);
        }
        attempts += 1;
        // One relaunching transition per attempt. The intent row counts
        // relaunch_attempts and mints a fresh relaunching_deadline_at on every
        // relaunching transition (akasha/src/restart/mod.rs `keeper_move`),
        // so a retry runs inside the House's own new window instead of a second
        // clock invented here.
        let detail = (attempts > 1).then(|| format!("relaunch attempt {attempts}"));
        let successor_proof = match until_answered(house, runtime, |house| {
            transition(
                house,
                pending,
                &claim.claim_token,
                RestartTransitionTarget::Relaunching,
                detail.clone(),
            )
        })? {
            Heard::Answered(receipt) => match receipt.state {
                RestartState::Relaunching => {
                    println!(
                        "omp-keeper: intent {} is relaunching (attempt {attempts})",
                        pending.intent_id
                    );
                    receipt.successor_proof
                }
                // the House spends the budget itself when a keeper asks once too often
                RestartState::Failed => {
                    return Ok(Relaunched::Completed(Outcome::Failed {
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
            Heard::Refused(refusal) => {
                return Ok(Relaunched::Completed(refusal_outcome(&refusal)));
            }
            Heard::Stopped => return Ok(Relaunched::Stopped),
        };

        if runtime.control.is_stop_requested() {
            return Ok(Relaunched::Stopped);
        }
        let failure = match successor_proof {
            Some(ref proof) => {
                match attempt_relaunch(config, house, pending, proof, &mut last_window, runtime)? {
                    Attempt::Verified(child) => {
                        println!(
                            "omp-keeper: the House saw the successor verify intent {}",
                            pending.intent_id
                        );
                        return Ok(Relaunched::Verified(child));
                    }
                    Attempt::Failed(failure) => failure,
                    Attempt::Stopped => return Ok(Relaunched::Stopped),
                }
            }
            None => "the relaunching transition returned no successor proof".to_owned(),
        };
        eprintln!("omp-keeper: relaunch attempt {attempts} failed: {failure}");

        if relaunch_action(attempts, claim.stage_deadlines.relaunch_attempt_limit)
            == RelaunchAction::Fail
        {
            let detail = format!("{attempts} relaunch attempts failed; the last: {failure}");
            match until_answered(house, runtime, |house| {
                transition(
                    house,
                    pending,
                    &claim.claim_token,
                    RestartTransitionTarget::Failed,
                    Some(detail.clone()),
                )
            })? {
                Heard::Answered(_) => {}
                Heard::Refused(refusal) => {
                    return Ok(Relaunched::Completed(refusal_outcome(&refusal)));
                }
                Heard::Stopped => return Ok(Relaunched::Stopped),
            }
            return Ok(Relaunched::Completed(Outcome::Failed {
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
    Stopped,
}

/// One attempt: start omp, then hold it against the House's relaunching window
/// until the successor verifies. A successor that runs but never verifies is not
/// the session Sol asked for, so it does not outlive its deadline.
fn attempt_relaunch(
    config: &KeeperConfig,
    house: &mut HouseLine<'_>,
    pending: &RestartStatusIntent,
    successor_proof: &str,
    last_window: &mut Option<Deadline>,
    runtime: &Runtime<'_>,
) -> Result<Attempt> {
    let mut child = match spawn_omp(
        config,
        Some(RestartLaunch {
            intent: pending,
            successor_proof,
        }),
        runtime,
    ) {
        Ok(child) => child,
        Err(error) => return Ok(Attempt::Failed(format!("{error:#}"))),
    };
    // Past this line the keeper owns a live omp, so no error may leave this
    // function. An escaping `?` drops the Child without killing it: on Windows
    // that leaves Sol's omp running with nothing watching it and the intent
    // stuck in relaunching, which is the one shape the House cannot clean up.
    // Every sad path below reaches the same kill and reports a failed attempt.
    match watch_relaunched(house, config, pending, &mut child, last_window, runtime) {
        Ok(Watched::Verified) => Ok(Attempt::Verified(child)),
        Ok(Watched::Unproven(reason)) => {
            leave_no_child(&mut child, runtime);
            Ok(Attempt::Failed(reason))
        }
        Ok(Watched::Stopped) => {
            leave_no_child(&mut child, runtime);
            Ok(Attempt::Stopped)
        }
        Err(error) => {
            leave_no_child(&mut child, runtime);
            Ok(Attempt::Failed(format!(
                "the keeper could not follow the relaunch: {error:#}"
            )))
        }
    }
}

/// Hold a relaunched omp against the House's relaunching window. A House that
/// cannot be asked decides nothing, so the child keeps running through it; only
/// a finish without a verify, or the window the House published, ends the
/// attempt (laws/LAWS.bend `watch`).
fn watch_relaunched(
    house: &mut HouseLine<'_>,
    config: &KeeperConfig,
    pending: &RestartStatusIntent,
    child: &mut OmpChild,
    last_window: &mut Option<Deadline>,
    runtime: &Runtime<'_>,
) -> Result<Watched> {
    let mut silence = Silence::default();
    let mut window = Window::Unheard;
    loop {
        if runtime.control.is_stop_requested() {
            return Ok(Watched::Stopped);
        }
        if let Window::Unheard = window {
            window = relaunching_window(house, config, pending, last_window, &mut silence)?;
        }
        let deadline = match window {
            Window::Stands(deadline) => Some(deadline),
            Window::Unheard => None,
            Window::Unnamed => {
                return Ok(Watched::Unproven(format!(
                    "the House has named no relaunching deadline for intent {}, so there is no window to wait inside",
                    pending.intent_id
                )));
            }
        };
        match observe(house, config, pending, &mut silence)? {
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
        if let Some(deadline) = deadline
            && deadline.has_passed(Utc::now())
        {
            return Ok(Watched::Unproven(format!(
                "the successor did not verify by {} ({})",
                deadline.at().to_rfc3339(),
                deadline.source()
            )));
        }
        if child
            .try_wait()
            .context("the relaunched omp child could not be inspected")?
            .is_some()
        {
            // Verified and exited in one breath is still verified, so ask once
            // more, and hear the House out: only it knows which one happened.
            let last = until_answered(house, runtime, |house| {
                ask_status(house, config, Some(&pending.intent_id))
            })?;
            return Ok(match last {
                Heard::Answered(observed)
                    if verify_watch(&pending.intent_id, observed.as_ref())
                        == VerifyWatch::Verified =>
                {
                    Watched::Verified
                }
                Heard::Stopped => Watched::Stopped,
                _ => Watched::Unproven("the successor exited before it verified".to_string()),
            });
        }
        std::thread::sleep(VERIFY_POLL);
    }
}

/// The window one attempt waits inside, as far as the keeper has heard.
#[derive(Clone, Copy)]
enum Window {
    /// The instant the House published, for this attempt or the one before.
    Stands(Deadline),
    /// The House answered and has named none: there is no window to wait inside.
    Unnamed,
    /// The House could not be asked, and has named none yet.
    Unheard,
}

/// The window this attempt may wait inside. Only the House sets one. When the
/// read fails or carries no instant, the deadline the House last published
/// stands: minting a fresh one here would hand a silent successor more time than
/// the House ever allowed, and that is the one direction this stage must not
/// fail. `claim.stageDeadlines.relaunchingSecs` is deliberately not a fallback.
fn relaunching_window(
    house: &mut HouseLine<'_>,
    config: &KeeperConfig,
    pending: &RestartStatusIntent,
    last_window: &mut Option<Deadline>,
    silence: &mut Silence,
) -> Result<Window> {
    let published = match ask_status(house, config, Some(&pending.intent_id)) {
        Ok(Answer::Ok(Some(intent))) if intent.intent_id == pending.intent_id => {
            silence.heard();
            intent.deadlines.relaunching_deadline_at
        }
        Ok(Answer::Ok(_)) => {
            silence.heard();
            None
        }
        Ok(Answer::Refused(refusal)) => {
            silence.heard();
            eprintln!(
                "omp-keeper: the House refused the relaunching window read ({}: {}); the deadline it last published stands",
                refusal.code, refusal.message
            );
            None
        }
        // Silence is not a read that failed: the House minted this attempt's
        // window at the transition, and the one before it says nothing about
        // it. Ask again until the House can say.
        Ok(Answer::Unreachable(reason)) => {
            silence.heard_nothing(&reason);
            return Ok(Window::Unheard);
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
            Ok(Window::Stands(window))
        }
        None => Ok(last_window.map_or(Window::Unnamed, Window::Stands)),
    }
}

/// One look at our own intent, by id. A read the House refused or could not
/// answer decides nothing: it is neither a verify nor a terminal sighting, so
/// the window is left to end the wait rather than this answer.
fn observe(
    house: &mut HouseLine<'_>,
    config: &KeeperConfig,
    pending: &RestartStatusIntent,
    silence: &mut Silence,
) -> Result<VerifyWatch> {
    match ask_status(house, config, Some(&pending.intent_id))? {
        Answer::Ok(observed) => {
            silence.heard();
            Ok(verify_watch(&pending.intent_id, observed.as_ref()))
        }
        Answer::Refused(refusal) => {
            silence.heard();
            eprintln!(
                "omp-keeper: the House refused a verification read ({}: {})",
                refusal.code, refusal.message
            );
            Ok(VerifyWatch::Waiting)
        }
        Answer::Unreachable(reason) => {
            silence.heard_nothing(&reason);
            Ok(VerifyWatch::Waiting)
        }
    }
}

/// What a move heard once the House was there to hear it.
enum Heard<T> {
    Answered(T),
    Refused(ProtocolErrorBody),
    Stopped,
}

/// Ask until the House answers. Silence is not a decision: the House was not
/// there to make one, so it never ends the loop, and the House alone decides
/// when a restart is dead, by refusing it when asked (laws/LAWS.bend `move`).
/// Only a stop from the operator ends the wait early.
fn until_answered<'h, T>(
    house: &mut HouseLine<'h>,
    runtime: &Runtime<'_>,
    mut ask: impl FnMut(&mut HouseLine<'h>) -> Result<Answer<T>>,
) -> Result<Heard<T>> {
    let mut silence = Silence::default();
    loop {
        if runtime.control.is_stop_requested() {
            return Ok(Heard::Stopped);
        }
        match ask(house)? {
            Answer::Ok(value) => {
                silence.heard();
                return Ok(Heard::Answered(value));
            }
            Answer::Refused(refusal) => {
                silence.heard();
                return Ok(Heard::Refused(refusal));
            }
            Answer::Unreachable(reason) => {
                silence.heard_nothing(&reason);
                if !rest(runtime, HOUSE_RETRY) {
                    return Ok(Heard::Stopped);
                }
            }
        }
    }
}

/// Sleep in child-poll steps so a stop is never waited out. False when a stop
/// arrived first.
fn rest(runtime: &Runtime<'_>, span: Duration) -> bool {
    let until = Instant::now() + span;
    loop {
        if runtime.control.is_stop_requested() {
            return false;
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return true;
        }
        std::thread::sleep(left.min(CHILD_POLL));
    }
}

/// One stretch of House silence on the console: told when it begins, again
/// once a minute while it lasts, and once more when the House answers.
#[derive(Default)]
struct Silence {
    began: Option<Instant>,
    told: Option<Instant>,
}

impl Silence {
    fn heard_nothing(&mut self, reason: &str) {
        let now = Instant::now();
        let Some(began) = self.began else {
            self.began = Some(now);
            self.told = Some(now);
            eprintln!("omp-keeper: the House cannot answer ({reason}); waiting for it");
            return;
        };
        if self
            .told
            .is_none_or(|told| now.duration_since(told) >= SILENCE_RETELL)
        {
            self.told = Some(now);
            eprintln!(
                "omp-keeper: still waiting for the House after {}s ({reason})",
                now.duration_since(began).as_secs()
            );
        }
    }

    fn heard(&mut self) {
        if let Some(began) = self.began.take() {
            self.told = None;
            eprintln!(
                "omp-keeper: the House answers again after {}s",
                began.elapsed().as_secs()
            );
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
    house: &mut HouseLine<'_>,
    pending: &RestartStatusIntent,
    claim_token: &str,
    to: RestartTransitionTarget,
    detail: Option<String>,
) -> Result<Answer<RestartTransitionReceipt>> {
    house.call(
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
    house: &mut HouseLine<'_>,
    config: &KeeperConfig,
    intent_id: Option<&str>,
) -> Result<Answer<Option<RestartStatusIntent>>> {
    let params = RestartStatusParams {
        workspace: config.workspace.clone(),
        intent_id: intent_id.map(str::to_string),
    };
    Ok(house
        .call::<_, RestartStatusReceipt>(METHOD_RESTART_STATUS, &params)?
        .map(|receipt| receipt.intent))
}

fn spawn_omp(
    config: &KeeperConfig,
    restart: Option<RestartLaunch<'_>>,
    runtime: &Runtime<'_>,
) -> Result<OmpChild> {
    let mut arguments: Vec<&str> = config.program_args().iter().map(String::as_str).collect();
    if let Some(restart) = &restart
        && restart.intent.mode == RestartMode::Resume
    {
        let session_id = restart.intent.session_id.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "resume intent {} carries no session id",
                restart.intent.intent_id
            )
        })?;
        arguments.extend(["--resume", session_id]);
    }

    let child = match runtime.console {
        ConsoleMode::Inherit => {
            let mut command = Command::new(config.program());
            command
                .env_remove(RESTART_INTENT_ENV)
                .env_remove(RESTART_SUCCESSOR_PROOF_ENV)
                .args(&arguments)
                .current_dir(&config.workspace)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
            if let Some(restart) = &restart {
                command
                    .env(RESTART_INTENT_ENV, &restart.intent.intent_id)
                    .env(RESTART_SUCCESSOR_PROOF_ENV, restart.successor_proof);
            }
            command.spawn().map(OmpChild::Inherited)
        }
        ConsoleMode::NewWindow => {
            let mut command = InteractiveCommand::new(config.program());
            command
                .env_remove(RESTART_INTENT_ENV)
                .env_remove(RESTART_SUCCESSOR_PROOF_ENV)
                .args(&arguments)
                .current_dir(&config.workspace);
            if let Some(restart) = &restart {
                command
                    .env(RESTART_INTENT_ENV, &restart.intent.intent_id)
                    .env(RESTART_SUCCESSOR_PROOF_ENV, restart.successor_proof);
            }
            command.spawn().map(OmpChild::Interactive)
        }
    }
    .with_context(|| format!("omp could not start: {}", config.program()))?;
    runtime.observer.child_started(child.id());
    Ok(child)
}

fn watch_child(
    config: &KeeperConfig,
    child: &mut OmpChild,
    runtime: &Runtime<'_>,
) -> Result<Option<i32>> {
    let mut last_poll = Instant::now();
    loop {
        if runtime.control.is_stop_requested() {
            return kill_child(child, runtime).map(|_| None);
        }
        if let Some(status) = child
            .try_wait()
            .context("omp child could not be inspected")?
        {
            runtime.observer.child_stopped(child.id());
            return Ok(Some(status.code().unwrap_or(UNKNOWN_EXIT_CODE)));
        }
        std::thread::sleep(CHILD_POLL);
        if runtime.control.is_stop_requested() {
            return kill_child(child, runtime).map(|_| None);
        }
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
            return kill_child(child, runtime).map(Some);
        }
    }
}

/// Kill and reap. An unreaped child is a handle the keeper would hold forever,
/// and a child that died between the look and the kill is already what we wanted.
fn kill_child(child: &mut OmpChild, runtime: &Runtime<'_>) -> Result<i32> {
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
    runtime.observer.child_stopped(child.id());
    Ok(status.code().unwrap_or(UNKNOWN_EXIT_CODE))
}

/// Make sure no omp survives a failed attempt. This is already the sad path, so
/// a kill the keeper cannot even report must not stop it from reaching `failed`:
/// that transition is what the House and the next keeper are waiting on.
fn leave_no_child(child: &mut OmpChild, runtime: &Runtime<'_>) {
    if let Err(error) = kill_child(child, runtime) {
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
    let mut house = HouseLine::new(&config.program_root, &config.state_root);
    let answer = ask_status(&mut house, config, None);
    house.close()?;
    match answer? {
        Answer::Ok(pending) => Ok(pending),
        Answer::Refused(refusal) => bail!("{}: {}", refusal.code, refusal.message),
        Answer::Unreachable(reason) => bail!("the House cannot answer ({reason})"),
    }
}

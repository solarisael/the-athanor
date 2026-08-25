use crate::config::KeeperConfig;
use crate::decide::{ExitingAction, RelaunchAction, StatusStep, armed_exit_hint, exiting_action, relaunch_action, status_step};
use crate::protocol::{
    METHOD_RESTART_CLAIM, METHOD_RESTART_STATUS, METHOD_RESTART_TRANSITION, PendingIntent,
    ProtocolErrorBody, RestartClaimParams, RestartClaimResult, RestartStatusParams,
    RestartStatusResult, RestartTransitionParams, RestartTransitionResult, TransitionTarget,
};
use crate::resolve::resolve_substrate_exe;
use crate::session::{Answer, SubstrateSession};
use anyhow::{Context, Result, bail};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const CHILD_POLL: Duration = Duration::from_millis(200);
const UNKNOWN_EXIT_CODE: i32 = -1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Stopped { exit_code: i32 },
    Refused { message: String },
    Failed { message: String },
}

pub fn run(config: &KeeperConfig) -> Result<Outcome> {
    config.validate()?;
    let mut child = spawn_omp(config).context("omp could not start")?;
    loop {
        let exit_code = watch_child(config, &mut child)?;
        report_exit(exit_code);

        // # enough: one substrate child for each ask; it keeps the release pointer fresh (census 1.9)
        let executable = resolve_substrate_exe(&config.program_root)
            .context("the current substrate could not be resolved")?;
        let mut session = SubstrateSession::start(&executable)?;

        let pending = match ask_status(&mut session, config)? {
            Ok(pending) => pending,
            Err(refusal) => {
                session.close()?;
                return Ok(refusal_outcome(&refusal));
            }
        };
        let pending = match pending {
            Some(pending) if status_step(Some(&pending.state.wire())) == StatusStep::Claim => pending,
            Some(pending) => {
                session.close()?;
                println!(
                    "omp-keeper: intent {} is {} for {}; nothing to relaunch",
                    pending.intent_id,
                    pending.state.wire(),
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
        let claim: RestartClaimResult = match session.call(
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

        match transition(
            &mut session,
            &pending,
            &claim.claim_token,
            TransitionTarget::Relaunching,
            None,
        )? {
            Answer::Ok(result) => println!(
                "omp-keeper: intent {} is {}",
                pending.intent_id,
                result.state.wire()
            ),
            Answer::Refused(refusal) => {
                session.close()?;
                return Ok(refusal_outcome(&refusal));
            }
        }

        let mut failed_attempts = 0;
        child = loop {
            match spawn_omp(config) {
                Ok(child) => break child,
                Err(error) => {
                    failed_attempts += 1;
                    eprintln!("omp-keeper: relaunch attempt {failed_attempts} failed: {error:#}");
                    if relaunch_action(failed_attempts) == RelaunchAction::Fail {
                        let detail = format!("relaunch failed twice: {error:#}");
                        let refusal = transition(
                            &mut session,
                            &pending,
                            &claim.claim_token,
                            TransitionTarget::Failed,
                            Some(detail.clone()),
                        )?;
                        session.close()?;
                        if let Answer::Refused(refusal) = refusal {
                            return Ok(refusal_outcome(&refusal));
                        }
                        let message = format!(
                            "omp-keeper: {detail}; intent {} is failed:relaunching and omp is not running",
                            pending.intent_id
                        );
                        return Ok(Outcome::Failed { message });
                    }
                }
            }
        };
        session.close()?;
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
    pending: &PendingIntent,
    claim_token: &str,
    to: TransitionTarget,
    detail: Option<String>,
) -> Result<Answer<RestartTransitionResult>> {
    session.call(
        METHOD_RESTART_TRANSITION,
        &RestartTransitionParams {
            intent_id: pending.intent_id.clone(),
            claim_token: claim_token.to_string(),
            to,
            detail,
        },
    )
}

fn ask_status(
    session: &mut SubstrateSession,
    config: &KeeperConfig,
) -> Result<std::result::Result<Option<PendingIntent>, ProtocolErrorBody>> {
    let params = RestartStatusParams {
        workspace: config.workspace.clone(),
    };
    match session.call::<_, RestartStatusResult>(METHOD_RESTART_STATUS, &params)? {
        Answer::Ok(result) => Ok(Ok(result.pending)),
        Answer::Refused(refusal) => Ok(Err(refusal)),
    }
}

fn spawn_omp(config: &KeeperConfig) -> Result<Child> {
    // console-inheriting on purpose: Sol watches this child, so no detach and no hidden window
    Command::new(config.program())
        .args(config.program_args())
        .current_dir(&config.workspace)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("omp could not start: {}", config.program()))
}

fn watch_child(config: &KeeperConfig, child: &mut Child) -> Result<i32> {
    let mut exiting_since: Option<Instant> = None;
    let mut last_poll = Instant::now();
    loop {
        if let Some(status) = child.try_wait().context("omp child could not be inspected")? {
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
        let state = watched.as_ref().map(|pending| pending.state.wire());
        // deadline measured from the keeper's first sight of exiting: the status shape carries stage seconds, not an entered-at instant
        if state.as_deref() == Some(crate::protocol::STATE_EXITING) {
            exiting_since.get_or_insert_with(Instant::now);
        } else {
            exiting_since = None;
        }
        let elapsed = exiting_since.map(|since| since.elapsed().as_secs()).unwrap_or(0);
        let deadline = watched
            .as_ref()
            .map(|pending| pending.deadlines.exiting_secs)
            .unwrap_or(crate::protocol::EXITING_DEADLINE_SECS);
        if exiting_action(state.as_deref(), elapsed, deadline) == ExitingAction::Kill {
            println!(
                "omp-keeper: omp did not leave within {deadline}s of its exiting intent; killing it"
            );
            child.kill().context("omp child could not be killed")?;
            let status = child.wait().context("killed omp child could not be reaped")?;
            return Ok(status.code().unwrap_or(UNKNOWN_EXIT_CODE));
        }
    }
}

fn watch_status(config: &KeeperConfig) -> Result<Option<PendingIntent>> {
    let executable = resolve_substrate_exe(&config.program_root)?;
    let mut session = SubstrateSession::start(&executable)?;
    let answer = ask_status(&mut session, config);
    session.close()?;
    match answer? {
        Ok(pending) => Ok(pending),
        Err(refusal) => bail!("{}: {}", refusal.code, refusal.message),
    }
}

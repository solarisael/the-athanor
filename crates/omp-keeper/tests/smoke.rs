//! Smoke tests: the real keeper program, real child processes, real spawning,
//! and a fake substrate that answers with real `house_protocol::restart` structs
//! and refuses any request the real door would refuse.
//!
//! No database. What these defend is the keeper's own seam: what it launches,
//! which clock it obeys, when it kills, when it retries, and what it says to Sol
//! when it stops.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const RELEASE_VERSION: &str = "0.0.0-smoke";

/// The wire values the fixture answers with. They mirror
/// `examples/fake_substrate.rs` on purpose: a canonical UUID and 64 lowercase
/// hex are the only shapes the real door's `validate()` accepts, and asserting
/// them here is what catches a keeper that mangles what the House handed it.
const INTENT_ID: &str = "3f6b9c2a-7d41-4e58-9a0b-1c8e5d2f4a67";
const CLAIM_TOKEN: &str = "9f2c7a1e4b8d60359f2c7a1e4b8d60359f2c7a1e4b8d60359f2c7a1e4b8d6035";

struct Tree {
    _temp: tempfile::TempDir,
    root: PathBuf,
    config: PathBuf,
    program: PathBuf,
    program_root: PathBuf,
    capability: PathBuf,
    runs: PathBuf,
    transcript: PathBuf,
    /// Where a child that outlived its sleep records the fact. An entry here
    /// means the keeper walked away from a live omp instead of killing it.
    survived: PathBuf,
}

fn example(name: &str) -> PathBuf {
    let mut directory = std::env::current_exe().expect("test executable path");
    directory.pop();
    if directory.ends_with("deps") {
        directory.pop();
    }
    let path = directory
        .join("examples")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.exists(),
        "the {name} fixture must be built first: cargo test -p omp-keeper builds examples"
    );
    path
}

fn substrate_exe_name() -> &'static str {
    if cfg!(windows) {
        "athanor-substrate.exe"
    } else {
        "athanor-substrate"
    }
}

fn tree() -> Tree {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path().to_path_buf();
    let program_root = root.join("program");
    let bin = program_root
        .join("versions")
        .join(RELEASE_VERSION)
        .join("bin");
    fs::create_dir_all(&bin).expect("release bin");
    fs::write(
        program_root.join("current.json"),
        format!("{{\"version\":\"{RELEASE_VERSION}\"}}"),
    )
    .expect("current pointer");
    fs::copy(example("fake_substrate"), bin.join(substrate_exe_name())).expect("fake substrate");

    fs::create_dir_all(root.join("state")).expect("state root");
    fs::create_dir_all(root.join("workspace")).expect("workspace");

    let program = root.join(format!("omp-run{}", std::env::consts::EXE_SUFFIX));
    fs::copy(example("fake_omp"), &program).expect("omp program copy");

    let capability = root.join("keeper.capability");
    fs::write(&capability, "smoke-capability\n").expect("capability file");

    let tree = Tree {
        config: root.join("omp-keeper.json"),
        program: program.clone(),
        program_root,
        capability,
        runs: root.join("omp-runs.log"),
        transcript: root.join("substrate-transcript.jsonl"),
        survived: root.join("omp-survived.log"),
        root,
        _temp: temp,
    };
    write_config(&tree, &[program.display().to_string()], 0);
    tree
}

/// `watch_interval_secs` of 0 turns the child-watch status poll off, which is
/// what every test that only cares about the exit path wants.
fn write_config(tree: &Tree, launch: &[String], watch_interval_secs: u64) {
    let config = serde_json::json!({
        "ompLaunch": launch,
        "workspace": tree.root.join("workspace"),
        "programRoot": &tree.program_root,
        "stateRoot": tree.root.join("state"),
        "capabilityPath": &tree.capability,
        "watchIntervalSecs": watch_interval_secs,
    });
    fs::write(
        &tree.config,
        serde_json::to_vec_pretty(&config).expect("config json"),
    )
    .expect("config file");
}

fn run_keeper_with(tree: &Tree, mode: &str, extra: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_omp-keeper"));
    command
        .arg("--config")
        .arg(&tree.config)
        .env("FAKE_OMP_RUNS", &tree.runs)
        .env("FAKE_SUBSTRATE_TRANSCRIPT", &tree.transcript)
        .env("FAKE_SUBSTRATE_MODE", mode)
        .env("FAKE_OMP_PROGRAM", &tree.program)
        .env("FAKE_OMP_SURVIVED", &tree.survived);
    for (name, value) in extra {
        command.env(name, value);
    }
    command.output().expect("keeper runs")
}

fn lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn requests(transcript: &Path) -> Vec<serde_json::Value> {
    lines(transcript)
        .iter()
        .map(|line| serde_json::from_str(line).expect("request json"))
        .collect()
}

fn methods(transcript: &Path) -> Vec<String> {
    requests(transcript)
        .iter()
        .map(|request| request["method"].as_str().expect("method name").to_string())
        .collect()
}

fn requests_for(transcript: &Path, method: &str) -> Vec<serde_json::Value> {
    requests(transcript)
        .into_iter()
        .filter(|request| request["method"] == method)
        .collect()
}

struct Ran {
    stdout: String,
    stderr: String,
    output: Output,
    elapsed: Duration,
}

fn run_keeper_timed(tree: &Tree, mode: &str, extra: &[(&str, &str)]) -> Ran {
    let started = Instant::now();
    let output = run_keeper_with(tree, mode, extra);
    Ran {
        elapsed: started.elapsed(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        output,
    }
}

/// Repair 4's proof: the whole loop, end to end, over shapes the real door
/// accepts — armed exit, status, claim, relaunching transition, relaunch, and a
/// successor the House saw verify.
#[test]
fn the_full_loop_runs_from_an_armed_exit_to_a_verified_successor() {
    let tree = tree();
    let state_root = tree.root.join("state").display().to_string();
    let ran = run_keeper_timed(
        &tree,
        "full-loop",
        &[
            ("ATHANOR_STATE_DIR", "D:/wrong-inherited-state"),
            ("FAKE_SUBSTRATE_EXPECT_STATE_ROOT", &state_root),
        ],
    );
    let (stdout, stderr) = (&ran.stdout, &ran.stderr);
    assert!(
        ran.output.status.success(),
        "the loop must close cleanly: {:?}\nstdout: {stdout}\nstderr: {stderr}",
        ran.output.status.code()
    );
    assert_eq!(
        methods(&tree.transcript),
        [
            // the 87 exit: the keeper asks the House what happened
            "restart_status",
            "restart_claim",
            // -> relaunching, then omp is started again
            "restart_transition",
            // the House's own relaunching window for this attempt
            "restart_status",
            // the intent left the pending set: the successor verified
            "restart_status",
            // the successor's own exit, with nothing pending behind it
            "restart_status",
        ],
        "the full loop asks, claims, transitions, relaunches, watches for the verify, then stops"
    );
    assert_eq!(
        lines(&tree.runs).len(),
        2,
        "omp ran twice: the armed exit and the relaunch: {stdout}"
    );

    let claim = &requests_for(&tree.transcript, "restart_claim")[0];
    assert_eq!(
        claim["params"]["intentId"], INTENT_ID,
        "the keeper echoes the House's own intent id, canonical UUID and all"
    );
    assert_eq!(claim["params"]["claimant"], "omp-keeper");
    assert_eq!(
        claim["params"]["idempotencyKey"],
        format!("omp-keeper:claim:{INTENT_ID}")
    );
    assert_eq!(
        claim["params"]["capability"], "smoke-capability",
        "the claim carries the provisioned capability, trimmed, from its file"
    );

    let transition = &requests_for(&tree.transcript, "restart_transition")[0];
    assert_eq!(transition["params"]["to"], "relaunching");
    assert_eq!(transition["params"]["intentId"], INTENT_ID);
    assert_eq!(
        transition["params"]["claimToken"], CLAIM_TOKEN,
        "the token goes back exactly as minted: 64 lowercase hex"
    );
    assert_eq!(transition["protocol"], 1);

    assert!(
        !stdout.contains("invalid_params") && !stderr.contains("invalid_params"),
        "the fixture validates every request with the real door's validate(); a \
         refusal here means the keeper speaks a shape the House rejects:\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("armed exit hint"),
        "the 87 hint is reported: {stdout}"
    );
    assert!(
        stdout.contains("saw the successor verify"),
        "the operator learns the House itself witnessed the verify: {stdout}"
    );
    assert!(
        stdout.contains("no restart intent"),
        "the loop ends on an empty status: {stdout}"
    );
    assert!(
        !stdout.contains("smoke-capability") && !stderr.contains("smoke-capability"),
        "the capability never reaches the console"
    );
}

/// A verified successor is already the keeper's child. Failure to close the
/// disposable substrate must stay a warning, then supervision continues.
#[test]
fn a_verified_successor_stays_supervised_when_its_substrate_close_fails() {
    let tree = tree();
    let ran = run_keeper_timed(
        &tree,
        "full-loop",
        &[("FAKE_SUBSTRATE_CLOSE_ERROR_AT_STATUSES", "3")],
    );
    let (stdout, stderr) = (&ran.stdout, &ran.stderr);
    assert!(
        ran.output.status.success(),
        "the keeper must outlive the failed close: {:?}\nstdout: {stdout}\nstderr: {stderr}",
        ran.output.status.code()
    );
    assert_eq!(
        lines(&tree.runs).len(),
        2,
        "the verified successor remains the supervised child"
    );
    assert!(
        stderr.contains("continuing to supervise omp"),
        "the close failure stays visible: {stderr}"
    );
    assert!(
        stdout.contains("no restart intent"),
        "the keeper reached the successor's later exit and asked the House again: {stdout}"
    );
}

/// Repair 1's proof: the intent's `exitingDeadlineAt` is an absolute instant. A
/// keeper that starts its own 60-second stopwatch on first sight kills a minute
/// late and never names the House's instant.
#[test]
fn an_exiting_child_is_killed_on_the_house_deadline_not_a_keeper_stopwatch() {
    let tree = tree();
    write_config(&tree, &[tree.program.display().to_string()], 1);
    // the child overstays; the House's exiting deadline is already in the past
    let ran = run_keeper_timed(
        &tree,
        "exiting-overrun",
        &[
            ("FAKE_OMP_SLEEP_SECS", "120"),
            ("FAKE_OMP_SLEEP_FROM_RUN", "1"),
        ],
    );
    let (stdout, stderr) = (&ran.stdout, &ran.stderr);
    assert!(
        ran.elapsed < Duration::from_secs(20),
        "a deadline already past must be obeyed at once, not restarted as a local \
         60s clock (took {:?}):\n{stdout}\n{stderr}",
        ran.elapsed
    );
    assert!(
        stdout.contains("did not leave by"),
        "the kill names the instant it obeyed: {stdout}"
    );
    assert!(
        stdout.contains("the House deadline"),
        "the console says which clock it obeyed: {stdout}"
    );
    assert_eq!(
        methods(&tree.transcript),
        ["restart_status", "restart_status"],
        "one poll saw the overrun, one asked what to do after the kill"
    );
    assert_eq!(
        lines(&tree.runs).len(),
        1,
        "nothing pending after the kill, so omp is not relaunched: {stdout}"
    );
    assert_eq!(
        ran.output.status.code(),
        Some(1),
        "a killed child leaves a nonzero code and the keeper reports it: {stdout}"
    );
}

/// Repair 2's proof: a successor that starts but never verifies inside the
/// House's relaunching window is retried exactly once, then the intent is
/// failed. Before this repair only a spawn *error* could retry, so a silent
/// successor was waited on forever and the intent was left relaunching.
#[test]
fn a_successor_that_never_verifies_is_retried_once_and_then_failed() {
    let tree = tree();
    // run 1 arms the exit; every successor stays alive and never verifies
    let ran = run_keeper_timed(
        &tree,
        "unverified",
        &[
            ("FAKE_OMP_SLEEP_SECS", "30"),
            ("FAKE_OMP_SLEEP_FROM_RUN", "2"),
        ],
    );
    let (stdout, stderr) = (&ran.stdout, &ran.stderr);
    assert_eq!(
        ran.output.status.code(),
        Some(1),
        "an unverified restart is a failure:\n{stdout}\n{stderr}"
    );
    let transitions = requests_for(&tree.transcript, "restart_transition");
    assert_eq!(
        transitions.len(),
        3,
        "two relaunch attempts, each asking the House for its own window, then \
         failed:\n{stdout}\n{stderr}"
    );
    assert_eq!(transitions[0]["params"]["to"], "relaunching");
    assert_eq!(
        transitions[1]["params"]["to"], "relaunching",
        "the retry re-enters relaunching so the House mints a fresh window \
         instead of the keeper inventing a second clock"
    );
    assert_eq!(transitions[2]["params"]["to"], "failed");
    assert_eq!(transitions[2]["params"]["claimToken"], CLAIM_TOKEN);
    assert!(
        transitions[2]["params"]["detail"]
            .as_str()
            .expect("failure detail")
            .contains("did not verify"),
        "the failure says the successor never verified: {}",
        transitions[2]
    );
    assert_eq!(
        lines(&tree.runs).len(),
        3,
        "the armed exit, then two relaunch attempts: {stdout}"
    );
    assert!(
        stderr.contains("relaunch attempt 1 failed")
            && stderr.contains("relaunch attempt 2 failed"),
        "one retry is reported before giving up: {stderr}"
    );
    assert!(
        stdout.contains("failed:relaunching") && stdout.contains("omp is not running"),
        "the operator learns omp is gone: {stdout}"
    );
    assert!(
        ran.elapsed < Duration::from_secs(60),
        "the published window is seconds, so the give-up is prompt: {:?}",
        ran.elapsed
    );
}

#[test]
fn a_relaunch_that_cannot_start_retries_once_and_then_transitions_to_failed() {
    let tree = tree();
    let ran = run_keeper_timed(&tree, "relaunch-broken", &[]);
    let (stdout, stderr) = (&ran.stdout, &ran.stderr);
    assert_eq!(
        ran.output.status.code(),
        Some(1),
        "a failed relaunch is not success"
    );
    let transitions = requests_for(&tree.transcript, "restart_transition");
    assert_eq!(
        transitions.len(),
        3,
        "each attempt enters relaunching, then the budget is spent:\n{stdout}\n{stderr}"
    );
    assert_eq!(transitions[0]["params"]["to"], "relaunching");
    assert_eq!(transitions[1]["params"]["to"], "relaunching");
    assert_eq!(transitions[2]["params"]["to"], "failed");
    assert_eq!(transitions[2]["params"]["claimToken"], CLAIM_TOKEN);
    assert!(
        transitions[2]["params"]["detail"]
            .as_str()
            .expect("failure detail")
            .contains("could not start"),
        "the failure carries its detail: {}",
        transitions[2]
    );
    assert_eq!(lines(&tree.runs).len(), 1, "omp never came back: {stdout}");
    assert!(
        stderr.contains("relaunch attempt 1 failed")
            && stderr.contains("relaunch attempt 2 failed"),
        "one retry is reported before giving up: {stderr}"
    );
    assert!(
        stdout.contains("failed:relaunching") && stdout.contains("omp is not running"),
        "the operator learns omp is gone: {stdout}"
    );
}

/// Repair 5's proof, first door: a storm refusal on the very first ask ends the
/// loop with the operator's line and never relaunches.
#[test]
fn a_storm_refusal_ends_the_loop_with_one_plain_message() {
    let tree = tree();
    let ran = run_keeper_timed(&tree, "storm", &[]);
    let stdout = &ran.stdout;
    assert_eq!(
        ran.output.status.code(),
        Some(1),
        "a refusal is not success"
    );
    assert_eq!(methods(&tree.transcript), ["restart_status"]);
    assert_eq!(
        lines(&tree.runs).len(),
        1,
        "a refused keeper never relaunches: {stdout}"
    );
    assert!(
        stdout.contains("refused another restart"),
        "the operator gets a plain message: {stdout}"
    );
    assert!(
        stdout.contains("start it yourself"),
        "the message says what to do: {stdout}"
    );
}

/// Repair 5's proof, second door: the refusal can also arrive mid-loop, on the
/// keeper's own claim, after the House already showed a pending intent. That
/// path ends the same way and, above all, never starts omp again.
#[test]
fn a_storm_refusal_on_the_claim_ends_the_loop_before_any_relaunch() {
    let tree = tree();
    let ran = run_keeper_timed(&tree, "storm-on-claim", &[]);
    let (stdout, stderr) = (&ran.stdout, &ran.stderr);
    assert_eq!(
        ran.output.status.code(),
        Some(1),
        "a refusal is not success:\n{stdout}\n{stderr}"
    );
    assert_eq!(
        methods(&tree.transcript),
        ["restart_status", "restart_claim"],
        "the keeper stops at the refusal: no transition, no relaunch"
    );
    assert!(
        requests_for(&tree.transcript, "restart_transition").is_empty(),
        "a refused claim never transitions the intent"
    );
    assert_eq!(
        lines(&tree.runs).len(),
        1,
        "omp is not started again after a storm refusal: {stdout}"
    );
    assert!(
        stdout.contains("refused another restart") && stdout.contains("start it yourself"),
        "the operator gets the same plain line wherever the refusal arrives: {stdout}"
    );
}

/// Repair 3's territory: Sol's documented invocation is an npm shim, `omp.cmd`,
/// and the review read `Command::new` as unable to start one. This drives the
/// real thing, in the hardest shape the documented config can take: a shim in a
/// directory whose name has a space, launched with an argument, with the console
/// inherited and the exit code carried back.
#[cfg(windows)]
#[test]
fn a_cmd_shim_launch_starts_omp_with_its_console_and_arguments_intact() {
    let tree = tree();
    // a space in the path is the case that breaks hand-rolled `cmd.exe /c` quoting
    let directory = tree.root.join("npm shim");
    fs::create_dir_all(&directory).expect("shim directory");
    let shim = directory.join("omp.cmd");
    fs::write(
        &shim,
        "@echo off\r\n\
         echo run>>\"%FAKE_OMP_RUNS%\"\r\n\
         echo fake omp cmd: arming an exit %1\r\n\
         exit /b 87\r\n",
    )
    .expect("cmd shim");
    write_config(
        &tree,
        &[shim.display().to_string(), "--resume".to_string()],
        0,
    );

    let ran = run_keeper_timed(&tree, "no-intent", &[]);
    let (stdout, stderr) = (&ran.stdout, &ran.stderr);
    assert!(
        ran.output.status.success(),
        "the documented .cmd invocation must start: {:?}\nstdout: {stdout}\nstderr: {stderr}",
        ran.output.status.code()
    );
    assert!(
        stdout.contains("fake omp cmd: arming an exit"),
        "the shim's own console output is inherited, not swallowed: {stdout}"
    );
    assert!(
        stdout.contains("--resume"),
        "the launch arguments reach the shim through the command processor: {stdout}"
    );
    assert!(
        stdout.contains("omp exited 87"),
        "the shim's exit code reaches the keeper: {stdout}"
    );
    assert_eq!(
        lines(&tree.runs).len(),
        1,
        "the shim ran once, with the keeper's environment: {stdout}"
    );
    assert_eq!(
        methods(&tree.transcript),
        ["restart_status"],
        "nothing pending, so the keeper asks once and stops"
    );
}

/// P1(2)a: the House vanishing after the spawn must not leave omp behind.
///
/// Pre-repair the error escaped `attempt_relaunch` through `?`, which drops the
/// Child without killing it -- on Windows that is a live omp with no keeper and
/// an intent stuck in relaunching. The keeper cannot reach `failed` here, because
/// the transition needs the same dead session; what it must still do is put the
/// child down and say so.
#[test]
fn a_house_that_vanishes_after_the_spawn_leaves_no_orphaned_omp() {
    let tree = tree();
    let ran = run_keeper_timed(
        &tree,
        "substrate-dies-mid-watch",
        &[
            ("FAKE_OMP_SLEEP_SECS", "6"),
            ("FAKE_OMP_SLEEP_FROM_RUN", "2"),
        ],
    );
    let (stdout, stderr) = (&ran.stdout, &ran.stderr);
    assert_ne!(
        ran.output.status.code(),
        Some(0),
        "losing the House mid-relaunch is not a success:\n{stdout}\n{stderr}"
    );
    assert!(
        !stdout.contains("saw the successor verify"),
        "a lost House is never a verify: {stdout}"
    );
    assert_eq!(
        lines(&tree.runs).len(),
        2,
        "the armed exit and one relaunch attempt: {stdout}"
    );
    // The guarantee, checked before any wording: outlive the child's own sleep,
    // because an orphan records itself only once it gets past that.
    std::thread::sleep(Duration::from_secs(9));
    assert!(
        lines(&tree.survived).is_empty(),
        "the keeper dropped a live omp instead of killing it -- orphaned: {:?}\n{stdout}\n{stderr}",
        lines(&tree.survived)
    );
    assert!(
        stderr.contains("lost the House mid-relaunch"),
        "the keeper names what happened to it: {stderr}"
    );
}

/// P1(2)b: a refused window read keeps the deadline the House last published.
///
/// The retry's window read is refused here. Pre-repair the keeper answered that
/// by minting a window of its own from the claim's `relaunchingSecs` -- it said
/// so out loud, "watching for 2s instead", and then killed on "a keeper fallback
/// deadline". Whatever that number is, it is the keeper granting a silent
/// successor time the House never granted. Post-repair the last House instant
/// stands, is already past, and the attempt ends on it.
#[test]
fn a_refused_window_read_keeps_the_last_house_deadline_and_never_mints_one() {
    let tree = tree();
    let ran = run_keeper_timed(
        &tree,
        "window-read-refused",
        &[
            ("FAKE_OMP_SLEEP_SECS", "200"),
            ("FAKE_OMP_SLEEP_FROM_RUN", "2"),
        ],
    );
    let (stdout, stderr) = (&ran.stdout, &ran.stderr);
    assert_eq!(
        ran.output.status.code(),
        Some(1),
        "an unverified restart is a failure:\n{stdout}\n{stderr}"
    );
    assert!(
        !stderr.contains("a keeper fallback deadline"),
        "no attempt may wait on a deadline the keeper invented:\n{stderr}"
    );
    assert!(
        stderr.contains("the deadline it last published stands"),
        "the keeper says whose clock it kept: {stderr}"
    );
    assert!(
        ran.elapsed < Duration::from_secs(60),
        "the kept deadline is already past, so the give-up is immediate \
         (took {:?}):\n{stdout}\n{stderr}",
        ran.elapsed
    );
    let transitions = requests_for(&tree.transcript, "restart_transition");
    assert_eq!(
        transitions.len(),
        3,
        "two attempts, then failed:\n{stdout}\n{stderr}"
    );
    assert_eq!(transitions[2]["params"]["to"], "failed");
    assert!(
        stdout.contains("failed:relaunching") && stdout.contains("omp is not running"),
        "the operator learns omp is gone: {stdout}"
    );
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        lines(&tree.survived).is_empty(),
        "both attempts were put down, not abandoned: {:?}",
        lines(&tree.survived)
    );
}

/// P1(1): a row that is not ours is never our verify.
///
/// Pre-repair the keeper read "the answer is not our intent" as proof our
/// successor had verified, and declared victory over a restart that never
/// happened -- a stranger's row, or plain absence, both counted as success. Now
/// only our own id, reported `verified` by the exact-id read, is proof; anything
/// else finished is finished-but-unproven and takes the retry path.
///
/// What this defends is a relaxed fence, not a shape today's wire can produce.
/// The exact-id read is scoped by workspace, so another workspace's row already
/// reads as none, and the restart schema makes a second LIVE intent in one
/// workspace unconstructible. Terminal rows do pile up per workspace, though,
/// which is the real reason the watch keys on the id it claimed and never on
/// "the newest thing this workspace has".
#[test]
fn a_stranger_intent_is_never_our_verify_even_if_the_live_intent_fence_relaxes() {
    let tree = tree();
    let ran = run_keeper_timed(
        &tree,
        "stranger-intent",
        &[
            ("FAKE_OMP_SLEEP_SECS", "30"),
            ("FAKE_OMP_SLEEP_FROM_RUN", "2"),
        ],
    );
    let (stdout, stderr) = (&ran.stdout, &ran.stderr);
    assert!(
        !stdout.contains("saw the successor verify"),
        "another intent is never our successor's verify:\n{stdout}"
    );
    assert_eq!(
        ran.output.status.code(),
        Some(1),
        "an unproven restart is a failure, not a success:\n{stdout}\n{stderr}"
    );
    // every status the keeper sent while watching named the intent it claimed
    let watch_reads: Vec<_> = requests_for(&tree.transcript, "restart_status")
        .into_iter()
        .filter(|request| !request["params"]["intentId"].is_null())
        .collect();
    assert!(
        !watch_reads.is_empty(),
        "the verify watch must ask by id, or it can never see a verify at all"
    );
    for read in &watch_reads {
        assert_eq!(
            read["params"]["intentId"], INTENT_ID,
            "the keeper asks about its own intent, never a stranger's: {read}"
        );
    }
    let transitions = requests_for(&tree.transcript, "restart_transition");
    assert_eq!(
        transitions.len(),
        3,
        "two attempts against a House that never confirms, then failed:\n{stdout}\n{stderr}"
    );
    assert_eq!(transitions[2]["params"]["to"], "failed");
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        lines(&tree.survived).is_empty(),
        "the unproven successors were put down: {:?}",
        lines(&tree.survived)
    );
}

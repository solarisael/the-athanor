use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const RELEASE_VERSION: &str = "0.0.0-smoke";

struct Tree {
    _temp: tempfile::TempDir,
    config: PathBuf,
    program: PathBuf,
    runs: PathBuf,
    transcript: PathBuf,
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
        path.is_file(),
        "example fixture {name} must be built by cargo test: {}",
        path.display()
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
    let root = temp.path();
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

    let workspace = root.join("workspace");
    fs::create_dir_all(&workspace).expect("workspace");

    let program = root.join(format!("omp-run{}", std::env::consts::EXE_SUFFIX));
    fs::copy(example("fake_omp"), &program).expect("omp program copy");

    let capability = root.join("keeper.capability");
    fs::write(&capability, "smoke-capability\n").expect("capability file");

    let config = root.join("omp-keeper.json");
    let launch = serde_json::json!({
        "ompLaunch": [&program],
        "workspace": workspace,
        "programRoot": program_root,
        "capabilityPath": capability,
        "watchIntervalSecs": 0,
    });
    fs::write(&config, serde_json::to_vec_pretty(&launch).expect("config json"))
        .expect("config file");

    Tree {
        config,
        program,
        runs: root.join("omp-runs.log"),
        transcript: root.join("substrate-transcript.jsonl"),
        _temp: temp,
    }
}

fn run_keeper(tree: &Tree, mode: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_omp-keeper"))
        .arg("--config")
        .arg(&tree.config)
        .env("FAKE_OMP_RUNS", &tree.runs)
        .env("FAKE_SUBSTRATE_TRANSCRIPT", &tree.transcript)
        .env("FAKE_SUBSTRATE_MODE", mode)
        .env("FAKE_OMP_PROGRAM", &tree.program)
        .output()
        .expect("keeper runs")
}

fn lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn methods(transcript: &Path) -> Vec<String> {
    lines(transcript)
        .iter()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).expect("request json")["method"]
                .as_str()
                .expect("method name")
                .to_string()
        })
        .collect()
}

#[test]
fn relaunches_omp_once_and_then_exits_on_an_empty_status() {
    let tree = tree();
    let output = run_keeper(&tree, "relaunch-once");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "keeper must exit cleanly: {:?}\nstdout: {stdout}\nstderr: {stderr}",
        output.status.code()
    );
    assert_eq!(
        methods(&tree.transcript),
        [
            "restart_status",
            "restart_claim",
            "restart_transition",
            "restart_status"
        ],
        "the keeper asks status for every exit, claims, transitions, then asks again"
    );
    assert_eq!(lines(&tree.runs).len(), 2, "omp ran twice: {stdout}");

    let transition = &lines(&tree.transcript)[2];
    let transition: serde_json::Value = serde_json::from_str(transition).expect("transition json");
    assert_eq!(transition["params"]["to"], "relaunching");
    assert_eq!(transition["params"]["intentId"], "intent-1");
    assert_eq!(transition["params"]["claimToken"], "claim-token-1");
    assert_eq!(transition["protocol"], 1);

    let claim = &lines(&tree.transcript)[1];
    let claim: serde_json::Value = serde_json::from_str(claim).expect("claim json");
    assert_eq!(claim["params"]["claimant"], "omp-keeper");
    assert_eq!(claim["params"]["idempotencyKey"], "omp-keeper:claim:intent-1");
    assert_eq!(
        claim["params"]["capability"], "smoke-capability",
        "the claim carries the provisioned capability, trimmed, from its file"
    );
    assert!(
        !stdout.contains("smoke-capability") && !stderr.contains("smoke-capability"),
        "the capability never reaches the console"
    );

    assert!(
        stdout.contains("armed exit hint"),
        "the 87 hint is reported: {stdout}"
    );
    assert!(
        stdout.contains("no restart intent"),
        "the second cycle stops on an empty status: {stdout}"
    );
}

#[test]
fn a_storm_refusal_ends_the_loop_with_one_plain_message() {
    let tree = tree();
    let output = run_keeper(&tree, "storm");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(output.status.code(), Some(1), "a refusal is not success");
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

#[test]
fn a_relaunch_that_cannot_start_retries_once_and_then_transitions_to_failed() {
    let tree = tree();
    let output = run_keeper(&tree, "relaunch-broken");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(output.status.code(), Some(1), "a failed relaunch is not success");
    assert_eq!(
        methods(&tree.transcript),
        [
            "restart_status",
            "restart_claim",
            "restart_transition",
            "restart_transition"
        ],
        "the keeper transitions once to relaunching and once to failed"
    );
    let transitions = lines(&tree.transcript);
    let relaunching: serde_json::Value =
        serde_json::from_str(&transitions[2]).expect("relaunching json");
    let failed: serde_json::Value = serde_json::from_str(&transitions[3]).expect("failed json");
    assert_eq!(relaunching["params"]["to"], "relaunching");
    assert_eq!(failed["params"]["to"], "failed");
    assert_eq!(failed["params"]["claimToken"], "claim-token-1");
    assert!(
        failed["params"]["detail"]
            .as_str()
            .expect("failure detail")
            .contains("relaunch failed twice"),
        "the failure carries its detail: {failed}"
    );
    assert_eq!(lines(&tree.runs).len(), 1, "omp never came back: {stdout}");
    assert!(
        stderr.contains("relaunch attempt 1 failed") && stderr.contains("relaunch attempt 2 failed"),
        "one retry is reported before giving up: {stderr}"
    );
    assert!(
        stdout.contains("failed:relaunching") && stdout.contains("omp is not running"),
        "the operator learns omp is gone: {stdout}"
    );
}

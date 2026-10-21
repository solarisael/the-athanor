//! The one exe's door contract: every mode is named by its first argument,
//! and an unknown or retired name is refused before any runtime file is read.

use std::process::Command;

fn athanor(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_athanor"))
        .args(arguments)
        .env("ATHANOR_PROGRAM_ROOT", "Z:/does-not-exist")
        .env("ATHANOR_DATA_ROOT", "Z:/does-not-exist")
        .output()
        .expect("run athanor")
}

#[test]
fn obsolete_and_unknown_modes_are_refused_before_the_runtime_is_read() {
    for arguments in [
        &["--room", "kintsu"][..],
        &["gui"][..],
        &["manage"][..],
        &["athanor-manage"][..],
    ] {
        let output = athanor(arguments);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("unknown mode"), "{stderr}");
        assert!(stderr.contains("athanor help"), "{stderr}");
        assert!(!stderr.contains("runtime.json"), "{stderr}");
        assert!(!stderr.contains("Godot"), "{stderr}");
    }
}

#[test]
fn the_keeper_mode_needs_a_room_or_a_named_config() {
    let output = athanor(&["keeper"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("athanor keeper <room>"), "{stderr}");
    assert!(stderr.contains("athanor keeper --config"), "{stderr}");
}

#[test]
fn a_room_name_is_answered_by_the_harness_registry() {
    let output = athanor(&["keeper", "kodo"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"kodo\""),
        "the refusal names the room asked for: {stderr}"
    );
    assert!(
        stderr.contains("harnesses.json"),
        "the refusal names the registry it read: {stderr}"
    );
    assert!(!stderr.contains("unknown mode"), "{stderr}");
}

#[test]
fn the_chat_mode_names_its_usage() {
    let output = athanor(&["chat", "--room"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage: athanor chat"), "{stderr}");
}

#[cfg(windows)]
#[test]
fn a_literal_checkout_path_reaches_the_real_powershell_output_guard() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("checkout with spaces & literal");
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::create_dir_all(source.join("substrate")).unwrap();
    std::fs::create_dir_all(source.join("installer")).unwrap();
    for file in [
        "substrate/deploy-local.ps1",
        "installer/prepared-release.ps1",
        "installer/native-release-contract.ps1",
    ] {
        std::fs::copy(repository.join(file), source.join(file)).unwrap();
    }
    std::fs::write(
        source.join("package.json"),
        r#"{"name":"the-athanor","version":"0.5.4"}"#,
    )
    .unwrap();
    std::fs::write(source.join("Cargo.toml"), "[workspace]\n").unwrap();
    let release = "0.5.4+occupied";
    let occupied = source.join("target").join("releases").join(release);
    std::fs::create_dir_all(&occupied).unwrap();
    let sentinel = occupied.join("owned-by-another-preparation");
    std::fs::write(&sentinel, b"keep this release").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_athanor"))
        .args(["deploy", "prepare", "--source"])
        .arg(&source)
        .args(["--build", "occupied"])
        .current_dir(temporary.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run the deployment CLI and real PowerShell driver");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(release),
        "the refusal must identify the occupied release, not fail to open its script: {stderr}"
    );
    assert_eq!(std::fs::read(sentinel).unwrap(), b"keep this release");
}

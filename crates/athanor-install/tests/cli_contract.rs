//! The one exe's door contract: every mode is named by its first argument,
//! an unknown or retired name is refused before any runtime file is read,
//! and help lists the doors.

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
    for arguments in [&["--room", "kintsu"][..], &["gui"][..], &["manage"][..], &["athanor-manage"][..]] {
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
fn help_names_every_door() {
    let output = athanor(&["help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for door in ["status", "start", "keeper ROOM", "keeper --config", "chat", "doctor", "install", "service"] {
        assert!(stdout.contains(door), "help must name {door}: {stdout}");
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
    assert!(stderr.contains("\"kodo\""), "the refusal names the room asked for: {stderr}");
    assert!(stderr.contains("harnesses.json"), "the refusal names the registry it read: {stderr}");
    assert!(!stderr.contains("unknown mode"), "{stderr}");
}

#[test]
fn the_chat_mode_names_its_usage() {
    let output = athanor(&["chat", "--room"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage: athanor chat"), "{stderr}");
}

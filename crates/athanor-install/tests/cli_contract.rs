use std::process::Command;

#[test]
fn canonical_executable_rejects_obsolete_modes() {
    for arguments in [&["--room", "kintsu"][..], &["gui"][..]] {
        let output = Command::new(env!("CARGO_BIN_EXE_athanor"))
            .args(arguments)
            .env("ATHANOR_PROGRAM_ROOT", "Z:/does-not-exist")
            .env("ATHANOR_DATA_ROOT", "Z:/does-not-exist")
            .output()
            .expect("run athanor");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("usage: athanor"), "{stderr}");
        assert!(!stderr.contains("runtime.json"), "{stderr}");
    }
}

#[test]
fn manager_rejects_the_removed_gui_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_athanor-manage"))
        .arg("gui")
        .env("ATHANOR_PROGRAM_ROOT", "Z:/does-not-exist")
        .env("ATHANOR_DATA_ROOT", "Z:/does-not-exist")
        .output()
        .expect("run athanor-manage");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown command \"gui\""), "{stderr}");
    assert!(!stderr.contains("Godot"), "{stderr}");
}

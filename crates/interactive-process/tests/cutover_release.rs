#![cfg(windows)]

use interactive_process::{JobOwnedCommand, ProcessWatch};
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!("remove owned process fixture {}: {error}", self.0.display());
        }
    }
}

fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready() {
        assert!(Instant::now() < deadline, "process fixture deadline");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn releasing_the_job_preserves_the_child_after_its_owner_drops() {
    for release in [false, true] {
        let scratch = Scratch(std::env::temp_dir().join(format!(
            "athanor-job-release-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        )));
        fs::create_dir(&scratch.0).unwrap();
        let ready = scratch.0.join("ready");
        let go = scratch.0.join("go");
        let done = scratch.0.join("done");
        let script = r#"
$deadline = (Get-Date).AddSeconds(12)
[IO.File]::WriteAllText($env:ATHANOR_FIXTURE_READY, 'ready')
while (-not (Test-Path -LiteralPath $env:ATHANOR_FIXTURE_GO)) {
    if ((Get-Date) -ge $deadline) { exit 2 }
    Start-Sleep -Milliseconds 20
}
[IO.File]::WriteAllText($env:ATHANOR_FIXTURE_DONE, 'completed after owner drop')
"#;
        let shell = PathBuf::from(std::env::var_os("SystemRoot").expect("Windows system root"))
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut command = JobOwnedCommand::new(shell);
        command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
        command.env("ATHANOR_FIXTURE_READY", &ready);
        command.env("ATHANOR_FIXTURE_GO", &go);
        command.env("ATHANOR_FIXTURE_DONE", &done);
        let mut child = command.spawn().expect("start owned child");
        let watch = ProcessWatch::open(child.id()).expect("watch exact child");
        wait_until(|| ready.exists());

        if release {
            child.release_on_drop().expect("release kill-on-close job");
        }
        drop(child);

        if release {
            thread::sleep(Duration::from_millis(100));
            assert!(
                !watch.has_exited().unwrap(),
                "released child died with its owner"
            );
            fs::write(&go, b"continue").unwrap();
            wait_until(|| watch.has_exited().unwrap());
            assert_eq!(
                fs::read_to_string(&done).unwrap(),
                "completed after owner drop"
            );
        } else {
            wait_until(|| watch.has_exited().unwrap());
            assert!(!done.exists(), "owned child escaped the kill-on-close job");
        }
    }
}

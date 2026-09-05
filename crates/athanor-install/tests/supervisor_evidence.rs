//! A managed child that dies during startup must leave evidence: the error
//! names the child and carries its stderr, the stderr file stays on disk, and
//! the supervisor reports progress while it waits. Real processes only.

#![cfg(windows)]

use athanor_install::supervisor::{
    NativeProcesses, ProcessSpec, StartProgress, Supervisor, START_PROGRESS_INTERVAL,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    net::{Ipv4Addr, SocketAddr, TcpListener},
    path::PathBuf,
    time::Instant,
};

fn cmd_exe() -> PathBuf {
    let system_root = std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows");
    PathBuf::from(system_root).join("System32").join("cmd.exe")
}

fn unused_port() -> SocketAddr {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind an ephemeral port");
    let address = listener.local_addr().expect("read the bound address");
    drop(listener);
    address
}

fn spec(name: &str, script: &str, ready_at: SocketAddr) -> ProcessSpec {
    ProcessSpec {
        name: name.to_owned(),
        executable: cmd_exe(),
        arguments: vec![OsString::from("/d"), OsString::from("/c"), OsString::from(script)],
        environment: BTreeMap::new(),
        ready_at,
    }
}

#[test]
fn a_child_that_exits_before_readiness_names_itself_and_keeps_its_stderr() {
    let logs = tempfile::tempdir().expect("temp log dir");
    let supervisor = Supervisor {
        processes: NativeProcesses::with_log_dir(logs.path().to_path_buf()),
    };
    let specs = vec![spec(
        "broken-child",
        "echo boom: the database is missing 1>&2 & exit 3",
        unused_port(),
    )];
    let mut progress = Vec::new();

    let error = supervisor
        .run(&specs, |name, phase| {
            progress.push((name.to_owned(), phase));
            Ok(())
        })
        .expect_err("a child that exits with 3 must fail the start");

    let message = format!("{error:#}");
    assert!(message.contains("broken-child"), "error names the child: {message}");
    assert!(
        message.contains("exit code: 3"),
        "error carries the exit status: {message}"
    );
    assert!(
        message.contains("boom: the database is missing"),
        "error carries the stderr tail: {message}"
    );
    let kept = std::fs::read_to_string(logs.path().join("broken-child.stderr.log"))
        .expect("stderr file stays on disk");
    assert!(kept.contains("boom: the database is missing"));
    assert_eq!(progress, vec![(String::from("broken-child"), StartProgress::Spawned)]);
}

#[test]
fn a_slow_child_reports_waiting_progress_before_it_is_ready() {
    let logs = tempfile::tempdir().expect("temp log dir");
    let supervisor = Supervisor {
        processes: NativeProcesses::with_log_dir(logs.path().to_path_buf()),
    };
    // The readiness port stays closed until the supervisor reports Waiting
    // once. The child itself only sleeps, longer than one progress interval.
    let ready_at = unused_port();
    let seconds = START_PROGRESS_INTERVAL.as_secs() + 4;
    let specs = vec![spec(
        "slow-child",
        &format!("ping -n {seconds} 127.0.0.1 >nul"),
        ready_at,
    )];
    let mut waiting = 0usize;
    let mut listener: Option<TcpListener> = None;
    let started = Instant::now();

    supervisor
        .run(&specs, |_, phase| {
            if phase == StartProgress::Waiting {
                waiting += 1;
                if listener.is_none() {
                    listener =
                        Some(TcpListener::bind(ready_at).expect("open the readiness port"));
                }
            }
            Ok(())
        })
        .expect("the child becomes ready once the port opens");

    assert!(waiting >= 1, "at least one Waiting report, got {waiting}");
    assert!(
        started.elapsed() >= START_PROGRESS_INTERVAL,
        "readiness could not arrive before the first progress report"
    );
    supervisor.stop(&specs).expect("stop the slow child");
}

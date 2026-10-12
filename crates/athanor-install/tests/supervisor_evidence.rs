//! A managed child that dies during startup must leave evidence: the error
//! names the child and carries its stderr, the stderr file stays on disk, and
//! the supervisor reports progress while it waits. Real processes only.

#![cfg(windows)]

use athanor_install::supervisor::{
    NativeProcesses, ProcessSpec, START_PROGRESS_INTERVAL, StartProgress, Supervisor,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    time::Instant,
};
use tokio::net::TcpSocket;

fn cmd_exe() -> PathBuf {
    let system_root = std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows");
    PathBuf::from(system_root).join("System32").join("cmd.exe")
}

fn reserved_socket() -> TcpSocket {
    let socket = TcpSocket::new_v4().expect("create a readiness socket");
    socket
        .bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .expect("reserve an ephemeral port without listening");
    socket
}

fn spec(name: &str, script: &str, ready_at: SocketAddr) -> ProcessSpec {
    ProcessSpec {
        name: name.to_owned(),
        executable: cmd_exe(),
        arguments: vec![
            OsString::from("/d"),
            OsString::from("/c"),
            OsString::from(script),
        ],
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
    let socket = reserved_socket();
    let specs = vec![spec(
        "broken-child",
        "echo boom: the database is missing 1>&2 & exit 3",
        socket.local_addr().expect("read the reserved address"),
    )];
    let mut progress = Vec::new();

    let error = supervisor
        .run(&specs, |name, phase| {
            progress.push((name.to_owned(), phase));
            Ok(())
        })
        .expect_err("a child that exits with 3 must fail the start");

    let message = format!("{error:#}");
    assert!(
        message.contains("broken-child"),
        "error names the child: {message}"
    );
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
    assert_eq!(
        progress,
        vec![(String::from("broken-child"), StartProgress::Spawned)]
    );
}

#[tokio::test]
async fn a_slow_child_reports_waiting_progress_before_it_is_ready() {
    let logs = tempfile::tempdir().expect("temp log dir");
    let supervisor = Supervisor {
        processes: NativeProcesses::with_log_dir(logs.path().to_path_buf()),
    };
    // Keep the port reserved without accepting connections. Parallel tests
    // cannot claim it and make this child appear ready before Waiting.
    let socket = reserved_socket();
    let ready_at = socket.local_addr().expect("read the reserved address");
    let mut socket = Some(socket);
    let seconds = START_PROGRESS_INTERVAL.as_secs() + 4;
    let specs = vec![spec(
        "slow-child",
        &format!("ping -n {seconds} 127.0.0.1 >nul"),
        ready_at,
    )];
    let mut waiting = 0usize;
    let mut listener = None;
    let started = Instant::now();

    let result = supervisor.run(&specs, |_, phase| {
        if phase == StartProgress::Waiting {
            waiting += 1;
            if let Some(socket) = socket.take() {
                listener = Some(socket.listen(1).expect("open the readiness port"));
            }
        }
        Ok(())
    });
    let elapsed = started.elapsed();
    let stopped = supervisor.stop(&specs);
    result.expect("the child becomes ready once the port opens");
    stopped.expect("stop the slow child");

    assert!(waiting >= 1, "at least one Waiting report, got {waiting}");
    assert!(
        elapsed >= START_PROGRESS_INTERVAL,
        "readiness could not arrive before the first progress report"
    );
}

//! `athanor start [--service]`: start only what `athanor status` reports
//! missing, in dependency order, and say what was done.
//!
//! - The Windows service owns PostgreSQL and NATS. Starting it needs
//!   administrator rights, so it is started only with `--service`; without
//!   the flag, or when the request is denied, the answer is a typed refusal
//!   (`needs_elevation`) and exit code 3 — never a guess.
//! - The Host is this same exe with no arguments, started detached so the
//!   caller (a terminal, Pulse) does not own it. A port already bound is a
//!   Host already running; nothing is started twice.
//! - Keepers are sessions the operator opens; this mode names the command
//!   and starts none.

use super::status;
use crate::{app::installed_runtime, layout::{InstallLayout, SERVICE_NAME}};
use anyhow::{Context, Result};
use protocol::LOOPBACK_HOST;
use serde::Serialize;
use std::{
    net::{SocketAddr, TcpStream},
    process::{Command, ExitCode, Stdio},
    thread,
    time::{Duration, Instant},
};

const HOST_START_TIMEOUT: Duration = Duration::from_secs(20);
const SERVICE_START_TIMEOUT: Duration = Duration::from_secs(90);
const POLL: Duration = Duration::from_millis(250);
const NEEDS_ELEVATION: &str = "needs_elevation";
/// Exit code for "something needs administrator rights"; the caller decides
/// whether to ask for them.
const EXIT_NEEDS_ELEVATION: u8 = 3;

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Refusal {
    pub component: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartReport {
    pub ok: bool,
    pub started: Vec<String>,
    pub skipped: Vec<String>,
    pub refused: Vec<Refusal>,
}

pub fn run(arguments: &[String]) -> Result<ExitCode> {
    let with_service = arguments.iter().any(|argument| argument == "--service");
    let layout = InstallLayout::from_environment()?;
    let report = start(&layout, with_service)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(if report.refused.iter().any(|refusal| refusal.reason == NEEDS_ELEVATION) {
        ExitCode::from(EXIT_NEEDS_ELEVATION)
    } else if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn start(layout: &InstallLayout, with_service: bool) -> Result<StartReport> {
    let before = status::read(layout)?;
    let (config, _) = installed_runtime(layout)?;
    let mut report = StartReport::default();
    let missing = |name: &str| before.missing.iter().any(|entry| entry == name);

    if missing("service") {
        if with_service {
            match start_service()? {
                ServiceStart::Started => report.started.push("service".into()),
                ServiceStart::Denied => report.refused.push(Refusal {
                    component: "service".into(),
                    reason: NEEDS_ELEVATION.into(),
                }),
                ServiceStart::Failed(detail) => report.refused.push(Refusal {
                    component: "service".into(),
                    reason: detail,
                }),
            }
        } else {
            report.refused.push(Refusal {
                component: "service".into(),
                reason: NEEDS_ELEVATION.into(),
            });
        }
    } else {
        report.skipped.push("service: already running".into());
    }

    // PostgreSQL and NATS belong to the service: when it is up they come up
    // with it, so they are waited for, never started here.
    let service_up = !missing("service") || report.started.iter().any(|name| name == "service");
    for (name, host, port) in [
        ("postgres", config.database_host.as_str(), config.database_port),
        ("nats", config.nats_host.as_str(), config.nats_port),
    ] {
        if !missing(name) {
            report.skipped.push(format!("{name}: already running"));
        } else if service_up && wait_for_port(host, port, SERVICE_START_TIMEOUT)? {
            report.started.push(name.into());
        } else {
            report.refused.push(Refusal {
                component: name.into(),
                reason: if service_up {
                    format!("{name} did not listen on {host}:{port} within {} s", SERVICE_START_TIMEOUT.as_secs())
                } else {
                    "waits for the service".into()
                },
            });
        }
    }

    if !missing("host") {
        report.skipped.push("host: already running".into());
    } else if report.refused.is_empty() {
        let app = layout.app();
        let mut command = Command::new(&app);
        command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        detach(&mut command);
        command
            .spawn()
            .with_context(|| format!("start the Host {}", app.display()))?;
        if wait_for_port(LOOPBACK_HOST, config.host_port, HOST_START_TIMEOUT)? {
            report.started.push("host".into());
        } else {
            report.refused.push(Refusal {
                component: "host".into(),
                reason: format!(
                    "Host did not listen on {LOOPBACK_HOST}:{} within {} s",
                    config.host_port,
                    HOST_START_TIMEOUT.as_secs()
                ),
            });
        }
    } else {
        report.refused.push(Refusal {
            component: "host".into(),
            reason: "waits for the service".into(),
        });
    }

    for component in &before.components {
        if component.name.starts_with("keeper:") && component.running == Some(false) {
            report.skipped.push(format!(
                "{}: a room is opened by its operator: athanor keeper --config <room>/.omp/runtime/omp-keeper.json",
                component.name
            ));
        }
    }

    report.ok = report.refused.is_empty();
    Ok(report)
}

enum ServiceStart {
    Started,
    Denied,
    Failed(String),
}

/// `sc start` speaks for the service manager and prints the Win32 code it
/// got: `[SC] StartService FAILED 5:` is access denied, the one answer that
/// means "ask again with administrator rights"; 1056 is already running;
/// every other failure is reported as it came.
fn start_service() -> Result<ServiceStart> {
    let output = Command::new("sc")
        .args(["start", SERVICE_NAME])
        .output()
        .context("run sc start")?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(classify_sc_start(output.status.success(), &text))
}

fn classify_sc_start(success: bool, text: &str) -> ServiceStart {
    let code = text
        .split("FAILED ")
        .nth(1)
        .and_then(|rest| rest.split(':').next())
        .and_then(|code| code.trim().parse::<u32>().ok());
    match (success, code) {
        (true, _) | (_, Some(1056)) => ServiceStart::Started,
        (_, Some(5)) => ServiceStart::Denied,
        _ => ServiceStart::Failed(text.trim().to_owned()),
    }
}

fn wait_for_port(host: &str, port: u16, timeout: Duration) -> Result<bool> {
    let address: SocketAddr = format!("{host}:{port}").parse()?;
    let started = Instant::now();
    loop {
        if TcpStream::connect_timeout(&address, POLL).is_ok() {
            return Ok(true);
        }
        if started.elapsed() >= timeout {
            return Ok(false);
        }
        thread::sleep(POLL);
    }
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: no console of ours, no
    // Ctrl+C of ours. The Host owns its own life from here.
    command.creation_flags(0x0000_0008 | 0x0000_0200);
}

#[cfg(not(windows))]
fn detach(_: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::{ServiceStart, classify_sc_start};

    #[test]
    fn sc_start_answers_are_read_by_their_win32_code() {
        assert!(matches!(classify_sc_start(true, ""), ServiceStart::Started));
        assert!(matches!(
            classify_sc_start(false, "[SC] StartService FAILED 1056:\r\n\r\nAn instance of the service is already running.\r\n"),
            ServiceStart::Started
        ));
        assert!(matches!(
            classify_sc_start(false, "[SC] StartService FAILED 5:\r\n\r\nAccess is denied.\r\n"),
            ServiceStart::Denied
        ));
        assert!(matches!(
            classify_sc_start(false, "[SC] StartService FAILED 1053:\r\n\r\nThe service did not respond.\r\n"),
            ServiceStart::Failed(_)
        ));
    }
}

//! `athanor status`: one JSON matrix of the House's components, each read at
//! the seam it actually lives behind. Four facts per component, each one
//! `null` when this mode has no honest way to know it:
//!
//! - installed: the file or service exists on this machine;
//! - running: a process holds the port, the lock, or the service state;
//! - reachable: the component answered a request;
//! - healthy: the answer said it is well.
//!
//! Nothing here starts, stops, or elevates. `athanor start` reads this same
//! matrix and acts on `missing`.

use crate::{
    app::installed_runtime,
    harness::{HarnessRegistry, registry_path},
    layout::{InstallLayout, SERVICE_NAME},
    supervisor::RuntimeConfig,
};
use anyhow::{Context, Result};
use protocol::LOOPBACK_HOST;
use serde::Serialize;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    process::{Command, ExitCode},
    time::Duration,
};

/// Ollama is not part of the installed runtime; the substrate's embedding
/// lane speaks to it when present. Its port is the one it publishes.
const OLLAMA_PORT: u16 = 11434;
const PROBE_TIMEOUT: Duration = Duration::from_millis(400);
const HTTP_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    pub name: String,
    pub installed: Option<bool>,
    pub running: Option<bool>,
    pub reachable: Option<bool>,
    pub healthy: Option<bool>,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatusReport {
    pub ok: bool,
    pub components: Vec<Component>,
    /// Components the House needs that are not running, in start order.
    pub missing: Vec<String>,
    /// The subset of `missing` that `athanor start` cannot start without
    /// administrator rights.
    pub elevation_required: Vec<String>,
}

pub fn run() -> Result<ExitCode> {
    let layout = InstallLayout::from_environment()?;
    let report = read(&layout)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn read(layout: &InstallLayout) -> Result<StatusReport> {
    let (config, secrets) = installed_runtime(layout)?;
    let mut components = vec![
        service_component(),
        port_component("postgres", &config.database_host, config.database_port, "PostgreSQL"),
        port_component("nats", &config.nats_host, config.nats_port, "NATS"),
        host_component(layout, &config, &secrets.host_token),
    ];
    components.extend(keeper_components(layout)?);
    components.push(port_component("ollama", LOOPBACK_HOST, OLLAMA_PORT, "Ollama (optional)"));

    let missing: Vec<String> = ["service", "postgres", "nats", "host"]
        .iter()
        .filter(|name| {
            components
                .iter()
                .any(|component| component.name == **name && component.running == Some(false))
        })
        .map(|name| (*name).to_owned())
        .collect();
    let elevation_required: Vec<String> = missing
        .iter()
        .filter(|name| name.as_str() == "service")
        .cloned()
        .collect();
    let ok = missing.is_empty()
        && components
            .iter()
            .find(|component| component.name == "host")
            .is_some_and(|host| host.healthy == Some(true));
    Ok(StatusReport {
        ok,
        components,
        missing,
        elevation_required,
    })
}

/// `sc query` needs no rights and answers for every caller; the state line
/// carries RUNNING, STOPPED, or a transition. A missing service exits 1060.
fn service_component() -> Component {
    let output = Command::new("sc").args(["query", SERVICE_NAME]).output();
    match output {
        Ok(output) => {
            let text = String::from_utf8_lossy(&output.stdout);
            let state = text
                .lines()
                .find(|line| line.trim_start().starts_with("STATE"))
                .map(|line| line.split_whitespace().last().unwrap_or("").to_owned());
            match state {
                Some(state) => Component {
                    name: "service".into(),
                    installed: Some(true),
                    running: Some(state == "RUNNING"),
                    reachable: None,
                    healthy: None,
                    detail: format!("{SERVICE_NAME} {state}"),
                },
                None => Component {
                    name: "service".into(),
                    installed: Some(false),
                    running: Some(false),
                    reachable: None,
                    healthy: None,
                    detail: format!(
                        "{SERVICE_NAME} not registered: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    ),
                },
            }
        }
        Err(error) => Component {
            name: "service".into(),
            installed: None,
            running: None,
            reachable: None,
            healthy: None,
            detail: format!("sc query failed: {error}"),
        },
    }
}

fn port_component(name: &str, host: &str, port: u16, label: &str) -> Component {
    let (running, detail) = match probe_port(host, port) {
        Ok(true) => (Some(true), format!("{label} listening on {host}:{port}")),
        Ok(false) => (Some(false), format!("{label} not listening on {host}:{port}")),
        Err(error) => (None, format!("{label} address {host}:{port}: {error}")),
    };
    Component {
        name: name.into(),
        installed: None,
        running,
        reachable: None,
        healthy: None,
        detail,
    }
}

fn probe_port(host: &str, port: u16) -> Result<bool> {
    let address: SocketAddr = format!("{host}:{port}").parse()?;
    Ok(TcpStream::connect_timeout(&address, PROBE_TIMEOUT).is_ok())
}

fn host_component(layout: &InstallLayout, config: &RuntimeConfig, token: &str) -> Component {
    let installed = layout.app().is_file();
    let running = probe_port(LOOPBACK_HOST, config.host_port).unwrap_or(false);
    if !running {
        return Component {
            name: "host".into(),
            installed: Some(installed),
            running: Some(false),
            reachable: Some(false),
            healthy: Some(false),
            detail: format!("Host not listening on {LOOPBACK_HOST}:{}", config.host_port),
        };
    }
    let (reachable, healthy, detail) =
        match health_answer(config.host_port, &config.default_room, token) {
            Ok((status, body)) => {
                let healthy = status == 200 && body.contains("\"status\":\"ok\"");
                (
                    Some(status == 200),
                    Some(healthy),
                    format!("room {} health answered {status}", config.default_room),
                )
            }
            Err(error) => (Some(false), Some(false), format!("health request failed: {error}")),
        };
    Component {
        name: "host".into(),
        installed: Some(installed),
        running: Some(true),
        reachable,
        healthy,
        detail,
    }
}

/// One bare HTTP/1.1 request over std TCP: the Host is loopback and the
/// answer is small, so a client crate would be a dependency with no owner.
fn health_answer(port: u16, room: &str, token: &str) -> Result<(u16, String)> {
    let address: SocketAddr = format!("{LOOPBACK_HOST}:{port}").parse()?;
    let mut stream = TcpStream::connect_timeout(&address, HTTP_TIMEOUT)?;
    stream.set_read_timeout(Some(HTTP_TIMEOUT))?;
    stream.set_write_timeout(Some(HTTP_TIMEOUT))?;
    write!(
        stream,
        "GET /room/{room}/health HTTP/1.1\r\nHost: {LOOPBACK_HOST}:{port}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    )?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer)?;
    let status: u16 = answer
        .split_whitespace()
        .nth(1)
        .context("no status line")?
        .parse()
        .context("status is not a number")?;
    let body = answer.split_once("\r\n\r\n").map(|(_, body)| body).unwrap_or("");
    let body = body.replace(char::is_whitespace, "");
    Ok((status, body))
}

/// Every registered keeper: installed when its config exists, running when a
/// keeper holds the room's lock.
fn keeper_components(layout: &InstallLayout) -> Result<Vec<Component>> {
    let registry = HarnessRegistry::load(&registry_path(layout))?;
    let mut components = Vec::new();
    for spec in registry.specs() {
        let Some(config_path) = spec.launch.keeper_config() else {
            continue;
        };
        let installed = config_path.is_file();
        let (running, detail) = match omp_keeper::keeper::try_hold_lock(&config_path) {
            Ok(None) => (Some(true), format!("keeper holds {}", lock_name(&config_path))),
            Ok(Some(_released)) => (Some(false), format!("no keeper holds {}", lock_name(&config_path))),
            Err(error) => (None, format!("lock probe failed: {error}")),
        };
        components.push(Component {
            name: format!("keeper:{}", spec.harness_id),
            installed: Some(installed),
            running,
            reachable: None,
            healthy: None,
            detail,
        });
    }
    Ok(components)
}

fn lock_name(config_path: &Path) -> String {
    omp_keeper::keeper::lock_path(config_path).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_port_reads_as_not_running_and_a_bad_address_as_unknown() {
        let closed = port_component("nats", LOOPBACK_HOST, 1, "NATS");
        assert_eq!(closed.running, Some(false));
        let bad = port_component("nats", "not an address", 1, "NATS");
        assert_eq!(bad.running, None);
        assert!(bad.detail.contains("not an address"));
    }
}

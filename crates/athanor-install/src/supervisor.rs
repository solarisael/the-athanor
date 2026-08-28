use crate::contract::LOOPBACK_HOST;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::GetLastError,
    System::{
        Console::{
            AllocConsole, CTRL_BREAK_EVENT, FreeConsole, GenerateConsoleCtrlEvent,
            SetConsoleCtrlHandler,
        },
        Threading::CREATE_NEW_PROCESS_GROUP,
    },
};

pub const START_TIMEOUT: Duration = Duration::from_secs(90);
pub const STOP_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(windows)]
unsafe extern "system" fn ignore_supervisor_console_control(_: u32) -> i32 {
    1
}

#[cfg(windows)]
pub fn prepare_service_console() -> Result<()> {
    // An SCM service normally has no console. Own one private console so every
    // managed child can inherit it while living in its own process group.
    unsafe { FreeConsole() };
    if unsafe { AllocConsole() } == 0 {
        bail!("AllocConsole failed with {}", unsafe { GetLastError() });
    }
    if unsafe { SetConsoleCtrlHandler(Some(ignore_supervisor_console_control), 1) } == 0 {
        let error = unsafe { GetLastError() };
        unsafe { FreeConsole() };
        bail!("SetConsoleCtrlHandler failed with {error}");
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Readiness {
    Tcp(SocketAddr),
    File(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessSpec {
    pub name: String,
    pub executable: PathBuf,
    pub arguments: Vec<OsString>,
    pub environment: BTreeMap<String, String>,
    pub readiness: Readiness,
}

pub trait Processes {
    fn spawn(&self, spec: &ProcessSpec) -> Result<u32>;
    fn ready(&self, name: &str, readiness: &Readiness) -> Result<bool>;
    fn request_stop(&self, name: &str) -> Result<()>;
    fn wait_exit(&self, name: &str, timeout: Duration) -> Result<bool>;
    fn kill_verified(&self, name: &str) -> Result<()>;
}

#[derive(Default)]
pub struct NativeProcesses {
    children: Mutex<BTreeMap<String, Child>>,
}
impl Processes for NativeProcesses {
    fn spawn(&self, spec: &ProcessSpec) -> Result<u32> {
        if let Readiness::File(path) = &spec.readiness {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("remove stale readiness file {}", path.display())
                    });
                }
            }
        }
        let mut command = Command::new(&spec.executable);
        command
            .args(&spec.arguments)
            .envs(&spec.environment)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .current_dir(
                spec.executable
                    .parent()
                    .context("managed executable has no parent")?,
            );
        #[cfg(windows)]
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
        let child = command
            .spawn()
            .with_context(|| format!("start managed {}", spec.name))?;
        let pid = child.id();
        self.children
            .lock()
            .unwrap()
            .insert(spec.name.clone(), child);
        Ok(pid)
    }
    fn ready(&self, name: &str, readiness: &Readiness) -> Result<bool> {
        if let Some(status) = self
            .children
            .lock()
            .unwrap()
            .get_mut(name)
            .with_context(|| format!("managed child {name} is not owned by this supervisor"))?
            .try_wait()?
        {
            bail!("managed child {name} exited before readiness with {status}");
        }
        match readiness {
            Readiness::Tcp(address) => {
                Ok(TcpStream::connect_timeout(address, Duration::from_millis(250)).is_ok())
            }
            Readiness::File(path) => Ok(path.is_file()),
        }
    }
    fn request_stop(&self, name: &str) -> Result<()> {
        #[cfg(windows)]
        {
            let process_group = self
                .children
                .lock()
                .unwrap()
                .get(name)
                .with_context(|| format!("managed child {name} is not owned by this supervisor"))?
                .id();
            if unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, process_group) } == 0 {
                bail!(
                    "CTRL_BREAK_EVENT for managed child {name} failed with {}",
                    unsafe { GetLastError() }
                );
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            bail!("graceful managed stop is supported only on Windows")
        }
    }
    fn wait_exit(&self, name: &str, timeout: Duration) -> Result<bool> {
        let deadline = Instant::now() + timeout;
        loop {
            let exited = self
                .children
                .lock()
                .unwrap()
                .get_mut(name)
                .map(|child| child.try_wait())
                .transpose()?
                .flatten()
                .is_some();
            if exited {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    fn kill_verified(&self, name: &str) -> Result<()> {
        let mut children = self.children.lock().unwrap();
        let child = children
            .get_mut(name)
            .with_context(|| format!("refusing to kill unverified process {name}"))?;
        child.kill()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostRoomConfig {
    pub room: String,
    pub spirit: String,
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorConfig {
    pub database_mode: String,
    pub database_host: String,
    pub database_port: u16,
    pub nats_host: String,
    pub nats_port: u16,
    pub rooms_root: PathBuf,
    pub house_id: String,
    pub rooms: Vec<HostRoomConfig>,
}

pub struct Supervisor<P> {
    pub processes: P,
}
impl<P: Processes> Supervisor<P> {
    pub fn run<F>(&self, specs: &[ProcessSpec], mut checkpoint: F) -> Result<()>
    where
        F: FnMut(u32, &str) -> Result<()>,
    {
        if specs.is_empty() {
            bail!("managed runtime plan has no children");
        }
        let mut started = Vec::new();
        for (index, spec) in specs.iter().enumerate() {
            let name = spec.name.as_str();
            if let Err(error) = self.processes.spawn(spec) {
                self.stop_names(&started)?;
                return Err(error);
            }
            started.push(name.to_owned());
            checkpoint((index + 1) as u32, name)?;
            let deadline = Instant::now() + START_TIMEOUT;
            loop {
                let ready = match self.processes.ready(name, &spec.readiness) {
                    Ok(ready) => ready,
                    Err(error) => {
                        if let Err(cleanup_error) = self.stop_names(&started) {
                            return Err(error.context(format!(
                                "managed startup cleanup also failed: {cleanup_error:#}"
                            )));
                        }
                        return Err(error);
                    }
                };
                if ready {
                    break;
                }
                if Instant::now() >= deadline {
                    self.stop_names(&started)?;
                    bail!(
                        "managed child {name} did not become ready within {} seconds",
                        START_TIMEOUT.as_secs()
                    );
                }
                thread::sleep(Duration::from_millis(250));
            }
        }
        Ok(())
    }

    pub fn stop(&self, specs: &[ProcessSpec]) -> Result<()> {
        let started = specs
            .iter()
            .map(|spec| spec.name.clone())
            .collect::<Vec<_>>();
        self.stop_names(&started)
    }

    fn stop_names(&self, started: &[String]) -> Result<()> {
        let mut failures = Vec::new();
        for name in started.iter().rev() {
            if let Err(error) = self.processes.request_stop(name) {
                failures.push(format!("{name}: {error}"));
                continue;
            }
            match self.processes.wait_exit(name, STOP_TIMEOUT) {
                Ok(true) => {}
                Ok(false) => {
                    if let Err(error) = self.processes.kill_verified(name) {
                        failures.push(format!("{name}: {error}"));
                    }
                }
                Err(error) => failures.push(format!("{name}: {error}")),
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            bail!("managed shutdown failures: {}", failures.join("; "))
        }
    }
}

pub fn runtime_plan(
    version_root: &Path,
    data_root: &Path,
    config: &SupervisorConfig,
    database_url: &str,
    host_token: &str,
) -> Result<Vec<ProcessSpec>> {
    let loopback = |host: &str, port: u16| -> Result<SocketAddr> {
        let address: SocketAddr = format!("{host}:{port}").parse()?;
        if !address.ip().is_loopback() {
            bail!("managed service address {address} must be loopback");
        }
        Ok(address)
    };
    let database = loopback(&config.database_host, config.database_port)?;
    let nats = loopback(&config.nats_host, config.nats_port)?;
    if config.house_id.trim().is_empty() {
        bail!("houseId must not be empty");
    }
    if !config.rooms_root.is_absolute() {
        bail!("roomsRoot must be absolute");
    }
    if config.rooms.is_empty() {
        bail!("at least one Host room is required");
    }
    let mut room_keys = std::collections::BTreeSet::new();
    let mut host_ports = std::collections::BTreeSet::new();
    for room in &config.rooms {
        let safe_room = !room.room.is_empty()
            && room.room != "house"
            && !room.room.starts_with('-')
            && !room.room.ends_with('-')
            && !room.room.contains("--")
            && room
                .room
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if !safe_room || room.spirit.trim().is_empty() {
            bail!("Host room identity is invalid for {:?}", room.room);
        }
        if !room_keys.insert(room.room.clone()) {
            bail!("duplicate Host room {:?}", room.room);
        }
        if !host_ports.insert(room.port)
            || room.port == config.database_port
            || room.port == config.nats_port
        {
            bail!("Host port {} is duplicate or reserved", room.port);
        }
        loopback(LOOPBACK_HOST, room.port)?;
    }

    let mut specs = Vec::new();
    if config.database_mode == "managed" {
        specs.push(ProcessSpec {
            name: "postgresql".into(),
            executable: version_root.join("runtime/postgresql/bin/postgres.exe"),
            arguments: vec![
                "-D".into(),
                data_root.join("data/postgresql").into_os_string(),
                "-h".into(),
                LOOPBACK_HOST.into(),
                "-p".into(),
                config.database_port.to_string().into(),
            ],
            environment: BTreeMap::new(),
            readiness: Readiness::Tcp(database),
        });
    } else if config.database_mode != "external" {
        bail!("databaseMode must be managed or external");
    }
    specs.push(ProcessSpec {
        name: "nats".into(),
        executable: version_root.join("runtime/nats/nats-server.exe"),
        arguments: vec![
            "-js".into(),
            "-a".into(),
            LOOPBACK_HOST.into(),
            "-p".into(),
            config.nats_port.to_string().into(),
            "-sd".into(),
            data_root.join("data/nats").into_os_string(),
        ],
        environment: BTreeMap::new(),
        readiness: Readiness::Tcp(nats),
    });
    let common = BTreeMap::from([
        ("DATABASE_URL".into(), database_url.into()),
        (
            "ATHANOR_NATS_URL".into(),
            format!("nats://{LOOPBACK_HOST}:{}", config.nats_port),
        ),
    ]);
    let delivery_executable = version_root.join("bin/athanor-house-delivery.exe");
    let delivery_ready = data_root.join("state/delivery.ready");
    let mut delivery_environment = common.clone();
    delivery_environment.insert(
        "ATHANOR_DELIVERY_READY_FILE".into(),
        delivery_ready.display().to_string(),
    );
    specs.push(ProcessSpec {
        name: "delivery".into(),
        executable: delivery_executable,
        arguments: vec!["run".into()],
        environment: delivery_environment,
        readiness: Readiness::File(delivery_ready),
    });
    for room in &config.rooms {
        let mut host_env = common.clone();
        host_env.insert("ATHANOR_HOST_TOKEN".into(), host_token.into());
        host_env.insert(
            "ATHANOR_HOST_ROOM_DIR".into(),
            config.rooms_root.join(&room.room).display().to_string(),
        );
        host_env.insert(
            "ATHANOR_HOST_STATE_DIR".into(),
            data_root
                .join("state/host")
                .join(&room.room)
                .display()
                .to_string(),
        );
        host_env.insert("ATHANOR_HOST_HOUSE_ID".into(), config.house_id.clone());
        host_env.insert("ATHANOR_HOST_ROOM".into(), room.room.clone());
        host_env.insert("ATHANOR_HOST_SPIRIT".into(), room.spirit.clone());
        host_env.insert(
            "ATHANOR_HOST_SESSION".into(),
            format!("managed:{}", room.room),
        );
        host_env.insert(
            "ATHANOR_HOST_BIND".into(),
            format!("{LOOPBACK_HOST}:{}", room.port),
        );
        specs.push(ProcessSpec {
            name: format!("host:{}", room.room),
            executable: version_root.join("bin/house-host.exe"),
            arguments: Vec::new(),
            environment: host_env,
            readiness: Readiness::Tcp(loopback(LOOPBACK_HOST, room.port)?),
        });
    }
    Ok(specs)
}

use anyhow::{Context, Result, bail};
use protocol::{LOOPBACK_HOST, is_safe_room_key};
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
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
/// How often a waiting start reports progress, so a slow child never looks hung.
pub const START_PROGRESS_INTERVAL: Duration = Duration::from_secs(5);
/// The most child stderr a startup failure carries. The file keeps the rest.
pub const STDERR_TAIL_BYTES: u64 = 2048;
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
pub struct ProcessSpec {
    pub name: String,
    pub executable: PathBuf,
    pub arguments: Vec<OsString>,
    pub environment: BTreeMap<String, String>,
    /// The child is ready once this loopback port accepts a connection.
    pub ready_at: SocketAddr,
}

pub trait Processes {
    fn spawn(&self, spec: &ProcessSpec) -> Result<u32>;
    fn ready(&self, name: &str, address: &SocketAddr) -> Result<bool>;
    fn request_stop(&self, name: &str) -> Result<()>;
    fn wait_exit(&self, name: &str, timeout: Duration) -> Result<bool>;
    fn kill_verified(&self, name: &str) -> Result<()>;
}

/// What the supervisor reports while it starts a child.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartProgress {
    /// The child process exists. Its readiness wait begins now.
    Spawned,
    /// The child is still starting. Nothing failed yet.
    Waiting,
}

#[derive(Default)]
pub struct NativeProcesses {
    children: Mutex<BTreeMap<String, Child>>,
    /// Each child's stderr lands in `<log_dir>/<name>.stderr.log`. `None`
    /// discards it.
    log_dir: Option<PathBuf>,
}

impl NativeProcesses {
    /// Keep every managed child's stderr under `log_dir`.
    pub fn with_log_dir(log_dir: PathBuf) -> Self {
        Self {
            children: Mutex::default(),
            log_dir: Some(log_dir),
        }
    }

    fn stderr_path(&self, name: &str) -> Option<PathBuf> {
        self.log_dir
            .as_ref()
            .map(|dir| dir.join(format!("{name}.stderr.log")))
    }

    /// The last `STDERR_TAIL_BYTES` of the child's stderr, or a plain note when
    /// nothing was kept.
    pub fn stderr_tail(&self, name: &str) -> String {
        let Some(path) = self.stderr_path(name) else {
            return String::from("(stderr not kept)");
        };
        read_tail(&path, STDERR_TAIL_BYTES).unwrap_or_else(|_| String::from("(stderr not kept)"))
    }
}

fn read_tail(path: &Path, bytes: u64) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    if length > bytes {
        file.seek(SeekFrom::Start(length - bytes))?;
    }
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    let text = String::from_utf8_lossy(&buffer).trim().to_owned();
    Ok(if text.is_empty() {
        String::from("(empty)")
    } else {
        text
    })
}
impl Processes for NativeProcesses {
    fn spawn(&self, spec: &ProcessSpec) -> Result<u32> {
        let stderr = match self.stderr_path(&spec.name) {
            Some(path) => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("create log directory for {}", spec.name))?;
                }
                let file = File::create(&path)
                    .with_context(|| format!("open stderr log for {}", spec.name))?;
                Stdio::from(file)
            }
            None => Stdio::null(),
        };
        let mut command = Command::new(&spec.executable);
        command
            .args(&spec.arguments)
            .envs(&spec.environment)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(stderr)
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
    fn ready(&self, name: &str, address: &SocketAddr) -> Result<bool> {
        if let Some(status) = self
            .children
            .lock()
            .unwrap()
            .get_mut(name)
            .with_context(|| format!("managed child {name} is not owned by this supervisor"))?
            .try_wait()?
        {
            bail!(
                "managed child {name} exited before readiness with {status}; stderr tail: {}",
                self.stderr_tail(name)
            );
        }
        Ok(TcpStream::connect_timeout(address, Duration::from_millis(250)).is_ok())
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
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeConfig {
    pub database_mode: String,
    pub database_host: String,
    pub database_port: u16,
    pub nats_host: String,
    pub nats_port: u16,
    pub host_port: u16,
    pub schema_version: u32,
    pub house_id: String,
    pub rooms_root: PathBuf,
    pub operator_state_root: PathBuf,
    pub default_room: String,
    pub rooms: Vec<HostRoomConfig>,
    pub omp_config_path: Option<PathBuf>,
    pub client_config_path: Option<PathBuf>,
}

impl RuntimeConfig {
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.database_mode.as_str(), "managed" | "external") {
            bail!("databaseMode must be managed or external");
        }
        if self.house_id.trim().is_empty() {
            bail!("houseId must not be empty");
        }
        if !self.rooms_root.is_absolute() || !self.operator_state_root.is_absolute() {
            bail!("roomsRoot and operatorStateRoot must be absolute");
        }
        self.validate_ports()?;
        self.validate_rooms()
    }

    fn validate_ports(&self) -> Result<()> {
        let ports = [
            loopback_address(&self.database_host, self.database_port)?.port(),
            loopback_address(&self.nats_host, self.nats_port)?.port(),
            loopback_address(LOOPBACK_HOST, self.host_port)?.port(),
        ];
        if ports.contains(&0) {
            bail!("runtime ports must be nonzero");
        }
        if ports
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != ports.len()
        {
            bail!("database, NATS, and Host ports must be distinct");
        }
        Ok(())
    }

    fn validate_rooms(&self) -> Result<()> {
        if self.rooms.is_empty() {
            bail!("at least one Host room is required");
        }
        let mut rooms = std::collections::BTreeSet::new();
        for room in &self.rooms {
            if !is_safe_room_key(&room.room) || room.spirit.trim().is_empty() {
                bail!("Host room identity is invalid for {:?}", room.room);
            }
            if !rooms.insert(room.room.clone()) {
                bail!("duplicate Host room {:?}", room.room);
            }
        }
        if !rooms.contains(&self.default_room) {
            bail!("defaultRoom must name one configured room");
        }
        Ok(())
    }
}

fn loopback_address(host: &str, port: u16) -> Result<SocketAddr> {
    let address: SocketAddr = format!("{host}:{port}").parse()?;
    if !address.ip().is_loopback() {
        bail!("managed service address {address} must be loopback");
    }
    Ok(address)
}

pub struct Supervisor<P> {
    pub processes: P,
}
impl<P: Processes> Supervisor<P> {
    pub fn run<F>(&self, specs: &[ProcessSpec], mut progress: F) -> Result<()>
    where
        F: FnMut(&str, StartProgress) -> Result<()>,
    {
        if specs.is_empty() {
            bail!("managed runtime plan has no children");
        }
        let mut started = Vec::new();
        for spec in specs {
            let name = spec.name.as_str();
            if let Err(error) = self.processes.spawn(spec) {
                self.stop_names(&started)?;
                return Err(error);
            }
            started.push(name.to_owned());
            progress(name, StartProgress::Spawned)?;
            let deadline = Instant::now() + START_TIMEOUT;
            let mut next_progress = Instant::now() + START_PROGRESS_INTERVAL;
            loop {
                let ready = match self.processes.ready(name, &spec.ready_at) {
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
                if Instant::now() >= next_progress {
                    progress(name, StartProgress::Waiting)?;
                    next_progress = Instant::now() + START_PROGRESS_INTERVAL;
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
    config: &RuntimeConfig,
) -> Result<Vec<ProcessSpec>> {
    config.validate()?;
    let database = loopback_address(&config.database_host, config.database_port)?;
    let nats = loopback_address(&config.nats_host, config.nats_port)?;

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
            ready_at: database,
        });
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
        ready_at: nats,
    });
    Ok(specs)
}

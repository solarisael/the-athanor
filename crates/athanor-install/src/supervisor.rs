use anyhow::{Context, Result, bail};
use serde::Deserialize;
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

pub const START_ORDER: &[&str] = &["postgresql", "nats", "delivery", "host"];
pub const STOP_ORDER: &[&str] = &["host", "delivery", "nats", "postgresql"];
pub const START_TIMEOUT: Duration = Duration::from_secs(90);
pub const STOP_TIMEOUT: Duration = Duration::from_secs(30);
fn default_room() -> String {
    "home".into()
}
fn default_house() -> String {
    "local".into()
}
fn default_spirit() -> String {
    "athanor".into()
}
fn default_session() -> String {
    "managed".into()
}
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
    Command {
        executable: PathBuf,
        arguments: Vec<OsString>,
        environment: BTreeMap<String, String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessSpec {
    pub name: &'static str,
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
            .insert(spec.name.into(), child);
        Ok(pid)
    }
    fn ready(&self, name: &str, readiness: &Readiness) -> Result<bool> {
        if self
            .children
            .lock()
            .unwrap()
            .get_mut(name)
            .is_some_and(|child| child.try_wait().ok().flatten().is_some())
        {
            return Ok(false);
        }
        match readiness {
            Readiness::Tcp(address) => {
                Ok(TcpStream::connect_timeout(address, Duration::from_millis(250)).is_ok())
            }
            Readiness::Command {
                executable,
                arguments,
                environment,
            } => Ok(Command::new(executable)
                .args(arguments)
                .envs(environment)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())),
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorConfig {
    pub database_mode: String,
    pub database_host: String,
    pub database_port: u16,
    pub nats_host: String,
    pub nats_port: u16,
    #[serde(default = "default_room")]
    pub room: String,
    #[serde(default = "default_house")]
    pub house_id: String,
    #[serde(default = "default_spirit")]
    pub spirit: String,
    #[serde(default = "default_session")]
    pub session: String,
}

pub struct Supervisor<P> {
    pub processes: P,
}
impl<P: Processes> Supervisor<P> {
    pub fn run<F>(&self, specs: &[ProcessSpec], mut checkpoint: F) -> Result<()>
    where
        F: FnMut(u32, &str) -> Result<()>,
    {
        let mut started: Vec<&str> = Vec::new();
        for (index, name) in START_ORDER.iter().enumerate() {
            let Some(spec) = specs.iter().find(|spec| spec.name == *name) else {
                if *name == "postgresql" {
                    continue;
                }
                bail!("managed runtime plan is missing required child {name}");
            };
            self.processes.spawn(spec)?;
            started.push(name);
            checkpoint((index + 1) as u32, name)?;
            let deadline = Instant::now() + START_TIMEOUT;
            while !self.processes.ready(name, &spec.readiness)? {
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
        let started: Vec<&str> = specs.iter().map(|spec| spec.name).collect();
        self.stop_names(&started)
    }

    fn stop_names(&self, started: &[&str]) -> Result<()> {
        let mut failures = Vec::new();
        for name in STOP_ORDER.iter().filter(|name| started.contains(name)) {
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
    let mut specs = Vec::new();
    if config.database_mode == "managed" {
        specs.push(ProcessSpec {
            name: "postgresql",
            executable: version_root.join("runtime/postgresql/bin/postgres.exe"),
            arguments: vec![
                "-D".into(),
                data_root.join("data/postgresql").into_os_string(),
                "-h".into(),
                "127.0.0.1".into(),
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
        name: "nats",
        executable: version_root.join("runtime/nats/nats-server.exe"),
        arguments: vec![
            "-js".into(),
            "-a".into(),
            "127.0.0.1".into(),
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
            "SOLARISAEL_NATS_URL".into(),
            format!("nats://127.0.0.1:{}", config.nats_port),
        ),
    ]);
    let delivery_executable = version_root.join("bin/athanor-house-delivery.exe");
    specs.push(ProcessSpec {
        name: "delivery",
        executable: delivery_executable.clone(),
        arguments: vec!["run".into()],
        environment: common.clone(),
        readiness: Readiness::Command {
            executable: delivery_executable,
            arguments: vec!["health".into()],
            environment: common.clone(),
        },
    });
    let mut host_env = common;
    host_env.insert("ATHANOR_HOST_TOKEN".into(), host_token.into());
    host_env.insert(
        "ATHANOR_HOST_ROOM_DIR".into(),
        data_root
            .join("rooms")
            .join(&config.room)
            .display()
            .to_string(),
    );
    host_env.insert(
        "ATHANOR_HOST_STATE_DIR".into(),
        data_root.join("state/host").display().to_string(),
    );
    host_env.insert("ATHANOR_HOST_HOUSE_ID".into(), config.house_id.clone());
    host_env.insert("ATHANOR_HOST_ROOM".into(), config.room.clone());
    host_env.insert("ATHANOR_HOST_SPIRIT".into(), config.spirit.clone());
    host_env.insert("ATHANOR_HOST_SESSION".into(), config.session.clone());
    specs.push(ProcessSpec {
        name: "host",
        executable: version_root.join("bin/house-host.exe"),
        arguments: Vec::new(),
        environment: host_env,
        readiness: Readiness::Tcp("127.0.0.1:8787".parse().unwrap()),
    });
    Ok(specs)
}

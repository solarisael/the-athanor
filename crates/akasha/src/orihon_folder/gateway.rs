//! The hosted backend: one `omp auth-gateway stdio` child per engine. Requests share the child
//! by id; replies arrive in completion order. The child loads its own credentials and settings;
//! source text crosses only its stdin. The stdio wire carries `{id, status, body}` and no HTTP
//! headers, and it answers each request whole, so a caller learns nothing before the reply ends.

use crate::AppError;
use hearth::orihon::FolderRuntime;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

const EXACT_ROUTE: &str =
    "retry:\n  modelFallback: false\nproviders:\n  anthropic:\n    serverSideFallback: false\n";
const EXIT_TIMEOUT: Duration = Duration::from_secs(5);
/// The longest line the child writes; the model catalog is the largest reply.
const LINE_BYTES: usize = 1024 * 1024;
/// Request lines queued for the writer. Engine permits keep fewer than this in flight.
const QUEUED_LINES: usize = origami::orihon::live::MAX_JOBS + 1;

fn failure(message: &str) -> AppError {
    AppError::Protocol(message.into())
}

pub fn hosted_route(model: &str) -> Result<(&str, &'static str), AppError> {
    if model.len() > 256
        || !model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-/".contains(&byte))
    {
        return Err(failure(
            "Orihon hosted folding requires an exact provider/model selector",
        ));
    }
    let (provider, _) = model
        .split_once('/')
        .filter(|(_, id)| !id.is_empty())
        .ok_or_else(|| {
            failure("Orihon hosted folding requires an exact provider/model selector")
        })?;
    let api = match provider {
        "anthropic" => "anthropic-messages",
        "openai-codex" => "openai-codex-responses",
        _ => {
            return Err(failure(
                "Orihon hosted folding supports only anthropic and openai-codex model routes",
            ));
        }
    };
    Ok((provider, api))
}

pub fn validate_runtime(runtime: &FolderRuntime) -> Result<(), AppError> {
    for value in [
        &runtime.executable,
        &runtime.home,
        &runtime.agent_dir,
        &runtime.cwd,
    ]
    .into_iter()
    .chain(runtime.config_files.iter())
    .chain(runtime.entrypoint.iter())
    {
        if value.len() > 4096
            || value.contains(['\0', '\n', '\r'])
            || !Path::new(value).is_absolute()
        {
            return Err(failure(
                "Orihon OMP binding requires absolute executable, home, agent directory, working directory, and config paths",
            ));
        }
    }
    let delimiter = if cfg!(windows) { ';' } else { ':' };
    if runtime.config_files.len() > 32
        || runtime
            .config_files
            .iter()
            .map(|path| path.len())
            .sum::<usize>()
            + runtime.config_files.len().saturating_sub(1)
            > 16_384
        || runtime
            .config_files
            .iter()
            .any(|path| path.contains(delimiter))
    {
        return Err(failure(
            "Orihon OMP config selection exceeds its bounds or cannot be represented by PI_CONFIG_FILES",
        ));
    }
    let name = Path::new(&runtime.executable)
        .file_name()
        .and_then(|name| name.to_str());
    let supported = match (name, runtime.entrypoint.as_deref()) {
        (Some(name), None) => name.eq_ignore_ascii_case("omp.exe") || name == "omp",
        (Some(name), Some(entrypoint)) => {
            (name.eq_ignore_ascii_case("bun.exe") || name == "bun")
                && Path::new(entrypoint)
                    .file_name()
                    .is_some_and(|name| name == "cli.js" || name == "cli.ts")
        }
        _ => false,
    };
    if !supported {
        return Err(failure(
            "Orihon hosted folding requires an OMP executable or its Bun CLI entrypoint",
        ));
    }
    for (key, value) in &runtime.environment {
        protocol::launch::validate_profile_value(key, value)
            .map_err(|_| failure("Orihon OMP profile binding is invalid"))?;
    }
    if runtime.environment.get("PI_CODING_AGENT_DIR") != Some(&runtime.agent_dir)
        || !runtime.environment.contains_key("OMP_PROFILE")
        || !runtime.environment.contains_key("PI_CONFIG_DIR")
    {
        return Err(failure(
            "Orihon OMP binding must explicitly select its agent directory, profile, and config directory",
        ));
    }
    Ok(())
}

struct Overlay(PathBuf);

impl Overlay {
    fn create(agent_dir: &Path) -> Result<Self, AppError> {
        let directory = agent_dir.join(format!(".orihon-gateway-{}", uuid::Uuid::new_v4()));
        let builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        builder
            .create(&directory)
            .map_err(|_| failure("Orihon OMP scoped configuration could not be created"))?;
        let overlay = Self(directory);
        std::fs::write(overlay.path(), EXACT_ROUTE)
            .map_err(|_| failure("Orihon OMP scoped configuration could not be written"))?;
        Ok(overlay)
    }

    fn path(&self) -> PathBuf {
        self.0.join("config.yml")
    }

    fn remove(&self) -> Result<(), AppError> {
        std::fs::remove_file(self.path())
            .map_err(|_| failure("Orihon OMP scoped configuration cleanup failed"))?;
        std::fs::remove_dir(&self.0)
            .map_err(|_| failure("Orihon OMP scoped directory cleanup failed"))
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        // Normal shutdown reports cleanup errors. This only covers failed startup or unwinding.
        let _ = std::fs::remove_file(self.path());
        let _ = std::fs::remove_dir(&self.0);
    }
}

/// Replies the reader has not delivered. It closes for good when the child's stdout ends or
/// breaks; dropping the senders then fails every waiting request.
struct Waiting {
    open: bool,
    replies: HashMap<u64, oneshot::Sender<Value>>,
}

fn lock(waiting: &Mutex<Waiting>) -> MutexGuard<'_, Waiting> {
    // Critical sections only move map entries; a poisoned table is still consistent.
    waiting.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Everything the child owns, released together by `Process::shutdown`.
struct Process {
    child: Child,
    writer: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
    stderr: JoinHandle<std::io::Result<u64>>,
    overlay: Overlay,
}

impl Process {
    /// Graceful shutdown waits for the exit that follows stdin EOF, then kills after the exit
    /// timeout; a forced one kills at once. Both await every task the child fed.
    async fn shutdown(mut self, graceful: bool) -> Result<(), AppError> {
        let exit = if graceful {
            match tokio::time::timeout(EXIT_TIMEOUT, self.child.wait()).await {
                Ok(result) => result.map(Some),
                Err(_) => self.child.kill().await.map(|()| None),
            }
        } else {
            match self.child.try_wait() {
                Ok(Some(status)) => Ok(Some(status)),
                _ => self.child.kill().await.map(|()| None),
            }
        };
        // The exit closes both pipes, so the reader meets EOF and the writer's next write fails.
        for task in [self.writer.take(), self.reader.take()]
            .into_iter()
            .flatten()
        {
            let mut task = task;
            if tokio::time::timeout(EXIT_TIMEOUT, &mut task).await.is_err() {
                task.abort();
                let _ = task.await;
            }
        }
        let stderr = match tokio::time::timeout(EXIT_TIMEOUT, &mut self.stderr).await {
            Ok(result) => result
                .map_err(|_| failure("Orihon OMP stderr drain did not finish"))?
                .map_err(|_| failure("Orihon OMP stderr drain failed")),
            Err(_) => {
                self.stderr.abort();
                let _ = (&mut self.stderr).await;
                Err(failure("Orihon OMP stderr did not close after child exit"))
            }
        };
        let cleanup = self.overlay.remove();
        let exit = exit.map_err(|_| failure("Orihon OMP gateway terminal wait failed"))?;
        stderr?;
        cleanup?;
        if graceful && !exit.is_some_and(|status| status.success()) {
            return Err(failure(
                "Orihon OMP gateway did not exit successfully after EOF",
            ));
        }
        Ok(())
    }
}

pub(super) struct Gateway {
    lines: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    waiting: Arc<Mutex<Waiting>>,
    next_id: AtomicU64,
    process: tokio::sync::Mutex<Option<Process>>,
}

#[derive(Serialize)]
struct Frame<'a, B: Serialize> {
    id: u64,
    method: &'static str,
    path: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<&'a B>,
}

impl Gateway {
    /// Starts the bound child, reads its ready line, and proves `model` is in its chat catalog
    /// before any source is sent. Each wait is bounded by `timeout`.
    pub async fn start(
        runtime: &FolderRuntime,
        model: &str,
        timeout: Duration,
    ) -> Result<Self, AppError> {
        let (provider, api) = hosted_route(model)?;
        let (mut child, overlay) = spawn(runtime)?;
        let stdin = child.stdin.take().expect("piped gateway stdin");
        let mut stdout = BufReader::new(child.stdout.take().expect("piped gateway stdout"));
        let mut stderr = child.stderr.take().expect("piped gateway stderr");
        // Provider errors on stderr can carry credentials; they are drained, never kept.
        let stderr =
            tokio::spawn(async move { tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await });
        let mut process = Process {
            child,
            writer: None,
            reader: None,
            stderr,
            overlay,
        };
        let ready = match tokio::time::timeout(timeout, line(&mut stdout)).await {
            Ok(Ok(ready))
                if ready["ready"] == true
                    && ready["version"]
                        .as_str()
                        .is_some_and(|version| !version.is_empty()) =>
            {
                Ok(())
            }
            _ => Err(failure(
                "Orihon requires OMP auth-gateway stdio with a version handshake",
            )),
        };
        if let Err(error) = ready {
            let _ = process.shutdown(false).await;
            return Err(error);
        }

        let waiting = Arc::new(Mutex::new(Waiting {
            open: true,
            replies: HashMap::new(),
        }));
        let (sender, receiver) = mpsc::channel(QUEUED_LINES);
        process.writer = Some(tokio::spawn(write(stdin, receiver)));
        process.reader = Some(tokio::spawn(read(stdout, Arc::clone(&waiting))));
        let gateway = Self {
            lines: Mutex::new(Some(sender)),
            waiting,
            next_id: AtomicU64::new(0),
            process: tokio::sync::Mutex::new(Some(process)),
        };

        let catalog = match gateway.send::<Value>("GET", "/v1/models", None).await {
            Ok((id, reply)) => match tokio::time::timeout(timeout, reply).await {
                Ok(Ok(catalog)) => Some(catalog),
                Ok(Err(_)) => None,
                Err(_) => {
                    gateway.forget(id);
                    None
                }
            },
            Err(_) => None,
        };
        let listed = catalog.is_some_and(|catalog| {
            catalog["status"] == 200
                && catalog["body"]["data"].as_array().is_some_and(|rows| {
                    rows.iter().any(|row| {
                        row["id"] == model
                            && row["owned_by"] == provider
                            && row["api"] == api
                            && (row.get("kind").is_none() || row["kind"] == "chat")
                    })
                })
        });
        if !listed {
            gateway.shutdown(false).await;
            return Err(failure(
                "Orihon provider/model and supported API pair is not available in the bound OMP chat catalog; no source sent",
            ));
        }
        Ok(gateway)
    }

    /// False once the child's stdout ended or broke; no later request can be answered.
    pub fn alive(&self) -> bool {
        lock(&self.waiting).open
    }

    /// Queues one request line and returns its id and reply slot. A queued request cannot be
    /// withdrawn: stdio has no abort, so the caller awaits the reply or `forget`s the id.
    pub async fn send<B: Serialize>(
        &self,
        method: &'static str,
        path: &'static str,
        body: Option<&B>,
    ) -> Result<(u64, oneshot::Receiver<Value>), String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut line = serde_json::to_vec(&Frame {
            id,
            method,
            path,
            body,
        })
        .map_err(|_| "the gateway request could not be encoded".to_owned())?;
        line.push(b'\n');
        let Some(lines) = self
            .lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        else {
            return Err("the gateway is closed".into());
        };
        let (sender, receiver) = oneshot::channel();
        {
            let mut waiting = lock(&self.waiting);
            if !waiting.open {
                return Err("the gateway exited".into());
            }
            waiting.replies.insert(id, sender);
        }
        // A whole line is queued or nothing is: the writer task alone touches stdin.
        if lines.send(line).await.is_err() {
            self.forget(id);
            return Err("the gateway stopped taking requests".into());
        }
        Ok((id, receiver))
    }

    /// Drops the reply slot of a request whose caller stopped waiting. A reply that arrives
    /// later is discarded with its usage.
    pub fn forget(&self, id: u64) {
        lock(&self.waiting).replies.remove(&id);
    }

    /// Ends stdin, waits for the child to answer what it holds and exit, then releases the
    /// overlay. Calls after the first return `Ok`.
    pub async fn close(&self) -> Result<(), AppError> {
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        match self.process.lock().await.take() {
            Some(process) => process.shutdown(true).await,
            None => Ok(()),
        }
    }

    /// Failed startup: kill without the graceful wait. The startup error is the one reported.
    async fn shutdown(&self, graceful: bool) {
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(process) = self.process.lock().await.take() {
            let _ = process.shutdown(graceful).await;
        }
    }
}

/// Reads one bounded JSON line. EOF, an oversize line, or invalid JSON ends the stream.
async fn line(stdout: &mut BufReader<ChildStdout>) -> Result<Value, AppError> {
    let mut bytes = Vec::new();
    // Take bounds the allocation even when a child never writes a newline.
    let count = (&mut *stdout)
        .take((LINE_BYTES + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .await
        .map_err(|_| failure("Orihon OMP gateway response was interrupted"))?;
    if count == 0 || count > LINE_BYTES || !bytes.ends_with(b"\n") {
        return Err(failure(
            "Orihon OMP gateway response is missing, incomplete, or exceeds its byte ceiling",
        ));
    }
    serde_json::from_slice(&bytes).map_err(|_| failure("Orihon OMP gateway returned invalid JSONL"))
}

async fn write(mut stdin: ChildStdin, mut lines: mpsc::Receiver<Vec<u8>>) {
    while let Some(line) = lines.recv().await {
        if stdin.write_all(&line).await.is_err() {
            break;
        }
    }
    // Dropping stdin here is the EOF that ends the child's serving loop.
}

async fn read(mut stdout: BufReader<ChildStdout>, waiting: Arc<Mutex<Waiting>>) {
    while let Ok(reply) = line(&mut stdout).await {
        let slot = reply
            .get("id")
            .and_then(Value::as_u64)
            .and_then(|id| lock(&waiting).replies.remove(&id));
        if let Some(slot) = slot {
            let _ = slot.send(reply);
        }
    }
    let mut waiting = lock(&waiting);
    waiting.open = false;
    waiting.replies.clear();
}

// No await between spawn and handing the child to its `Process` owner.
fn spawn(runtime: &FolderRuntime) -> Result<(Child, Overlay), AppError> {
    validate_runtime(runtime)?;
    let home_key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let host_home = std::env::var_os(home_key).and_then(|path| std::fs::canonicalize(path).ok());
    let operator_home = std::fs::canonicalize(&runtime.home).ok();
    if operator_home.is_none() || host_home != operator_home {
        return Err(failure(
            "Orihon hosted folding requires the bound operator's Host account; SYSTEM or another user's defaults are refused",
        ));
    }
    if !Path::new(&runtime.executable).is_file()
        || !Path::new(&runtime.agent_dir).is_dir()
        || runtime
            .entrypoint
            .as_ref()
            .is_some_and(|path| !Path::new(path).is_file())
    {
        return Err(failure(
            "Orihon bound OMP executable or agent directory is unavailable; configure fold hosted again",
        ));
    }
    let overlay = Overlay::create(Path::new(&runtime.agent_dir))?;
    let mut command = Command::new(&runtime.executable);
    command.args(runtime.entrypoint.iter());
    command
        .args(["auth-gateway", "stdio"])
        .current_dir(&runtime.cwd)
        .env_clear();
    // OMP loads its own auth/config. Never inherit Host provider keys or a different OMP profile.
    for key in [
        "PATH",
        "SYSTEMROOT",
        "SYSTEMDRIVE",
        "COMSPEC",
        "TEMP",
        "TMP",
        "HOME",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
        "APPDATA",
        "LOCALAPPDATA",
        "PATHEXT",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let configs = std::env::join_paths(
        runtime
            .config_files
            .iter()
            .map(PathBuf::from)
            .chain(std::iter::once(overlay.path())),
    )
    .map_err(|_| failure("Orihon OMP config selection could not be encoded"))?;
    command
        .envs(&runtime.environment)
        .env("PI_CONFIG_FILES", configs)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let child = command
        .spawn()
        .map_err(|_| failure("Orihon could not start the bound OMP auth-gateway stdio runtime"))?;
    Ok((child, overlay))
}

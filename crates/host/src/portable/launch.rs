//! Tern relaunch through the Athanor launch helper.
//!
//! Tern starts a new tab's program with Tern's own environment, never the
//! caller's (observed: a variable set on `tern.com` did not reach the child),
//! so the scoped room environment cannot ride on the Tern call. The Host writes
//! the exact launch into a one-use manifest in its private state directory and
//! Tern starts `athanor launch-session --manifest <path>` in the new tab; that
//! helper (athanor-install cli/launch.rs) claims and removes the manifest,
//! installs the environment and runs the harness with the pane's console.
//! Only the manifest path is on any command line; nothing is typed.

use super::restart::id_text;
use super::{Refusal, Terminal};
use crate::config::HostConfig;
use interactive_process::ProcessWatch;
use protocol::launch::{
    LAUNCH_SESSION_MODE, LaunchManifest, MANIFEST_DIR, MANIFEST_FLAG, MANIFEST_FORMAT,
    MANIFEST_TTL_SECS, PROFILE_ENV, is_launch_id, validate_profile_value,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// What the adapter registered for relaunching its conversation: the harness
/// program and argv, and the OMP profile selection of the live process.
#[derive(Clone, Debug)]
pub(super) struct Launch {
    program: String,
    args: Vec<String>,
    /// Only `PROFILE_ENV` keys; an absent key is absent in the successor too.
    profile: BTreeMap<String, String>,
}

/// The ids Tern printed for the opened tab. `block` is the new pane, which the
/// successor reports as its TERN_PANE.
pub(super) struct Opened {
    pub(super) session: String,
    pub(super) tab: String,
    pub(super) block: String,
}

fn shim(program: &str) -> bool {
    Path::new(program)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
        })
}

const CMD_METACHARACTERS: [char; 8] = ['&', '|', '<', '>', '^', '%', '"', '!'];

impl Launch {
    /// Attach-time validation of `launcher: {kind:'tern', program, args, environment}`.
    /// cmd.exe re-parses a .cmd/.bat shim's arguments, so those refuse what it
    /// would expand. Session selectors are the Host's to append.
    pub(super) fn validated(
        program: &str,
        args: &[String],
        environment: BTreeMap<String, String>,
    ) -> Result<Self, Refusal> {
        if !Path::new(program).is_absolute() {
            return Err(Refusal::invalid(
                "launcher.program must be an absolute path",
            ));
        }
        if !Path::new(program).is_file() {
            return Err(Refusal::invalid("launcher.program does not exist"));
        }
        if args.len() > 256 {
            return Err(Refusal::invalid("launcher.args is too long"));
        }
        for arg in args {
            if arg.len() > 4096 || arg.contains(['\0', '\n', '\r']) {
                return Err(Refusal::invalid("launcher.args holds an unsafe argument"));
            }
            if matches!(
                arg.as_str(),
                "--resume" | "-r" | "--session" | "--restore" | "--fork" | "-c" | "--continue"
            ) || arg.starts_with("--resume=")
                || arg.starts_with("--session=")
            {
                return Err(Refusal::invalid(
                    "launcher.args must not select a session; the Host appends --resume",
                ));
            }
        }
        if shim(program) && args.iter().any(|arg| arg.contains(CMD_METACHARACTERS)) {
            return Err(Refusal::invalid(
                "launcher.args carries cmd metacharacters for a shim program",
            ));
        }
        for (key, value) in &environment {
            if !PROFILE_ENV.contains(&key.as_str()) {
                return Err(Refusal::invalid(format!(
                    "launcher.environment may carry only {}; {key} is not carried",
                    PROFILE_ENV.join(", ")
                )));
            }
            validate_profile_value(key, value)
                .map_err(|error| Refusal::invalid(format!("launcher.environment: {error}")))?;
        }
        Ok(Self {
            program: program.to_owned(),
            args: args.to_vec(),
            profile: environment,
        })
    }

    /// The registration advertises capability, never authority. The program
    /// must be the executable image the Host itself observes behind the
    /// attaching process handle, and the terminal must be the Tern the Host's
    /// own context resolves: a caller can never name a different executable.
    /// The Host only ever runs that Tern and its own `athanor` helper; the
    /// harness program is started by the user's Tern in the new pane, with the
    /// Tern user's rights, never the Host's.
    pub(super) fn bind(
        self,
        watch: Option<&ProcessWatch>,
        terminal: Option<&Terminal>,
    ) -> Result<Self, String> {
        let watch = watch.ok_or("the Host holds no handle on the attaching process")?;
        let image = watch
            .image_path()
            .map_err(|error| format!("the attaching process image is unreadable: {error}"))?;
        if !same_file(&image, Path::new(&self.program)) {
            return Err(format!(
                "launcher.program {} is not the attaching process image {}",
                self.program,
                image.display()
            ));
        }
        let terminal = terminal.ok_or("a tern launcher needs terminal identity")?;
        let trusted = trusted_tern()?;
        let claimed = Path::new(&terminal.executable).parent();
        if !claimed
            .is_some_and(|claimed| trusted.parent().is_some_and(|dir| same_file(claimed, dir)))
        {
            return Err(format!(
                "terminal.executable {} is not the Host's installed Tern {}",
                terminal.executable,
                trusted.display()
            ));
        }
        Ok(self)
    }
}

fn same_file(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) if cfg!(windows) => {
            left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase()
        }
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// The helper is the normal `athanor` executable: the installed Host is that
/// executable itself; the source Host (`athanor-host`) has it beside it.
fn helper_executable() -> Result<PathBuf, Refusal> {
    let current = std::env::current_exe().map_err(|error| {
        Refusal::internal(format!("the Host executable path is unknown: {error}"))
    })?;
    let name = format!("athanor{}", std::env::consts::EXE_SUFFIX);
    let is_helper = current
        .file_name()
        .and_then(|file| file.to_str())
        .is_some_and(|file| file.eq_ignore_ascii_case(&name));
    let helper = if is_helper {
        current
    } else {
        current
            .parent()
            .ok_or_else(|| Refusal::internal("the Host executable has no directory"))?
            .join(&name)
    };
    if !helper.is_file() {
        return Err(Refusal::internal(format!(
            "the Athanor launch helper {} is missing beside the Host",
            helper.display()
        )));
    }
    Ok(helper)
}

/// The Tern CLI the Host's own context resolves: its LOCALAPPDATA install,
/// then its PATH. On Windows only the console shim answers with JSON.
///
/// Requirement: Tern relaunch needs a Host running as the operator in the
/// operator's interactive session (`athanor start` / the source
/// `athanor-host`), because its Tern CLI must reach the operator's Tern and
/// the operator's helper must read the manifest. A Windows-service Host
/// (another account, session 0) has no such Tern and reports the launcher
/// unsupported; that is the correct answer, not a fault.
fn trusted_tern() -> Result<PathBuf, String> {
    let name = if cfg!(windows) { "tern.com" } else { "tern" };
    let mut candidates = Vec::new();
    if cfg!(windows)
        && let Some(local) = std::env::var_os("LOCALAPPDATA")
    {
        candidates.push(
            PathBuf::from(local)
                .join("Programs")
                .join("Tern")
                .join(name),
        );
    }
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(
            std::env::split_paths(&path)
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join(name)),
        );
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            format!(
                "Tern relaunch needs a Host running in the operator's interactive session; \
             this Host's context has no {name} (a Windows-service Host reports this)"
            )
        })
}

/// Owner-only access for the manifest directory and file, applied explicitly
/// rather than trusting whatever the state directory inherited: inheritance
/// removed, full control to the Host's own user (the operator in the only
/// supported mode, see `trusted_tern`), SYSTEM and Administrators. The same
/// grant set as the installer's `NativeFileSystem::restrict_user_acl`
/// (athanor-install boundaries.rs), which this crate cannot depend on.
#[cfg(windows)]
fn restrict_to_host_user(path: &Path, directory: bool) -> Result<(), String> {
    let system32 = std::env::var_os("SystemRoot")
        .map(|root| PathBuf::from(root).join("System32"))
        .ok_or("SystemRoot is not set")?;
    let output = Command::new(system32.join("whoami.exe"))
        .args(["/user", "/fo", "csv", "/nh"])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("whoami could not run: {error}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let sid = text
        .trim()
        .rsplit(',')
        .next()
        .map(|field| field.trim().trim_matches('"'))
        .filter(|sid| {
            sid.starts_with("S-1-")
                && sid
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '-' || c == 'S')
        })
        .ok_or("the Host user's SID could not be read")?
        .to_owned();
    if !output.status.success() {
        return Err("whoami failed".into());
    }
    let inherit = if directory { "(OI)(CI)" } else { "" };
    let status = Command::new(system32.join("icacls.exe"))
        .arg(path)
        .args([
            "/inheritance:r",
            "/grant:r",
            &format!("*{sid}:{inherit}F"),
            &format!("*S-1-5-18:{inherit}F"),
            &format!("*S-1-5-32-544:{inherit}F"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("icacls could not run: {error}"))?;
    if !status.success() {
        return Err(format!("icacls could not restrict {}", path.display()));
    }
    Ok(())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// A manifest whose tab never started its helper is removed by the next launch.
fn sweep_stale(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let stale = Duration::from_secs(MANIFEST_TTL_SECS * 2);
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let id = name
            .strip_suffix(".json.claimed")
            .or_else(|| name.strip_suffix(".json"));
        if !id.is_some_and(is_launch_id) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > stale);
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Created empty with create-new, made owner-only, and only then written, so
/// the bearer never sits in a file with an inherited (possibly permissive)
/// access list. Unix gets 0700/0600.
fn write_manifest(state_dir: &Path, manifest: &LaunchManifest) -> Result<PathBuf, Refusal> {
    let dir = state_dir.join(MANIFEST_DIR);
    fs::create_dir_all(&dir).map_err(|error| {
        Refusal::internal(format!(
            "the launch manifest directory could not be created: {error}"
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|error| {
            Refusal::internal(format!(
                "the launch manifest directory could not be made private: {error}"
            ))
        })?;
    }
    #[cfg(windows)]
    restrict_to_host_user(&dir, true).map_err(|error| {
        Refusal::internal(format!(
            "the launch manifest directory could not be made private: {error}"
        ))
    })?;
    sweep_stale(&dir);
    let path = dir.join(format!("{}.json", manifest.launch_id));
    let bytes = serde_json::to_vec(manifest).map_err(|error| {
        Refusal::internal(format!("the launch manifest could not be encoded: {error}"))
    })?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = options
        .open(&path)
        .map_err(|error| error.to_string())
        .and_then(|mut file| {
            #[cfg(windows)]
            restrict_to_host_user(&path, false)?;
            file.write_all(&bytes).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())
        });
    if let Err(error) = written {
        let _ = fs::remove_file(&path);
        return Err(Refusal::internal(format!(
            "the launch manifest could not be written privately: {error}"
        )));
    }
    Ok(path)
}

/// Blocking. Writes the one-use manifest, then runs
/// `tern new tab SESSION --cwd WORKSPACE --keep-open --json -- athanor launch-session --manifest PATH`.
/// --keep-open keeps this successor's pane when it later restarts in turn.
/// Any failure removes the manifest before answering.
pub(super) fn new_tab(
    config: &HostConfig,
    terminal: &Terminal,
    workspace: &str,
    launch: &Launch,
    resume: Option<&str>,
) -> Result<Opened, Refusal> {
    if shim(&launch.program) && resume.is_some_and(|file| file.contains(CMD_METACHARACTERS)) {
        return Err(Refusal::internal(
            "the session file path carries cmd metacharacters for a shim program",
        ));
    }
    let helper = helper_executable()?;
    // Never the caller's path: the Tern the Host's own context resolves.
    let tern = trusted_tern().map_err(Refusal::internal)?;
    let mut args = launch.args.clone();
    if let Some(session_file) = resume {
        args.extend(["--resume".to_owned(), session_file.to_owned()]);
    }
    let mut environment = launch.profile.clone();
    environment.insert(
        "ATHANOR_CLIENT_ROOM_DIR".into(),
        config.room_dir.display().to_string(),
    );
    environment.insert("ATHANOR_HOST_URL".into(), format!("ws://{}", config.bind));
    environment.insert("ATHANOR_HOST_TOKEN".into(), config.bearer_token.clone());
    environment.insert("ATHANOR_HOST_HOUSE_ID".into(), config.house_id.clone());
    let manifest = LaunchManifest {
        format: MANIFEST_FORMAT,
        launch_id: Uuid::new_v4().hyphenated().to_string(),
        expires_at_unix: unix_now() + MANIFEST_TTL_SECS,
        program: launch.program.clone(),
        args,
        cwd: workspace.to_owned(),
        environment,
    };
    manifest
        .validate()
        .map_err(|error| Refusal::internal(format!("the launch manifest is invalid: {error}")))?;
    let path = write_manifest(&config.state_dir, &manifest)?;
    let opened = open_tab(&tern, terminal, workspace, &helper, &path);
    match &opened {
        // Ids and the launch id only; the manifest's environment is never logged.
        Ok(tab) => tracing::info!(
            launch_id = %manifest.launch_id,
            session = %tab.session,
            tab = %tab.tab,
            block = %tab.block,
            "tern opened the relaunch tab"
        ),
        Err(_) => {
            // A failed tab never ran the helper; a claimed manifest is already gone.
            let _ = fs::remove_file(&path);
        }
    }
    opened
}

fn terminal_command(tern: &Path, terminal: &Terminal) -> Command {
    let mut command = Command::new(tern);
    command
        .env("TERN_DAEMON_SOCKET", &terminal.socket)
        .env("TERN_PANE_SOCKET", &terminal.socket)
        .env("TERN_PANE", &terminal.pane)
        .env_remove("TERN_WINDOW_KEY")
        .env_remove("TERN_WINDOW_SOCKET");
    command
}

pub(super) fn preserve_predecessor(terminal: &Terminal) -> Result<(), Refusal> {
    let tern = trusted_tern().map_err(Refusal::internal)?;
    let output = terminal_command(&tern, terminal)
        .args([
            "keep-open",
            &format!("{}/{}", terminal.session, terminal.pane),
            "on",
            "--json",
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| {
            Refusal::internal(format!(
                "the predecessor pane could not be preserved: {error}"
            ))
        })?;
    if !output.status.success() {
        return Err(Refusal::internal(format!(
            "Tern refused to preserve the predecessor pane: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn encoded_powershell_launch(helper: &Path, manifest: &Path) -> Result<String, Refusal> {
    use base64::Engine;

    let helper = helper
        .to_str()
        .ok_or_else(|| Refusal::internal("the helper path is not UTF-8"))?;
    let manifest = manifest
        .to_str()
        .ok_or_else(|| Refusal::internal("the manifest path is not UTF-8"))?;
    let mut script = String::with_capacity(helper.len() + manifest.len() + 96);
    script.push_str("& ");
    for (index, value) in [helper, LAUNCH_SESSION_MODE, MANIFEST_FLAG, manifest]
        .into_iter()
        .enumerate()
    {
        if index != 0 {
            script.push(' ');
        }
        script.push('\'');
        for character in value.chars() {
            script.push(character);
            if character == '\'' {
                script.push('\'');
            }
        }
        script.push('\'');
    }
    script.push_str("; exit $LASTEXITCODE");
    let mut utf16 = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(&utf16))
}

fn open_tab(
    tern: &Path,
    terminal: &Terminal,
    workspace: &str,
    helper: &Path,
    manifest: &Path,
) -> Result<Opened, Refusal> {
    let mut command = terminal_command(tern, terminal);
    command.args([
        "new",
        "tab",
        terminal.session.as_str(),
        "--cwd",
        workspace,
        "--keep-open",
        "--json",
        "--",
    ]);
    // Tern renders argv through the user's shell. A quoted PowerShell command
    // path needs `&`; an encoded invocation also preserves spaces and quotes.
    #[cfg(windows)]
    command.args([
        "powershell.exe",
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-EncodedCommand",
        &encoded_powershell_launch(helper, manifest)?,
    ]);
    #[cfg(not(windows))]
    command
        .arg(helper)
        .args([LAUNCH_SESSION_MODE, MANIFEST_FLAG])
        .arg(manifest);
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|error| Refusal::internal(format!("tern could not start: {error}")))?;
    if !output.status.success() {
        return Err(Refusal::internal(format!(
            "tern new tab failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let found: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| Refusal::internal(format!("tern new tab printed no JSON: {error}")))?;
    let field = |name: &str| {
        id_text(&found[name])
            .ok_or_else(|| Refusal::internal(format!("tern new tab returned no {name} id")))
    };
    let opened = Opened {
        session: field("session")?,
        tab: field("tab")?,
        block: field("block")?,
    };
    if opened.session != terminal.session {
        return Err(Refusal::internal(format!(
            "tern opened the tab in session {}, not {}",
            opened.session, terminal.session
        )));
    }
    Ok(opened)
}

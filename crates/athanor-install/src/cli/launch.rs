//! `athanor launch-session --manifest <path>`: the far end of a Host-driven
//! Tern relaunch (host portable/launch.rs). Tern starts this in the new tab
//! with Tern's own environment. It claims the Host's one-use manifest, removes
//! it, installs the exact room scope and OMP profile selection, runs the
//! harness in the pane's console with the manifest's cwd, and answers with the
//! harness exit code. Nothing from the manifest is printed.

use anyhow::{Context, Result, bail};
use protocol::launch::{
    CLEARED_ENV, LaunchManifest, MANIFEST_DIR, MANIFEST_FLAG, MANIFEST_MAX_BYTES, PROFILE_ENV,
    ROOM_ENV, is_launch_id,
};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str =
    "usage: athanor launch-session --manifest <stateDir>/portable-launch/<launchId>.json";

pub fn run(arguments: &[String]) -> Result<ExitCode> {
    if arguments.len() != 2 || arguments[0] != MANIFEST_FLAG {
        bail!("{USAGE}");
    }
    let manifest = claim(Path::new(&arguments[1]))?;
    ignore_console_interrupts()?;
    let mut command = Command::new(&manifest.program);
    command.args(&manifest.args).current_dir(&manifest.cwd);
    // An inherited value never stands in for one the predecessor lacked.
    for key in ROOM_ENV.iter().chain(&PROFILE_ENV).chain(&CLEARED_ENV) {
        command.env_remove(key);
    }
    command.envs(&manifest.environment);
    // stdio is inherited: the harness owns the Tern pane's console.
    let mut child = command
        .spawn()
        .with_context(|| format!("the harness {} could not start", manifest.program))?;
    drop(manifest);
    let status = child.wait().context("the harness could not be waited on")?;
    Ok(match status.code() {
        Some(code) => u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from),
        None => ExitCode::FAILURE,
    })
}

/// One use: renaming the file to its claimed name is the single winner's act,
/// and the claimed file is removed before the harness starts. The manifest is
/// bound to its name (`<launchId>.json` in `portable-launch`), is a plain file
/// (never a link), bounded, unexpired, and structurally valid.
fn claim(path: &Path) -> Result<LaunchManifest> {
    if !path.is_absolute() {
        bail!("the launch manifest path must be absolute");
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("the launch manifest has no file name")?;
    let launch_id = name
        .strip_suffix(".json")
        .filter(|id| is_launch_id(id))
        .context("the launch manifest is not named <launchId>.json")?
        .to_owned();
    let dir = path
        .parent()
        .context("the launch manifest has no directory")?;
    if dir.file_name().and_then(|name| name.to_str()) != Some(MANIFEST_DIR) {
        bail!("the launch manifest is not in a {MANIFEST_DIR} directory");
    }
    for checked in [dir, path] {
        let metadata = fs::symlink_metadata(checked)
            .with_context(|| format!("the launch manifest {} is not available", path.display()))?;
        if metadata.file_type().is_symlink() {
            bail!("the launch manifest path holds a link");
        }
    }
    let claimed: PathBuf = dir.join(format!("{name}.claimed"));
    fs::rename(path, &claimed)
        .context("the launch manifest was already claimed or is not accessible")?;
    let bytes = read_bounded(&claimed);
    let removed = fs::remove_file(&claimed);
    let bytes = bytes?;
    removed.context("the claimed launch manifest could not be removed")?;
    let manifest: LaunchManifest =
        serde_json::from_slice(&bytes).context("the launch manifest is not valid JSON")?;
    manifest
        .validate()
        .map_err(|error| anyhow::anyhow!("the launch manifest is invalid: {error}"))?;
    if manifest.launch_id != launch_id {
        bail!("the launch manifest content does not belong to its file name");
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(u64::MAX, |elapsed| elapsed.as_secs());
    if now > manifest.expires_at_unix {
        bail!("the launch manifest expired before the tab started");
    }
    Ok(manifest)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let file = fs::File::open(path).context("the claimed launch manifest could not be opened")?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MANIFEST_MAX_BYTES {
        bail!("the launch manifest is not a bounded regular file");
    }
    let mut bytes = Vec::new();
    file.take(MANIFEST_MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MANIFEST_MAX_BYTES {
        bail!("the launch manifest is too large");
    }
    Ok(bytes)
}

/// The harness shares this console and reads Ctrl+C itself; the helper must
/// outlive it so the pane's root process ends only when the harness does. A
/// handler routine (unlike a NULL handler) is not inherited by the child.
#[cfg(windows)]
fn ignore_console_interrupts() -> Result<()> {
    use windows_sys::Win32::System::Console::{
        CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler,
    };
    unsafe extern "system" fn interrupts(control: u32) -> i32 {
        i32::from(control == CTRL_C_EVENT || control == CTRL_BREAK_EVENT)
    }
    if unsafe { SetConsoleCtrlHandler(Some(interrupts), 1) } == 0 {
        bail!(
            "SetConsoleCtrlHandler failed: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn ignore_console_interrupts() -> Result<()> {
    // No console control handler exists off Windows; the helper keeps default
    // signal handling (no extra dependency is taken for a Tern-on-Unix path).
    Ok(())
}

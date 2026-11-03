//! The cutover pause: a native deployment replacing the release while omp runs.
//!
//! `substrate/deploy-local.ps1` writes `<stateRoot>/cutover/cutover.json`
//! before it installs and removes it after the health proof. While the marker
//! stands the keeper starts no substrate child and closes the one it holds, so
//! no keeper process reaches PostgreSQL or NATS across the migration. The omp
//! child keeps running. The keeper then records one acknowledgement per
//! cutover beside the marker as `keeper-<pid>.json`; the deployment waits for
//! every running keeper's acknowledgement and refuses a keeper that never
//! writes one, because a keeper built before this pause ignores the marker.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const CUTOVER_DIRECTORY: &str = "cutover";
pub const MARKER_FILE_NAME: &str = "cutover.json";
/// A marker that exists but cannot be read still pauses: only its absence
/// permits a substrate child. Such a pause is never acknowledged, so the
/// deployment that wrote a damaged marker refuses instead of proceeding.
const UNREADABLE: &str = "unreadable";
const MAX_ID_LENGTH: usize = 128;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Marker {
    cutover_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Acknowledgement<'a> {
    cutover_id: &'a str,
    pid: u32,
}

/// The cutover this keeper last acknowledged, so a paused keeper writes once
/// per cutover instead of once per child poll.
static ACKNOWLEDGED: Mutex<Option<String>> = Mutex::new(None);

/// The id of the cutover a deployment holds open, or None when none does.
pub fn active(state_root: &Path) -> Option<String> {
    let bytes = match std::fs::read(marker_path(state_root)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => return Some(UNREADABLE.into()),
    };

    match serde_json::from_slice::<Marker>(&bytes) {
        Ok(marker) if valid_id(&marker.cutover_id) => Some(marker.cutover_id),
        _ => Some(UNREADABLE.into()),
    }
}

pub fn marker_path(state_root: &Path) -> PathBuf {
    state_root.join(CUTOVER_DIRECTORY).join(MARKER_FILE_NAME)
}

pub fn ack_path(state_root: &Path, pid: u32) -> PathBuf {
    state_root
        .join(CUTOVER_DIRECTORY)
        .join(format!("keeper-{pid}.json"))
}

/// Records that this keeper holds no substrate child and starts none while
/// `cutover_id` stands. Call it only after the held child is closed.
pub fn acknowledge(state_root: &Path, cutover_id: &str) {
    if cutover_id == UNREADABLE {
        return;
    }
    let mut last = ACKNOWLEDGED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if last.as_deref() == Some(cutover_id) {
        return;
    }

    // One attempt per cutover: a failed write leaves the deployment without
    // this keeper's acknowledgement, and that deployment refuses by name.
    *last = Some(cutover_id.to_owned());
    match write_ack(state_root, cutover_id) {
        Ok(path) => println!(
            "omp-keeper: cutover {cutover_id} in progress; no substrate child starts until it ends ({})",
            path.display()
        ),
        Err(error) => {
            eprintln!("omp-keeper: cutover {cutover_id} could not be acknowledged: {error:#}")
        }
    }
}

fn write_ack(state_root: &Path, cutover_id: &str) -> Result<PathBuf> {
    let pid = std::process::id();
    let path = ack_path(state_root, pid);
    let body = serde_json::to_vec(&Acknowledgement { cutover_id, pid })?;
    let staged = path.with_extension("json.tmp");

    std::fs::write(&staged, body).with_context(|| format!("write {}", staged.display()))?;
    std::fs::rename(&staged, &path).with_context(|| format!("publish {}", path.display()))?;
    Ok(path)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_marker(state_root: &Path, body: &[u8]) {
        let path = marker_path(state_root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn absent_marker_permits_and_damaged_marker_pauses() {
        let state = tempfile::tempdir().unwrap();
        assert_eq!(active(state.path()), None);

        write_marker(state.path(), b"{not json");
        assert_eq!(active(state.path()).as_deref(), Some(UNREADABLE));

        write_marker(state.path(), br#"{"cutoverId":"../x"}"#);
        assert_eq!(active(state.path()).as_deref(), Some(UNREADABLE));

        write_marker(
            state.path(),
            br#"{"format":1,"cutoverId":"c-1","release":"0.11.0"}"#,
        );
        assert_eq!(active(state.path()).as_deref(), Some("c-1"));
    }

    #[test]
    fn damaged_marker_is_never_acknowledged() {
        let state = tempfile::tempdir().unwrap();
        write_marker(state.path(), b"{not json");
        acknowledge(state.path(), UNREADABLE);
        assert!(!ack_path(state.path(), std::process::id()).exists());
    }
}

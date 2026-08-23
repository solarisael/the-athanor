use crate::error::VaultError;
use crate::walk::{normalize_lexical, normalized_path};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const ROOM_MARKER: &str = ".solarisael-room.json";
const DEFAULT_MAX_FILE_BYTES: u64 = 512 * 1024;
const DEFAULT_MAX_FILES: usize = 5_000;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoomMarker {
    #[serde(default)]
    vault_roots: Vec<String>,
    #[serde(default)]
    vault_ignore: Vec<String>,
    vault_max_file_bytes: Option<u64>,
    vault_max_files: Option<usize>,
}
pub(crate) struct VaultConfig {
    pub(crate) roots: Vec<PathBuf>,
    pub(crate) ignore: Vec<String>,
    pub(crate) max_file_bytes: u64,
    pub(crate) max_files: usize,
}

fn bounded_u64(value: Option<u64>, fallback: u64, minimum: u64, maximum: u64) -> u64 {
    value
        .filter(|value| (*value >= minimum) && (*value <= maximum))
        .unwrap_or(fallback)
}
fn bounded_usize(value: Option<usize>, fallback: usize, minimum: usize, maximum: usize) -> usize {
    value
        .filter(|value| (*value >= minimum) && (*value <= maximum))
        .unwrap_or(fallback)
}

pub(crate) fn load_config(room_dir: &Path) -> Result<VaultConfig, VaultError> {
    let metadata = fs::symlink_metadata(room_dir).map_err(|error| {
        VaultError::InvalidRoomDirectory(format!(
            "Vault room unavailable: {} ({error})",
            normalized_path(room_dir)
        ))
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(VaultError::InvalidRoomDirectory(format!(
            "Vault room is not a direct readable directory: {}",
            normalized_path(room_dir)
        )));
    }
    let marker = fs::read_to_string(room_dir.join(ROOM_MARKER))
        .ok()
        .and_then(|body| serde_json::from_str::<RoomMarker>(&body).ok())
        .unwrap_or_default();
    let configured = if marker
        .vault_roots
        .iter()
        .any(|root| !root.trim().is_empty())
    {
        marker.vault_roots
    } else {
        vec![".".into()]
    };
    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    for root in configured {
        let trimmed = root.trim();
        if trimmed.is_empty() {
            continue;
        }
        let candidate = if Path::new(trimmed).is_absolute() {
            PathBuf::from(trimmed)
        } else {
            room_dir.join(trimmed)
        };
        let candidate = normalize_lexical(&candidate);
        if seen.insert(normalized_path(&candidate)) {
            roots.push(candidate);
        }
    }
    Ok(VaultConfig {
        roots,
        ignore: marker
            .vault_ignore
            .into_iter()
            .filter(|rule| !rule.trim().is_empty())
            .collect(),
        max_file_bytes: bounded_u64(
            marker.vault_max_file_bytes,
            DEFAULT_MAX_FILE_BYTES,
            16 * 1024,
            4 * 1024 * 1024,
        ),
        max_files: bounded_usize(marker.vault_max_files, DEFAULT_MAX_FILES, 1, 50_000),
    })
}

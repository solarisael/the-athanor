use crate::documents::{DEFAULT_CHUNK_CHARS, DEFAULT_CHUNK_OVERLAP};
use crate::error::VaultError;
use crate::rank::{
    DEFAULT_EXCERPT_CHARS, DEFAULT_FIELD_LENGTH_NORMALIZATIONS, DEFAULT_FIELD_WEIGHTS,
    DEFAULT_MAX_RESULTS, FIELD_COUNT, FIELD_NAMES,
};
use crate::walk::{normalize_lexical, normalized_path};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
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
    vault_max_results: Option<usize>,
    vault_excerpt_chars: Option<usize>,
    vault_chunk_chars: Option<usize>,
    vault_chunk_overlap: Option<usize>,
    #[serde(default)]
    vault_field_tuning: BTreeMap<String, FieldTuning>,
}
/// One row of the optional `vaultFieldTuning` table, keyed by the field names
/// the ranker publishes (`path`, `title`, `heading`, `keys`, `tags`, `body`,
/// `metadata`). An unknown key names no field and is ignored.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FieldTuning {
    weight: Option<f64>,
    length_normalization: Option<f64>,
}
/// The room's retrieval knobs, already bounded. Vault is the no-database mode,
/// so the room file is the only place a room can speak; a room that says nothing
/// gets the in-code defaults and behaves exactly as the hardcoded ranker did.
#[derive(Clone, Copy)]
pub(crate) struct VaultSettings {
    pub(crate) max_results: usize,
    pub(crate) excerpt_chars: usize,
    pub(crate) chunk_chars: usize,
    pub(crate) chunk_overlap: usize,
    pub(crate) field_weights: [f64; FIELD_COUNT],
    pub(crate) field_length_normalizations: [f64; FIELD_COUNT],
}
pub(crate) struct VaultConfig {
    pub(crate) roots: Vec<PathBuf>,
    pub(crate) ignore: Vec<String>,
    pub(crate) max_file_bytes: u64,
    pub(crate) max_files: usize,
    pub(crate) settings: VaultSettings,
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
fn bounded_f64(value: Option<f64>, fallback: f64, minimum: f64, maximum: f64) -> f64 {
    value
        .filter(|value| (*value >= minimum) && (*value <= maximum))
        .unwrap_or(fallback)
}
fn load_settings(marker: &RoomMarker) -> VaultSettings {
    let chunk_chars = bounded_usize(marker.vault_chunk_chars, DEFAULT_CHUNK_CHARS, 500, 200_000);
    // Overlap never reaches half a chunk: the chunk walk steps back by it, and a
    // step back as long as the step forward would never finish the file.
    let chunk_overlap_ceiling = chunk_chars / 2;
    let mut field_weights = DEFAULT_FIELD_WEIGHTS;
    let mut field_length_normalizations = DEFAULT_FIELD_LENGTH_NORMALIZATIONS;
    for (index, name) in FIELD_NAMES.iter().enumerate() {
        let Some(tuning) = marker.vault_field_tuning.get(*name) else {
            continue;
        };
        field_weights[index] = bounded_f64(tuning.weight, field_weights[index], 0.0, 100.0);
        field_length_normalizations[index] = bounded_f64(
            tuning.length_normalization,
            field_length_normalizations[index],
            0.0,
            1.0,
        );
    }
    VaultSettings {
        max_results: bounded_usize(marker.vault_max_results, DEFAULT_MAX_RESULTS, 1, 100),
        excerpt_chars: bounded_usize(
            marker.vault_excerpt_chars,
            DEFAULT_EXCERPT_CHARS,
            80,
            20_000,
        ),
        chunk_chars,
        chunk_overlap: bounded_usize(
            marker.vault_chunk_overlap,
            DEFAULT_CHUNK_OVERLAP.min(chunk_overlap_ceiling),
            0,
            chunk_overlap_ceiling,
        ),
        field_weights,
        field_length_normalizations,
    }
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
    let settings = load_settings(&marker);
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
        settings,
    })
}

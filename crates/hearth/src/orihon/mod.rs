pub mod live;

use serde::{Deserialize, Serialize};

pub const DIRECTORY: &str = "orihon";
pub const MAX_BATCH_RECORDS: usize = 64;
pub const MAX_READ_RECORDS: usize = 200;
pub const ECHO_CHARACTERS: usize = 30_000;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Off,
    #[default]
    Log,
    Fold,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecordingState {
    pub mode: Mode,
    pub generation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<FolderSettings>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecordingDefaults {
    pub mode: Mode,
    pub folder: Option<FolderSettings>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FolderSettings {
    pub model: String,
    #[serde(default)]
    pub backend: FolderBackend,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<FolderRuntime>,
}

impl FolderSettings {
    pub fn provider(&self) -> &str {
        match self.backend {
            FolderBackend::Local => "ollama",
            FolderBackend::Hosted => self
                .model
                .split_once('/')
                .map_or("", |(provider, _)| provider),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderBackend {
    #[default]
    Local,
    Hosted,
}

pub const HOSTED_FOLDER_MODEL: &str = "anthropic/claude-sonnet-5-5";

/// Non-secret launch identity. OMP alone reads provider credentials.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FolderRuntime {
    pub executable: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<String>,
    pub home: String,
    pub agent_dir: String,
    pub cwd: String,
    pub config_files: Vec<String>,
    pub environment: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    User,
    Talk,
    Tool,
    Echo,
    Work,
    Note,
}

/// Exact identity of a child agent whose report a work record carries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ChildRef {
    pub name: String,
    pub session: String,
}

pub const MAX_CHILDREN: usize = 16;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CaptureRecord {
    pub source_id: String,
    pub parent_user_id: Option<String>,
    pub kind: Kind,
    pub speaker: String,
    pub text: String,
    pub said_at: String,
    #[serde(default)]
    pub timestamp_observed: bool,
    #[serde(default)]
    pub redacted: bool,
    /// Semantic fold input for a tool record whose text holds an expanded dispatch payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fold_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<ChildRef>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CaptureBatch {
    pub harness: String,
    pub source_session: String,
    pub records: Vec<CaptureRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReadRequest {
    pub after_seq: Option<i64>,
    pub until_seq: Option<i64>,
    pub turn_no: Option<i64>,
    pub day: Option<String>,
    #[serde(default = "default_read_limit")]
    pub limit: usize,
}

fn default_read_limit() -> usize {
    50
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub room: String,
    pub seq: i64,
    pub turn_no: i64,
    pub session_id: String,
    pub harness: String,
    pub source_session: String,
    pub source_id: String,
    pub parent_user_id: Option<String>,
    pub kind: Kind,
    pub speaker: String,
    pub text: String,
    pub said_at: String,
    pub timestamp_observed: bool,
    pub written_at: String,
    pub truncated: bool,
    pub redacted: bool,
    pub content_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fold_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<ChildRef>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureReceipt {
    pub accepted_source_ids: Vec<String>,
    pub appended: usize,
    /// Every accepted record that was already stored, reconciled ones included.
    pub replayed: usize,
    pub reconciled: Vec<Reconciled>,
    #[serde(skip_serializing)]
    pub accepted_records: Vec<Record>,
}

/// A replay of a record accepted before captures carried semantic fields. The stored record
/// and its digest stand unchanged; `added` names the replayed fields that were not stored.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reconciled {
    pub source_id: String,
    pub seq: i64,
    pub content_digest: String,
    pub added: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadReceipt {
    pub rows: Vec<Record>,
    pub has_more: bool,
    pub next_after: Option<i64>,
}

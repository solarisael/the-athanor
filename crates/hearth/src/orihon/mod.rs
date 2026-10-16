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
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureReceipt {
    pub accepted_source_ids: Vec<String>,
    pub appended: usize,
    pub replayed: usize,
    #[serde(skip_serializing)]
    pub accepted_records: Vec<Record>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadReceipt {
    pub rows: Vec<Record>,
    pub has_more: bool,
    pub next_after: Option<i64>,
}

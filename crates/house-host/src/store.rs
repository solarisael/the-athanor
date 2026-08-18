use crate::policy::RecallPolicySession;
use atomicwrites::{AllowOverwrite, AtomicFile, Error as AtomicError};
use chrono::{SecondsFormat, Utc};
use house_protocol::{
    CommandOutcomeEvent, RecallPolicyState, RecallRequestedMode, RecallResolvedMode, RecoveryState,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const RECEIPT_LIMIT: usize = 512;

#[derive(Clone, Debug)]
pub struct RoomStateStore {
    path: PathBuf,
    room: String,
}

impl RoomStateStore {
    pub fn new(path: PathBuf, room: String) -> Self {
        Self { path, room }
    }

    pub fn load(&self) -> Result<RecallPolicyState, String> {
        let root = self.read_root()?;
        projection_from_root(&root, &self.room)
    }

    pub fn write_policy(&self, state: &RecallPolicyState) -> Result<(), String> {
        let mut root = self.read_root()?;
        projection_from_root(&root, &self.room)?;
        let now = state.updated_at.clone().unwrap_or_else(timestamp);
        let policy = object_mut(&mut root, "recallPolicy")?;
        policy.insert(
            "requestedMode".into(),
            Value::String(state.requested_mode.as_str().into()),
        );
        policy.insert(
            "resolvedMode".into(),
            Value::String(resolved_mode_name(state.resolved_mode).into()),
        );
        policy.insert(
            "activeProject".into(),
            option_string_value(state.active_project.as_deref()),
        );
        policy.insert(
            "resolutionReason".into(),
            Value::String(state.resolution_reason.clone()),
        );
        policy.insert(
            "lastRefreshReason".into(),
            option_string_value(state.last_refresh_reason.as_deref()),
        );
        policy.insert(
            "lastRefreshAt".into(),
            option_string_value(state.last_refresh_at.as_deref()),
        );
        policy.insert(
            "workingSetEntries".into(),
            Value::Number(state.working_set_entries.into()),
        );
        policy.insert(
            "recoveryPending".into(),
            Value::Bool(state.recovery_state.is_pending()),
        );
        policy.insert(
            "recoveryTerms".into(),
            Value::Array(
                state
                    .recovery_state
                    .terms()
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
        policy.insert(
            "degraded".into(),
            option_string_value(state.degraded.as_deref()),
        );
        policy.insert("updatedAt".into(), Value::String(now.clone()));
        root.as_object_mut()
            .expect("validated room state root")
            .insert("lastUpdatedAt".into(), Value::String(now));
        atomic_json_write(&self.path, &root)
    }

    fn read_root(&self) -> Result<Value, String> {
        let bytes = fs::read(&self.path)
            .map_err(|error| format!("cannot read room state {}: {error}", self.path.display()))?;
        let root: Value = serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "room state {} is malformed JSON: {error}",
                self.path.display()
            )
        })?;
        if !root.is_object() {
            return Err("room state root must be a JSON object".into());
        }
        Ok(root)
    }
}

fn projection_from_root(root: &Value, configured_room: &str) -> Result<RecallPolicyState, String> {
    let root_object = root
        .as_object()
        .ok_or_else(|| "room state root must be a JSON object".to_owned())?;
    let room = required_string(root_object, "room")?;
    if room != configured_room {
        return Err(format!(
            "room state belongs to foreign room {room}; configured room is {configured_room}"
        ));
    }
    let policy = root_object
        .get("recallPolicy")
        .and_then(Value::as_object)
        .ok_or_else(|| "room state recallPolicy must be an object".to_owned())?;
    let requested_mode = parse_requested(required_string(policy, "requestedMode")?)?;
    let resolved_mode = parse_resolved(required_string(policy, "resolvedMode")?)?;
    let working_set_entries = policy
        .get("workingSetEntries")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "recallPolicy.workingSetEntries must be a non-negative integer".to_owned()
        })?;
    let recovery = LegacyRecovery {
        pending: policy
            .get("recoveryPending")
            .and_then(Value::as_bool)
            .ok_or_else(|| "recallPolicy.recoveryPending must be boolean".to_owned())?,
        terms: policy
            .get("recoveryTerms")
            .and_then(Value::as_array)
            .ok_or_else(|| "recallPolicy.recoveryTerms must be an array".to_owned())?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .filter(|text| !text.trim().is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        "recallPolicy.recoveryTerms must contain nonblank strings".to_owned()
                    })
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(RecallPolicyState {
        requested_mode,
        resolved_mode,
        active_project: nullable_string(policy, "activeProject")?,
        resolution_reason: required_string(policy, "resolutionReason")?.to_owned(),
        last_refresh_reason: nullable_string(policy, "lastRefreshReason")?,
        last_refresh_at: nullable_string(policy, "lastRefreshAt")?,
        working_set_entries,
        recovery_state: recovery.into_state(),
        degraded: nullable_string(policy, "degraded")?,
        updated_at: nullable_string(policy, "updatedAt")?,
    })
}

fn parse_requested(value: &str) -> Result<RecallRequestedMode, String> {
    match value {
        "auto" => Ok(RecallRequestedMode::Auto),
        "conversation" => Ok(RecallRequestedMode::Conversation),
        "work" => Ok(RecallRequestedMode::Work),
        "quiet" => Ok(RecallRequestedMode::Quiet),
        other => Err(format!("unknown recallPolicy.requestedMode {other}")),
    }
}

fn parse_resolved(value: &str) -> Result<RecallResolvedMode, String> {
    match value {
        "conversation" => Ok(RecallResolvedMode::Conversation),
        "work" => Ok(RecallResolvedMode::Work),
        "mixed" => Ok(RecallResolvedMode::Mixed),
        "quiet" => Ok(RecallResolvedMode::Quiet),
        other => Err(format!("unknown recallPolicy.resolvedMode {other}")),
    }
}

/// Private legacy wire DTO; persisted room JSON keeps these established fields.
struct LegacyRecovery {
    pending: bool,
    terms: Vec<String>,
}

impl LegacyRecovery {
    fn into_state(self) -> RecoveryState {
        if self.pending {
            RecoveryState::Pending { terms: self.terms }
        } else {
            RecoveryState::Idle
        }
    }
}

fn resolved_mode_name(mode: RecallResolvedMode) -> &'static str {
    match mode {
        RecallResolvedMode::Conversation => "conversation",
        RecallResolvedMode::Work => "work",
        RecallResolvedMode::Mixed => "mixed",
        RecallResolvedMode::Quiet => "quiet",
    }
}

fn option_string_value(value: Option<&str>) -> Value {
    value
        .map(|value| Value::String(value.to_owned()))
        .unwrap_or(Value::Null)
}

fn required_string<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{key} must be a nonblank string"))
}

fn nullable_string(object: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match object.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(Some(value.clone())),
        Some(_) => Err(format!("{key} must be null or a nonblank string")),
        None => Err(format!("{key} is required")),
    }
}

fn object_mut<'a>(root: &'a mut Value, key: &str) -> Result<&'a mut Map<String, Value>, String> {
    root.as_object_mut()
        .and_then(|root| root.get_mut(key))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("room state {key} must be an object"))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionCursor {
    pub projection_id: String,
    pub version: u64,
    pub sequence: u64,
    pub state_hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DurableReceipt {
    pub idempotency_key: String,
    pub body_hash: String,
    pub outcome: CommandOutcomeEvent,
    pub stored_at: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReceiptFile {
    receipts: Vec<DurableReceipt>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SessionFile {
    sessions: HashMap<String, RecallPolicySession>,
}

pub struct HostDurableStore {
    cursor_path: PathBuf,
    receipts_path: PathBuf,
    sessions_path: PathBuf,
    receipts: Vec<DurableReceipt>,
}

impl HostDurableStore {
    pub fn open(
        state_dir: &Path,
        projection: &RecallPolicyState,
    ) -> Result<(Self, ProjectionCursor, HashMap<String, RecallPolicySession>), String> {
        fs::create_dir_all(state_dir).map_err(|error| {
            format!(
                "cannot create Host state directory {}: {error}",
                state_dir.display()
            )
        })?;
        let cursor_path = state_dir.join("recall-policy-cursor.json");
        let receipts_path = state_dir.join("recall-policy-receipts.json");
        let sessions_path = state_dir.join("recall-policy-sessions.json");
        let state_hash = state_hash(projection)?;
        let cursor = match fs::read(&cursor_path) {
            Ok(bytes) => {
                let previous: ProjectionCursor = serde_json::from_slice(&bytes)
                    .map_err(|error| format!("Host projection cursor is malformed: {error}"))?;
                if previous.projection_id != house_protocol::RECALL_POLICY_PROJECTION_ID {
                    return Err("Host projection cursor names a foreign projection".into());
                }
                if previous.state_hash == state_hash {
                    previous
                } else {
                    ProjectionCursor {
                        projection_id: house_protocol::RECALL_POLICY_PROJECTION_ID.into(),
                        version: previous
                            .version
                            .checked_add(1)
                            .ok_or("projection version overflow")?,
                        sequence: previous
                            .sequence
                            .checked_add(1)
                            .ok_or("projection sequence overflow")?,
                        state_hash,
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => ProjectionCursor {
                projection_id: house_protocol::RECALL_POLICY_PROJECTION_ID.into(),
                version: 1,
                sequence: 1,
                state_hash,
            },
            Err(error) => return Err(format!("cannot read Host projection cursor: {error}")),
        };
        atomic_json_write(&cursor_path, &cursor)?;
        let receipts = match fs::read(&receipts_path) {
            Ok(bytes) => {
                let file: ReceiptFile = serde_json::from_slice(&bytes)
                    .map_err(|error| format!("Host receipts are malformed: {error}"))?;
                if file.receipts.len() > RECEIPT_LIMIT {
                    return Err(format!(
                        "Host receipts exceed bounded limit {RECEIPT_LIMIT}"
                    ));
                }
                file.receipts
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(format!("cannot read Host receipts: {error}")),
        };
        let sessions = match fs::read(&sessions_path) {
            Ok(bytes) => {
                let file: SessionFile = serde_json::from_slice(&bytes).map_err(|error| {
                    format!("Host Recall Policy sessions are malformed: {error}")
                })?;
                file.sessions
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => HashMap::new(),
            Err(error) => return Err(format!("cannot read Host Recall Policy sessions: {error}")),
        };
        Ok((
            Self {
                cursor_path,
                receipts_path,
                sessions_path,
                receipts,
            },
            cursor,
            sessions,
        ))
    }

    pub fn receipt(&self, key: &str) -> Option<&DurableReceipt> {
        self.receipts
            .iter()
            .find(|receipt| receipt.idempotency_key == key)
    }

    pub fn save_cursor(&self, cursor: &ProjectionCursor) -> Result<(), String> {
        atomic_json_write(&self.cursor_path, cursor)
    }

    pub fn save_receipt(&mut self, receipt: DurableReceipt) -> Result<(), String> {
        self.receipts.push(receipt);
        if self.receipts.len() > RECEIPT_LIMIT {
            let remove = self.receipts.len() - RECEIPT_LIMIT;
            self.receipts.drain(..remove);
        }
        atomic_json_write(
            &self.receipts_path,
            &ReceiptFile {
                receipts: self.receipts.clone(),
            },
        )
    }

    pub fn save_sessions(
        &self,
        sessions: &HashMap<String, RecallPolicySession>,
    ) -> Result<(), String> {
        atomic_json_write(
            &self.sessions_path,
            &SessionFile {
                sessions: sessions.clone(),
            },
        )
    }
}

pub fn state_hash(state: &RecallPolicyState) -> Result<String, String> {
    let bytes =
        serde_json::to_vec(state).map_err(|error| format!("cannot hash projection: {error}"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn body_hash(value: &Value) -> Result<String, String> {
    let mut semantic = value.clone();
    if let Some(object) = semantic.as_object_mut() {
        for volatile in [
            "message_id",
            "correlation_id",
            "causation_id",
            "created_at",
            "expires_at",
        ] {
            object.remove(volatile);
        }
    }
    let bytes =
        serde_json::to_vec(&semantic).map_err(|error| format!("cannot hash command: {error}"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn atomic_json_write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("cannot serialize {}: {error}", path.display()))?;
    bytes.push(b'\n');
    let file = AtomicFile::new(path, AllowOverwrite);
    match file.write(|target| -> io::Result<()> {
        target.write_all(&bytes)?;
        target.sync_all()
    }) {
        Ok(()) => Ok(()),
        Err(AtomicError::Internal(error)) | Err(AtomicError::User(error)) => Err(format!(
            "cannot atomically replace {}: {error}",
            path.display()
        )),
    }
}

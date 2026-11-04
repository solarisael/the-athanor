use crate::policy::RecallPolicySession;
use atomicwrites::{AllowOverwrite, AtomicFile, Error as AtomicError};
use chrono::{SecondsFormat, Utc};
use protocol::{
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

/// The room's own answer to who is present, read from room state rather than
/// asserted by a caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomIdentity {
    pub room: String,
    pub spirit: String,
    pub operator: String,
}

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
    pub(crate) fn migrate_legacy_files(&self, room_dir: &Path) -> Result<(), String> {
        migrate_file(
            &room_dir.join(".athanor-room.json"),
            &room_dir.join(".solarisael-room.json"),
        )?;
        let parent = self
            .path
            .parent()
            .ok_or_else(|| "room state path has no parent directory".to_owned())?;
        migrate_file(&self.path, &parent.join("solarisael-house-state.json"))
    }

    pub(crate) fn embodied_spirit(&self, configured: &str) -> Result<String, String> {
        let root = self.read_root()?;
        let object = root
            .as_object()
            .ok_or_else(|| "room state root must be a JSON object".to_owned())?;
        let room = required_string(object, "room")?;
        if room != self.room {
            return Err(format!(
                "room state belongs to foreign room {room}; configured room is {}",
                self.room
            ));
        }
        match object.get("embodiedSpirit") {
            None => Ok(configured.to_owned()),
            Some(Value::String(spirit)) => {
                let spirit = spirit.trim();
                if spirit.is_empty()
                    || spirit.encode_utf16().count() > 80
                    || spirit
                        .chars()
                        .any(|character| matches!(character, '\r' | '\n' | '|'))
                {
                    return Err("room state embodiedSpirit is invalid".into());
                }
                Ok(spirit.to_owned())
            }
            Some(_) => Err("room state embodiedSpirit must be a string".into()),
        }
    }

    pub(crate) fn write_room_state(&self, root: &Value) -> Result<(), String> {
        projection_from_root(root, &self.room)?;
        atomic_json_write(&self.path, root)
    }

    pub(crate) fn write_text_mirror(&self, path: &Path, text: &str) -> Result<(), String> {
        atomic_text_write(path, text)
    }

    /// Resolve this room's identity from its bound state and Host configuration.
    pub fn identity(&self, configured_spirit: &str) -> Result<RoomIdentity, String> {
        let root = self.read_root()?;
        let object = root
            .as_object()
            .ok_or_else(|| "room state root must be a JSON object".to_owned())?;
        let room = required_string(object, "room")?;
        if room != self.room {
            return Err(format!(
                "room state belongs to foreign room {room}; configured room is {}",
                self.room
            ));
        }
        let spirit = self.embodied_spirit(configured_spirit)?;
        let operator = required_string(object, "operator")
            .map_err(|_| "room state must name the operator before Presence opens".to_owned())?;
        Ok(RoomIdentity {
            room: room.to_owned(),
            spirit,
            operator: operator.trim().to_owned(),
        })
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

    pub(crate) fn read_root(&self) -> Result<Value, String> {
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

fn migrate_file(current: &Path, legacy: &Path) -> Result<(), String> {
    match fs::symlink_metadata(current) {
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!("cannot inspect {}: {error}", current.display()));
        }
    }
    match fs::symlink_metadata(legacy) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!("cannot inspect {}: {error}", legacy.display()));
        }
    }
    fs::rename(legacy, current).map_err(|error| {
        format!(
            "cannot migrate {} to {}: {error}",
            legacy.display(),
            current.display()
        )
    })
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

impl ProjectionCursor {
    pub fn advance(&self, state_hash: String) -> Result<Self, String> {
        Ok(Self {
            projection_id: protocol::RECALL_POLICY_PROJECTION_ID.into(),
            version: self
                .version
                .checked_add(1)
                .ok_or("projection version overflow")?,
            sequence: self
                .sequence
                .checked_add(1)
                .ok_or("projection sequence overflow")?,
            state_hash,
        })
    }
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
struct ReceiptFile<T> {
    receipts: T,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SessionFile<T> {
    sessions: T,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyCheckpoint {
    pub projection: RecallPolicyState,
    pub cursor: ProjectionCursor,
    pub sessions: HashMap<String, RecallPolicySession>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PolicyCommit {
    format_version: u8,
    room: String,
    checkpoint: PolicyCheckpoint,
    receipts: Vec<DurableReceipt>,
}

impl PolicyCommit {
    fn validate(&self, room: &str) -> Result<(), String> {
        if self.format_version != 1 || self.room != room {
            return Err("Host policy recovery record has a foreign room or format".into());
        }

        let cursor = &self.checkpoint.cursor;

        if cursor.projection_id != protocol::RECALL_POLICY_PROJECTION_ID
            || cursor.version == 0
            || cursor.sequence == 0
            || cursor.state_hash != state_hash(&self.checkpoint.projection)?
            || self.receipts.len() > RECEIPT_LIMIT
        {
            return Err("Host policy recovery record is inconsistent".into());
        }

        Ok(())
    }
}

pub struct HostDurableStore {
    cursor_path: PathBuf,
    receipts_path: PathBuf,
    sessions_path: PathBuf,
    commit_path: PathBuf,
    receipts: Vec<DurableReceipt>,
}

impl HostDurableStore {
    pub fn open(
        state_dir: &Path,
        room_store: &RoomStateStore,
    ) -> Result<(Self, PolicyCheckpoint), String> {
        fs::create_dir_all(state_dir).map_err(|error| {
            format!(
                "cannot create Host state directory {}: {error}",
                state_dir.display()
            )
        })?;
        let mut store = Self {
            cursor_path: state_dir.join("recall-policy-cursor.json"),
            receipts_path: state_dir.join("recall-policy-receipts.json"),
            sessions_path: state_dir.join("recall-policy-sessions.json"),
            commit_path: state_dir.join("recall-policy-commit.json"),
            receipts: Vec::new(),
        };
        store.finish_pending(room_store)?;

        let projection = room_store.load()?;
        let state_hash = state_hash(&projection)?;
        let cursor = match fs::read(&store.cursor_path) {
            Ok(bytes) => {
                let previous: ProjectionCursor = serde_json::from_slice(&bytes)
                    .map_err(|error| format!("Host projection cursor is malformed: {error}"))?;

                if previous.projection_id != protocol::RECALL_POLICY_PROJECTION_ID {
                    return Err("Host projection cursor names a foreign projection".into());
                }

                if previous.state_hash == state_hash {
                    previous
                } else {
                    previous.advance(state_hash)?
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => ProjectionCursor {
                projection_id: protocol::RECALL_POLICY_PROJECTION_ID.into(),
                version: 1,
                sequence: 1,
                state_hash,
            },
            Err(error) => return Err(format!("cannot read Host projection cursor: {error}")),
        };
        let receipts = match fs::read(&store.receipts_path) {
            Ok(bytes) => {
                let file: ReceiptFile<Vec<DurableReceipt>> = serde_json::from_slice(&bytes)
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
        let sessions = match fs::read(&store.sessions_path) {
            Ok(bytes) => {
                let file: SessionFile<HashMap<String, RecallPolicySession>> =
                    serde_json::from_slice(&bytes).map_err(|error| {
                        format!("Host Recall Policy sessions are malformed: {error}")
                    })?;
                file.sessions
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => HashMap::new(),
            Err(error) => return Err(format!("cannot read Host Recall Policy sessions: {error}")),
        };
        atomic_json_write(&store.cursor_path, &cursor)?;
        store.receipts = receipts;

        Ok((
            store,
            PolicyCheckpoint {
                projection,
                cursor,
                sessions,
            },
        ))
    }

    pub fn commit_policy(
        &mut self,
        room_store: &RoomStateStore,
        checkpoint: PolicyCheckpoint,
        receipt: Option<DurableReceipt>,
    ) -> Result<PolicyCheckpoint, String> {
        self.finish_pending(room_store)?;
        room_store.load()?;

        let mut receipts = self.receipts.clone();

        if let Some(receipt) = receipt {
            receipts.push(receipt);
            let excess = receipts.len().saturating_sub(RECEIPT_LIMIT);
            receipts.drain(..excess);
        }

        let commit = PolicyCommit {
            format_version: 1,
            room: room_store.room.clone(),
            checkpoint,
            receipts,
        };
        commit.validate(&room_store.room)?;

        // This replacement commits the whole change. Mirrors may lag, never authority.
        atomic_json_write(&self.commit_path, &commit)?;

        if let Err(error) = self.materialize(room_store, &commit) {
            tracing::warn!(%error, "Recall policy committed; checkpoint recovery is pending");
        }

        self.receipts = commit.receipts;

        Ok(commit.checkpoint)
    }

    pub fn finish_pending(&self, room_store: &RoomStateStore) -> Result<(), String> {
        let bytes = match fs::read(&self.commit_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("cannot read Host policy recovery record: {error}")),
        };
        let commit: PolicyCommit = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Host policy recovery record is malformed: {error}"))?;
        commit.validate(&room_store.room)?;

        self.materialize(room_store, &commit)
    }

    fn materialize(
        &self,
        room_store: &RoomStateStore,
        commit: &PolicyCommit,
    ) -> Result<(), String> {
        // Merge policy fields only; later room edits and the manual spirit body are not ours.
        if room_store.load()? != commit.checkpoint.projection {
            room_store.write_policy(&commit.checkpoint.projection)?;
        }

        atomic_json_write(
            &self.sessions_path,
            &SessionFile {
                sessions: &commit.checkpoint.sessions,
            },
        )?;
        atomic_json_write(&self.cursor_path, &commit.checkpoint.cursor)?;
        atomic_json_write(
            &self.receipts_path,
            &ReceiptFile {
                receipts: &commit.receipts,
            },
        )?;
        fs::remove_file(&self.commit_path)
            .map_err(|error| format!("cannot retire Host policy recovery record: {error}"))
    }

    pub fn receipt(&self, key: &str) -> Option<&DurableReceipt> {
        self.receipts
            .iter()
            .find(|receipt| receipt.idempotency_key == key)
    }

    fn require_no_pending(&self) -> Result<(), String> {
        match fs::symlink_metadata(&self.commit_path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "cannot inspect Host policy recovery record: {error}"
            )),
            Ok(_) => Err("Host policy checkpoint recovery must finish before another write".into()),
        }
    }

    pub fn save_receipt(&mut self, receipt: DurableReceipt) -> Result<(), String> {
        self.require_no_pending()?;
        let mut receipts = self.receipts.clone();
        receipts.push(receipt);
        let excess = receipts.len().saturating_sub(RECEIPT_LIMIT);
        receipts.drain(..excess);
        atomic_json_write(
            &self.receipts_path,
            &ReceiptFile {
                receipts: &receipts,
            },
        )?;
        self.receipts = receipts;

        Ok(())
    }

    pub fn save_sessions(
        &self,
        sessions: &HashMap<String, RecallPolicySession>,
    ) -> Result<(), String> {
        self.require_no_pending()?;

        atomic_json_write(&self.sessions_path, &SessionFile { sessions })
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

pub(crate) fn atomic_json_write(path: &Path, value: &impl Serialize) -> Result<(), String> {
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

fn atomic_text_write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let file = AtomicFile::new(path, AllowOverwrite);
    match file.write(|target| -> io::Result<()> {
        target.write_all(text.as_bytes())?;
        target.sync_all()
    }) {
        Ok(()) => Ok(()),
        Err(AtomicError::Internal(error)) | Err(AtomicError::User(error)) => Err(format!(
            "cannot atomically replace {}: {error}",
            path.display()
        )),
    }
}

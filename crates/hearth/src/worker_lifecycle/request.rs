use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// The authenticated caller. The Host injects every field from its own
/// binding; organ params can never supply them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub house_id: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PredecessorRef {
    pub book_id: String,
    pub instance_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Prepare {
    pub swarm_key: String,
    pub slot_key: String,
    pub agent_name: String,
    pub task: String,
    pub token: String,
    #[serde(default)]
    pub predecessor: Option<PredecessorRef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Open {
    pub token: String,
    pub agent_id: String,
    pub agent_name: String,
    #[serde(default)]
    pub transcript_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Read {
    pub book_id: String,
    #[serde(default)]
    pub after: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Append {
    pub book_id: String,
    pub kind: String,
    pub body: Value,
    #[serde(default)]
    pub operation_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Observe {
    pub book_id: String,
    pub tokens: i64,
    pub context_window: i64,
    #[serde(default)]
    pub request_started: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Checkpoint {
    pub book_id: String,
    pub report: Value,
    #[serde(default)]
    pub terminal: Option<bool>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettleOutcome {
    Failed,
    Aborted,
    Completed,
}

impl SettleOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Aborted => "aborted",
            Self::Completed => "completed",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Settle {
    pub book_id: String,
    #[serde(default)]
    pub instance_id: Option<String>,
    pub outcome: SettleOutcome,
    #[serde(default)]
    pub report: Option<Value>,
    #[serde(default)]
    pub transcript_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Inspect {
    #[serde(default)]
    pub book_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Accept {
    pub book_id: String,
    pub body: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Repair {
    pub book_id: String,
    pub instance_id: String,
    pub report: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Dispose {
    pub swarm_id: String,
    #[serde(default)]
    pub discard_reason: Option<String>,
}

/// Page accepted evidence of one owned swarm, before or after disposal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Retained {
    pub swarm_id: String,
    /// A retainedId of this swarm; the page starts after it.
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum Action {
    Prepare(Prepare),
    Open(Open),
    Read(Read),
    Append(Append),
    Observe(Observe),
    Checkpoint(Checkpoint),
    Settle(Settle),
    Inspect(Inspect),
    Accept(Accept),
    Repair(Repair),
    Dispose(Dispose),
    Retained(Retained),
}

impl Action {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Prepare(_) => "prepare",
            Self::Open(_) => "open",
            Self::Read(_) => "read",
            Self::Append(_) => "append",
            Self::Observe(_) => "observe",
            Self::Checkpoint(_) => "checkpoint",
            Self::Settle(_) => "settle",
            Self::Inspect(_) => "inspect",
            Self::Accept(_) => "accept",
            Self::Repair(_) => "repair",
            Self::Dispose(_) => "dispose",
            Self::Retained(_) => "retained",
        }
    }

    /// Reads change nothing and keep no idempotency record.
    pub const fn is_write(&self) -> bool {
        !matches!(self, Self::Read(_) | Self::Inspect(_) | Self::Retained(_))
    }
}

/// Where a store finds (and locks) the swarm aggregate a call acts on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Target {
    /// prepare without predecessor: the caller's own swarm for this key,
    /// created when absent.
    NewSwarm { swarm_key: String },
    Book(String),
    Swarm(String),
    TokenHash(String),
    /// inspect without bookId: every swarm the caller owns plus the swarm
    /// holding the caller's own bound instance.
    Caller,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub actor: Actor,
    pub request_id: String,
    pub action: Action,
}

/// sha256 hex of an enrollment token. Only the hash is ever stored.
pub fn token_hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn take_string(map: &mut Map<String, Value>, key: &str) -> Result<String, String> {
    match map.remove(key) {
        Some(Value::String(value)) => Ok(value),
        Some(_) => Err(format!("{key} must be a string")),
        None => Err(format!("missing field `{key}`")),
    }
}

fn body<T: DeserializeOwned>(rest: Map<String, Value>) -> Result<T, String> {
    serde_json::from_value(Value::Object(rest)).map_err(|error| error.to_string())
}

fn check_text(field: &str, value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must be non-empty"));
    }
    if value.chars().count() > max {
        return Err(format!("{field} exceeds {max} characters"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{field} must not contain control characters"));
    }
    Ok(())
}

fn check_optional_text(field: &str, value: Option<&str>, max: usize) -> Result<(), String> {
    value.map_or(Ok(()), |value| check_text(field, value, max))
}

/// At least 128 bits of entropy cannot be proven from text, but a token too
/// short to carry it, or outside a url-safe alphabet, is refused outright.
fn check_token(token: &str) -> Result<(), String> {
    if token.len() < 32 || token.len() > 256 {
        return Err("token must be 32..256 url-safe characters".into());
    }
    if !token
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("token must be 32..256 url-safe characters".into());
    }
    Ok(())
}

impl Call {
    /// Decode Host-bound organ params. The Host has already injected
    /// `houseId`, `room`, `spirit`, and `session`.
    pub fn from_params(params: Value) -> Result<Self, String> {
        let Value::Object(mut map) = params else {
            return Err("worker_lifecycle params must be an object".into());
        };
        let actor = Actor {
            house_id: take_string(&mut map, "houseId")?,
            room: take_string(&mut map, "room")?,
            spirit: take_string(&mut map, "spirit")?,
            session: take_string(&mut map, "session")?,
        };
        let request_id = take_string(&mut map, "requestId")?;
        let action = take_string(&mut map, "action")?;
        let action = match action.as_str() {
            "prepare" => Action::Prepare(body(map)?),
            "open" => Action::Open(body(map)?),
            "read" => Action::Read(body(map)?),
            "append" => Action::Append(body(map)?),
            "observe" => Action::Observe(body(map)?),
            "checkpoint" => Action::Checkpoint(body(map)?),
            "settle" => Action::Settle(body(map)?),
            "inspect" => Action::Inspect(body(map)?),
            "accept" => Action::Accept(body(map)?),
            "repair" => Action::Repair(body(map)?),
            "dispose" => Action::Dispose(body(map)?),
            "retained" => Action::Retained(body(map)?),
            other => return Err(format!("unsupported worker_lifecycle action: {other}")),
        };
        let call = Self {
            actor,
            request_id,
            action,
        };
        call.validate()?;
        Ok(call)
    }

    pub fn validate(&self) -> Result<(), String> {
        check_text("houseId", &self.actor.house_id, 64)?;
        check_text("room", &self.actor.room, 128)?;
        check_text("spirit", &self.actor.spirit, 128)?;
        check_text("session", &self.actor.session, 256)?;
        check_text("requestId", &self.request_id, 200)?;
        match &self.action {
            Action::Prepare(prepare) => {
                check_text("swarmKey", &prepare.swarm_key, 200)?;
                check_text("slotKey", &prepare.slot_key, 200)?;
                check_text("agentName", &prepare.agent_name, 200)?;
                if prepare.task.trim().is_empty() {
                    return Err("task must be non-empty".into());
                }
                if prepare.task.len() > 512 * 1024 {
                    return Err("task exceeds 524288 bytes".into());
                }
                check_token(&prepare.token)?;
                if let Some(predecessor) = &prepare.predecessor {
                    check_text("predecessor.bookId", &predecessor.book_id, 200)?;
                    check_text("predecessor.instanceId", &predecessor.instance_id, 200)?;
                }
            }
            Action::Open(open) => {
                check_token(&open.token)?;
                check_text("agentId", &open.agent_id, 200)?;
                check_text("agentName", &open.agent_name, 200)?;
                check_optional_text("transcriptPath", open.transcript_path.as_deref(), 1024)?;
            }
            Action::Read(read) => {
                check_text("bookId", &read.book_id, 200)?;
                if read.after.is_some_and(|after| after < 0) {
                    return Err("after must be zero or greater".into());
                }
                if read
                    .limit
                    .is_some_and(|limit| !(1..=super::READ_LIMIT_MAX).contains(&limit))
                {
                    return Err(format!("limit must be 1..{}", super::READ_LIMIT_MAX));
                }
            }
            Action::Append(append) => {
                check_text("bookId", &append.book_id, 200)?;
                check_text("kind", &append.kind, 40)?;
                check_optional_text("operationId", append.operation_id.as_deref(), 200)?;
            }
            Action::Observe(observe) => check_text("bookId", &observe.book_id, 200)?,
            Action::Checkpoint(checkpoint) => check_text("bookId", &checkpoint.book_id, 200)?,
            Action::Settle(settle) => {
                check_text("bookId", &settle.book_id, 200)?;
                check_optional_text("instanceId", settle.instance_id.as_deref(), 200)?;
                check_optional_text("transcriptPath", settle.transcript_path.as_deref(), 1024)?;
            }
            Action::Inspect(inspect) => {
                check_optional_text("bookId", inspect.book_id.as_deref(), 200)?;
            }
            Action::Accept(accept) => check_text("bookId", &accept.book_id, 200)?,
            Action::Repair(repair) => {
                check_text("bookId", &repair.book_id, 200)?;
                check_text("instanceId", &repair.instance_id, 200)?;
            }
            Action::Dispose(dispose) => {
                check_text("swarmId", &dispose.swarm_id, 200)?;
                check_optional_text("discardReason", dispose.discard_reason.as_deref(), 2000)?;
            }
            Action::Retained(retained) => {
                check_text("swarmId", &retained.swarm_id, 200)?;
                check_optional_text("after", retained.after.as_deref(), 200)?;
                if retained
                    .limit
                    .is_some_and(|limit| !(1..=super::READ_LIMIT_MAX).contains(&limit))
                {
                    return Err(format!("limit must be 1..{}", super::READ_LIMIT_MAX));
                }
            }
        }
        Ok(())
    }

    pub fn target(&self) -> Target {
        match &self.action {
            Action::Prepare(prepare) => match &prepare.predecessor {
                Some(predecessor) => Target::Book(predecessor.book_id.clone()),
                None => Target::NewSwarm {
                    swarm_key: prepare.swarm_key.clone(),
                },
            },
            Action::Open(open) => Target::TokenHash(token_hash(&open.token)),
            Action::Read(Read { book_id, .. })
            | Action::Append(Append { book_id, .. })
            | Action::Observe(Observe { book_id, .. })
            | Action::Checkpoint(Checkpoint { book_id, .. })
            | Action::Settle(Settle { book_id, .. })
            | Action::Accept(Accept { book_id, .. })
            | Action::Repair(Repair { book_id, .. }) => Target::Book(book_id.clone()),
            Action::Inspect(inspect) => match &inspect.book_id {
                Some(book_id) => Target::Book(book_id.clone()),
                None => Target::Caller,
            },
            Action::Dispose(dispose) => Target::Swarm(dispose.swarm_id.clone()),
            Action::Retained(retained) => Target::Swarm(retained.swarm_id.clone()),
        }
    }

    /// Stable digest of what this request asks for. A retried requestId must
    /// carry the same digest; tokens enter only as their hash.
    pub fn fingerprint(&self) -> String {
        let mut action = self.action.clone();
        match &mut action {
            Action::Prepare(prepare) => prepare.token = token_hash(&prepare.token),
            Action::Open(open) => open.token = token_hash(&open.token),
            _ => {}
        }
        let text = serde_json::to_string(&action).expect("worker lifecycle action serializes");
        token_hash(&text)
    }

    /// The token this request echoes back, never read from storage.
    pub fn echoed_token(&self) -> Option<&str> {
        match &self.action {
            Action::Prepare(prepare) => Some(&prepare.token),
            _ => None,
        }
    }
}

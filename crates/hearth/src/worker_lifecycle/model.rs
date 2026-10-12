use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Monotonic instance state. Order matters: a transition only ever moves
/// right, and compaction or restart never moves it back.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceState {
    Working,
    Wrapping,
    HandoffOnly,
    Retired,
}

impl InstanceState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Wrapping => "wrapping",
            Self::HandoffOnly => "handoff_only",
            Self::Retired => "retired",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "working" => Some(Self::Working),
            "wrapping" => Some(Self::Wrapping),
            "handoff_only" => Some(Self::HandoffOnly),
            "retired" => Some(Self::Retired),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwarmRecord {
    pub swarm_id: String,
    pub house_id: String,
    pub room: String,
    pub owner_spirit: String,
    pub owner_session: String,
    pub swarm_key: String,
    pub created_ms: i64,
    pub disposed_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookRecord {
    pub book_id: String,
    pub swarm_id: String,
    pub slot_key: String,
    pub agent_name: String,
    pub task_excerpt: String,
    /// The sequence the next appended entry receives. Starts at 1.
    pub next_sequence: i64,
    pub accepted_count: i64,
    pub created_ms: i64,
}

/// An operation whose start was recorded and whose end was not yet.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecord {
    pub operation_id: String,
    pub instance_id: String,
    pub started_sequence: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffRecord {
    /// True only when the worker itself committed its handoff report.
    pub complete: bool,
    /// `handoff` for a committed handoff, else the settlement outcome.
    pub outcome: String,
    /// Book sequence of the handoff entry that carries the report.
    pub sequence: i64,
    pub unresolved: Vec<OperationRecord>,
    pub transcript_path: Option<String>,
    /// Book sequence of a parent-supplied repair of an incomplete handoff.
    pub repair_sequence: Option<i64>,
}

impl HandoffRecord {
    /// A successor may continue from a committed or explicitly repaired handoff.
    pub const fn usable(&self) -> bool {
        self.complete || self.repair_sequence.is_some()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceRecord {
    pub instance_id: String,
    pub book_id: String,
    pub ordinal: i64,
    pub predecessor_instance_id: Option<String>,
    /// `swarmKey` + NUL + `slotKey` of the native task call that reserved it.
    pub reservation_key: String,
    pub token_hash: String,
    pub agent_name: String,
    pub bound_session: Option<String>,
    pub agent_id: Option<String>,
    pub transcript_path: Option<String>,
    pub state: InstanceState,
    pub warned: bool,
    pub handoff_requests: i64,
    pub checkpoint_sequence: Option<i64>,
    pub handoff: Option<HandoffRecord>,
    /// Set by a completed settlement; cleared when the session reopens.
    pub settled_outcome: Option<String>,
    pub own_entries: i64,
    pub last_own_sequence: Option<i64>,
    pub opened_count: i64,
    pub created_ms: i64,
    pub updated_ms: i64,
}

impl InstanceRecord {
    pub fn handoff_usable(&self) -> bool {
        self.handoff.as_ref().is_some_and(HandoffRecord::usable)
    }

    /// Disposal may proceed only over instances that are not live work.
    pub fn inactive(&self) -> bool {
        self.state == InstanceState::Retired || self.settled_outcome.is_some()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookState {
    pub book: BookRecord,
    /// Ordered by ordinal; the last one is the book's current instance.
    pub instances: Vec<InstanceRecord>,
    /// Unresolved operations only.
    pub operations: Vec<OperationRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwarmState {
    pub swarm: SwarmRecord,
    pub books: Vec<BookState>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryAuthor {
    Worker,
    Parent,
    Lifecycle,
}

impl EntryAuthor {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Worker => "worker",
            Self::Parent => "parent",
            Self::Lifecycle => "lifecycle",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "worker" => Some(Self::Worker),
            "parent" => Some(Self::Parent),
            "lifecycle" => Some(Self::Lifecycle),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryRecord {
    pub swarm_id: String,
    pub book_id: String,
    pub sequence: i64,
    pub instance_id: Option<String>,
    pub author: EntryAuthor,
    pub kind: String,
    pub body: Value,
    pub created_ms: i64,
}

/// Accepted evidence, retained outside the disposable book.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetainedRecord {
    pub retained_id: String,
    pub swarm_id: String,
    pub book_id: String,
    pub slot_key: String,
    pub agent_name: String,
    pub instance_ids: Vec<String>,
    pub accepted_by_room: String,
    pub accepted_by_spirit: String,
    pub accepted_by_session: String,
    pub body: Value,
    pub created_ms: i64,
}

/// The retained receipt of one whole-swarm disposal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DispositionRecord {
    pub swarm_id: String,
    pub disposed_by_room: String,
    pub disposed_by_spirit: String,
    pub disposed_by_session: String,
    pub discard_reason: Option<String>,
    pub books: Value,
    pub disposed_ms: i64,
}

/// One committed write request and its exact response, keyed by the caller's
/// authenticated session and requestId.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestRecord {
    pub house_id: String,
    pub room: String,
    pub session: String,
    pub request_id: String,
    pub action: String,
    pub fingerprint: String,
    pub response: Value,
    /// Cleared request records of a disposed swarm go with its transient rows.
    pub swarm_id: Option<String>,
    pub created_ms: i64,
}

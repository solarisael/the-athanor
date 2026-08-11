use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const HOST_SCHEMA_VERSION: u8 = 1;
pub const DEFAULT_HOST_WS_PATH: &str = "/athanor/v1/ws";
pub const DEFAULT_HOST_WS_URL: &str = "ws://127.0.0.1:8787/athanor/v1/ws";
pub const BOAT_RECEIPT_SCHEMA_VERSION: u8 = 1;
pub const BOAT_RECEIPT_STREAM_NAME: &str = "ATHANOR_BOAT_RECEIPTS";
pub const BOAT_RECEIPT_SUBJECT: &str = "athanor.boat.receipt.v1";
pub const PAPER_BOAT_RECEIPT_PROJECTION_ID: &str = "paper_boat_receipt";
pub const PAPER_BOAT_RECEIPT_SUBSCRIBE: &str = "athanor.paper_boat_receipt.subscribe";
pub const PAPER_BOAT_RECEIPT_SNAPSHOT: &str = "athanor.paper_boat_receipt.snapshot";
pub const RECALL_POLICY_PROJECTION_ID: &str = "recall_policy";
pub const RECALL_POLICY_SUBSCRIBE: &str = "athanor.recall_policy.subscribe";
pub const RECALL_POLICY_RESYNC: &str = "athanor.recall_policy.resync";
pub const RECALL_POLICY_SET_REQUESTED_MODE: &str = "athanor.recall_policy.set_requested_mode";
pub const RECALL_POLICY_ACKNOWLEDGE: &str = "athanor.recall_policy.acknowledge";
pub const RECALL_POLICY_EVALUATE: &str = "athanor.recall_policy.evaluate";
pub const RECALL_POLICY_COMPLETE_REFRESH: &str = "athanor.recall_policy.complete_refresh";
pub const RECALL_POLICY_FAIL_REFRESH: &str = "athanor.recall_policy.fail_refresh";
pub const RECALL_POLICY_INVALIDATE_AFTER_COMPACTION: &str =
    "athanor.recall_policy.invalidate_after_compaction";
pub const RECALL_POLICY_SNAPSHOT: &str = "athanor.recall_policy.snapshot";
pub const RECALL_POLICY_DELTA: &str = "athanor.recall_policy.delta";
pub const RECALL_POLICY_COMMAND_ACCEPTED: &str = "athanor.recall_policy.command_accepted";
pub const RECALL_POLICY_COMMAND_REFUSED: &str = "athanor.recall_policy.command_refused";
pub const RECALL_POLICY_COMMAND_FAILED: &str = "athanor.recall_policy.command_failed";
pub const RECALL_POLICY_FIELD_UPDATE: &str = "field_update";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallRequestedMode {
    Auto,
    Conversation,
    Work,
    Quiet,
}

impl RecallRequestedMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Conversation => "conversation",
            Self::Work => "work",
            Self::Quiet => "quiet",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallResolvedMode {
    Conversation,
    Work,
    Mixed,
    Quiet,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecallPolicyState {
    pub requested_mode: RecallRequestedMode,
    pub resolved_mode: RecallResolvedMode,
    pub active_project: Option<String>,
    pub resolution_reason: String,
    pub last_refresh_reason: Option<String>,
    pub last_refresh_at: Option<String>,
    pub working_set_entries: u64,
    pub recovery_pending: bool,
    pub recovery_terms: Vec<String>,
    pub degraded: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecallQueryRoute {
    #[serde(default)]
    pub intent: String,
    #[serde(default)]
    pub terms: Vec<String>,
    #[serde(default)]
    pub required_terms: Vec<String>,
    #[serde(default)]
    pub recognized_entities: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecallPolicyFacts {
    pub query_route: RecallQueryRoute,
    #[serde(default)]
    pub active_project: Option<String>,
    #[serde(default)]
    pub conversation_tokens: u64,
    #[serde(default)]
    pub working_set_present: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecallRefreshCompletion {
    pub query_terms: Vec<String>,
    pub refresh_reason: String,
    pub entries: u64,
    pub has_working_set: bool,
    #[serde(default)]
    pub warning: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecallPolicyDecision {
    pub should_recall: bool,
    pub clear_working_set: bool,
    pub query: String,
    pub query_terms: Vec<String>,
    pub refresh_reason: Option<String>,
    pub intent: String,
    pub resolved_mode: RecallResolvedMode,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRecordRef {
    pub record_type: String,
    pub record_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawClientCommand {
    schema_version: u8,
    message_id: String,
    house_id: String,
    sender_room: String,
    sender_spirit: String,
    sender_session: String,
    recipient: String,
    command_or_event_type: String,
    correlation_id: String,
    causation_id: String,
    reply_target: String,
    idempotency_key: String,
    source_record_refs: Vec<SourceRecordRef>,
    scope: String,
    visibility: String,
    authority_class: String,
    created_at: String,
    expires_at: String,
    max_hops: u8,
    projection_id: String,
    #[serde(default)]
    base_version: Option<u64>,
    #[serde(default)]
    mutations: Option<Vec<RawMutation>>,
    #[serde(default)]
    version: Option<u64>,
    #[serde(default)]
    sequence: Option<u64>,
    #[serde(default)]
    facts: Option<RecallPolicyFacts>,
    #[serde(default)]
    refresh: Option<RecallRefreshCompletion>,
    #[serde(default)]
    failure_reason: Option<String>,
    #[serde(default)]
    compaction_summary: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMutation {
    mutation_type: String,
    field: String,
    value: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandMeta {
    pub schema_version: u8,
    pub message_id: String,
    pub house_id: String,
    pub sender_room: String,
    pub sender_spirit: String,
    pub sender_session: String,
    pub recipient: String,
    pub correlation_id: String,
    pub causation_id: String,
    pub reply_target: String,
    pub idempotency_key: String,
    pub source_record_refs: Vec<SourceRecordRef>,
    pub scope: String,
    pub visibility: String,
    pub authority_class: String,
    pub created_at: String,
    pub expires_at: String,
    pub max_hops: u8,
    pub projection_id: String,
}

impl RawClientCommand {
    fn meta(&self) -> Result<CommandMeta, CommandParseError> {
        if self.schema_version != HOST_SCHEMA_VERSION {
            return Err(CommandParseError::new(
                self.message_id.clone(),
                self.idempotency_key.clone(),
                format!(
                    "unsupported schema_version {}; expected {}",
                    self.schema_version, HOST_SCHEMA_VERSION
                ),
            ));
        }
        if self.message_id.trim().is_empty() {
            return Err(CommandParseError::new(
                String::new(),
                self.idempotency_key.clone(),
                "message_id must not be blank",
            ));
        }
        if self.idempotency_key.trim().is_empty() {
            return Err(CommandParseError::new(
                self.message_id.clone(),
                String::new(),
                "idempotency_key must not be blank",
            ));
        }
        let expected_projection = match self.command_or_event_type.as_str() {
            PAPER_BOAT_RECEIPT_SUBSCRIBE => PAPER_BOAT_RECEIPT_PROJECTION_ID,
            _ => RECALL_POLICY_PROJECTION_ID,
        };
        if self.projection_id != expected_projection {
            return Err(CommandParseError::new(
                self.message_id.clone(),
                self.idempotency_key.clone(),
                format!("foreign projection_id {}", self.projection_id),
            ));
        }
        Ok(CommandMeta {
            schema_version: self.schema_version,
            message_id: self.message_id.clone(),
            house_id: self.house_id.clone(),
            sender_room: self.sender_room.clone(),
            sender_spirit: self.sender_spirit.clone(),
            sender_session: self.sender_session.clone(),
            recipient: self.recipient.clone(),
            correlation_id: self.correlation_id.clone(),
            causation_id: self.causation_id.clone(),
            reply_target: self.reply_target.clone(),
            idempotency_key: self.idempotency_key.clone(),
            source_record_refs: self.source_record_refs.clone(),
            scope: self.scope.clone(),
            visibility: self.visibility.clone(),
            authority_class: self.authority_class.clone(),
            created_at: self.created_at.clone(),
            expires_at: self.expires_at.clone(),
            max_hops: self.max_hops,
            projection_id: self.projection_id.clone(),
        })
    }

    fn no_command_payload(&self, meta: &CommandMeta) -> Result<(), CommandParseError> {
        if self.base_version.is_some()
            || self.mutations.is_some()
            || self.version.is_some()
            || self.sequence.is_some()
            || self.facts.is_some()
            || self.refresh.is_some()
            || self.failure_reason.is_some()
            || self.compaction_summary.is_some()
        {
            return Err(CommandParseError::from_meta(
                meta,
                "command carries fields not allowed for its type",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClientCommand {
    Subscribe {
        meta: CommandMeta,
    },
    PaperBoatReceiptSubscribe {
        meta: CommandMeta,
    },
    Resync {
        meta: CommandMeta,
    },
    SetRequestedMode {
        meta: CommandMeta,
        base_version: u64,
        requested_mode: RecallRequestedMode,
    },
    Evaluate {
        meta: CommandMeta,
        facts: RecallPolicyFacts,
    },
    CompleteRefresh {
        meta: CommandMeta,
        refresh: RecallRefreshCompletion,
    },
    FailRefresh {
        meta: CommandMeta,
        reason: String,
    },
    InvalidateAfterCompaction {
        meta: CommandMeta,
        summary: String,
    },
    Acknowledge {
        meta: CommandMeta,
        version: u64,
        sequence: u64,
    },
}

impl ClientCommand {
    pub fn meta(&self) -> &CommandMeta {
        match self {
            Self::Subscribe { meta }
            | Self::PaperBoatReceiptSubscribe { meta }
            | Self::Resync { meta }
            | Self::SetRequestedMode { meta, .. }
            | Self::Evaluate { meta, .. }
            | Self::CompleteRefresh { meta, .. }
            | Self::FailRefresh { meta, .. }
            | Self::InvalidateAfterCompaction { meta, .. }
            | Self::Acknowledge { meta, .. } => meta,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandParseError {
    pub message_id: String,
    pub idempotency_key: String,
    pub reason: String,
}

impl CommandParseError {
    fn new(message_id: String, idempotency_key: String, reason: impl Into<String>) -> Self {
        Self {
            message_id,
            idempotency_key,
            reason: reason.into(),
        }
    }

    fn from_meta(meta: &CommandMeta, reason: impl Into<String>) -> Self {
        Self::new(
            meta.message_id.clone(),
            meta.idempotency_key.clone(),
            reason,
        )
    }
}

pub fn parse_client_command(value: Value) -> Result<ClientCommand, CommandParseError> {
    let fallback_message_id = value
        .get("message_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let fallback_idempotency_key = value
        .get("idempotency_key")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let raw: RawClientCommand = serde_json::from_value(value).map_err(|error| {
        CommandParseError::new(
            fallback_message_id,
            fallback_idempotency_key,
            format!("malformed command envelope: {error}"),
        )
    })?;
    let meta = raw.meta()?;
    match raw.command_or_event_type.as_str() {
        PAPER_BOAT_RECEIPT_SUBSCRIBE => {
            raw.no_command_payload(&meta)?;
            Ok(ClientCommand::PaperBoatReceiptSubscribe { meta })
        }
        RECALL_POLICY_SUBSCRIBE => {
            raw.no_command_payload(&meta)?;
            Ok(ClientCommand::Subscribe { meta })
        }
        RECALL_POLICY_RESYNC => {
            raw.no_command_payload(&meta)?;
            Ok(ClientCommand::Resync { meta })
        }
        RECALL_POLICY_SET_REQUESTED_MODE => {
            if raw.version.is_some()
                || raw.sequence.is_some()
                || raw.facts.is_some()
                || raw.refresh.is_some()
                || raw.failure_reason.is_some()
                || raw.compaction_summary.is_some()
            {
                return Err(CommandParseError::from_meta(
                    &meta,
                    "set command carries fields for another command type",
                ));
            }
            let base_version = raw.base_version.ok_or_else(|| {
                CommandParseError::from_meta(&meta, "set command requires base_version")
            })?;
            let mutations = raw.mutations.ok_or_else(|| {
                CommandParseError::from_meta(&meta, "set command requires mutations")
            })?;
            let [mutation]: [RawMutation; 1] = mutations.try_into().map_err(|_| {
                CommandParseError::from_meta(
                    &meta,
                    "set command requires exactly one requested_mode mutation",
                )
            })?;
            if mutation.mutation_type != RECALL_POLICY_FIELD_UPDATE
                || mutation.field != "requested_mode"
            {
                return Err(CommandParseError::from_meta(
                    &meta,
                    "set command may update only requested_mode",
                ));
            }
            let requested_mode = serde_json::from_value(mutation.value).map_err(|error| {
                CommandParseError::from_meta(&meta, format!("unknown requested_mode: {error}"))
            })?;
            Ok(ClientCommand::SetRequestedMode {
                meta,
                base_version,
                requested_mode,
            })
        }
        RECALL_POLICY_EVALUATE => {
            if raw.base_version.is_some()
                || raw.mutations.is_some()
                || raw.version.is_some()
                || raw.sequence.is_some()
                || raw.refresh.is_some()
                || raw.failure_reason.is_some()
                || raw.compaction_summary.is_some()
            {
                return Err(CommandParseError::from_meta(
                    &meta,
                    "evaluate command carries fields for another command type",
                ));
            }
            let facts = raw.facts.ok_or_else(|| {
                CommandParseError::from_meta(&meta, "evaluate command requires facts")
            })?;
            Ok(ClientCommand::Evaluate { meta, facts })
        }
        RECALL_POLICY_COMPLETE_REFRESH => {
            if raw.base_version.is_some()
                || raw.mutations.is_some()
                || raw.version.is_some()
                || raw.sequence.is_some()
                || raw.facts.is_some()
                || raw.failure_reason.is_some()
                || raw.compaction_summary.is_some()
            {
                return Err(CommandParseError::from_meta(
                    &meta,
                    "complete_refresh command carries fields for another command type",
                ));
            }
            let refresh = raw.refresh.ok_or_else(|| {
                CommandParseError::from_meta(&meta, "complete_refresh command requires refresh")
            })?;
            Ok(ClientCommand::CompleteRefresh { meta, refresh })
        }
        RECALL_POLICY_FAIL_REFRESH => {
            if raw.base_version.is_some()
                || raw.mutations.is_some()
                || raw.version.is_some()
                || raw.sequence.is_some()
                || raw.facts.is_some()
                || raw.refresh.is_some()
                || raw.compaction_summary.is_some()
            {
                return Err(CommandParseError::from_meta(
                    &meta,
                    "fail_refresh command carries fields for another command type",
                ));
            }
            let reason = raw
                .failure_reason
                .filter(|reason| !reason.trim().is_empty())
                .ok_or_else(|| {
                    CommandParseError::from_meta(
                        &meta,
                        "fail_refresh command requires a nonblank failure_reason",
                    )
                })?;
            Ok(ClientCommand::FailRefresh { meta, reason })
        }
        RECALL_POLICY_INVALIDATE_AFTER_COMPACTION => {
            if raw.base_version.is_some()
                || raw.mutations.is_some()
                || raw.version.is_some()
                || raw.sequence.is_some()
                || raw.facts.is_some()
                || raw.refresh.is_some()
                || raw.failure_reason.is_some()
            {
                return Err(CommandParseError::from_meta(
                    &meta,
                    "invalidate command carries fields for another command type",
                ));
            }
            Ok(ClientCommand::InvalidateAfterCompaction {
                meta,
                summary: raw.compaction_summary.unwrap_or_default(),
            })
        }
        RECALL_POLICY_ACKNOWLEDGE => {
            if raw.base_version.is_some()
                || raw.mutations.is_some()
                || raw.facts.is_some()
                || raw.refresh.is_some()
                || raw.failure_reason.is_some()
                || raw.compaction_summary.is_some()
            {
                return Err(CommandParseError::from_meta(
                    &meta,
                    "acknowledge command carries fields for another command type",
                ));
            }
            let version = raw.version.ok_or_else(|| {
                CommandParseError::from_meta(&meta, "acknowledge requires version")
            })?;
            let sequence = raw.sequence.ok_or_else(|| {
                CommandParseError::from_meta(&meta, "acknowledge requires sequence")
            })?;
            Ok(ClientCommand::Acknowledge {
                meta,
                version,
                sequence,
            })
        }
        unknown => Err(CommandParseError::from_meta(
            &meta,
            format!("unknown command_or_event_type {unknown}"),
        )),
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventMeta {
    pub schema_version: u8,
    pub event_id: String,
    pub house_id: String,
    pub sender_room: String,
    pub sender_spirit: String,
    pub sender_session: String,
    pub recipient: String,
    pub command_or_event_type: String,
    pub correlation_id: String,
    pub causation_id: String,
    pub reply_target: String,
    pub idempotency_key: String,
    pub source_record_refs: Vec<SourceRecordRef>,
    pub scope: String,
    pub visibility: String,
    pub authority_class: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub max_hops: u8,
    pub projection_id: String,
    pub sequence: u64,
    pub state_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotEvent {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub snapshot_id: String,
    pub version: u64,
    pub state: RecallPolicyState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecallPolicyMutation {
    mutation_type: RecallPolicyMutationType,
    field: RecallPolicyField,
    value: Value,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RecallPolicyMutationType {
    FieldUpdate,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RecallPolicyField {
    RequestedMode,
    ResolvedMode,
    ActiveProject,
    ResolutionReason,
    LastRefreshReason,
    LastRefreshAt,
    WorkingSetEntries,
    RecoveryPending,
    RecoveryTerms,
    Degraded,
    UpdatedAt,
}

impl RecallPolicyMutation {
    pub fn between(previous: &RecallPolicyState, next: &RecallPolicyState) -> Vec<Self> {
        let mut mutations = Vec::with_capacity(4);
        macro_rules! changed {
            ($field:ident, $name:ident) => {
                if previous.$field != next.$field {
                    mutations.push(Self {
                        mutation_type: RecallPolicyMutationType::FieldUpdate,
                        field: RecallPolicyField::$name,
                        value: serde_json::to_value(&next.$field)
                            .expect("Recall Policy fields are JSON serializable"),
                    });
                }
            };
        }
        changed!(requested_mode, RequestedMode);
        changed!(resolved_mode, ResolvedMode);
        changed!(active_project, ActiveProject);
        changed!(resolution_reason, ResolutionReason);
        changed!(last_refresh_reason, LastRefreshReason);
        changed!(last_refresh_at, LastRefreshAt);
        changed!(working_set_entries, WorkingSetEntries);
        changed!(recovery_pending, RecoveryPending);
        changed!(recovery_terms, RecoveryTerms);
        changed!(degraded, Degraded);
        changed!(updated_at, UpdatedAt);
        mutations
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeltaEvent {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub delta_id: String,
    pub base_version: u64,
    pub next_version: u64,
    pub source_event_ids: Vec<String>,
    pub mutations: Vec<RecallPolicyMutation>,
    pub coalesce_key: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandOutcomeEvent {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub reason: Option<String>,
    #[serde(default)]
    pub version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<RecallPolicyState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<RecallPolicyDecision>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoatReceiptProjection {
    pub schema_version: u8,
    pub event_id: String,
    pub record_id: String,
    pub room: String,
    pub processed_at: String,
    pub original_stream_sequence: u64,
    pub integrity_sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperBoatReceiptStatus {
    Pending,
    Delivered,
    Degraded,
    Refused,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaperBoatReceiptState {
    pub status: PaperBoatReceiptStatus,
    pub receipt: Option<BoatReceiptProjection>,
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PaperBoatReceiptEvent {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub snapshot_id: String,
    pub state: PaperBoatReceiptState,
}

#[cfg(test)]
mod receipt_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn broker_receipt_projection_is_exact_and_private_fields_are_refused() {
        let valid = json!({
            "schema_version": 1,
            "event_id": "8d2c04ae-ef20-4fbc-8141-d0259cbf495f",
            "record_id": "42",
            "room": "work",
            "processed_at": "2026-08-10T09:30:00Z",
            "original_stream_sequence": 7,
            "integrity_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        });
        let parsed: BoatReceiptProjection = serde_json::from_value(valid.clone()).unwrap();
        assert_eq!(parsed.record_id, "42");

        for private_field in ["body", "title"] {
            let mut private = valid.clone();
            private[private_field] = json!("must not cross");
            assert!(serde_json::from_value::<BoatReceiptProjection>(private).is_err());
        }
    }

    #[test]
    fn paper_boat_subscription_uses_its_own_projection() {
        let command = json!({
            "schema_version": 1,
            "message_id": "message-1",
            "house_id": "",
            "sender_room": "",
            "sender_spirit": "",
            "sender_session": "",
            "recipient": "",
            "command_or_event_type": PAPER_BOAT_RECEIPT_SUBSCRIBE,
            "correlation_id": "message-1",
            "causation_id": "",
            "reply_target": "",
            "idempotency_key": "receipt-subscribe-1",
            "source_record_refs": [],
            "scope": "",
            "visibility": "",
            "authority_class": "",
            "created_at": "2026-08-10T09:30:00Z",
            "expires_at": "2026-08-10T09:31:00Z",
            "max_hops": 1,
            "projection_id": PAPER_BOAT_RECEIPT_PROJECTION_ID
        });
        assert!(matches!(
            parse_client_command(command),
            Ok(ClientCommand::PaperBoatReceiptSubscribe { .. })
        ));
    }
}

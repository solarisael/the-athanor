use house_core::context::{ContextAnalysis, ContextAnalysisRequest};
use house_core::conversation::VisibleMessage;
use house_core::hallway::HallwayInboxReceipt;
use house_core::lineage::{QuestBatch, QuestLifecycle, QuestMemory};
use house_core::triggers::ProcessLesson;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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
pub const CONTEXT_PROJECTION_ID: &str = "context";
pub const CONTEXT_ANALYZE: &str = "athanor.context.analyze";
pub const CONTEXT_ANALYZED: &str = "athanor.context.analyzed";
pub const CONTEXT_VIEWPORT: &str = "athanor.context.viewport";
pub const CONTEXT_VIEWPORTED: &str = "athanor.context.viewported";
pub const HALLWAY_PROJECTION_ID: &str = "hallway";
pub const HALLWAY_INBOX_PROJECT: &str = "athanor.hallway.inbox_project";
pub const HALLWAY_INBOX_PROJECTED: &str = "athanor.hallway.inbox_projected";
pub const HALLWAY_KNOCK_CLAIM: &str = "athanor.hallway.knock_claim";
pub const HALLWAY_KNOCK_CLAIMED: &str = "athanor.hallway.knock_claimed";
pub const HALLWAY_KNOCK_SETTLE: &str = "athanor.hallway.knock_settle";
pub const HALLWAY_KNOCK_SETTLED: &str = "athanor.hallway.knock_settled";
pub const HALLWAY_KNOCK_COMMAND_FAILED: &str = "athanor.hallway.knock_command_failed";
pub const HALLWAY_KNOCK_COMMAND_REFUSED: &str = "athanor.hallway.knock_command_refused";
pub const ROUTING_PROJECTION_ID: &str = "routing";
pub const ROUTING_STATUS: &str = "athanor.routing.status";
pub const ROUTING_DISPATCH: &str = "athanor.routing.dispatch";
pub const FAMILIAR_STATUS: &str = "athanor.familiar.status";
pub const ROUTING_RESULT: &str = "athanor.routing.result";
pub const LINEAGE_PROJECTION_ID: &str = "lineage";
pub const LINEAGE_NORMALIZE: &str = "athanor.lineage.normalize";
pub const LINEAGE_LIFECYCLE: &str = "athanor.lineage.lifecycle";
pub const LINEAGE_NORMALIZED: &str = "athanor.lineage.normalized";
pub const SHELL_PROJECTION_ID: &str = "shell";
pub const SHELL_CONVERSATION_LOG: &str = "athanor.shell.conversation_log";
pub const SHELL_LESSON_PLAN: &str = "athanor.shell.lesson_plan";
pub const SHELL_PROCESS_LESSONS: &str = "athanor.shell.process_lessons";
pub const SHELL_RESULT: &str = "athanor.shell.result";
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallPolicyState {
    pub requested_mode: RecallRequestedMode,
    pub resolved_mode: RecallResolvedMode,
    pub active_project: Option<String>,
    pub resolution_reason: String,
    pub last_refresh_reason: Option<String>,
    pub last_refresh_at: Option<String>,
    pub working_set_entries: u64,
    pub recovery_state: RecoveryState,
    pub degraded: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryState {
    Idle,
    Pending { terms: Vec<String> },
}

impl RecoveryState {
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending { .. })
    }

    pub fn terms(&self) -> &[String] {
        match self {
            Self::Idle => &[],
            Self::Pending { terms } => terms,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LegacyRecallPolicyState {
    #[serde(alias = "requested_mode")]
    requested_mode: RecallRequestedMode,
    #[serde(alias = "resolved_mode")]
    resolved_mode: RecallResolvedMode,
    #[serde(alias = "active_project")]
    active_project: Option<String>,
    #[serde(alias = "resolution_reason")]
    resolution_reason: String,
    #[serde(alias = "last_refresh_reason")]
    last_refresh_reason: Option<String>,
    #[serde(alias = "last_refresh_at")]
    last_refresh_at: Option<String>,
    #[serde(alias = "working_set_entries")]
    working_set_entries: u64,
    #[serde(alias = "recovery_pending")]
    recovery_pending: bool,
    #[serde(alias = "recovery_terms")]
    recovery_terms: Vec<String>,
    degraded: Option<String>,
    #[serde(alias = "updated_at")]
    updated_at: Option<String>,
}

impl<'de> Deserialize<'de> for RecallPolicyState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let legacy = LegacyRecallPolicyState::deserialize(deserializer)?;
        let recovery_state = if legacy.recovery_pending {
            RecoveryState::Pending {
                terms: legacy.recovery_terms,
            }
        } else {
            RecoveryState::Idle
        };
        Ok(Self {
            requested_mode: legacy.requested_mode,
            resolved_mode: legacy.resolved_mode,
            active_project: legacy.active_project,
            resolution_reason: legacy.resolution_reason,
            last_refresh_reason: legacy.last_refresh_reason,
            last_refresh_at: legacy.last_refresh_at,
            working_set_entries: legacy.working_set_entries,
            recovery_state,
            degraded: legacy.degraded,
            updated_at: legacy.updated_at,
        })
    }
}

impl Serialize for RecallPolicyState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Legacy<'a> {
            requested_mode: RecallRequestedMode,
            resolved_mode: RecallResolvedMode,
            active_project: &'a Option<String>,
            resolution_reason: &'a String,
            last_refresh_reason: &'a Option<String>,
            last_refresh_at: &'a Option<String>,
            working_set_entries: u64,
            recovery_pending: bool,
            recovery_terms: &'a [String],
            degraded: &'a Option<String>,
            updated_at: &'a Option<String>,
        }
        Legacy {
            requested_mode: self.requested_mode,
            resolved_mode: self.resolved_mode,
            active_project: &self.active_project,
            resolution_reason: &self.resolution_reason,
            last_refresh_reason: &self.last_refresh_reason,
            last_refresh_at: &self.last_refresh_at,
            working_set_entries: self.working_set_entries,
            recovery_pending: self.recovery_state.is_pending(),
            recovery_terms: self.recovery_state.terms(),
            degraded: &self.degraded,
            updated_at: &self.updated_at,
        }
        .serialize(serializer)
    }
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
    #[serde(default)]
    pub tool_evidence: bool,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallPolicyDecision {
    pub action: RecallAction,
    pub query: String,
    pub query_terms: Vec<String>,
    pub refresh_reason: Option<String>,
    pub intent: String,
    pub resolved_mode: RecallResolvedMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecallAction {
    None,
    Clear,
    Refresh,
    ClearThenRefresh,
}

impl RecallAction {
    pub const fn clears_working_set(self) -> bool {
        matches!(self, Self::Clear | Self::ClearThenRefresh)
    }

    pub const fn refreshes(self) -> bool {
        matches!(self, Self::Refresh | Self::ClearThenRefresh)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LegacyRecallPolicyDecision {
    #[serde(alias = "should_recall")]
    should_recall: bool,
    #[serde(alias = "clear_working_set")]
    clear_working_set: bool,
    query: String,
    #[serde(alias = "query_terms")]
    query_terms: Vec<String>,
    #[serde(alias = "refresh_reason")]
    refresh_reason: Option<String>,
    intent: String,
    #[serde(alias = "resolved_mode")]
    resolved_mode: RecallResolvedMode,
}

impl<'de> Deserialize<'de> for RecallPolicyDecision {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let legacy = LegacyRecallPolicyDecision::deserialize(deserializer)?;
        let action = match (legacy.clear_working_set, legacy.should_recall) {
            (false, false) => RecallAction::None,
            (true, false) => RecallAction::Clear,
            (false, true) => RecallAction::Refresh,
            (true, true) => RecallAction::ClearThenRefresh,
        };
        Ok(Self {
            action,
            query: legacy.query,
            query_terms: legacy.query_terms,
            refresh_reason: legacy.refresh_reason,
            intent: legacy.intent,
            resolved_mode: legacy.resolved_mode,
        })
    }
}

impl Serialize for RecallPolicyDecision {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Legacy<'a> {
            should_recall: bool,
            clear_working_set: bool,
            query: &'a String,
            query_terms: &'a Vec<String>,
            refresh_reason: &'a Option<String>,
            intent: &'a String,
            resolved_mode: RecallResolvedMode,
        }
        Legacy {
            should_recall: self.action.refreshes(),
            clear_working_set: self.action.clears_working_set(),
            query: &self.query,
            query_terms: &self.query_terms,
            refresh_reason: &self.refresh_reason,
            intent: &self.intent,
            resolved_mode: self.resolved_mode,
        }
        .serialize(serializer)
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRecordRef {
    pub record_type: String,
    pub record_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HallwayKnockSettlePayload {
    pub knock_id: String,
    pub outcome: house_core::hallway::HallwayKnockOutcome,
    #[serde(default)]
    pub reason: Option<String>,
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
    #[serde(default)]
    context_request: Option<ContextAnalysisRequest>,
    #[serde(default)]
    context_viewport_mode: Option<crate::RecallViewportMode>,
    #[serde(default)]
    routing_request: Option<Value>,
    #[serde(default)]
    room_dir: Option<String>,
    #[serde(default)]
    lineage_request: Option<QuestBatch>,
    #[serde(default)]
    lineage_lifecycle: Option<QuestLifecycle>,
    #[serde(default)]
    conversation_request: Option<ConversationLogRequest>,
    #[serde(default)]
    trigger_request: Option<ProcessTriggerRequest>,
    #[serde(default)]
    recall_result: Option<crate::RecallResultInput>,
    #[serde(default)]
    hallway_knock_settle: Option<HallwayKnockSettlePayload>,
}

/// One conversation-capture request: the visible window as the harness renders
/// it, plus who the room attributes each side to. Every rule about identity,
/// freshness, dedupe, and transcript shape lives in `house_core::conversation`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ConversationLogRequest {
    pub room_dir: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub operator: String,
    #[serde(default)]
    pub spirit: String,
    #[serde(default)]
    pub source: String,
    /// Replayed sessions observe turns without making them durable.
    #[serde(default)]
    pub persist: bool,
    #[serde(default)]
    pub messages: Vec<VisibleMessage>,
}

/// A matched process trigger, optionally carrying the lesson rows the adapter
/// fetched for the plan this Host issued.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProcessTriggerRequest {
    #[serde(default)]
    pub trigger: Option<String>,
    #[serde(default)]
    pub lessons: Vec<ProcessLesson>,
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
            CONTEXT_ANALYZE | CONTEXT_VIEWPORT => CONTEXT_PROJECTION_ID,
            HALLWAY_INBOX_PROJECT | HALLWAY_KNOCK_CLAIM | HALLWAY_KNOCK_SETTLE => {
                HALLWAY_PROJECTION_ID
            }
            ROUTING_STATUS | ROUTING_DISPATCH | FAMILIAR_STATUS => ROUTING_PROJECTION_ID,
            LINEAGE_NORMALIZE | LINEAGE_LIFECYCLE => LINEAGE_PROJECTION_ID,
            SHELL_CONVERSATION_LOG | SHELL_LESSON_PLAN | SHELL_PROCESS_LESSONS => {
                SHELL_PROJECTION_ID
            }
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
            || self.routing_request.is_some()
            || self.room_dir.is_some()
            || self.context_request.is_some()
            || self.context_viewport_mode.is_some()
            || self.lineage_request.is_some()
            || self.lineage_lifecycle.is_some()
            || self.conversation_request.is_some()
            || self.trigger_request.is_some()
            || self.recall_result.is_some()
        {
            return Err(CommandParseError::from_meta(
                meta,
                "command carries fields not allowed for its type",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
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
    AnalyzeContext {
        meta: CommandMeta,
        request: ContextAnalysisRequest,
    },
    ProjectHallwayInbox {
        meta: CommandMeta,
    },
    ClaimHallwayKnock {
        meta: CommandMeta,
    },
    SettleHallwayKnock {
        meta: CommandMeta,
        request: HallwayKnockSettlePayload,
    },
    RoutingStatus {
        meta: CommandMeta,
    },
    /// One dispatch selector — a lane or a familiar — resolved by the House.
    /// `room_dir` lets the Host read the room spellbook a familiar needs.
    RoutingDispatch {
        meta: CommandMeta,
        room_dir: Option<String>,
        request: Value,
    },
    FamiliarStatus {
        meta: CommandMeta,
        room_dir: Option<String>,
    },
    NormalizeLineage {
        meta: CommandMeta,
        request: QuestBatch,
    },
    SettleLineage {
        meta: CommandMeta,
        lifecycle: QuestLifecycle,
    },
    LogConversation {
        meta: CommandMeta,
        request: ConversationLogRequest,
    },
    PlanTriggerLessons {
        meta: CommandMeta,
        request: ProcessTriggerRequest,
    },
    BraidTriggerLessons {
        meta: CommandMeta,
        request: ProcessTriggerRequest,
    },
    ApplyRecallViewport {
        meta: CommandMeta,
        result: crate::RecallResultInput,
        mode: crate::RecallViewportMode,
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
            | Self::AnalyzeContext { meta, .. }
            | Self::ProjectHallwayInbox { meta }
            | Self::ClaimHallwayKnock { meta }
            | Self::SettleHallwayKnock { meta, .. }
            | Self::RoutingStatus { meta }
            | Self::RoutingDispatch { meta, .. }
            | Self::FamiliarStatus { meta, .. }
            | Self::NormalizeLineage { meta, .. }
            | Self::SettleLineage { meta, .. }
            | Self::LogConversation { meta, .. }
            | Self::PlanTriggerLessons { meta, .. }
            | Self::BraidTriggerLessons { meta, .. }
            | Self::ApplyRecallViewport { meta, .. }
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
    if raw.hallway_knock_settle.is_some() && raw.command_or_event_type != HALLWAY_KNOCK_SETTLE {
        return Err(CommandParseError::from_meta(
            &meta,
            "hallway knock settle payload belongs only to its command",
        ));
    }
    match raw.command_or_event_type.as_str() {
        PAPER_BOAT_RECEIPT_SUBSCRIBE => {
            raw.no_command_payload(&meta)?;
            Ok(ClientCommand::PaperBoatReceiptSubscribe { meta })
        }
        HALLWAY_INBOX_PROJECT => {
            raw.no_command_payload(&meta)?;
            Ok(ClientCommand::ProjectHallwayInbox { meta })
        }
        HALLWAY_KNOCK_CLAIM => {
            raw.no_command_payload(&meta)?;
            Ok(ClientCommand::ClaimHallwayKnock { meta })
        }
        HALLWAY_KNOCK_SETTLE => {
            let request = raw.hallway_knock_settle.clone().ok_or_else(|| {
                CommandParseError::from_meta(
                    &meta,
                    "knock settle command requires hallwayKnockSettle",
                )
            })?;
            if raw.base_version.is_some()
                || raw.mutations.is_some()
                || raw.version.is_some()
                || raw.sequence.is_some()
                || raw.facts.is_some()
                || raw.refresh.is_some()
                || raw.failure_reason.is_some()
                || raw.compaction_summary.is_some()
                || raw.routing_request.is_some()
                || raw.room_dir.is_some()
                || raw.context_request.is_some()
                || raw.context_viewport_mode.is_some()
                || raw.lineage_request.is_some()
                || raw.lineage_lifecycle.is_some()
                || raw.conversation_request.is_some()
                || raw.trigger_request.is_some()
                || raw.recall_result.is_some()
            {
                return Err(CommandParseError::from_meta(
                    &meta,
                    "knock settle command carries fields for another command type",
                ));
            }
            Ok(ClientCommand::SettleHallwayKnock { meta, request })
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
        CONTEXT_ANALYZE => {
            if raw.base_version.is_some()
                || raw.mutations.is_some()
                || raw.version.is_some()
                || raw.sequence.is_some()
                || raw.facts.is_some()
                || raw.refresh.is_some()
                || raw.failure_reason.is_some()
                || raw.compaction_summary.is_some()
                || raw.context_viewport_mode.is_some()
                || raw.recall_result.is_some()
            {
                return Err(CommandParseError::from_meta(
                    &meta,
                    "context analyze carries fields not allowed for its type",
                ));
            }
            let request = raw.context_request.ok_or_else(|| {
                CommandParseError::from_meta(&meta, "context analyze requires context_request")
            })?;
            request
                .validate(&meta.sender_room)
                .map_err(|error| CommandParseError::from_meta(&meta, error.to_string()))?;
            Ok(ClientCommand::AnalyzeContext { meta, request })
        }
        ROUTING_STATUS => {
            raw.no_command_payload(&meta)?;
            Ok(ClientCommand::RoutingStatus { meta })
        }
        ROUTING_DISPATCH => {
            let request = raw.routing_request.ok_or_else(|| {
                CommandParseError::from_meta(&meta, "routing dispatch requires routing_request")
            })?;
            Ok(ClientCommand::RoutingDispatch {
                meta,
                room_dir: raw.room_dir.filter(|value| !value.trim().is_empty()),
                request,
            })
        }
        FAMILIAR_STATUS => Ok(ClientCommand::FamiliarStatus {
            meta,
            room_dir: raw.room_dir.filter(|value| !value.trim().is_empty()),
        }),
        LINEAGE_NORMALIZE => {
            let request = raw.lineage_request.ok_or_else(|| {
                CommandParseError::from_meta(&meta, "lineage normalize requires lineage_request")
            })?;
            Ok(ClientCommand::NormalizeLineage { meta, request })
        }
        LINEAGE_LIFECYCLE => {
            let lifecycle = raw.lineage_lifecycle.ok_or_else(|| {
                CommandParseError::from_meta(&meta, "lineage lifecycle requires lineage_lifecycle")
            })?;
            Ok(ClientCommand::SettleLineage { meta, lifecycle })
        }
        SHELL_CONVERSATION_LOG => {
            let request = raw.conversation_request.ok_or_else(|| {
                CommandParseError::from_meta(
                    &meta,
                    "conversation log requires conversation_request",
                )
            })?;
            if request.room_dir.trim().is_empty() {
                return Err(CommandParseError::from_meta(
                    &meta,
                    "conversation log requires room_dir",
                ));
            }
            Ok(ClientCommand::LogConversation { meta, request })
        }
        SHELL_LESSON_PLAN => {
            let request = raw.trigger_request.ok_or_else(|| {
                CommandParseError::from_meta(&meta, "lesson plan requires trigger_request")
            })?;
            Ok(ClientCommand::PlanTriggerLessons { meta, request })
        }
        SHELL_PROCESS_LESSONS => {
            let request = raw.trigger_request.ok_or_else(|| {
                CommandParseError::from_meta(&meta, "process lessons requires trigger_request")
            })?;
            Ok(ClientCommand::BraidTriggerLessons { meta, request })
        }
        CONTEXT_VIEWPORT => {
            if raw.context_request.is_some()
                || raw.base_version.is_some()
                || raw.mutations.is_some()
                || raw.version.is_some()
                || raw.sequence.is_some()
                || raw.facts.is_some()
                || raw.refresh.is_some()
                || raw.failure_reason.is_some()
                || raw.compaction_summary.is_some()
            {
                return Err(CommandParseError::from_meta(
                    &meta,
                    "context viewport carries fields not allowed for its type",
                ));
            }
            let result = raw.recall_result.ok_or_else(|| {
                CommandParseError::from_meta(&meta, "context viewport requires recall_result")
            })?;
            Ok(ClientCommand::ApplyRecallViewport {
                meta,
                result,
                mode: raw
                    .context_viewport_mode
                    .unwrap_or(crate::RecallViewportMode::Automatic),
            })
        }
        unknown => Err(CommandParseError::from_meta(
            &meta,
            format!("unknown command_or_event_type {unknown}"),
        )),
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextAnalysisEvent {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub analysis: ContextAnalysis,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContextViewportEvent {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub result: crate::RecallViewportResult,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HallwayInboxProjectionEvent {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub changed: bool,
    pub inbox: HallwayInboxReceipt,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HallwayKnockClaimedEvent {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub result: house_core::hallway::HallwayKnockClaimReceipt,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HallwayKnockSettledEvent {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub result: house_core::hallway::HallwayKnockSettleReceipt,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RoutingResultEvent {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub result: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LineageResultEvent {
    #[serde(flatten)]
    pub meta: EventMeta,
    /// Whether the quest this event answers has reached a terminal state. The
    /// adapter releases its join bookkeeping on this, never on its own reading
    /// of a status string.
    pub settled: bool,
    pub memories: Vec<QuestMemory>,
}

/// Bounded result of one OMP-shell decision: conversation capture, a lesson
/// plan, or a process-lesson braid.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ShellResultEvent {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub result: Value,
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
        if previous.recovery_state.is_pending() != next.recovery_state.is_pending() {
            mutations.push(Self {
                mutation_type: RecallPolicyMutationType::FieldUpdate,
                field: RecallPolicyField::RecoveryPending,
                value: Value::Bool(next.recovery_state.is_pending()),
            });
        }
        if previous.recovery_state.terms() != next.recovery_state.terms() {
            mutations.push(Self {
                mutation_type: RecallPolicyMutationType::FieldUpdate,
                field: RecallPolicyField::RecoveryTerms,
                value: serde_json::to_value(next.recovery_state.terms())
                    .expect("Recall recovery terms are JSON serializable"),
            });
        }
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
    fn recall_policy_state_reads_legacy_snake_case_receipts_but_writes_camel_case() {
        let legacy = json!({
            "requested_mode": "auto",
            "resolved_mode": "conversation",
            "active_project": null,
            "resolution_reason": "explicit-lookup",
            "last_refresh_reason": "empty-working-set",
            "last_refresh_at": "2026-08-12T22:44:47.061Z",
            "working_set_entries": 0,
            "recovery_pending": false,
            "recovery_terms": [],
            "degraded": null,
            "updated_at": "2026-08-12T22:44:49.981Z"
        });
        let state: RecallPolicyState = serde_json::from_value(legacy).unwrap();
        assert_eq!(state.recovery_state, RecoveryState::Idle);
        let current = serde_json::to_value(state).unwrap();
        assert_eq!(current["requestedMode"], "auto");
        assert!(current.get("requested_mode").is_none());
    }

    #[test]
    fn recall_policy_decision_reads_legacy_snake_case_but_writes_camel_case() {
        let legacy = json!({
            "should_recall": true,
            "clear_working_set": false,
            "query": "exact evidence",
            "query_terms": ["exact", "evidence"],
            "refresh_reason": "explicit-lookup",
            "intent": "work",
            "resolved_mode": "conversation"
        });
        let decision: RecallPolicyDecision = serde_json::from_value(legacy).unwrap();
        assert_eq!(decision.action, RecallAction::Refresh);
        let current = serde_json::to_value(decision).unwrap();
        assert_eq!(current["shouldRecall"], true);
        assert_eq!(current["resolvedMode"], "conversation");
        assert!(current.get("should_recall").is_none());
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
    #[test]
    fn hallway_inbox_projection_uses_host_owned_projection() {
        let command = json!({
            "schema_version": 1,
            "message_id": "hallway-1",
            "house_id": "solarisael",
            "sender_room": "tuner",
            "sender_spirit": "Tuner",
            "sender_session": "session-1",
            "recipient": "house-host",
            "command_or_event_type": HALLWAY_INBOX_PROJECT,
            "correlation_id": "hallway-1",
            "causation_id": "",
            "reply_target": "session-1",
            "idempotency_key": "hallway-1",
            "source_record_refs": [],
            "scope": "room:tuner:recall_policy",
            "visibility": "operator",
            "authority_class": "room_state",
            "created_at": "2026-08-18T18:00:00Z",
            "expires_at": "2026-08-18T18:01:00Z",
            "max_hops": 1,
            "projection_id": HALLWAY_PROJECTION_ID
        });
        assert!(matches!(
            parse_client_command(command),
            Ok(ClientCommand::ProjectHallwayInbox { .. })
        ));
    }

    #[test]
    fn hallway_knock_claim_and_settle_keep_the_hallway_projection() {
        let envelope = |kind: &str, message_id: &str| {
            json!({
                "schema_version": 1,
                "message_id": message_id,
                "house_id": "solarisael",
                "sender_room": "kodo",
                "sender_spirit": "Kodo",
                "sender_session": "session-knock",
                "recipient": "house-host",
                "command_or_event_type": kind,
                "correlation_id": message_id,
                "causation_id": "",
                "reply_target": "session-knock",
                "idempotency_key": message_id,
                "source_record_refs": [],
                "scope": "room:kodo:recall_policy",
                "visibility": "operator",
                "authority_class": "room_state",
                "created_at": "2026-08-18T18:00:00Z",
                "expires_at": "2026-08-18T18:01:00Z",
                "max_hops": 1,
                "projection_id": HALLWAY_PROJECTION_ID
            })
        };
        assert!(matches!(
            parse_client_command(envelope(HALLWAY_KNOCK_CLAIM, "knock-claim")),
            Ok(ClientCommand::ClaimHallwayKnock { .. })
        ));

        let mut settle = envelope(HALLWAY_KNOCK_SETTLE, "knock-settle");
        settle["hallway_knock_settle"] = json!({
            "knockId": "5af35bb5-e9a1-4e58-849b-b78b6614bc15",
            "outcome": "completed",
            "reason": null
        });
        assert!(matches!(
            parse_client_command(settle),
            Ok(ClientCommand::SettleHallwayKnock { .. })
        ));
    }
}

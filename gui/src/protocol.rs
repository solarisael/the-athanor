//! Versioned Athanor Host envelope binding for the Recall Policy projection.
//!
//! Every wire literal used by the client lives in this module. Display text
//! never reaches the wire and wire values are never shown as labels without
//! being marked as wire values.
//!
//! Envelope fields follow `docs/RUNTIME_ARCHITECTURE.md` sections 4.1 and 4.5.
//! Wire names are imported from `house-protocol`, the same crate used by the
//! Host, so the native client cannot drift onto a guessed path or type.

use godot::classes::Time;
use godot::prelude::*;
use house_protocol::{
    DEFAULT_HOST_WS_URL, FAMILIAR_STATUS, HOST_SCHEMA_VERSION, PAPER_BOAT_RECEIPT_PROJECTION_ID,
    PAPER_BOAT_RECEIPT_SNAPSHOT, PAPER_BOAT_RECEIPT_SUBSCRIBE, RECALL_POLICY_ACKNOWLEDGE,
    RECALL_POLICY_COMMAND_ACCEPTED, RECALL_POLICY_COMMAND_FAILED, RECALL_POLICY_COMMAND_REFUSED,
    RECALL_POLICY_DELTA, RECALL_POLICY_FIELD_UPDATE, RECALL_POLICY_PROJECTION_ID,
    RECALL_POLICY_RESYNC, RECALL_POLICY_SET_REQUESTED_MODE, RECALL_POLICY_SNAPSHOT,
    RECALL_POLICY_SUBSCRIBE, ROUTING_DISPATCH, ROUTING_PROJECTION_ID, ROUTING_RESULT,
    ROUTING_STATUS,
};

/// Explicit schema version negotiated with the Host. A mismatch is refused,
/// never coerced.
pub const SCHEMA_VERSION: i64 = HOST_SCHEMA_VERSION as i64;

/// Projection identifier for the Recall Policy state owned by the Host.
pub const PROJECTION_ID: &str = RECALL_POLICY_PROJECTION_ID;
pub const DEFAULT_HOST_URL: &str = DEFAULT_HOST_WS_URL;

/// The client speaks to the Host directly; no relay hops are allowed.
pub const MAX_HOPS: i64 = 1;

/// Bound lifetime of an operator-authored command.
pub const COMMAND_EXPIRY_SECONDS: i64 = 30;

// ---------------------------------------------------------------------------
// Command and event type vocabulary
// ---------------------------------------------------------------------------

pub const CMD_SUBSCRIBE: &str = RECALL_POLICY_SUBSCRIBE;
pub const CMD_RESYNC: &str = RECALL_POLICY_RESYNC;
pub const CMD_SET_REQUESTED_MODE: &str = RECALL_POLICY_SET_REQUESTED_MODE;
pub const CMD_ACKNOWLEDGE: &str = RECALL_POLICY_ACKNOWLEDGE;

pub const EVT_SNAPSHOT: &str = RECALL_POLICY_SNAPSHOT;
pub const EVT_DELTA: &str = RECALL_POLICY_DELTA;
pub const EVT_COMMAND_ACCEPTED: &str = RECALL_POLICY_COMMAND_ACCEPTED;
pub const EVT_COMMAND_REFUSED: &str = RECALL_POLICY_COMMAND_REFUSED;
pub const EVT_COMMAND_FAILED: &str = RECALL_POLICY_COMMAND_FAILED;

/// Command and event mutations use separate exact vocabularies.
pub const MUTATION_FIELD_UPDATE: &str = RECALL_POLICY_FIELD_UPDATE;

// ---------------------------------------------------------------------------
// Requested mode: the operator-writable axis
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RequestedMode {
    Auto,
    Conversation,
    Work,
    Quiet,
}

impl RequestedMode {
    pub const ALL: [RequestedMode; 4] = [
        RequestedMode::Auto,
        RequestedMode::Conversation,
        RequestedMode::Work,
        RequestedMode::Quiet,
    ];

    /// Value written to the wire. Never a display label.
    pub fn wire(self) -> &'static str {
        match self {
            RequestedMode::Auto => "auto",
            RequestedMode::Conversation => "conversation",
            RequestedMode::Work => "work",
            RequestedMode::Quiet => "quiet",
        }
    }

    /// Operator-facing label. Never compared to or sent as a wire value.
    pub fn display(self) -> &'static str {
        match self {
            RequestedMode::Auto => "AUTO",
            RequestedMode::Conversation => "CONVERSATION",
            RequestedMode::Work => "WORK",
            RequestedMode::Quiet => "QUIET",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.wire() == value)
    }
}

// ---------------------------------------------------------------------------
// Resolved mode: Host-derived, read-only
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ResolvedMode {
    Conversation,
    Work,
    Mixed,
    Quiet,
}

impl ResolvedMode {
    const ALL: [ResolvedMode; 4] = [
        ResolvedMode::Conversation,
        ResolvedMode::Work,
        ResolvedMode::Mixed,
        ResolvedMode::Quiet,
    ];

    pub fn wire(self) -> &'static str {
        match self {
            ResolvedMode::Conversation => "conversation",
            ResolvedMode::Work => "work",
            ResolvedMode::Mixed => "mixed",
            ResolvedMode::Quiet => "quiet",
        }
    }

    pub fn display(self) -> &'static str {
        match self {
            ResolvedMode::Conversation => "CONVERSATION",
            ResolvedMode::Work => "WORK",
            ResolvedMode::Mixed => "MIXED",
            ResolvedMode::Quiet => "QUIET",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.wire() == value)
    }
}

// ---------------------------------------------------------------------------
// Dictionary access helpers: strict, never defaulting
// ---------------------------------------------------------------------------

fn optional_text(value: &Variant) -> Option<String> {
    if value.is_nil() {
        return None;
    }
    let text = value.try_to::<GString>().ok()?.to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn required_text(source: &VarDictionary, key: &str) -> Result<String, String> {
    let raw = source
        .get(key)
        .ok_or_else(|| format!("required field missing: {key}"))?;
    optional_text(&raw).ok_or_else(|| format!("required field is empty or not text: {key}"))
}

fn nullable_text(source: &VarDictionary, key: &str) -> Result<Option<String>, String> {
    let raw = source
        .get(key)
        .ok_or_else(|| format!("required field missing: {key}"))?;
    Ok(optional_text(&raw))
}

/// JSON numbers may arrive as float variants; only exact integers are accepted.
fn optional_int(value: &Variant) -> Option<i64> {
    if let Ok(integer) = value.try_to::<i64>() {
        return Some(integer);
    }
    let float = value.try_to::<f64>().ok()?;
    if float.fract() == 0.0 && float.is_finite() {
        Some(float as i64)
    } else {
        None
    }
}

fn required_int(source: &VarDictionary, key: &str) -> Result<i64, String> {
    let raw = source
        .get(key)
        .ok_or_else(|| format!("required field missing: {key}"))?;
    optional_int(&raw).ok_or_else(|| format!("required field is not an integer: {key}"))
}

fn required_bool(source: &VarDictionary, key: &str) -> Result<bool, String> {
    let raw = source
        .get(key)
        .ok_or_else(|| format!("required field missing: {key}"))?;
    raw.try_to::<bool>()
        .map_err(|_| format!("required field is not boolean: {key}"))
}

fn required_text_list(source: &VarDictionary, key: &str) -> Result<Vec<String>, String> {
    let raw = source
        .get(key)
        .ok_or_else(|| format!("required field missing: {key}"))?;
    let array = raw
        .try_to::<VarArray>()
        .map_err(|_| format!("required field is not a list: {key}"))?;
    let mut terms = Vec::new();
    for item in array.iter_shared() {
        match optional_text(&item) {
            Some(term) => terms.push(term),
            None => return Err(format!("list {key} contains an empty or non-text item")),
        }
    }
    Ok(terms)
}

fn reject_unknown_fields(source: &VarDictionary, allowed: &[&str]) -> Result<(), String> {
    for raw_key in source.keys_shared() {
        let key = optional_text(&raw_key)
            .ok_or_else(|| "dictionary contains a non-text key".to_string())?;
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unknown wire field: {key}"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Host binding: identity the Host authenticated, echoed back on commands
// ---------------------------------------------------------------------------

/// Identity and authority context taken verbatim from the Host snapshot
/// envelope. The client never infers a House, room, spirit, or session from
/// its own window, configuration, or working directory.
#[derive(Clone, Debug)]
pub struct HostBinding {
    pub house_id: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
    /// `reply_target` published by the Host, used as the command `recipient`.
    pub recipient: String,
    pub scope: String,
    pub visibility: String,
    pub authority_class: String,
}

impl HostBinding {
    pub fn parse(envelope: &VarDictionary) -> Result<Self, String> {
        Ok(Self {
            house_id: required_text(envelope, "house_id")?,
            room: required_text(envelope, "sender_room")?,
            spirit: required_text(envelope, "sender_spirit")?,
            session: required_text(envelope, "sender_session")?,
            recipient: required_text(envelope, "reply_target")?,
            scope: required_text(envelope, "scope")?,
            visibility: required_text(envelope, "visibility")?,
            authority_class: required_text(envelope, "authority_class")?,
        })
    }
}

pub fn event_projection_id(envelope: &VarDictionary) -> Result<String, String> {
    required_text(envelope, "projection_id")
}

pub fn event_correlation_id(envelope: &VarDictionary) -> Result<String, String> {
    required_text(envelope, "correlation_id")
}

// ---------------------------------------------------------------------------
// Projection cursor
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ProjectionCursor {
    pub snapshot_id: String,
    pub version: i64,
    pub sequence: i64,
    /// Present only while the applied state is still the snapshot state.
    /// Deltas carry no `state_hash`, so an applied delta clears it.
    pub state_hash: Option<String>,
}

// ---------------------------------------------------------------------------
// Recall Policy projection state
// ---------------------------------------------------------------------------

/// Host-owned Recall Policy state. Every field is required in a snapshot;
/// a missing or unknown value refuses the snapshot instead of inventing a
/// default, so no plausible-looking state can ever be displayed.
#[derive(Clone, Debug)]
pub struct RecallPolicyProjection {
    pub requested_mode: RequestedMode,
    pub resolved_mode: ResolvedMode,
    pub active_project: Option<String>,
    pub resolution_reason: String,
    pub last_refresh_reason: Option<String>,
    pub last_refresh_at: Option<String>,
    pub working_set_entries: i64,
    pub recovery_pending: bool,
    pub recovery_terms: Vec<String>,
    pub degraded: Option<String>,
    pub updated_at: Option<String>,
}

impl RecallPolicyProjection {
    pub fn parse(state: &VarDictionary) -> Result<Self, String> {
        reject_unknown_fields(
            state,
            &[
                "requestedMode",
                "resolvedMode",
                "activeProject",
                "resolutionReason",
                "lastRefreshReason",
                "lastRefreshAt",
                "workingSetEntries",
                "recoveryPending",
                "recoveryTerms",
                "degraded",
                "updatedAt",
            ],
        )?;
        let requested_wire = required_text(state, "requestedMode")?;
        let requested_mode = RequestedMode::from_wire(&requested_wire)
            .ok_or_else(|| format!("unknown requestedMode on the wire: {requested_wire}"))?;
        let resolved_wire = required_text(state, "resolvedMode")?;
        let resolved_mode = ResolvedMode::from_wire(&resolved_wire)
            .ok_or_else(|| format!("unknown resolvedMode on the wire: {resolved_wire}"))?;
        let working_set_entries = required_int(state, "workingSetEntries")?;
        if working_set_entries < 0 {
            return Err("workingSetEntries is negative".to_string());
        }
        Ok(Self {
            requested_mode,
            resolved_mode,
            active_project: nullable_text(state, "activeProject")?,
            resolution_reason: required_text(state, "resolutionReason")?,
            last_refresh_reason: nullable_text(state, "lastRefreshReason")?,
            last_refresh_at: nullable_text(state, "lastRefreshAt")?,
            working_set_entries,
            recovery_pending: required_bool(state, "recoveryPending")?,
            recovery_terms: required_text_list(state, "recoveryTerms")?,
            degraded: nullable_text(state, "degraded")?,
            updated_at: nullable_text(state, "updatedAt")?,
        })
    }

    fn apply_field_update(&mut self, field: &str, value: &Variant) -> Result<(), String> {
        match field {
            "requested_mode" => {
                let wire = optional_text(value)
                    .ok_or_else(|| "requested_mode is empty in the delta".to_string())?;
                self.requested_mode = RequestedMode::from_wire(&wire)
                    .ok_or_else(|| format!("unknown requested_mode on the wire: {wire}"))?;
            }
            "resolved_mode" => {
                let wire = optional_text(value)
                    .ok_or_else(|| "resolved_mode is empty in the delta".to_string())?;
                self.resolved_mode = ResolvedMode::from_wire(&wire)
                    .ok_or_else(|| format!("unknown resolved_mode on the wire: {wire}"))?;
            }
            "active_project" => self.active_project = optional_text(value),
            "resolution_reason" => {
                self.resolution_reason = optional_text(value)
                    .ok_or_else(|| "resolution_reason is empty in the delta".to_string())?;
            }
            "last_refresh_reason" => self.last_refresh_reason = optional_text(value),
            "last_refresh_at" => self.last_refresh_at = optional_text(value),
            "working_set_entries" => {
                let entries = optional_int(value).ok_or_else(|| {
                    "working_set_entries is not an integer in the delta".to_string()
                })?;
                if entries < 0 {
                    return Err("working_set_entries is negative in the delta".to_string());
                }
                self.working_set_entries = entries;
            }
            "recovery_pending" => {
                self.recovery_pending = value
                    .try_to::<bool>()
                    .map_err(|_| "recovery_pending is not boolean in the delta".to_string())?;
            }
            "recovery_terms" => {
                let array = value
                    .try_to::<VarArray>()
                    .map_err(|_| "recovery_terms is not a list in the delta".to_string())?;
                let mut terms = Vec::new();
                for item in array.iter_shared() {
                    match optional_text(&item) {
                        Some(term) => terms.push(term),
                        None => return Err("recovery_terms contains an invalid item".to_string()),
                    }
                }
                self.recovery_terms = terms;
            }
            "degraded" => self.degraded = optional_text(value),
            "updated_at" => self.updated_at = optional_text(value),
            other => return Err(format!("unmapped projection field: {other}")),
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Parsed inbound messages
// ---------------------------------------------------------------------------

pub struct Snapshot {
    pub binding: HostBinding,
    pub cursor: ProjectionCursor,
    pub projection: RecallPolicyProjection,
}

enum DeltaMutation {
    FieldUpdate { field: String, value: Variant },
}

pub struct Delta {
    pub delta_id: String,
    pub base_version: i64,
    pub next_version: i64,
    pub sequence: i64,
    pub state_hash: String,
    mutations: Vec<DeltaMutation>,
}

pub struct CommandOutcome {
    pub correlation_id: String,
    pub reason: Option<String>,
}

/// Every inbound message the client is prepared to interpret.
pub enum Inbound {
    Snapshot(Snapshot),
    Delta(Delta),
    CommandAccepted(CommandOutcome),
    CommandRefused(CommandOutcome),
    CommandFailed(CommandOutcome),
}
const EVENT_COMMON_FIELDS: &[&str] = &[
    "schema_version",
    "event_id",
    "house_id",
    "sender_room",
    "sender_spirit",
    "sender_session",
    "recipient",
    "command_or_event_type",
    "correlation_id",
    "causation_id",
    "reply_target",
    "idempotency_key",
    "source_record_refs",
    "scope",
    "visibility",
    "authority_class",
    "created_at",
    "expires_at",
    "max_hops",
    "projection_id",
    "sequence",
    "state_hash",
];

fn reject_unknown_event_fields(
    envelope: &VarDictionary,
    event_fields: &[&str],
) -> Result<(), String> {
    for raw_key in envelope.keys_shared() {
        let key = optional_text(&raw_key)
            .ok_or_else(|| "envelope contains a non-text key".to_string())?;
        if !EVENT_COMMON_FIELDS.contains(&key.as_str()) && !event_fields.contains(&key.as_str()) {
            return Err(format!("unknown envelope field: {key}"));
        }
    }
    Ok(())
}

fn require_event_meta(envelope: &VarDictionary) -> Result<(), String> {
    for key in [
        "event_id",
        "house_id",
        "sender_room",
        "sender_spirit",
        "sender_session",
        "recipient",
        "correlation_id",
        "causation_id",
        "reply_target",
        "idempotency_key",
        "scope",
        "visibility",
        "authority_class",
        "created_at",
        "state_hash",
    ] {
        required_text(envelope, key)?;
    }
    required_int(envelope, "max_hops")?;
    required_int(envelope, "sequence")?;
    envelope
        .get("source_record_refs")
        .ok_or_else(|| "required field missing: source_record_refs".to_string())?
        .try_to::<VarArray>()
        .map_err(|_| "source_record_refs is not a list".to_string())?;
    nullable_text(envelope, "expires_at")?;
    Ok(())
}

fn require_projection(envelope: &VarDictionary) -> Result<(), String> {
    let projection_id = required_text(envelope, "projection_id")?;
    if projection_id != PROJECTION_ID {
        return Err(format!(
            "projection_id inesperado: {projection_id} (esperado {PROJECTION_ID})"
        ));
    }
    Ok(())
}

/// Parses one inbound envelope. Schema negotiation happens first: a version
/// the client cannot speak is refused rather than partially interpreted.
pub fn parse_inbound(envelope: &VarDictionary) -> Result<Inbound, String> {
    let schema_version = required_int(envelope, "schema_version")?;
    if schema_version != SCHEMA_VERSION {
        return Err(format!(
            "schema_version {schema_version} is not supported by this client (supports {SCHEMA_VERSION})"
        ));
    }
    let event_type = required_text(envelope, "command_or_event_type")?;
    require_event_meta(envelope)?;

    match event_type.as_str() {
        EVT_SNAPSHOT => {
            reject_unknown_event_fields(envelope, &["snapshot_id", "version", "state"])?;
            require_projection(envelope)?;
            let state = envelope
                .get("state")
                .ok_or_else(|| "required field missing: state".to_string())?
                .try_to::<VarDictionary>()
                .map_err(|_| "state is not a dictionary".to_string())?;
            Ok(Inbound::Snapshot(Snapshot {
                binding: HostBinding::parse(envelope)?,
                cursor: ProjectionCursor {
                    snapshot_id: required_text(envelope, "snapshot_id")?,
                    version: required_int(envelope, "version")?,
                    sequence: required_int(envelope, "sequence")?,
                    state_hash: Some(required_text(envelope, "state_hash")?),
                },
                projection: RecallPolicyProjection::parse(&state)?,
            }))
        }
        EVT_DELTA => {
            reject_unknown_event_fields(
                envelope,
                &[
                    "delta_id",
                    "base_version",
                    "next_version",
                    "source_event_ids",
                    "mutations",
                    "coalesce_key",
                ],
            )?;
            require_projection(envelope)?;
            let raw_mutations = envelope
                .get("mutations")
                .ok_or_else(|| "required field missing: mutations".to_string())?
                .try_to::<VarArray>()
                .map_err(|_| "mutations is not a list".to_string())?;
            let mut mutations = Vec::new();
            for item in raw_mutations.iter_shared() {
                let mutation = item
                    .try_to::<VarDictionary>()
                    .map_err(|_| "mutation is not a dictionary".to_string())?;
                let mutation_type = required_text(&mutation, "mutation_type")?;
                match mutation_type.as_str() {
                    MUTATION_FIELD_UPDATE => {
                        reject_unknown_fields(&mutation, &["mutation_type", "field", "value"])?;
                        let field = required_text(&mutation, "field")?;
                        let value = mutation
                            .get("value")
                            .ok_or_else(|| "required field missing: value".to_string())?;
                        mutations.push(DeltaMutation::FieldUpdate { field, value });
                    }
                    other => return Err(format!("unmapped mutation_type: {other}")),
                }
            }
            Ok(Inbound::Delta(Delta {
                delta_id: required_text(envelope, "delta_id")?,
                base_version: required_int(envelope, "base_version")?,
                next_version: required_int(envelope, "next_version")?,
                sequence: required_int(envelope, "sequence")?,
                state_hash: required_text(envelope, "state_hash")?,
                mutations,
            }))
        }
        EVT_COMMAND_ACCEPTED | EVT_COMMAND_REFUSED | EVT_COMMAND_FAILED => {
            reject_unknown_event_fields(envelope, &["reason", "version", "state", "decision"])?;
            required_int(envelope, "version")?;
            let outcome_state = envelope
                .get("state")
                .ok_or_else(|| "required field missing: state".to_string())?
                .try_to::<VarDictionary>()
                .map_err(|_| "result state is not a dictionary".to_string())?;
            RecallPolicyProjection::parse(&outcome_state)?;
            if let Some(decision) = envelope.get("decision") {
                decision
                    .try_to::<VarDictionary>()
                    .map_err(|_| "result decision is not a dictionary".to_string())?;
            }
            let outcome = CommandOutcome {
                correlation_id: required_text(envelope, "correlation_id")?,
                reason: envelope.get("reason").as_ref().and_then(optional_text),
            };
            Ok(match event_type.as_str() {
                EVT_COMMAND_ACCEPTED => Inbound::CommandAccepted(outcome),
                EVT_COMMAND_REFUSED => Inbound::CommandRefused(outcome),
                _ => Inbound::CommandFailed(outcome),
            })
        }
        other => Err(format!("command_or_event_type desconhecido: {other}")),
    }
}

/// Applies a delta onto a cursor and projection. Returns the reason a replay
/// is required when the delta cannot be applied in order.
pub fn apply_delta(
    cursor: &mut ProjectionCursor,
    projection: &mut RecallPolicyProjection,
    delta: &Delta,
) -> Result<(), String> {
    if delta.base_version != cursor.version {
        return Err(format!(
            "base_version {} does not match applied version {}",
            delta.base_version, cursor.version
        ));
    }
    if delta.sequence != cursor.sequence + 1 {
        return Err(format!(
            "sequence {} does not follow {}",
            delta.sequence, cursor.sequence
        ));
    }
    if delta.next_version <= cursor.version {
        return Err(format!(
            "next_version {} does not advance version {}",
            delta.next_version, cursor.version
        ));
    }
    // Apply onto copies so a rejected mutation cannot leave a half-applied
    // projection on screen.
    let mut candidate = projection.clone();
    for mutation in &delta.mutations {
        match mutation {
            DeltaMutation::FieldUpdate { field, value } => {
                candidate.apply_field_update(field, value)?
            }
        }
    }
    *projection = candidate;
    cursor.version = delta.next_version;
    cursor.sequence = delta.sequence;
    cursor.state_hash = Some(delta.state_hash.clone());
    Ok(())
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptStatus {
    Pending,
    Delivered,
    Degraded,
    Refused,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaperBoatReceipt {
    pub event_id: String,
    pub record_id: String,
    pub room: String,
    pub processed_at: String,
    pub original_stream_sequence: i64,
    pub integrity_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaperBoatReceiptSnapshot {
    pub event_id: String,
    pub sender_room: String,
    pub sequence: i64,
    pub status: ReceiptStatus,
    pub receipt: Option<PaperBoatReceipt>,
    pub diagnostic: Option<String>,
}

pub fn parse_paper_boat_receipt(
    envelope: &VarDictionary,
) -> Result<PaperBoatReceiptSnapshot, String> {
    if required_int(envelope, "schema_version")? != SCHEMA_VERSION {
        return Err("receipt schema_version is not supported".into());
    }
    if required_text(envelope, "command_or_event_type")? != PAPER_BOAT_RECEIPT_SNAPSHOT {
        return Err("event is not a Paper Boat receipt snapshot".into());
    }
    require_event_meta(envelope)?;
    reject_unknown_event_fields(envelope, &["snapshot_id", "state"])?;
    if required_text(envelope, "projection_id")? != PAPER_BOAT_RECEIPT_PROJECTION_ID {
        return Err("receipt projection_id is foreign".into());
    }
    if required_text(envelope, "authority_class")? != "delivery_receipt" {
        return Err("receipt authority class is unexpected".into());
    }
    let event_id = required_text(envelope, "event_id")?;
    let sender_room = required_text(envelope, "sender_room")?;
    let sequence = required_int(envelope, "sequence")?;
    uuid::Uuid::parse_str(&event_id)
        .map_err(|_| "receipt envelope event_id is not a UUID".to_string())?;
    if required_int(envelope, "max_hops")? != MAX_HOPS
        || required_text(envelope, "visibility")? != "operator"
        || required_text(envelope, "scope")? != format!("room:{sender_room}:paper_boat_receipt")
    {
        return Err("receipt scope, visibility, or hops are unexpected".into());
    }
    let source_refs = envelope
        .get("source_record_refs")
        .expect("require_event_meta checked source_record_refs")
        .try_to::<VarArray>()
        .expect("require_event_meta checked source_record_refs type");
    if !source_refs.is_empty() {
        return Err("Paper Boat receipt does not accept additional source_record_refs".into());
    }
    let state = envelope
        .get("state")
        .ok_or_else(|| "required field missing: state".to_string())?
        .try_to::<VarDictionary>()
        .map_err(|_| "receipt state is not a dictionary".to_string())?;
    reject_unknown_fields(&state, &["status", "receipt", "diagnostic"])?;
    let status = match required_text(&state, "status")?.as_str() {
        "pending" => ReceiptStatus::Pending,
        "delivered" => ReceiptStatus::Delivered,
        "degraded" => ReceiptStatus::Degraded,
        "refused" => ReceiptStatus::Refused,
        other => return Err(format!("status de recibo desconhecido: {other}")),
    };
    let diagnostic = nullable_text(&state, "diagnostic")?;
    let raw_receipt = state
        .get("receipt")
        .ok_or_else(|| "required field missing: receipt".to_string())?;
    let receipt = if raw_receipt.is_nil() {
        None
    } else {
        let receipt = raw_receipt
            .try_to::<VarDictionary>()
            .map_err(|_| "receipt is not a dictionary".to_string())?;
        reject_unknown_fields(
            &receipt,
            &[
                "schema_version",
                "event_id",
                "record_id",
                "room",
                "processed_at",
                "original_stream_sequence",
                "integrity_sha256",
            ],
        )?;
        if required_int(&receipt, "schema_version")? != SCHEMA_VERSION {
            return Err("internal receipt schema_version is not supported".into());
        }
        let inner_event_id = required_text(&receipt, "event_id")?;
        uuid::Uuid::parse_str(&inner_event_id)
            .map_err(|_| "internal receipt event_id is not a UUID".to_string())?;
        let record_id = required_text(&receipt, "record_id")?;
        let parsed_record_id = record_id
            .parse::<u64>()
            .map_err(|_| "receipt record_id is not a positive integer".to_string())?;
        if parsed_record_id == 0 || parsed_record_id.to_string() != record_id {
            return Err("receipt record_id is not canonical positive decimal".into());
        }
        let room = required_text(&receipt, "room")?;
        let processed_at = required_text(&receipt, "processed_at")?;
        chrono::DateTime::parse_from_rfc3339(&processed_at)
            .map_err(|_| "receipt processed_at is not RFC 3339".to_string())?;
        let original_stream_sequence = required_int(&receipt, "original_stream_sequence")?;
        let integrity_sha256 = required_text(&receipt, "integrity_sha256")?;
        if integrity_sha256.len() != 64
            || !integrity_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("receipt integrity_sha256 is not hexadecimal SHA-256".into());
        }
        if inner_event_id != event_id
            || room != sender_room
            || original_stream_sequence <= 0
            || original_stream_sequence != sequence
        {
            return Err("identidade, room ou ordem do recibo diverge do envelope Host".into());
        }
        Some(PaperBoatReceipt {
            event_id: inner_event_id,
            record_id,
            room,
            processed_at,
            original_stream_sequence,
            integrity_sha256,
        })
    };
    match status {
        ReceiptStatus::Delivered if receipt.is_none() || diagnostic.is_some() => {
            return Err(
                "delivered status requires a sanitized receipt without a diagnostic".into(),
            );
        }
        ReceiptStatus::Pending | ReceiptStatus::Degraded | ReceiptStatus::Refused
            if receipt.is_some() || diagnostic.is_none() =>
        {
            return Err("non-delivered status requires a diagnostic and rejects a receipt".into());
        }
        _ => {}
    }
    Ok(PaperBoatReceiptSnapshot {
        event_id,
        sender_room,
        sequence,
        status,
        receipt,
        diagnostic,
    })
}

// ---------------------------------------------------------------------------
// Routing status: read-only House worker-lane projection
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct RoutingLane {
    pub name: String,
    pub description: String,
    pub omp_agent: String,
    pub model_role: String,
    pub tools: Vec<String>,
    pub can_edit: bool,
    pub can_infer_intent: bool,
    pub allowed_context_modes: Vec<String>,
    pub requires_acceptance: bool,
}

#[derive(Clone, Debug)]
pub struct RoutingAdvisor {
    pub name: String,
    pub description: String,
    pub dispatchable: bool,
}

#[derive(Clone, Debug)]
pub struct RoutingStatusProjection {
    pub event_id: String,
    pub sequence: i64,
    pub lanes: Vec<RoutingLane>,
    pub advisor: RoutingAdvisor,
    pub correlation_id: String,
}

fn routing_lane(value: &Variant) -> Result<RoutingLane, String> {
    let lane = value
        .try_to::<VarDictionary>()
        .map_err(|_| "routing lane is not a dictionary".to_string())?;
    reject_unknown_fields(
        &lane,
        &[
            "name",
            "description",
            "ompAgent",
            "modelRole",
            "tools",
            "canEdit",
            "canInferIntent",
            "allowedContextModes",
            "requiresAcceptance",
        ],
    )?;
    Ok(RoutingLane {
        name: required_text(&lane, "name")?,
        description: required_text(&lane, "description")?,
        omp_agent: required_text(&lane, "ompAgent")?,
        model_role: required_text(&lane, "modelRole")?,
        tools: required_text_list(&lane, "tools")?,
        can_edit: required_bool(&lane, "canEdit")?,
        can_infer_intent: required_bool(&lane, "canInferIntent")?,
        allowed_context_modes: required_text_list(&lane, "allowedContextModes")?,
        requires_acceptance: required_bool(&lane, "requiresAcceptance")?,
    })
}

pub fn parse_routing_status(envelope: &VarDictionary) -> Result<RoutingStatusProjection, String> {
    if required_int(envelope, "schema_version")? != SCHEMA_VERSION {
        return Err("routing schema_version is not accepted".into());
    }
    if required_text(envelope, "command_or_event_type")? != ROUTING_RESULT {
        return Err("event is not a routing result".into());
    }
    require_event_meta(envelope)?;
    if event_projection_id(envelope)? != ROUTING_PROJECTION_ID {
        return Err("routing projection_id is foreign".into());
    }
    reject_unknown_event_fields(envelope, &["result"])?;

    let result = envelope
        .get("result")
        .ok_or_else(|| "resultado de routing ausente".to_string())?
        .try_to::<VarDictionary>()
        .map_err(|_| "routing result is not a dictionary".to_string())?;
    reject_unknown_fields(&result, &["ok", "lanes", "advisor"])?;
    if !required_bool(&result, "ok")? {
        return Err("Host refused routing status".into());
    }

    let raw_lanes = result
        .get("lanes")
        .ok_or_else(|| "lanes de routing ausentes".to_string())?
        .try_to::<VarArray>()
        .map_err(|_| "routing lanes are not a list".to_string())?;
    let lanes = raw_lanes
        .iter_shared()
        .map(|lane| routing_lane(&lane))
        .collect::<Result<Vec<_>, _>>()?;
    if lanes.is_empty() {
        return Err("Host returned no worker lanes".into());
    }

    let advisor = result
        .get("advisor")
        .ok_or_else(|| "advisor de routing ausente".to_string())?
        .try_to::<VarDictionary>()
        .map_err(|_| "routing advisor is not a dictionary".to_string())?;
    reject_unknown_fields(&advisor, &["name", "description", "dispatchable"])?;

    Ok(RoutingStatusProjection {
        event_id: required_text(envelope, "event_id")?,
        sequence: required_int(envelope, "sequence")?,
        correlation_id: event_correlation_id(envelope)?,
        lanes,
        advisor: RoutingAdvisor {
            name: required_text(&advisor, "name")?,
            description: required_text(&advisor, "description")?,
            dispatchable: required_bool(&advisor, "dispatchable")?,
        },
    })
}

// ---------------------------------------------------------------------------
// Familiar status and bounded dispatch: exact routing result shapes
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct FamiliarView {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub lane: String,
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct FamiliarStatusProjection {
    pub event_id: String,
    pub sequence: i64,
    pub ok: bool,
    pub errors: Vec<String>,
    pub source: Option<String>,
    pub source_alias: bool,
    pub collective: Option<String>,
    pub collective_aliases: Vec<String>,
    pub spellbook_aliases: Vec<String>,
    pub familiars: Vec<FamiliarView>,
}

fn nullable_dictionary(source: &VarDictionary, key: &str) -> Result<Option<VarDictionary>, String> {
    let value = source
        .get(key)
        .ok_or_else(|| format!("required field missing: {key}"))?;
    if value.is_nil() {
        return Ok(None);
    }
    value
        .try_to::<VarDictionary>()
        .map(Some)
        .map_err(|_| format!("required field is not a dictionary or null: {key}"))
}

fn familiar_view(value: &Variant) -> Result<FamiliarView, String> {
    let familiar = value
        .try_to::<VarDictionary>()
        .map_err(|_| "familiar is not a dictionary".to_string())?;
    reject_unknown_fields(
        &familiar,
        &[
            "id",
            "name",
            "aliases",
            "lane",
            "description",
            "temperament",
            "appearance",
        ],
    )?;
    nullable_text(&familiar, "temperament")?;
    nullable_text(&familiar, "appearance")?;
    Ok(FamiliarView {
        id: required_text(&familiar, "id")?,
        name: required_text(&familiar, "name")?,
        aliases: required_text_list(&familiar, "aliases")?,
        lane: required_text(&familiar, "lane")?,
        description: required_text(&familiar, "description")?,
    })
}

fn routing_result(envelope: &VarDictionary) -> Result<VarDictionary, String> {
    if required_int(envelope, "schema_version")? != SCHEMA_VERSION {
        return Err("routing schema_version is not accepted".into());
    }
    if required_text(envelope, "command_or_event_type")? != ROUTING_RESULT {
        return Err("event is not a routing result".into());
    }
    require_event_meta(envelope)?;
    if event_projection_id(envelope)? != ROUTING_PROJECTION_ID {
        return Err("routing projection_id is foreign".into());
    }
    reject_unknown_event_fields(envelope, &["result"])?;
    envelope
        .get("result")
        .ok_or_else(|| "routing result is missing".to_string())?
        .try_to::<VarDictionary>()
        .map_err(|_| "routing result is not a dictionary".to_string())
}

pub fn parse_familiar_status(
    envelope: &VarDictionary,
) -> Result<FamiliarStatusProjection, String> {
    let result = routing_result(envelope)?;
    reject_unknown_fields(
        &result,
        &["ok", "errors", "spellbook", "source", "sourceAlias"],
    )?;
    let ok = required_bool(&result, "ok")?;
    let errors = required_text_list(&result, "errors")?;
    let source = nullable_text(&result, "source")?;
    let source_alias = required_bool(&result, "sourceAlias")?;

    let (collective, collective_aliases, spellbook_aliases, familiars) =
        match nullable_dictionary(&result, "spellbook")? {
            Some(spellbook) => {
                reject_unknown_fields(
                    &spellbook,
                    &[
                        "version",
                        "collective",
                        "collectiveAliases",
                        "spellbookAliases",
                        "familiars",
                    ],
                )?;
                if required_int(&spellbook, "version")? != 1 {
                    return Err("familiar spellbook version is not accepted".into());
                }
                let raw_familiars = spellbook
                    .get("familiars")
                    .ok_or_else(|| "required field missing: familiars".to_string())?
                    .try_to::<VarArray>()
                    .map_err(|_| "familiars is not a list".to_string())?;
                let familiars = raw_familiars
                    .iter_shared()
                    .map(|value| familiar_view(&value))
                    .collect::<Result<Vec<_>, _>>()?;
                (
                    Some(required_text(&spellbook, "collective")?),
                    required_text_list(&spellbook, "collectiveAliases")?,
                    required_text_list(&spellbook, "spellbookAliases")?,
                    familiars,
                )
            }
            None => (None, Vec::new(), Vec::new(), Vec::new()),
        };
    if ok && collective.is_none() {
        return Err("successful familiar status has no spellbook".into());
    }
    if !ok && errors.is_empty() {
        return Err("refused familiar status has no errors".into());
    }

    Ok(FamiliarStatusProjection {
        event_id: required_text(envelope, "event_id")?,
        sequence: required_int(envelope, "sequence")?,
        ok,
        errors,
        source,
        source_alias,
        collective,
        collective_aliases,
        spellbook_aliases,
        familiars,
    })
}

#[derive(Clone, Debug)]
pub struct SpawnTaskView {
    pub name: String,
    pub agent: Option<String>,
    pub task: String,
}

#[derive(Clone, Debug)]
pub struct SpawnPacketView {
    pub tool: String,
    pub context: String,
    pub tasks: Vec<SpawnTaskView>,
}

#[derive(Clone, Debug)]
pub struct DispatchProjection {
    pub event_id: String,
    pub sequence: i64,
    pub ok: bool,
    pub status: String,
    pub lane: Option<String>,
    pub model_role: Option<String>,
    pub omp_agent: Option<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub dispatcher_executed: bool,
    pub dispatcher_reason: String,
    pub spawn_packet: Option<SpawnPacketView>,
}

fn spawn_packet(value: VarDictionary) -> Result<SpawnPacketView, String> {
    reject_unknown_fields(&value, &["tool", "args"])?;
    let args = value
        .get("args")
        .ok_or_else(|| "required field missing: args".to_string())?
        .try_to::<VarDictionary>()
        .map_err(|_| "spawnPacket args is not a dictionary".to_string())?;
    reject_unknown_fields(&args, &["context", "tasks"])?;
    let raw_tasks = args
        .get("tasks")
        .ok_or_else(|| "required field missing: tasks".to_string())?
        .try_to::<VarArray>()
        .map_err(|_| "spawnPacket tasks is not a list".to_string())?;
    let tasks = raw_tasks
        .iter_shared()
        .map(|value| {
            let task = value
                .try_to::<VarDictionary>()
                .map_err(|_| "spawnPacket task is not a dictionary".to_string())?;
            reject_unknown_fields(&task, &["name", "agent", "task"])?;
            let agent = match task.get("agent") {
                Some(value) => Some(
                    optional_text(&value)
                        .ok_or_else(|| "spawnPacket agent is empty or not text".to_string())?,
                ),
                None => None,
            };
            Ok(SpawnTaskView {
                name: required_text(&task, "name")?,
                agent,
                task: required_text(&task, "task")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(SpawnPacketView {
        tool: required_text(&value, "tool")?,
        context: required_text(&args, "context")?,
        tasks,
    })
}

pub fn parse_dispatch_result(envelope: &VarDictionary) -> Result<DispatchProjection, String> {
    let result = routing_result(envelope)?;
    reject_unknown_fields(
        &result,
        &[
            "ok",
            "status",
            "lane",
            "modelRole",
            "ompAgent",
            "errors",
            "warnings",
            "dispatcher",
            "spawnPacket",
            "selector",
            "familiar",
            "source",
            "sourceAlias",
            "spellbook",
        ],
    )?;
    let status = required_text(&result, "status")?;
    if status != "ready" && status != "rejected" {
        return Err(format!("unknown dispatch status: {status}"));
    }
    let dispatcher = result
        .get("dispatcher")
        .ok_or_else(|| "required field missing: dispatcher".to_string())?
        .try_to::<VarDictionary>()
        .map_err(|_| "dispatcher is not a dictionary".to_string())?;
    reject_unknown_fields(&dispatcher, &["executed", "reason"])?;

    let packet = nullable_dictionary(&result, "spawnPacket")?
        .map(spawn_packet)
        .transpose()?;
    if let Some(selector) = nullable_dictionary(&result, "selector")? {
        reject_unknown_fields(&selector, &["kind", "value"])?;
        let kind = required_text(&selector, "kind")?;
        if kind != "lane" && kind != "familiar" {
            return Err(format!("unknown dispatch selector kind: {kind}"));
        }
        required_text(&selector, "value")?;
    }
    match nullable_dictionary(&result, "familiar")? {
        Some(familiar) => {
            familiar_view(&familiar.to_variant())?;
        }
        None => {}
    }
    nullable_text(&result, "source")?;
    required_bool(&result, "sourceAlias")?;
    if let Some(spellbook) = nullable_dictionary(&result, "spellbook")? {
        reject_unknown_fields(
            &spellbook,
            &["collective", "collectiveAliases", "spellbookAliases"],
        )?;
        required_text(&spellbook, "collective")?;
        required_text_list(&spellbook, "collectiveAliases")?;
        required_text_list(&spellbook, "spellbookAliases")?;
    }

    let ok = required_bool(&result, "ok")?;
    if ok != (status == "ready") {
        return Err("dispatch ok/status disagree".into());
    }
    if ok && packet.is_none() {
        return Err("ready dispatch has no spawnPacket".into());
    }

    Ok(DispatchProjection {
        event_id: required_text(envelope, "event_id")?,
        sequence: required_int(envelope, "sequence")?,
        ok,
        status,
        lane: nullable_text(&result, "lane")?,
        model_role: nullable_text(&result, "modelRole")?,
        omp_agent: nullable_text(&result, "ompAgent")?,
        errors: required_text_list(&result, "errors")?,
        warnings: required_text_list(&result, "warnings")?,
        dispatcher_executed: required_bool(&dispatcher, "executed")?,
        dispatcher_reason: required_text(&dispatcher, "reason")?,
        spawn_packet: packet,
    })
}

// ---------------------------------------------------------------------------
// Outbound envelope construction
// ---------------------------------------------------------------------------

/// Correlation identity for one outbound command.
pub struct CommandIdentity {
    pub message_id: String,
    /// Empty for read-only commands; required for mutations.
    pub idempotency_key: String,
    pub causation_id: String,
}

fn utc_timestamp(unix_seconds: i64) -> String {
    let time = Time::singleton();
    format!("{}Z", time.get_datetime_string_from_unix_time(unix_seconds))
}

/// Builds the section 4.1 envelope. Identity and authority fields stay empty
/// until the Host has authenticated the connection and published a binding:
/// the client never asserts a House, room, spirit, session, scope, visibility,
/// or authority class of its own.
fn base_envelope(
    binding: Option<&HostBinding>,
    command_type: &str,
    identity: &CommandIdentity,
) -> VarDictionary {
    let time = Time::singleton();
    let now = time.get_unix_time_from_system() as i64;
    let field = |pick: fn(&HostBinding) -> &String| -> String {
        binding.map(pick).cloned().unwrap_or_default()
    };

    let mut envelope = VarDictionary::new();
    envelope.set("schema_version", SCHEMA_VERSION);
    envelope.set("message_id", identity.message_id.clone());
    envelope.set("house_id", field(|b| &b.house_id));
    envelope.set("sender_room", field(|b| &b.room));
    envelope.set("sender_spirit", field(|b| &b.spirit));
    envelope.set("sender_session", field(|b| &b.session));
    envelope.set("recipient", field(|b| &b.recipient));
    envelope.set("command_or_event_type", command_type);
    envelope.set("correlation_id", identity.message_id.clone());
    envelope.set("causation_id", identity.causation_id.clone());
    envelope.set("reply_target", field(|b| &b.session));
    envelope.set("idempotency_key", identity.idempotency_key.clone());
    envelope.set("source_record_refs", &VarArray::new().to_variant());
    envelope.set("scope", field(|b| &b.scope));
    envelope.set("visibility", field(|b| &b.visibility));
    envelope.set("authority_class", field(|b| &b.authority_class));
    envelope.set("created_at", utc_timestamp(now));
    envelope.set("expires_at", utc_timestamp(now + COMMAND_EXPIRY_SECONDS));
    envelope.set("max_hops", MAX_HOPS);
    envelope.set("projection_id", PROJECTION_ID);
    envelope
}
pub fn paper_boat_receipt_subscribe_command(identity: &CommandIdentity) -> VarDictionary {
    let mut envelope = base_envelope(None, PAPER_BOAT_RECEIPT_SUBSCRIBE, identity);
    envelope.set("projection_id", PAPER_BOAT_RECEIPT_PROJECTION_ID);
    envelope
}

pub fn routing_status_command(binding: &HostBinding, identity: &CommandIdentity) -> VarDictionary {
    let mut envelope = base_envelope(Some(binding), ROUTING_STATUS, identity);
    envelope.set("projection_id", ROUTING_PROJECTION_ID);
    envelope
}

pub fn familiar_status_command(
    binding: &HostBinding,
    identity: &CommandIdentity,
) -> VarDictionary {
    let mut envelope = base_envelope(Some(binding), FAMILIAR_STATUS, identity);
    envelope.set("projection_id", ROUTING_PROJECTION_ID);
    envelope
}

#[allow(clippy::too_many_arguments)]
pub fn routing_dispatch_command(
    binding: &HostBinding,
    identity: &CommandIdentity,
    lane: &str,
    familiar: &str,
    task: &str,
    target: Option<&str>,
    acceptance: &[String],
    risk: &str,
) -> VarDictionary {
    let mut request = VarDictionary::new();
    request.set("lane", lane);
    request.set("familiar", familiar);
    request.set("task", task);
    if let Some(target) = target {
        request.set("target", target);
    }
    request.set("context", &VarArray::new().to_variant());
    let mut acceptance_values = VarArray::new();
    for line in acceptance {
        acceptance_values.push(&line.to_variant());
    }
    request.set("acceptance", &acceptance_values.to_variant());
    request.set("risk", risk);
    request.set("lessonBodies", &VarArray::new().to_variant());

    let mut envelope = base_envelope(Some(binding), ROUTING_DISPATCH, identity);
    envelope.set("projection_id", ROUTING_PROJECTION_ID);
    envelope.set("routing_request", &request.to_variant());
    envelope
}

/// Read-only subscription used for the first snapshot. It is the only command
/// the client may send before the Host has published a binding.
pub fn subscribe_command(
    identity: &CommandIdentity,
    binding: Option<&HostBinding>,
) -> VarDictionary {
    base_envelope(binding, CMD_SUBSCRIBE, identity)
}

/// Explicit recovery command used after the authenticated binding exists.
pub fn resync_command(identity: &CommandIdentity, binding: &HostBinding) -> VarDictionary {
    base_envelope(Some(binding), CMD_RESYNC, identity)
}

/// The single mutation this instrument can author.
pub fn set_requested_mode_command(
    binding: &HostBinding,
    identity: &CommandIdentity,
    mode: RequestedMode,
    base_version: i64,
) -> VarDictionary {
    let mut mutation = VarDictionary::new();
    mutation.set("mutation_type", MUTATION_FIELD_UPDATE);
    mutation.set("field", "requested_mode");
    mutation.set("value", mode.wire());

    let mut mutations = VarArray::new();
    mutations.push(&mutation.to_variant());

    let mut envelope = base_envelope(Some(binding), CMD_SET_REQUESTED_MODE, identity);
    envelope.set("base_version", base_version);
    envelope.set("mutations", &mutations.to_variant());
    envelope
}

/// Acknowledges the applied projection version, as section 4.5 requires.
pub fn acknowledge_command(
    binding: &HostBinding,
    identity: &CommandIdentity,
    cursor: &ProjectionCursor,
) -> VarDictionary {
    let mut envelope = base_envelope(Some(binding), CMD_ACKNOWLEDGE, identity);
    envelope.set("version", cursor.version);
    envelope.set("sequence", cursor.sequence);
    envelope
}

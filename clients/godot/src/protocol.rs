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
    DEFAULT_HOST_WS_URL, HOST_SCHEMA_VERSION, PAPER_BOAT_RECEIPT_PROJECTION_ID,
    PAPER_BOAT_RECEIPT_SNAPSHOT, PAPER_BOAT_RECEIPT_SUBSCRIBE, RECALL_POLICY_ACKNOWLEDGE,
    RECALL_POLICY_COMMAND_ACCEPTED, RECALL_POLICY_COMMAND_FAILED, RECALL_POLICY_COMMAND_REFUSED,
    RECALL_POLICY_DELTA, RECALL_POLICY_FIELD_UPDATE, RECALL_POLICY_PROJECTION_ID,
    RECALL_POLICY_RESYNC, RECALL_POLICY_SET_REQUESTED_MODE, RECALL_POLICY_SNAPSHOT,
    RECALL_POLICY_SUBSCRIBE,
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
            RequestedMode::Conversation => "CONVERSA",
            RequestedMode::Work => "TRABALHO",
            RequestedMode::Quiet => "SILÊNCIO",
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
            ResolvedMode::Conversation => "CONVERSA",
            ResolvedMode::Work => "TRABALHO",
            ResolvedMode::Mixed => "MISTO",
            ResolvedMode::Quiet => "SILÊNCIO",
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
        .ok_or_else(|| format!("campo obrigatório ausente: {key}"))?;
    optional_text(&raw).ok_or_else(|| format!("campo obrigatório vazio ou não textual: {key}"))
}

fn nullable_text(source: &VarDictionary, key: &str) -> Result<Option<String>, String> {
    let raw = source
        .get(key)
        .ok_or_else(|| format!("campo obrigatório ausente: {key}"))?;
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
        .ok_or_else(|| format!("campo obrigatório ausente: {key}"))?;
    optional_int(&raw).ok_or_else(|| format!("campo obrigatório não inteiro: {key}"))
}

fn required_bool(source: &VarDictionary, key: &str) -> Result<bool, String> {
    let raw = source
        .get(key)
        .ok_or_else(|| format!("campo obrigatório ausente: {key}"))?;
    raw.try_to::<bool>()
        .map_err(|_| format!("campo obrigatório não booleano: {key}"))
}

fn required_text_list(source: &VarDictionary, key: &str) -> Result<Vec<String>, String> {
    let raw = source
        .get(key)
        .ok_or_else(|| format!("campo obrigatório ausente: {key}"))?;
    let array = raw
        .try_to::<VarArray>()
        .map_err(|_| format!("campo obrigatório não é lista: {key}"))?;
    let mut terms = Vec::new();
    for item in array.iter_shared() {
        match optional_text(&item) {
            Some(term) => terms.push(term),
            None => return Err(format!("lista {key} contém item vazio ou não textual")),
        }
    }
    Ok(terms)
}

fn reject_unknown_fields(source: &VarDictionary, allowed: &[&str]) -> Result<(), String> {
    for raw_key in source.keys_shared() {
        let key = optional_text(&raw_key)
            .ok_or_else(|| "dicionário contém chave não textual".to_string())?;
        if !allowed.contains(&key.as_str()) {
            return Err(format!("campo desconhecido no wire: {key}"));
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
                "requested_mode",
                "resolved_mode",
                "active_project",
                "resolution_reason",
                "last_refresh_reason",
                "last_refresh_at",
                "working_set_entries",
                "recovery_pending",
                "recovery_terms",
                "degraded",
                "updated_at",
            ],
        )?;
        let requested_wire = required_text(state, "requested_mode")?;
        let requested_mode = RequestedMode::from_wire(&requested_wire)
            .ok_or_else(|| format!("requested_mode desconhecido no wire: {requested_wire}"))?;
        let resolved_wire = required_text(state, "resolved_mode")?;
        let resolved_mode = ResolvedMode::from_wire(&resolved_wire)
            .ok_or_else(|| format!("resolved_mode desconhecido no wire: {resolved_wire}"))?;
        let working_set_entries = required_int(state, "working_set_entries")?;
        if working_set_entries < 0 {
            return Err("working_set_entries negativo".to_string());
        }
        Ok(Self {
            requested_mode,
            resolved_mode,
            active_project: nullable_text(state, "active_project")?,
            resolution_reason: required_text(state, "resolution_reason")?,
            last_refresh_reason: nullable_text(state, "last_refresh_reason")?,
            last_refresh_at: nullable_text(state, "last_refresh_at")?,
            working_set_entries,
            recovery_pending: required_bool(state, "recovery_pending")?,
            recovery_terms: required_text_list(state, "recovery_terms")?,
            degraded: nullable_text(state, "degraded")?,
            updated_at: nullable_text(state, "updated_at")?,
        })
    }

    fn apply_field_update(&mut self, field: &str, value: &Variant) -> Result<(), String> {
        match field {
            "requested_mode" => {
                let wire = optional_text(value)
                    .ok_or_else(|| "requested_mode vazio na delta".to_string())?;
                self.requested_mode = RequestedMode::from_wire(&wire)
                    .ok_or_else(|| format!("requested_mode desconhecido no wire: {wire}"))?;
            }
            "resolved_mode" => {
                let wire = optional_text(value)
                    .ok_or_else(|| "resolved_mode vazio na delta".to_string())?;
                self.resolved_mode = ResolvedMode::from_wire(&wire)
                    .ok_or_else(|| format!("resolved_mode desconhecido no wire: {wire}"))?;
            }
            "active_project" => self.active_project = optional_text(value),
            "resolution_reason" => {
                self.resolution_reason = optional_text(value)
                    .ok_or_else(|| "resolution_reason vazio na delta".to_string())?;
            }
            "last_refresh_reason" => self.last_refresh_reason = optional_text(value),
            "last_refresh_at" => self.last_refresh_at = optional_text(value),
            "working_set_entries" => {
                let entries = optional_int(value)
                    .ok_or_else(|| "working_set_entries não inteiro na delta".to_string())?;
                if entries < 0 {
                    return Err("working_set_entries negativo na delta".to_string());
                }
                self.working_set_entries = entries;
            }
            "recovery_pending" => {
                self.recovery_pending = value
                    .try_to::<bool>()
                    .map_err(|_| "recovery_pending não booleano na delta".to_string())?;
            }
            "recovery_terms" => {
                let array = value
                    .try_to::<VarArray>()
                    .map_err(|_| "recovery_terms não é lista na delta".to_string())?;
                let mut terms = Vec::new();
                for item in array.iter_shared() {
                    match optional_text(&item) {
                        Some(term) => terms.push(term),
                        None => return Err("recovery_terms contém item inválido".to_string()),
                    }
                }
                self.recovery_terms = terms;
            }
            "degraded" => self.degraded = optional_text(value),
            "updated_at" => self.updated_at = optional_text(value),
            other => return Err(format!("campo de projeção não mapeado: {other}")),
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
            .ok_or_else(|| "envelope contém chave não textual".to_string())?;
        if !EVENT_COMMON_FIELDS.contains(&key.as_str()) && !event_fields.contains(&key.as_str()) {
            return Err(format!("campo desconhecido no envelope: {key}"));
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
        .ok_or_else(|| "campo obrigatório ausente: source_record_refs".to_string())?
        .try_to::<VarArray>()
        .map_err(|_| "source_record_refs não é lista".to_string())?;
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
            "schema_version {schema_version} não é falado por este cliente (fala {SCHEMA_VERSION})"
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
                .ok_or_else(|| "campo obrigatório ausente: state".to_string())?
                .try_to::<VarDictionary>()
                .map_err(|_| "state não é um dicionário".to_string())?;
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
                .ok_or_else(|| "campo obrigatório ausente: mutations".to_string())?
                .try_to::<VarArray>()
                .map_err(|_| "mutations não é lista".to_string())?;
            let mut mutations = Vec::new();
            for item in raw_mutations.iter_shared() {
                let mutation = item
                    .try_to::<VarDictionary>()
                    .map_err(|_| "mutation não é dicionário".to_string())?;
                let mutation_type = required_text(&mutation, "mutation_type")?;
                match mutation_type.as_str() {
                    MUTATION_FIELD_UPDATE => {
                        reject_unknown_fields(&mutation, &["mutation_type", "field", "value"])?;
                        let field = required_text(&mutation, "field")?;
                        let value = mutation
                            .get("value")
                            .ok_or_else(|| "campo obrigatório ausente: value".to_string())?;
                        mutations.push(DeltaMutation::FieldUpdate { field, value });
                    }
                    other => return Err(format!("mutation_type não mapeado: {other}")),
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
                .ok_or_else(|| "campo obrigatório ausente: state".to_string())?
                .try_to::<VarDictionary>()
                .map_err(|_| "state de resultado não é dicionário".to_string())?;
            RecallPolicyProjection::parse(&outcome_state)?;
            if let Some(decision) = envelope.get("decision") {
                decision
                    .try_to::<VarDictionary>()
                    .map_err(|_| "decision de resultado não é dicionário".to_string())?;
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
            "base_version {} não casa com a versão aplicada {}",
            delta.base_version, cursor.version
        ));
    }
    if delta.sequence != cursor.sequence + 1 {
        return Err(format!(
            "sequência {} não é a próxima depois de {}",
            delta.sequence, cursor.sequence
        ));
    }
    if delta.next_version <= cursor.version {
        return Err(format!(
            "next_version {} não avança a versão {}",
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
        return Err("schema_version do recibo não é suportado".into());
    }
    if required_text(envelope, "command_or_event_type")? != PAPER_BOAT_RECEIPT_SNAPSHOT {
        return Err("evento não é um snapshot de recibo de Paper Boat".into());
    }
    require_event_meta(envelope)?;
    reject_unknown_event_fields(envelope, &["snapshot_id", "state"])?;
    if required_text(envelope, "projection_id")? != PAPER_BOAT_RECEIPT_PROJECTION_ID {
        return Err("projection_id do recibo é estrangeiro".into());
    }
    if required_text(envelope, "authority_class")? != "delivery_receipt" {
        return Err("classe de autoridade do recibo é inesperada".into());
    }
    let event_id = required_text(envelope, "event_id")?;
    let sender_room = required_text(envelope, "sender_room")?;
    let sequence = required_int(envelope, "sequence")?;
    uuid::Uuid::parse_str(&event_id)
        .map_err(|_| "event_id do envelope de recibo não é UUID".to_string())?;
    if required_int(envelope, "max_hops")? != MAX_HOPS
        || required_text(envelope, "visibility")? != "operator"
        || required_text(envelope, "scope")? != format!("room:{sender_room}:paper_boat_receipt")
    {
        return Err("escopo, visibilidade ou hops do recibo são inesperados".into());
    }
    let source_refs = envelope
        .get("source_record_refs")
        .expect("require_event_meta checked source_record_refs")
        .try_to::<VarArray>()
        .expect("require_event_meta checked source_record_refs type");
    if !source_refs.is_empty() {
        return Err("recibo de Paper Boat não aceita source_record_refs adicionais".into());
    }
    let state = envelope
        .get("state")
        .ok_or_else(|| "campo obrigatório ausente: state".to_string())?
        .try_to::<VarDictionary>()
        .map_err(|_| "state do recibo não é um dicionário".to_string())?;
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
        .ok_or_else(|| "campo obrigatório ausente: receipt".to_string())?;
    let receipt = if raw_receipt.is_nil() {
        None
    } else {
        let receipt = raw_receipt
            .try_to::<VarDictionary>()
            .map_err(|_| "receipt não é um dicionário".to_string())?;
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
            return Err("schema_version interno do recibo não é suportado".into());
        }
        let inner_event_id = required_text(&receipt, "event_id")?;
        uuid::Uuid::parse_str(&inner_event_id)
            .map_err(|_| "event_id interno do recibo não é UUID".to_string())?;
        let record_id = required_text(&receipt, "record_id")?;
        let parsed_record_id = record_id
            .parse::<u64>()
            .map_err(|_| "record_id do recibo não é inteiro positivo".to_string())?;
        if parsed_record_id == 0 || parsed_record_id.to_string() != record_id {
            return Err("record_id do recibo não é decimal canônico positivo".into());
        }
        let room = required_text(&receipt, "room")?;
        let processed_at = required_text(&receipt, "processed_at")?;
        chrono::DateTime::parse_from_rfc3339(&processed_at)
            .map_err(|_| "processed_at do recibo não é RFC 3339".to_string())?;
        let original_stream_sequence = required_int(&receipt, "original_stream_sequence")?;
        let integrity_sha256 = required_text(&receipt, "integrity_sha256")?;
        if integrity_sha256.len() != 64
            || !integrity_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("integrity_sha256 do recibo não é SHA-256 hexadecimal".into());
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
            return Err("status delivered exige recibo sanitizado sem diagnóstico".into());
        }
        ReceiptStatus::Pending | ReceiptStatus::Degraded | ReceiptStatus::Refused
            if receipt.is_some() || diagnostic.is_none() =>
        {
            return Err("status não entregue exige diagnóstico e não aceita recibo".into());
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

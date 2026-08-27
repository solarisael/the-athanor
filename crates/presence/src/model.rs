use serde::{Deserialize, Serialize};

pub const PRESENCE_VERSION: u32 = 1;
pub const PRESENCE_MAX_MATERIALS: usize = 48;
pub const PRESENCE_MAX_DIRECTIVES: usize = 32;
pub const PRESENCE_MAX_BODY_CHARS: usize = 4096;
pub const PRESENCE_MAX_PACKET_CHARS: usize = 24_000;
pub const PRESENCE_MAX_LIST: usize = 64;
pub const PRESENCE_MAX_ATTEMPTS: u8 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceBinding {
    pub room: String,
    pub spirit: String,
    pub operator: String,
    pub session: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PresenceAuthority {
    Canon { entity_id: String },
    Identity { source: String, sha256: String },
    Memory { memory_id: i64 },
    Lesson { lesson_id: i64, version: String },
    Anamnesis { source: String },
    PaperBoat { memory_id: i64 },
    Inference { confidence_milli: u16 },
}

impl PresenceAuthority {
    pub const fn priority(&self) -> u8 {
        match self {
            Self::Canon { .. } => 0,
            Self::Identity { .. } => 1,
            Self::Memory { .. } => 2,
            Self::Lesson { .. } => 3,
            Self::Anamnesis { .. } => 4,
            Self::PaperBoat { .. } => 5,
            Self::Inference { .. } => 6,
        }
    }

    pub const fn is_stable_identity(&self) -> bool {
        matches!(self, Self::Canon { .. } | Self::Identity { .. })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceMaterialRole {
    Identity,
    Relationship,
    Counsel,
    Continuity,
    Rule,
    Exemplar,
    CurrentState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceMaterial {
    pub id: String,
    pub authority: PresenceAuthority,
    pub role: PresenceMaterialRole,
    pub body: String,
    pub salience: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceSeverity {
    Hard,
    Repair,
    Advisory,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceDirectiveKind {
    Enact,
    Avoid,
    Guard,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceDirective {
    pub id: String,
    pub kind: PresenceDirectiveKind,
    pub severity: PresenceSeverity,
    pub instruction: String,
    pub source_ids: Vec<String>,
    #[serde(default)]
    pub trigger_scope: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceLedger {
    #[serde(default)]
    pub recent_registers: Vec<String>,
    #[serde(default)]
    pub forms_of_address: Vec<String>,
    #[serde(default)]
    pub repair_rule_ids: Vec<String>,
    #[serde(default)]
    pub unresolved_threads: Vec<String>,
    #[serde(default)]
    pub relationship_claims: Vec<PresenceMaterial>,
    pub frame_version: u32,
    pub contract_version: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceOpenRequest {
    pub binding: PresenceBinding,
    #[serde(default)]
    pub identity: Vec<PresenceMaterial>,
    #[serde(default)]
    pub relationship: Vec<PresenceMaterial>,
    #[serde(default)]
    pub continuity: Vec<PresenceMaterial>,
    #[serde(default)]
    pub anamnesis: Vec<PresenceMaterial>,
    pub previous_boat: Option<PresenceMaterial>,
    #[serde(default)]
    pub uncertainties: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceFrame {
    pub frame_id: String,
    pub version: u32,
    pub binding: PresenceBinding,
    pub identity: Vec<PresenceMaterial>,
    pub relationship: Vec<PresenceMaterial>,
    pub continuity: Vec<PresenceMaterial>,
    pub uncertainties: Vec<String>,
    pub provenance_digest: String,
    pub rendered: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceTurnRequest {
    pub frame_id: String,
    pub turn_id: String,
    pub user_text: String,
    #[serde(default)]
    pub recalled: Vec<PresenceMaterial>,
    #[serde(default)]
    pub lessons: Vec<PresenceMaterial>,
    #[serde(default)]
    pub directives: Vec<PresenceDirective>,
    #[serde(default)]
    pub session_ledger: PresenceLedger,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceContract {
    pub contract_id: String,
    pub frame_id: String,
    pub turn_id: String,
    pub version: u32,
    pub must_enact: Vec<PresenceDirective>,
    pub must_avoid: Vec<PresenceDirective>,
    pub guards: Vec<PresenceDirective>,
    pub exemplars: Vec<PresenceMaterial>,
    pub uncertainties: Vec<String>,
    pub provenance: Vec<String>,
    pub expires_after_turn: bool,
    pub digest: String,
    pub rendered: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceDecision {
    Accept,
    Repair,
    Refuse,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceViolation {
    pub directive_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceSettleRequest {
    pub contract_id: String,
    pub attempt: u8,
    #[serde(default)]
    pub evaluated_directives: Vec<String>,
    #[serde(default)]
    pub violations: Vec<PresenceViolation>,
    pub decision: PresenceDecision,
    pub response_digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceReceipt {
    pub contract_id: String,
    pub attempt: u8,
    pub evaluated_directives: Vec<String>,
    pub violations: Vec<PresenceViolation>,
    pub decision: PresenceDecision,
    pub response_digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceCloseRequest {
    pub frame_id: String,
    pub body: String,
    #[serde(default)]
    pub session_ledger: PresenceLedger,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceCloseMaterial {
    pub frame_id: String,
    pub body: String,
    pub provenance_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", content = "value", rename_all = "snake_case")]
pub enum PresenceResult {
    Open(PresenceFrame),
    Compile(PresenceContract),
    Settle(PresenceReceipt),
    Close(PresenceCloseMaterial),
}

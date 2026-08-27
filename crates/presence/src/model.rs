use serde::{Deserialize, Serialize};

pub const PRESENCE_VERSION: u32 = 1;
pub const PRESENCE_MAX_MATERIALS: usize = 48;
pub const PRESENCE_MAX_DIRECTIVES: usize = 32;
pub const PRESENCE_MAX_BODY_CHARS: usize = 4096;
pub const PRESENCE_MAX_PACKET_CHARS: usize = 24_000;
pub const PRESENCE_MAX_LIST: usize = 64;
pub const PRESENCE_MAX_ATTEMPTS: u8 = 2;

/// Ledger lists are bounded where they are declared: a long session must not
/// grow the packet every later turn pays for.
pub const PRESENCE_MAX_REPAIR_RULES: usize = 16;
pub const PRESENCE_MAX_LEDGER_CLAIMS: usize = 8;

/// The largest close body Presence will seal.
///
/// enough: close material becomes a paper boat body, and Summoning owns that
/// bound. Presence cannot depend on Summoning without a cycle, so the number
/// lives here and `summoning` pins it to `PAPER_BOAT_MAX_BODY_BYTES` with a
/// compile-time assertion. Change one alone and the build stops.
pub const PRESENCE_MAX_CLOSE_BODY_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceBinding {
    pub room: String,
    pub spirit: String,
    pub operator: String,
    pub session: String,
}

/// A capability the Host has actually validated for this session.
///
/// enough: only conditions the Host can check against its own configuration
/// appear here — the room state file it parsed, an AKASHA pool it built, a
/// receipt bridge it dialled. GIGA and Omega have no Host-side probe yet, so
/// they are absent rather than assumed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceCapability {
    RoomState,
    Akasha,
    Receipts,
}

/// Capability names in declaration order.
const CAPABILITY_NAMES: [&str; 3] = ["room_state", "akasha", "receipts"];

impl PresenceCapability {
    pub const fn as_str(self) -> &'static str {
        CAPABILITY_NAMES[self as usize]
    }
}

/// Who the Host proved is present, and what it proved they can reach.
///
/// The caller never authors this. `PresenceOpenRequest` still carries a
/// binding, but that is a claim to be checked against this authority, not the
/// identity the frame is built from.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceAuthentication {
    pub binding: PresenceBinding,
    #[serde(default)]
    pub capabilities: Vec<PresenceCapability>,
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

/// What one authority is: its standing, and the checks its fields must pass.
///
/// enough: three matches over these seven variants used to state this — one
/// for priority, one for stable identity, one for validation — kept in
/// agreement by hand. A variant added to two of the three was a silent wrong
/// answer, not a compile error. One dispatch now produces the facts.
pub(crate) struct AuthorityFacts<'a> {
    pub priority: u8,
    /// Canon and identity are the authorities a spirit may not lose under
    /// budget pressure. This is stated per variant rather than derived from
    /// `priority`, so inserting a new authority near the top cannot quietly
    /// promote it to load-bearing identity.
    pub stable: bool,
    /// field, value, maximum characters.
    pub text: Option<(&'static str, &'a str, usize)>,
    /// field, value that must be a lowercase SHA-256 digest.
    pub digest: Option<(&'static str, &'a str)>,
    /// field, value that must be greater than zero.
    pub positive: Option<(&'static str, i64)>,
    /// field, value, inclusive maximum.
    pub ceiling: Option<(&'static str, u16, u16)>,
}

impl AuthorityFacts<'_> {
    const NONE: AuthorityFacts<'static> = AuthorityFacts {
        priority: 0,
        stable: false,
        text: None,
        digest: None,
        positive: None,
        ceiling: None,
    };
}

impl PresenceAuthority {
    pub(crate) fn facts(&self) -> AuthorityFacts<'_> {
        match self {
            Self::Canon { entity_id } => AuthorityFacts {
                priority: 0,
                stable: true,
                text: Some(("authority.entityId", entity_id, 160)),
                ..AuthorityFacts::NONE
            },
            Self::Identity {
                source,
                sha256: hash,
            } => AuthorityFacts {
                priority: 1,
                stable: true,
                text: Some(("authority.source", source, 512)),
                digest: Some(("authority.sha256", hash)),
                ..AuthorityFacts::NONE
            },
            Self::Memory { memory_id } => AuthorityFacts {
                priority: 2,
                positive: Some(("authority.memoryId", *memory_id)),
                ..AuthorityFacts::NONE
            },
            Self::Lesson { lesson_id, version } => AuthorityFacts {
                priority: 3,
                text: Some(("authority.version", version, 80)),
                positive: Some(("authority.lessonId", *lesson_id)),
                ..AuthorityFacts::NONE
            },
            Self::Anamnesis { source } => AuthorityFacts {
                priority: 4,
                text: Some(("authority.source", source, 512)),
                ..AuthorityFacts::NONE
            },
            Self::PaperBoat { memory_id } => AuthorityFacts {
                priority: 5,
                positive: Some(("authority.memoryId", *memory_id)),
                ..AuthorityFacts::NONE
            },
            Self::Inference { confidence_milli } => AuthorityFacts {
                priority: 6,
                ceiling: Some(("authority.confidenceMilli", *confidence_milli, 1000)),
                ..AuthorityFacts::NONE
            },
        }
    }

    pub fn priority(&self) -> u8 {
        self.facts().priority
    }

    pub fn is_stable_identity(&self) -> bool {
        self.facts().stable
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

/// Role names in declaration order, so `role as usize` indexes them.
///
/// A seven-arm match to spell seven names is seven chances to disagree with
/// the wire contract these same names travel under.
/// `name_tables_follow_variant_order` pins the order.
const ROLE_NAMES: [&str; 7] = [
    "identity",
    "relationship",
    "counsel",
    "continuity",
    "rule",
    "exemplar",
    "current_state",
];

impl PresenceMaterialRole {
    pub const fn as_str(self) -> &'static str {
        ROLE_NAMES[self as usize]
    }
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

/// Directive kind names in declaration order.
const DIRECTIVE_KIND_NAMES: [&str; 3] = ["enact", "avoid", "guard"];

impl PresenceDirectiveKind {
    pub const fn as_str(self) -> &'static str {
        DIRECTIVE_KIND_NAMES[self as usize]
    }
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

/// What the session has learned about itself so far.
///
/// enough: this used to arrive on the wire, which meant a caller could hand
/// the Host any register history, any repair rule, and any contract version it
/// liked and watch them seal into a paper boat. The Host owns it now. This
/// type is the shape the Host injects, and the bounds in `validate_ledger` are
/// what a pure function checks before trusting even that.
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
    pub capabilities: Vec<PresenceCapability>,
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
    /// The frame version the caller believes it is compiling against. This is
    /// a staleness assertion, not ledger material: a caller that reconnected
    /// against a newer frame is refused by name instead of quietly compiling
    /// into the wrong one.
    pub frame_version: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PresenceContract {
    pub contract_id: String,
    pub frame_id: String,
    pub turn_id: String,
    pub version: u32,
    /// The Host ledger's contract counter at the moment this contract was
    /// compiled. It makes a stale contract visible as stale rather than as a
    /// contract that merely looks unfamiliar.
    pub contract_version: u32,
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
    /// The same staleness assertion the turn request carries, for the same
    /// reason: a boat must not be sealed against a frame the caller has
    /// already lost.
    pub frame_version: u32,
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

//! Domain types and invariants for the House remember vertical slice.

use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HouseMode {
    Base,
    Full,
    Degraded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HealthVerdict {
    Healthy,
    Unhealthy { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Authority {
    Base,
    Full,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DomainError {
    InvalidRoomKey(String),
    ReservedRoomKey,
    EmptyTitle,
    EmptyBody,
    UnsupportedKind(String),
    EmptySourcePath,
    InvalidSupersedes,
    InvalidContinuation,
    DuplicateContinuationThread(String),
    ContinuationThreadNotMember(String),
    InvalidField { field: String, kind: String },
    MissingProject,
    TooManyValues { field: String },
    FullUnhealthy { reason: String },
    DegradedUnavailable,
    EmptyQuery,
    InvalidTopK { field: String, value: u32 },
    InvalidThreshold { field: String, value: f64 },
    InvalidAnamnesis { field: String, message: String },
    InvalidClusterMaintenance { field: String, message: String },
    InvalidAnamnesisLimit { value: u32 },
    MissingAnamnesisQuery,
    MissingAnamnesisSeed,
    ExistingAnamnesisCycleRequired,
    InvalidAnamnesisRepNumber,
    InvalidGiga { field: String, message: String },
    InvalidGigaTransition { from: String, to: String },
    InvalidGigaHash { field: String },
    InvalidGigaScore { field: String, value: f64 },
    GigaProofNotSource,
    GigaScopeViolation,
    GigaPointerOnly,
    UnknownGigaValue { field: String, value: String },
}
impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRoomKey(value) => write!(f, "invalid room key: {value}"),
            Self::ReservedRoomKey => f.write_str("room key 'house' is reserved for shared use"),
            Self::EmptyTitle => f.write_str("lesson or memory title must not be empty"),
            Self::EmptyBody => f.write_str("lesson or memory body must not be empty"),
            Self::UnsupportedKind(kind) => write!(f, "unsupported remember kind: {kind}"),
            Self::EmptySourcePath => f.write_str("source path must not be empty"),
            Self::InvalidSupersedes => f.write_str("supersedes IDs must be positive"),
            Self::InvalidContinuation => f.write_str(
                "continuations require a non-empty thread and positive previous memory ID",
            ),
            Self::DuplicateContinuationThread(thread) => {
                write!(f, "continuations may name thread '{thread}' only once")
            }
            Self::ContinuationThreadNotMember(thread) => {
                write!(
                    f,
                    "continuation thread '{thread}' must also be listed in threads"
                )
            }
            Self::InvalidField { field, kind } => write!(f, "{field} is not valid for {kind}"),
            Self::MissingProject => f.write_str("project lesson requires a non-empty project"),
            Self::TooManyValues { field } => write!(f, "{field} contains too many values"),
            Self::FullUnhealthy { reason } => write!(f, "full authority is unhealthy: {reason}"),
            Self::DegradedUnavailable => f.write_str("degraded mode cannot durably remember"),
            Self::EmptyQuery => f.write_str("recall query must not be empty"),
            Self::InvalidTopK { field, value } => {
                write!(f, "{field} must be positive and at most 1000: {value}")
            }
            Self::InvalidThreshold { field, value } => {
                write!(f, "{field} must be finite and in [0, 1]: {value}")
            }
            Self::InvalidAnamnesis { field, message } => {
                write!(f, "invalid anamnesis {field}: {message}")
            }
            Self::InvalidClusterMaintenance { field, message } => {
                write!(f, "invalid cluster maintenance {field}: {message}")
            }
            Self::InvalidAnamnesisLimit { value } => {
                write!(f, "anamnesis limit must be between 1 and 50: {value}")
            }
            Self::MissingAnamnesisQuery => {
                f.write_str("anamnesis consult requires a non-empty query")
            }
            Self::MissingAnamnesisSeed => f.write_str("anamnesis add requires a seed"),
            Self::ExistingAnamnesisCycleRequired => {
                f.write_str("anamnesis append requires an existing cycle")
            }
            Self::InvalidAnamnesisRepNumber => f.write_str("rep number must be a positive integer"),
            Self::InvalidGiga { field, message } => write!(f, "invalid GIGA {field}: {message}"),
            Self::InvalidGigaTransition { from, to } => {
                write!(f, "invalid GIGA review transition: {from} -> {to}")
            }
            Self::InvalidGigaHash { field } => write!(f, "{field} must be a SHA-256 hex hash"),
            Self::InvalidGigaScore { field, value } => {
                write!(f, "{field} must be finite and in [0, 1]: {value}")
            }
            Self::GigaProofNotSource => {
                f.write_str("GIGA proof refs must be included in source refs")
            }
            Self::GigaScopeViolation => f.write_str("GIGA scope exceeds source scope"),
            Self::GigaPointerOnly => f.write_str("GIGA candidates must be pointer-only"),
            Self::UnknownGigaValue { field, value } => write!(f, "unknown GIGA {field}: {value}"),
        }
    }
}

impl std::error::Error for DomainError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnamnesisReadMode {
    Wake,
    Consult,
}

impl AnamnesisReadMode {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "wake" => Ok(Self::Wake),
            "consult" => Ok(Self::Consult),
            other => Err(DomainError::InvalidAnamnesis {
                field: "mode".into(),
                message: format!("unsupported value: {other}"),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wake => "wake",
            Self::Consult => "consult",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnamnesisReadRequest {
    room: RoomKey,
    mode: AnamnesisReadMode,
    query: Option<String>,
    limit: u32,
}

impl AnamnesisReadRequest {
    pub fn new(
        room: RoomKey,
        mode: AnamnesisReadMode,
        query: Option<String>,
        limit: u32,
    ) -> Result<Self, DomainError> {
        if !(1..=50).contains(&limit) {
            return Err(DomainError::InvalidAnamnesisLimit { value: limit });
        }
        let query = query.map(|q| q.trim().to_owned()).filter(|q| !q.is_empty());
        if mode == AnamnesisReadMode::Consult && query.is_none() {
            return Err(DomainError::MissingAnamnesisQuery);
        }
        Ok(Self {
            room,
            mode,
            query,
            limit,
        })
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub const fn mode(&self) -> AnamnesisReadMode {
        self.mode
    }
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }
    pub const fn limit(&self) -> u32 {
        self.limit
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnamnesisKind {
    Pillar,
    Cycle,
}
impl AnamnesisKind {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "pillar" => Ok(Self::Pillar),
            "cycle" => Ok(Self::Cycle),
            other => Err(DomainError::InvalidAnamnesis {
                field: "kind".into(),
                message: format!("unsupported value: {other}"),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pillar => "pillar",
            Self::Cycle => "cycle",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnamnesisFidelity {
    Record,
    RawMaterial,
}
impl AnamnesisFidelity {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "record" => Ok(Self::Record),
            "raw-material" => Ok(Self::RawMaterial),
            other => Err(DomainError::InvalidAnamnesis {
                field: "fidelity".into(),
                message: format!("unsupported value: {other}"),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::RawMaterial => "raw-material",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnamnesisActivation {
    Wake,
    Fork,
}
impl AnamnesisActivation {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "wake" => Ok(Self::Wake),
            "fork" => Ok(Self::Fork),
            other => Err(DomainError::InvalidAnamnesis {
                field: "activation".into(),
                message: format!("unsupported value: {other}"),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wake => "wake",
            Self::Fork => "fork",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnamnesisSeedRep {
    number: u32,
    occurred_on: Option<String>,
    how_it_went: String,
    portal_pull: String,
    lighter: String,
}
impl AnamnesisSeedRep {
    pub fn new(
        number: u32,
        occurred_on: Option<String>,
        how_it_went: String,
        portal_pull: String,
        lighter: String,
    ) -> Result<Self, DomainError> {
        if number == 0 {
            return Err(DomainError::InvalidAnamnesisRepNumber);
        }
        for (field, value) in [
            ("how_it_went", &how_it_went),
            ("portal_pull", &portal_pull),
            ("lighter", &lighter),
        ] {
            if value.trim().is_empty() {
                return Err(DomainError::InvalidAnamnesis {
                    field: field.into(),
                    message: "must not be empty".into(),
                });
            }
        }
        Ok(Self {
            number,
            occurred_on,
            how_it_went,
            portal_pull,
            lighter,
        })
    }
    pub const fn number(&self) -> u32 {
        self.number
    }
    pub fn occurred_on(&self) -> Option<&str> {
        self.occurred_on.as_deref()
    }
    pub fn how_it_went(&self) -> &str {
        &self.how_it_went
    }
    pub fn portal_pull(&self) -> &str {
        &self.portal_pull
    }
    pub fn lighter(&self) -> &str {
        &self.lighter
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnamnesisAddDetails {
    pub shape: Option<String>,
    pub dormant: bool,
    pub ramp: String,
    pub counsel: Option<String>,
    pub peak: Option<String>,
    pub beginning: Option<String>,
    pub verify_note: Option<String>,
    pub canon: Vec<String>,
    pub source_paths: Vec<String>,
    pub tags: Vec<String>,
    pub allow_empty_cycle: bool,
    pub seed_rep: Option<AnamnesisSeedRep>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnamnesisAddRequest {
    room: RoomKey,
    kind: AnamnesisKind,
    fidelity: AnamnesisFidelity,
    activation: AnamnesisActivation,
    title: String,
    shape: Option<String>,
    dormant: bool,
    ramp: String,
    counsel: Option<String>,
    peak: Option<String>,
    beginning: Option<String>,
    verify_note: Option<String>,
    canon: Vec<String>,
    source_paths: Vec<String>,
    tags: Vec<String>,
    allow_empty_cycle: bool,
    seed_rep: Option<AnamnesisSeedRep>,
}
impl AnamnesisAddRequest {
    pub fn new(
        room: RoomKey,
        kind: AnamnesisKind,
        fidelity: AnamnesisFidelity,
        activation: AnamnesisActivation,
        title: String,
        details: AnamnesisAddDetails,
    ) -> Result<Self, DomainError> {
        let AnamnesisAddDetails {
            shape,
            dormant,
            ramp,
            counsel,
            peak,
            beginning,
            verify_note,
            canon,
            source_paths,
            tags,
            allow_empty_cycle,
            seed_rep,
        } = details;
        if title.trim().is_empty() {
            return Err(DomainError::InvalidAnamnesis {
                field: "title".into(),
                message: "must not be empty".into(),
            });
        }
        if ramp.trim().is_empty() {
            return Err(DomainError::InvalidAnamnesis {
                field: "ramp".into(),
                message: "must not be empty".into(),
            });
        }
        if kind == AnamnesisKind::Pillar && seed_rep.is_some() {
            return Err(DomainError::InvalidAnamnesis {
                field: "seed_rep".into(),
                message: "pillars cannot include seed_rep".into(),
            });
        }
        if kind == AnamnesisKind::Cycle && seed_rep.is_none() && !allow_empty_cycle {
            return Err(DomainError::MissingAnamnesisSeed);
        }
        if kind == AnamnesisKind::Cycle
            && activation == AnamnesisActivation::Wake
            && verify_note.as_deref().is_none_or(|v| v.trim().is_empty())
        {
            return Err(DomainError::InvalidAnamnesis {
                field: "verify_note".into(),
                message: "wake cycle requires a non-empty verify note".into(),
            });
        }
        Ok(Self {
            room,
            kind,
            fidelity,
            activation,
            title,
            shape,
            dormant,
            ramp,
            counsel,
            peak,
            beginning,
            verify_note,
            canon,
            source_paths,
            tags,
            allow_empty_cycle,
            seed_rep,
        })
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub const fn kind(&self) -> AnamnesisKind {
        self.kind
    }
    pub const fn fidelity(&self) -> AnamnesisFidelity {
        self.fidelity
    }
    pub const fn activation(&self) -> AnamnesisActivation {
        self.activation
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn shape(&self) -> Option<&str> {
        self.shape.as_deref()
    }
    pub const fn dormant(&self) -> bool {
        self.dormant
    }
    pub fn ramp(&self) -> &str {
        &self.ramp
    }
    pub fn counsel(&self) -> Option<&str> {
        self.counsel.as_deref()
    }
    pub fn peak(&self) -> Option<&str> {
        self.peak.as_deref()
    }
    pub fn beginning(&self) -> Option<&str> {
        self.beginning.as_deref()
    }
    pub fn verify_note(&self) -> Option<&str> {
        self.verify_note.as_deref()
    }
    pub fn canon(&self) -> &[String] {
        &self.canon
    }
    pub fn source_paths(&self) -> &[String] {
        &self.source_paths
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
    pub const fn allow_empty_cycle(&self) -> bool {
        self.allow_empty_cycle
    }
    pub fn seed_rep(&self) -> Option<&AnamnesisSeedRep> {
        self.seed_rep.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnamnesisAppendRequest {
    room: RoomKey,
    title: String,
    rep_number: u32,
    occurred_on: Option<String>,
    how_it_went: String,
    portal_pull: String,
    lighter: String,
    source_paths: Vec<String>,
}
impl AnamnesisAppendRequest {
    pub fn new(
        room: RoomKey,
        title: String,
        rep: AnamnesisSeedRep,
        source_paths: Vec<String>,
    ) -> Result<Self, DomainError> {
        if title.trim().is_empty() {
            return Err(DomainError::InvalidAnamnesis {
                field: "title".into(),
                message: "must not be empty".into(),
            });
        }
        if source_paths.iter().any(|v| v.trim().is_empty()) {
            return Err(DomainError::InvalidAnamnesis {
                field: "source_paths".into(),
                message: "paths must not be empty".into(),
            });
        }
        let AnamnesisSeedRep {
            number: rep_number,
            occurred_on,
            how_it_went,
            portal_pull,
            lighter,
        } = rep;
        Ok(Self {
            room,
            title,
            rep_number,
            occurred_on,
            how_it_went,
            portal_pull,
            lighter,
            source_paths,
        })
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn rep_number(&self) -> u32 {
        self.rep_number
    }
    pub fn occurred_on(&self) -> Option<&str> {
        self.occurred_on.as_deref()
    }
    pub fn how_it_went(&self) -> &str {
        &self.how_it_went
    }
    pub fn portal_pull(&self) -> &str {
        &self.portal_pull
    }
    pub fn lighter(&self) -> &str {
        &self.lighter
    }
    pub fn source_paths(&self) -> &[String] {
        &self.source_paths
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnamnesisReceipt {
    room: RoomKey,
    title: String,
    kind: AnamnesisKind,
    durable: bool,
    warnings: Vec<String>,
}
impl AnamnesisReceipt {
    pub fn committed(
        room: RoomKey,
        title: String,
        kind: AnamnesisKind,
        warnings: Vec<String>,
    ) -> Result<Self, DomainError> {
        if title.trim().is_empty() {
            return Err(DomainError::InvalidAnamnesis {
                field: "title".into(),
                message: "must not be empty".into(),
            });
        }
        Ok(Self {
            room,
            title,
            kind,
            durable: true,
            warnings,
        })
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn kind(&self) -> AnamnesisKind {
        self.kind
    }
    pub const fn durable(&self) -> bool {
        self.durable
    }
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnamnesisAppendReceipt {
    room: RoomKey,
    title: String,
    rep_number: u32,
    durable: bool,
    warnings: Vec<String>,
}
impl AnamnesisAppendReceipt {
    pub fn committed(
        room: RoomKey,
        title: String,
        rep_number: u32,
        warnings: Vec<String>,
    ) -> Result<Self, DomainError> {
        if rep_number == 0 {
            return Err(DomainError::InvalidAnamnesisRepNumber);
        }
        if title.trim().is_empty() {
            return Err(DomainError::InvalidAnamnesis {
                field: "title".into(),
                message: "must not be empty".into(),
            });
        }
        Ok(Self {
            room,
            title,
            rep_number,
            durable: true,
            warnings,
        })
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn rep_number(&self) -> u32 {
        self.rep_number
    }
    pub const fn durable(&self) -> bool {
        self.durable
    }
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnamnesisOperation {
    Add,
    AppendRep,
}
impl AnamnesisOperation {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "add" => Ok(Self::Add),
            "append-rep" => Ok(Self::AppendRep),
            other => Err(DomainError::InvalidAnamnesis {
                field: "operation".into(),
                message: format!("unsupported value: {other}"),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::AppendRep => "append-rep",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RoomKey(String);

impl RoomKey {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        Self::build(value.into(), false)
    }

    pub fn for_anamnesis(value: impl Into<String>) -> Result<Self, DomainError> {
        Self::build(value.into(), true)
    }
    /// Memory writes may target the shared house commons; lesson writes may not.
    pub fn for_memory_write(value: impl Into<String>) -> Result<Self, DomainError> {
        Self::build(value.into(), true)
    }

    fn build(value: String, allow_house: bool) -> Result<Self, DomainError> {
        if value == "house" && !allow_house {
            return Err(DomainError::ReservedRoomKey);
        }
        let valid = !value.is_empty()
            && !value.starts_with('-')
            && !value.ends_with('-')
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !value.contains("--");
        if !valid {
            return Err(DomainError::InvalidRoomKey(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

const MAX_RECALL_TOP_K: u32 = 1_000;

#[derive(Clone, Debug, PartialEq)]
pub struct RecallRequest {
    room: RoomKey,
    query: String,
    semantic_top_k: u32,
    semantic_min_similarity: f64,
    content_top_k: u32,
    content_min_similarity: f64,
    temporal_decay: bool,
}

impl RecallRequest {
    pub fn new(
        room: RoomKey,
        query: String,
        semantic_top_k: u32,
        semantic_min_similarity: f64,
        content_top_k: u32,
        content_min_similarity: f64,
    ) -> Result<Self, DomainError> {
        if query.trim().is_empty() {
            return Err(DomainError::EmptyQuery);
        }
        for (field, value) in [
            ("semantic_top_k", semantic_top_k),
            ("content_top_k", content_top_k),
        ] {
            if value == 0 || value > MAX_RECALL_TOP_K {
                return Err(DomainError::InvalidTopK {
                    field: field.into(),
                    value,
                });
            }
        }
        for (field, value) in [
            ("semantic_min_similarity", semantic_min_similarity),
            ("content_min_similarity", content_min_similarity),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(DomainError::InvalidThreshold {
                    field: field.into(),
                    value,
                });
            }
        }
        Ok(Self {
            room,
            query,
            semantic_top_k,
            semantic_min_similarity,
            content_top_k,
            content_min_similarity,
            temporal_decay: false,
        })
    }

    pub fn with_temporal_decay(mut self, temporal_decay: bool) -> Self {
        self.temporal_decay = temporal_decay;
        self
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn query(&self) -> &str {
        &self.query
    }
    pub const fn semantic_top_k(&self) -> u32 {
        self.semantic_top_k
    }
    pub const fn semantic_min_similarity(&self) -> f64 {
        self.semantic_min_similarity
    }
    pub const fn content_top_k(&self) -> u32 {
        self.content_top_k
    }
    pub const fn content_min_similarity(&self) -> f64 {
        self.content_min_similarity
    }
    pub const fn temporal_decay(&self) -> bool {
        self.temporal_decay
    }
}

const MAX_CLUSTER_K: u32 = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClusterMaintenanceOperation {
    Check,
    Rebuild,
}

impl ClusterMaintenanceOperation {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "check" => Ok(Self::Check),
            "rebuild" => Ok(Self::Rebuild),
            other => Err(DomainError::InvalidClusterMaintenance {
                field: "operation".into(),
                message: format!("unsupported value: {other}"),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Rebuild => "rebuild",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClusterMaintenanceRequest {
    room: RoomKey,
    operation: ClusterMaintenanceOperation,
    dry_run: bool,
    if_stale: bool,
    k: u32,
}

impl ClusterMaintenanceRequest {
    pub fn new(
        room: RoomKey,
        operation: ClusterMaintenanceOperation,
        dry_run: bool,
        if_stale: bool,
        k: u32,
    ) -> Result<Self, DomainError> {
        if k == 0 || k > MAX_CLUSTER_K {
            return Err(DomainError::InvalidClusterMaintenance {
                field: "k".into(),
                message: format!("must be between 1 and {MAX_CLUSTER_K}"),
            });
        }
        if operation == ClusterMaintenanceOperation::Check && dry_run {
            return Err(DomainError::InvalidClusterMaintenance {
                field: "dryRun".into(),
                message: "check does not accept dryRun".into(),
            });
        }
        Ok(Self {
            room,
            operation,
            dry_run,
            if_stale,
            k,
        })
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub const fn operation(&self) -> ClusterMaintenanceOperation {
        self.operation
    }
    pub const fn dry_run(&self) -> bool {
        self.dry_run
    }
    pub const fn if_stale(&self) -> bool {
        self.if_stale
    }
    pub const fn k(&self) -> u32 {
        self.k
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClusterStaleness {
    built_at: Option<String>,
    chunks_since_build: u64,
    fraction_unseen: f64,
}

impl ClusterStaleness {
    pub fn new(
        built_at: Option<String>,
        chunks_since_build: u64,
        fraction_unseen: f64,
    ) -> Result<Self, DomainError> {
        if !fraction_unseen.is_finite() || !(0.0..=1.0).contains(&fraction_unseen) {
            return Err(DomainError::InvalidClusterMaintenance {
                field: "fractionUnseen".into(),
                message: "must be finite and between 0 and 1".into(),
            });
        }
        Ok(Self {
            built_at,
            chunks_since_build,
            fraction_unseen,
        })
    }
    pub fn built_at(&self) -> Option<&str> {
        self.built_at.as_deref()
    }
    pub const fn chunks_since_build(&self) -> u64 {
        self.chunks_since_build
    }
    pub const fn fraction_unseen(&self) -> f64 {
        self.fraction_unseen
    }
    pub const fn is_stale(&self, age_days: u64) -> bool {
        self.built_at.is_none()
            || (self.chunks_since_build > 0
                && (self.fraction_unseen >= 0.05
                    || self.chunks_since_build >= 250
                    || age_days >= 7))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClusterSummary {
    label: String,
    member_count: u64,
    accepted: bool,
}

impl ClusterSummary {
    pub fn new(
        label: impl Into<String>,
        member_count: u64,
        accepted: bool,
    ) -> Result<Self, DomainError> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(DomainError::InvalidClusterMaintenance {
                field: "label".into(),
                message: "must not be empty".into(),
            });
        }
        Ok(Self {
            label,
            member_count,
            accepted,
        })
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub const fn member_count(&self) -> u64 {
        self.member_count
    }
    pub const fn accepted(&self) -> bool {
        self.accepted
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClusterMaintenanceStatus {
    pub stale: bool,
    pub reason: String,
    pub staleness: ClusterStaleness,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClusterMaintenanceResult {
    pub ok: bool,
    pub operation: ClusterMaintenanceOperation,
    pub dry_run: bool,
    pub rebuilt: bool,
    pub status: ClusterMaintenanceStatus,
    pub clusters: Vec<ClusterSummary>,
}

impl fmt::Display for RoomKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RememberKind {
    Memory,
    CodingLesson,
    ProjectLesson,
    WritingLesson,
    AudioLesson,
}

impl RememberKind {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "memory" => Ok(Self::Memory),
            "coding-lesson" => Ok(Self::CodingLesson),
            "project-lesson" => Ok(Self::ProjectLesson),
            "writing-lesson" => Ok(Self::WritingLesson),
            "audio-lesson" => Ok(Self::AudioLesson),
            other => Err(DomainError::UnsupportedKind(other.to_owned())),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::CodingLesson => "coding-lesson",
            Self::ProjectLesson => "project-lesson",
            Self::WritingLesson => "writing-lesson",
            Self::AudioLesson => "audio-lesson",
        }
    }
    pub const fn is_lesson(self) -> bool {
        !matches!(self, Self::Memory)
    }
}

const MAX_ARRAY_VALUES: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadContinuation {
    pub thread: String,
    pub previous_memory_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RememberMemoryDetails {
    pub source_path: Option<String>,
    pub threads: Vec<String>,
    pub continues: Vec<ThreadContinuation>,
    pub supersedes: Vec<u64>,
    pub backup: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RememberLessonDetails {
    pub backup: bool,
    pub shape: Option<String>,
    pub voice: Option<String>,
    pub scope: Option<String>,
    pub project: Option<String>,
    pub proof_pattern: Option<String>,
    pub trigger_context: Option<String>,
    pub tags: Vec<String>,
}

enum RememberDetails {
    Memory(RememberMemoryDetails),
    Lesson(RememberLessonDetails),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RememberRequest {
    room: RoomKey,
    kind: RememberKind,
    title: String,
    body: String,
    source_path: Option<String>,
    threads: Vec<String>,
    continues: Vec<ThreadContinuation>,
    supersedes: Vec<u64>,
    backup: bool,
    shape: Option<String>,
    voice: Option<String>,
    scope: Option<String>,
    project: Option<String>,
    proof_pattern: Option<String>,
    trigger_context: Option<String>,
    tags: Vec<String>,
}

impl RememberRequest {
    pub fn new_memory(
        room: RoomKey,
        title: String,
        body: String,
        details: RememberMemoryDetails,
    ) -> Result<Self, DomainError> {
        Self::build(
            room,
            RememberKind::Memory,
            title,
            body,
            RememberDetails::Memory(details),
        )
    }

    pub fn new_lesson(
        room: RoomKey,
        kind: RememberKind,
        title: String,
        body: String,
        details: RememberLessonDetails,
    ) -> Result<Self, DomainError> {
        Self::build(room, kind, title, body, RememberDetails::Lesson(details))
    }

    fn build(
        room: RoomKey,
        kind: RememberKind,
        title: String,
        body: String,
        details: RememberDetails,
    ) -> Result<Self, DomainError> {
        if title.trim().is_empty() {
            return Err(DomainError::EmptyTitle);
        }
        if body.trim().is_empty() {
            return Err(DomainError::EmptyBody);
        }
        let (
            source_path,
            threads,
            continues,
            supersedes,
            backup,
            shape,
            voice,
            scope,
            project,
            proof_pattern,
            trigger_context,
            tags,
        ) = match details {
            RememberDetails::Memory(details) => {
                if kind.is_lesson() {
                    return Err(DomainError::InvalidField {
                        field: "memory fields".into(),
                        kind: kind.as_str().into(),
                    });
                }
                (
                    details.source_path,
                    details.threads,
                    details.continues,
                    details.supersedes,
                    details.backup,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Vec::new(),
                )
            }
            RememberDetails::Lesson(details) => {
                if !kind.is_lesson() {
                    return Err(DomainError::InvalidField {
                        field: "lesson fields".into(),
                        kind: kind.as_str().into(),
                    });
                }
                (
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    details.backup,
                    details.shape,
                    details.voice,
                    details.scope,
                    details.project,
                    details.proof_pattern,
                    details.trigger_context,
                    details.tags,
                )
            }
        };
        if threads.len() > MAX_ARRAY_VALUES
            || continues.len() > MAX_ARRAY_VALUES
            || supersedes.len() > MAX_ARRAY_VALUES
            || tags.len() > MAX_ARRAY_VALUES
        {
            return Err(DomainError::TooManyValues {
                field: "array".into(),
            });
        }
        if supersedes.contains(&0) {
            return Err(DomainError::InvalidSupersedes);
        }
        if matches!(kind, RememberKind::ProjectLesson)
            && project.as_deref().is_none_or(|p| p.trim().is_empty())
        {
            return Err(DomainError::MissingProject);
        }
        if matches!(kind, RememberKind::WritingLesson)
            && (scope.is_some() || project.is_some() || proof_pattern.is_some())
        {
            return Err(DomainError::InvalidField {
                field: "scope/project/proof_pattern".into(),
                kind: kind.as_str().into(),
            });
        }
        if matches!(kind, RememberKind::AudioLesson)
            && (voice.is_some() || scope.is_some() || project.is_some() || proof_pattern.is_some())
        {
            return Err(DomainError::InvalidField {
                field: "voice/scope/project/proof_pattern".into(),
                kind: kind.as_str().into(),
            });
        }
        if matches!(kind, RememberKind::ProjectLesson) && (voice.is_some() || scope.is_some()) {
            return Err(DomainError::InvalidField {
                field: "voice/scope".into(),
                kind: kind.as_str().into(),
            });
        }
        let mut normalized_threads = Vec::with_capacity(threads.len());
        for thread in threads {
            let thread = thread.trim();
            if !thread.is_empty() && !normalized_threads.iter().any(|entry| entry == thread) {
                normalized_threads.push(thread.to_owned());
            }
        }
        let threads = normalized_threads;
        let source_path = source_path.and_then(|path| (!path.trim().is_empty()).then_some(path));
        let mut normalized_continues = Vec::with_capacity(continues.len());
        for continuation in continues {
            let thread = continuation.thread.trim();
            if thread.is_empty() || continuation.previous_memory_id == 0 {
                return Err(DomainError::InvalidContinuation);
            }
            if normalized_continues
                .iter()
                .any(|entry: &ThreadContinuation| entry.thread == thread)
            {
                return Err(DomainError::DuplicateContinuationThread(thread.into()));
            }
            if !threads.iter().any(|candidate| candidate.trim() == thread) {
                return Err(DomainError::ContinuationThreadNotMember(thread.into()));
            }
            normalized_continues.push(ThreadContinuation {
                thread: thread.into(),
                previous_memory_id: continuation.previous_memory_id,
            });
        }
        let mut unique_supersedes = Vec::with_capacity(supersedes.len());
        for id in supersedes {
            if !unique_supersedes.contains(&id) {
                unique_supersedes.push(id);
            }
        }
        Ok(Self {
            room,
            kind,
            title,
            body,
            source_path,
            threads,
            continues: normalized_continues,
            supersedes: unique_supersedes,
            backup,
            shape,
            voice,
            scope,
            project,
            proof_pattern,
            trigger_context,
            tags,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn kind(&self) -> RememberKind {
        self.kind
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }
    pub fn threads(&self) -> &[String] {
        &self.threads
    }
    pub fn continues(&self) -> &[ThreadContinuation] {
        &self.continues
    }
    pub fn supersedes(&self) -> &[u64] {
        &self.supersedes
    }
    pub const fn backup(&self) -> bool {
        self.backup
    }
    pub fn shape(&self) -> Option<&str> {
        self.shape.as_deref()
    }
    pub fn voice(&self) -> Option<&str> {
        self.voice.as_deref()
    }
    pub fn scope(&self) -> Option<&str> {
        self.scope.as_deref()
    }
    pub fn project(&self) -> Option<&str> {
        self.project.as_deref()
    }
    pub fn proof_pattern(&self) -> Option<&str> {
        self.proof_pattern.as_deref()
    }
    pub fn trigger_context(&self) -> Option<&str> {
        self.trigger_context.as_deref()
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RememberReceipt {
    memory_id: Option<u64>,
    lesson_id: Option<u64>,
    kind: RememberKind,
    room: RoomKey,
    source_path: Option<String>,
    warnings: Vec<String>,
}

impl RememberReceipt {
    pub fn committed(
        memory_id: u64,
        room: RoomKey,
        source_path: String,
        warnings: Vec<String>,
    ) -> Result<Self, DomainError> {
        if source_path.trim().is_empty() {
            return Err(DomainError::EmptySourcePath);
        }
        Ok(Self {
            memory_id: Some(memory_id),
            lesson_id: None,
            kind: RememberKind::Memory,
            room,
            source_path: Some(source_path),
            warnings,
        })
    }
    pub fn committed_lesson(
        lesson_id: u64,
        kind: RememberKind,
        room: RoomKey,
        warnings: Vec<String>,
    ) -> Result<Self, DomainError> {
        if !kind.is_lesson() {
            return Err(DomainError::UnsupportedKind(kind.as_str().into()));
        }
        Ok(Self {
            memory_id: None,
            lesson_id: Some(lesson_id),
            kind,
            room,
            source_path: None,
            warnings,
        })
    }
    pub fn memory_id(&self) -> u64 {
        self.memory_id.unwrap_or(0)
    }
    pub fn lesson_id(&self) -> u64 {
        self.lesson_id.unwrap_or(0)
    }
    pub const fn kind(&self) -> RememberKind {
        self.kind
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn source_path(&self) -> &str {
        self.source_path.as_deref().unwrap_or("")
    }
    pub const fn durable(&self) -> bool {
        true
    }
    pub const fn authority(&self) -> Authority {
        Authority::Full
    }
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

pub fn authorize(mode: HouseMode, health: HealthVerdict) -> Result<Authority, DomainError> {
    match (mode, health) {
        (HouseMode::Full, HealthVerdict::Healthy) => Ok(Authority::Full),
        (HouseMode::Full, HealthVerdict::Unhealthy { reason }) => {
            Err(DomainError::FullUnhealthy { reason })
        }
        (HouseMode::Degraded, _) => Err(DomainError::DegradedUnavailable),
        (HouseMode::Base, _) => Ok(Authority::Base),
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaVisibility {
    Private,
    Shared,
}

impl GigaVisibility {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "private" => Ok(Self::Private),
            "shared" => Ok(Self::Shared),
            other => Err(DomainError::UnknownGigaValue {
                field: "visibility".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Shared => "shared",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaScope {
    room: Option<RoomKey>,
    project: Option<String>,
    visibility: GigaVisibility,
    publication_review_required: bool,
}

impl GigaScope {
    pub fn new(
        room: Option<String>,
        project: Option<String>,
        visibility: GigaVisibility,
        publication_review_required: bool,
    ) -> Result<Self, DomainError> {
        let room = room.map(RoomKey::new).transpose()?;
        if (visibility == GigaVisibility::Private) != room.is_some() {
            return Err(DomainError::InvalidGiga {
                field: "scope.room".into(),
                message: "private scope requires one room and shared scope requires null room"
                    .into(),
            });
        }
        Ok(Self {
            room,
            project: project
                .map(|value| giga_nonempty("project", value))
                .transpose()?,
            visibility,
            publication_review_required,
        })
    }

    pub fn room(&self) -> Option<&RoomKey> {
        self.room.as_ref()
    }
    pub fn project(&self) -> Option<&str> {
        self.project.as_deref()
    }
    pub const fn visibility(&self) -> GigaVisibility {
        self.visibility
    }
    pub const fn publication_review_required(&self) -> bool {
        self.publication_review_required
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaSourceType {
    Turn,
    LifecycleEvent,
    ToolResultSummary,
    TaskContract,
}

impl GigaSourceType {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "turn" => Ok(Self::Turn),
            "lifecycle_event" => Ok(Self::LifecycleEvent),
            "tool_result_summary" => Ok(Self::ToolResultSummary),
            "task_contract" => Ok(Self::TaskContract),
            other => Err(DomainError::UnknownGigaValue {
                field: "source_type".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::LifecycleEvent => "lifecycle_event",
            Self::ToolResultSummary => "tool_result_summary",
            Self::TaskContract => "task_contract",
        }
    }
}

fn giga_nonempty(field: &str, value: String) -> Result<String, DomainError> {
    if value.trim().is_empty() {
        Err(DomainError::InvalidGiga {
            field: field.into(),
            message: "must not be empty".into(),
        })
    } else {
        Ok(value)
    }
}

fn giga_strings(field: &str, values: Vec<String>) -> Result<Vec<String>, DomainError> {
    for value in &values {
        giga_nonempty(field, value.clone())?;
    }
    Ok(values)
}

fn giga_hash(field: &str, value: String) -> Result<String, DomainError> {
    if value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && value.bytes().all(|byte| !byte.is_ascii_uppercase())
    {
        Ok(value)
    } else {
        Err(DomainError::InvalidGigaHash {
            field: field.into(),
        })
    }
}

fn giga_rfc3339(field: &str, value: String) -> Result<String, DomainError> {
    fn digits(bytes: &[u8]) -> bool {
        bytes.iter().all(u8::is_ascii_digit)
    }
    fn number(bytes: &[u8]) -> u32 {
        bytes
            .iter()
            .fold(0, |value, byte| value * 10 + u32::from(byte - b'0'))
    }
    let bytes = value.as_bytes();
    let invalid = || DomainError::InvalidGiga {
        field: field.into(),
        message: "must be an RFC3339 timestamp".into(),
    };
    if bytes.len() < 20
        || !digits(&bytes[0..4])
        || bytes[4] != b'-'
        || !digits(&bytes[5..7])
        || bytes[7] != b'-'
        || !digits(&bytes[8..10])
        || bytes[10] != b'T'
        || !digits(&bytes[11..13])
        || bytes[13] != b':'
        || !digits(&bytes[14..16])
        || bytes[16] != b':'
        || !digits(&bytes[17..19])
    {
        return Err(invalid());
    }
    let year = number(&bytes[0..4]);
    let month = number(&bytes[5..7]);
    let day = number(&bytes[8..10]);
    let hour = number(&bytes[11..13]);
    let minute = number(&bytes[14..16]);
    let second = number(&bytes[17..19]);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(invalid()),
    };
    if day == 0 || day > max_day || hour > 23 || minute > 59 || second > 59 {
        return Err(invalid());
    }
    let mut zone = 19;
    if bytes.get(zone) == Some(&b'.') {
        zone += 1;
        let start = zone;
        while bytes.get(zone).is_some_and(u8::is_ascii_digit) {
            zone += 1;
        }
        if zone == start {
            return Err(invalid());
        }
    }
    match bytes.get(zone) {
        Some(b'Z') if zone + 1 == bytes.len() => {}
        Some(b'+' | b'-')
            if zone + 6 == bytes.len()
                && digits(&bytes[zone + 1..zone + 3])
                && bytes[zone + 3] == b':'
                && digits(&bytes[zone + 4..zone + 6])
                && number(&bytes[zone + 1..zone + 3]) <= 23
                && number(&bytes[zone + 4..zone + 6]) <= 59 => {}
        _ => return Err(invalid()),
    }
    Ok(value)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaSourceRange {
    start: u64,
    end: u64,
}

impl GigaSourceRange {
    pub fn new(start: u64, end: u64) -> Result<Self, DomainError> {
        if start >= end {
            return Err(DomainError::InvalidGiga {
                field: "range".into(),
                message: "start must be less than end".into(),
            });
        }
        Ok(Self { start, end })
    }
    pub const fn start(&self) -> u64 {
        self.start
    }
    pub const fn end(&self) -> u64 {
        self.end
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaSourceRef {
    source_type: GigaSourceType,
    source_id: String,
    role: String,
    timestamp: String,
    content_hash: String,
    scope: GigaScope,
    range: Option<GigaSourceRange>,
}

impl GigaSourceRef {
    pub fn new(
        source_type: GigaSourceType,
        source_id: String,
        role: String,
        timestamp: String,
        content_hash: String,
        scope: GigaScope,
        range: Option<GigaSourceRange>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            source_type,
            source_id: giga_nonempty("source_id", source_id)?,
            role: giga_nonempty("role", role)?,
            timestamp: giga_rfc3339("timestamp", timestamp)?,
            content_hash: giga_hash("content_hash", content_hash)?,
            scope,
            range,
        })
    }
    pub const fn source_type(&self) -> GigaSourceType {
        self.source_type
    }
    pub fn source_id(&self) -> &str {
        &self.source_id
    }
    pub fn role(&self) -> &str {
        &self.role
    }
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
    pub fn scope(&self) -> &GigaScope {
        &self.scope
    }
    pub fn range(&self) -> Option<&GigaSourceRange> {
        self.range.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaRisk {
    Low,
    Medium,
    High,
}
impl GigaRisk {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            other => Err(DomainError::UnknownGigaValue {
                field: "risk".into(),
                value: other.into(),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaEventType {
    ConversationWindow,
    TaskStarted,
    TaskCompleted,
    SubagentDispatched,
    SubagentCompleted,
    TodoTransition,
    ToolOutcome,
    ManualReprocess,
}
impl GigaEventType {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "conversation_window" => Ok(Self::ConversationWindow),
            "task_started" => Ok(Self::TaskStarted),
            "task_completed" => Ok(Self::TaskCompleted),
            "subagent_dispatched" => Ok(Self::SubagentDispatched),
            "subagent_completed" => Ok(Self::SubagentCompleted),
            "todo_transition" => Ok(Self::TodoTransition),
            "tool_outcome" => Ok(Self::ToolOutcome),
            "manual_reprocess" => Ok(Self::ManualReprocess),
            other => Err(DomainError::UnknownGigaValue {
                field: "event_type".into(),
                value: other.into(),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConversationWindow => "conversation_window",
            Self::TaskStarted => "task_started",
            Self::TaskCompleted => "task_completed",
            Self::SubagentDispatched => "subagent_dispatched",
            Self::SubagentCompleted => "subagent_completed",
            Self::TodoTransition => "todo_transition",
            Self::ToolOutcome => "tool_outcome",
            Self::ManualReprocess => "manual_reprocess",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaLifecycle {
    event_type: GigaEventType,
    fields: Vec<(String, String)>,
    proof_contract: Vec<String>,
    source_range: Option<GigaSourceRange>,
    risk: Option<GigaRisk>,
}

impl GigaLifecycle {
    fn fields(values: &[(&str, String)]) -> Result<Vec<(String, String)>, DomainError> {
        values
            .iter()
            .map(|(name, value)| Ok(((*name).into(), giga_nonempty(name, value.clone())?)))
            .collect()
    }
    pub fn conversation_window() -> Self {
        Self {
            event_type: GigaEventType::ConversationWindow,
            fields: Vec::new(),
            proof_contract: Vec::new(),
            source_range: None,
            risk: None,
        }
    }
    pub fn task_started(
        task_reference: String,
        worker_id: String,
        worker_role: String,
        phase: String,
        project_key: String,
        task_kind: String,
        risk: GigaRisk,
        target: String,
        change: String,
        proof_contract: Vec<String>,
    ) -> Result<Self, DomainError> {
        if proof_contract.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "proof_contract".into(),
                message: "must not be empty".into(),
            });
        }
        Ok(Self {
            event_type: GigaEventType::TaskStarted,
            fields: Self::fields(&[
                ("task_reference", task_reference),
                ("worker_id", worker_id),
                ("worker_role", worker_role),
                ("phase", phase),
                ("project_key", project_key),
                ("task_kind", task_kind),
                ("target", target),
                ("change", change),
            ])?,
            proof_contract: giga_strings("proof_contract", proof_contract)?,
            source_range: None,
            risk: Some(risk),
        })
    }
    pub fn task_completed(
        task_reference: String,
        outcome: String,
        verification_result: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            event_type: GigaEventType::TaskCompleted,
            fields: Self::fields(&[
                ("task_reference", task_reference),
                ("outcome", outcome),
                ("verification_result", verification_result),
            ])?,
            proof_contract: Vec::new(),
            source_range: None,
            risk: None,
        })
    }
    pub fn subagent_dispatched(
        subagent_reference: String,
        parent_task: String,
        role: String,
        target: String,
        change: String,
        acceptance: Vec<String>,
    ) -> Result<Self, DomainError> {
        if acceptance.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "acceptance".into(),
                message: "must not be empty".into(),
            });
        }
        Ok(Self {
            event_type: GigaEventType::SubagentDispatched,
            fields: Self::fields(&[
                ("subagent_reference", subagent_reference),
                ("parent_task", parent_task),
                ("role", role),
                ("target", target),
                ("change", change),
            ])?,
            proof_contract: giga_strings("acceptance", acceptance)?,
            source_range: None,
            risk: None,
        })
    }
    pub fn subagent_completed(
        subagent_reference: String,
        parent_task: String,
        outcome: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            event_type: GigaEventType::SubagentCompleted,
            fields: Self::fields(&[
                ("subagent_reference", subagent_reference),
                ("parent_task", parent_task),
                ("outcome", outcome),
            ])?,
            proof_contract: Vec::new(),
            source_range: None,
            risk: None,
        })
    }
    pub fn todo_transition(
        todo_reference: String,
        previous_state: String,
        new_state: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            event_type: GigaEventType::TodoTransition,
            fields: Self::fields(&[
                ("todo_reference", todo_reference),
                ("previous_state", previous_state),
                ("new_state", new_state),
            ])?,
            proof_contract: Vec::new(),
            source_range: None,
            risk: None,
        })
    }
    pub fn tool_outcome(
        tool_name: String,
        status: String,
        sanitized_outcome: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            event_type: GigaEventType::ToolOutcome,
            fields: Self::fields(&[
                ("tool_name", tool_name),
                ("status", status),
                ("sanitized_outcome", sanitized_outcome),
            ])?,
            proof_contract: Vec::new(),
            source_range: None,
            risk: None,
        })
    }
    pub fn manual_reprocess(
        source_range: GigaSourceRange,
        reason: String,
        operator_identity: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            event_type: GigaEventType::ManualReprocess,
            fields: Self::fields(&[("reason", reason), ("operator_identity", operator_identity)])?,
            proof_contract: Vec::new(),
            source_range: Some(source_range),
            risk: None,
        })
    }
    pub const fn event_type(&self) -> GigaEventType {
        self.event_type
    }
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
    pub fn proof_contract(&self) -> &[String] {
        &self.proof_contract
    }
    pub fn source_range(&self) -> Option<&GigaSourceRange> {
        self.source_range.as_ref()
    }
    pub const fn risk(&self) -> Option<GigaRisk> {
        self.risk
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEvent {
    event_schema_version: u8,
    event_id: String,
    event_type: GigaEventType,
    room: RoomKey,
    session_id: String,
    project_keys: Vec<String>,
    source_refs: Vec<GigaSourceRef>,
    lifecycle: GigaLifecycle,
    created_at: String,
}
impl GigaEvent {
    pub fn new(
        event_id: String,
        event_type: GigaEventType,
        room: RoomKey,
        session_id: String,
        project_keys: Vec<String>,
        source_refs: Vec<GigaSourceRef>,
        lifecycle: GigaLifecycle,
        created_at: String,
    ) -> Result<Self, DomainError> {
        if source_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "source_refs".into(),
                message: "must not be empty".into(),
            });
        }
        if lifecycle.event_type() != event_type {
            return Err(DomainError::InvalidGiga {
                field: "lifecycle".into(),
                message: "does not match event_type".into(),
            });
        }
        let project_keys = giga_strings("project_keys", project_keys)?;
        let has = |kind| {
            source_refs
                .iter()
                .any(|source| source.source_type() == kind)
        };
        let valid_sources = match event_type {
            GigaEventType::ConversationWindow => has(GigaSourceType::Turn),
            GigaEventType::TaskStarted => has(GigaSourceType::TaskContract),
            GigaEventType::TaskCompleted => {
                has(GigaSourceType::TaskContract) && has(GigaSourceType::LifecycleEvent)
            }
            GigaEventType::SubagentDispatched => has(GigaSourceType::TaskContract),
            GigaEventType::SubagentCompleted => {
                has(GigaSourceType::TaskContract) && has(GigaSourceType::LifecycleEvent)
            }
            GigaEventType::TodoTransition => has(GigaSourceType::LifecycleEvent),
            GigaEventType::ToolOutcome => has(GigaSourceType::ToolResultSummary),
            GigaEventType::ManualReprocess => source_refs
                .iter()
                .any(|source| source.range() == lifecycle.source_range()),
        };
        if !valid_sources {
            return Err(DomainError::InvalidGiga {
                field: "source_refs".into(),
                message: "missing source type required by event_type".into(),
            });
        }
        if event_type == GigaEventType::TaskStarted {
            let project = lifecycle.field("project_key").expect("validated lifecycle");
            if project_keys.len() != 1 || project_keys[0] != project {
                return Err(DomainError::InvalidGiga {
                    field: "project_keys".into(),
                    message: "task_started requires its one lifecycle project_key".into(),
                });
            }
        }
        let mut source_project: Option<&str> = None;
        for source in &source_refs {
            if source.scope().visibility() == GigaVisibility::Private
                && source.scope().room() != Some(&room)
            {
                return Err(DomainError::GigaScopeViolation);
            }
            if let Some(project) = source.scope().project() {
                if source_project.is_some_and(|known| known != project) {
                    return Err(DomainError::GigaScopeViolation);
                }
                source_project = Some(project);
            }
        }
        if let Some(project) = source_project {
            if project_keys.len() != 1 || project_keys[0] != project {
                return Err(DomainError::GigaScopeViolation);
            }
        }
        Ok(Self {
            event_schema_version: 1,
            event_id: giga_nonempty("event_id", event_id)?,
            event_type,
            room,
            session_id: giga_nonempty("session_id", session_id)?,
            project_keys,
            source_refs,
            lifecycle,
            created_at: giga_rfc3339("created_at", created_at)?,
        })
    }
    pub const fn event_schema_version(&self) -> u8 {
        self.event_schema_version
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub const fn event_type(&self) -> GigaEventType {
        self.event_type
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn project_keys(&self) -> &[String] {
        &self.project_keys
    }
    pub fn source_refs(&self) -> &[GigaSourceRef] {
        &self.source_refs
    }
    pub fn lifecycle(&self) -> &GigaLifecycle {
        &self.lifecycle
    }
    pub fn created_at(&self) -> &str {
        &self.created_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaCandidateKind {
    Memory,
    CodingLesson,
    ProjectLesson,
    Correction,
    Supersession,
    EntityUpdate,
    ThreadUpdate,
}
impl GigaCandidateKind {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "memory" => Ok(Self::Memory),
            "coding_lesson" => Ok(Self::CodingLesson),
            "project_lesson" => Ok(Self::ProjectLesson),
            "correction" => Ok(Self::Correction),
            "supersession" => Ok(Self::Supersession),
            "entity_update" => Ok(Self::EntityUpdate),
            "thread_update" => Ok(Self::ThreadUpdate),
            other => Err(DomainError::UnknownGigaValue {
                field: "kind".into(),
                value: other.into(),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::CodingLesson => "coding_lesson",
            Self::ProjectLesson => "project_lesson",
            Self::Correction => "correction",
            Self::Supersession => "supersession",
            Self::EntityUpdate => "entity_update",
            Self::ThreadUpdate => "thread_update",
        }
    }
    pub const fn requires_proof(self) -> bool {
        matches!(
            self,
            Self::CodingLesson | Self::ProjectLesson | Self::Correction | Self::Supersession
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GigaScores {
    priority: f64,
    novelty: f64,
    durability: f64,
    confidence: f64,
}
impl GigaScores {
    pub fn new(
        priority: f64,
        novelty: f64,
        durability: f64,
        confidence: f64,
    ) -> Result<Self, DomainError> {
        for (field, value) in [
            ("priority", priority),
            ("novelty", novelty),
            ("durability", durability),
            ("confidence", confidence),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(DomainError::InvalidGigaScore {
                    field: field.into(),
                    value,
                });
            }
        }
        Ok(Self {
            priority,
            novelty,
            durability,
            confidence,
        })
    }
    pub const fn priority(&self) -> f64 {
        self.priority
    }
    pub const fn novelty(&self) -> f64 {
        self.novelty
    }
    pub const fn durability(&self) -> f64 {
        self.durability
    }
    pub const fn confidence(&self) -> f64 {
        self.confidence
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaClassifierIdentity {
    model: String,
    provider_type: String,
    model_version: String,
    prompt_version: String,
    configuration_digest: String,
    run_id: String,
    completed_at: String,
}
impl GigaClassifierIdentity {
    pub fn new(
        model: String,
        provider_type: String,
        model_version: String,
        prompt_version: String,
        configuration_digest: String,
        run_id: String,
        completed_at: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            model: giga_nonempty("model", model)?,
            provider_type: giga_nonempty("provider_type", provider_type)?,
            model_version: giga_nonempty("model_version", model_version)?,
            prompt_version: giga_nonempty("prompt_version", prompt_version)?,
            configuration_digest: giga_hash("configuration_digest", configuration_digest)?,
            run_id: giga_nonempty("run_id", run_id)?,
            completed_at: giga_rfc3339("completed_at", completed_at)?,
        })
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn provider_type(&self) -> &str {
        &self.provider_type
    }
    pub fn model_version(&self) -> &str {
        &self.model_version
    }
    pub fn prompt_version(&self) -> &str {
        &self.prompt_version
    }
    pub fn configuration_digest(&self) -> &str {
        &self.configuration_digest
    }
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    pub fn completed_at(&self) -> &str {
        &self.completed_at
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaAuthority {
    PointerOnly,
}
impl GigaAuthority {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "pointer-only" => Ok(Self::PointerOnly),
            other => Err(DomainError::UnknownGigaValue {
                field: "authority".into(),
                value: other.into(),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        "pointer-only"
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaReviewState {
    Unreviewed,
    InReview,
    Promoted,
    Merged,
    Corrected,
    Dismissed,
    Unresolved,
    Curio,
    Expired,
    Superseded,
}
impl GigaReviewState {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "unreviewed" => Ok(Self::Unreviewed),
            "in_review" => Ok(Self::InReview),
            "promoted" => Ok(Self::Promoted),
            "merged" => Ok(Self::Merged),
            "corrected" => Ok(Self::Corrected),
            "dismissed" => Ok(Self::Dismissed),
            "unresolved" => Ok(Self::Unresolved),
            "curio" => Ok(Self::Curio),
            "expired" => Ok(Self::Expired),
            "superseded" => Ok(Self::Superseded),
            other => Err(DomainError::UnknownGigaValue {
                field: "review_state".into(),
                value: other.into(),
            }),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unreviewed => "unreviewed",
            Self::InReview => "in_review",
            Self::Promoted => "promoted",
            Self::Merged => "merged",
            Self::Corrected => "corrected",
            Self::Dismissed => "dismissed",
            Self::Unresolved => "unresolved",
            Self::Curio => "curio",
            Self::Expired => "expired",
            Self::Superseded => "superseded",
        }
    }
    pub const fn can_transition(self, to: Self) -> bool {
        matches!(
            (self, to),
            (
                Self::Unreviewed,
                Self::InReview | Self::Dismissed | Self::Expired
            ) | (
                Self::InReview,
                Self::Promoted
                    | Self::Merged
                    | Self::Corrected
                    | Self::Dismissed
                    | Self::Unresolved
                    | Self::Curio
            ) | (Self::Unresolved, Self::InReview)
                | (
                    Self::Curio,
                    Self::InReview | Self::Dismissed | Self::Expired | Self::Superseded
                )
                | (
                    Self::Promoted | Self::Merged | Self::Corrected,
                    Self::Superseded
                )
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GigaCandidate {
    candidate_schema_version: u8,
    candidate_id: String,
    event_id: String,
    room: RoomKey,
    session_id: String,
    kind: GigaCandidateKind,
    source_refs: Vec<GigaSourceRef>,
    proof_refs: Vec<String>,
    scores: GigaScores,
    project_keys: Vec<String>,
    thread_keys: Vec<String>,
    entity_hints: Vec<String>,
    retrieval_terms: Vec<String>,
    proposed_title: String,
    gist: String,
    rationale: String,
    scope: GigaScope,
    authority: GigaAuthority,
    review_state: GigaReviewState,
    classifier: GigaClassifierIdentity,
    created_at: String,
    expires_at: Option<String>,
    promotion_refs: Vec<String>,
}
impl GigaCandidate {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        candidate_id: String,
        event_id: String,
        room: RoomKey,
        session_id: String,
        kind: GigaCandidateKind,
        source_refs: Vec<GigaSourceRef>,
        proof_refs: Vec<String>,
        scores: GigaScores,
        project_keys: Vec<String>,
        thread_keys: Vec<String>,
        entity_hints: Vec<String>,
        retrieval_terms: Vec<String>,
        proposed_title: String,
        gist: String,
        rationale: String,
        scope: GigaScope,
        authority: GigaAuthority,
        review_state: GigaReviewState,
        classifier: GigaClassifierIdentity,
        created_at: String,
        expires_at: Option<String>,
        promotion_refs: Vec<String>,
    ) -> Result<Self, DomainError> {
        if source_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "source_refs".into(),
                message: "must not be empty".into(),
            });
        }
        if authority != GigaAuthority::PointerOnly {
            return Err(DomainError::GigaPointerOnly);
        }
        if review_state != GigaReviewState::Unreviewed || !promotion_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "review_state".into(),
                message: "new candidates must be unreviewed with no promotion refs".into(),
            });
        }
        let proof_refs = giga_strings("proof_refs", proof_refs)?;
        if kind.requires_proof() && proof_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "proof_refs".into(),
                message: "required for this candidate kind".into(),
            });
        }
        for proof in &proof_refs {
            if !source_refs.iter().any(|source| source.source_id() == proof) {
                return Err(DomainError::GigaProofNotSource);
            }
        }
        if (scope.visibility() == GigaVisibility::Private && scope.room() != Some(&room))
            || (scope.visibility() == GigaVisibility::Shared && scope.room().is_some())
        {
            return Err(DomainError::GigaScopeViolation);
        }
        let mut source_project: Option<&str> = None;
        let all_shared = source_refs
            .iter()
            .all(|source| source.scope().visibility() == GigaVisibility::Shared);
        let requires_review = source_refs
            .iter()
            .any(|source| source.scope().publication_review_required());
        for source in &source_refs {
            if source.scope().visibility() == GigaVisibility::Private
                && source.scope().room() != Some(&room)
            {
                return Err(DomainError::GigaScopeViolation);
            }
            if let Some(project) = source.scope().project() {
                if source_project.is_some_and(|known| known != project) {
                    return Err(DomainError::GigaScopeViolation);
                }
                source_project = Some(project);
            }
        }
        if scope.visibility() == GigaVisibility::Shared && !all_shared {
            return Err(DomainError::GigaScopeViolation);
        }
        if requires_review && !scope.publication_review_required() {
            return Err(DomainError::GigaScopeViolation);
        }
        let project_keys = giga_strings("project_keys", project_keys)?;
        if let Some(project) = scope.project() {
            if !project_keys.iter().any(|key| key == project) {
                return Err(DomainError::GigaScopeViolation);
            }
        }
        if let Some(project) = source_project {
            if scope.project() != Some(project) {
                return Err(DomainError::GigaScopeViolation);
            }
        }
        if kind == GigaCandidateKind::ProjectLesson
            && (project_keys.len() != 1 || scope.project() != Some(project_keys[0].as_str()))
        {
            return Err(DomainError::InvalidGiga {
                field: "project_keys".into(),
                message: "project_lesson requires one explicit matching project".into(),
            });
        }
        if kind == GigaCandidateKind::EntityUpdate && entity_hints.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "entity_hints".into(),
                message: "entity_update requires an explicit entity identity".into(),
            });
        }
        if kind == GigaCandidateKind::ThreadUpdate && thread_keys.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "thread_keys".into(),
                message: "thread_update requires an explicit thread key".into(),
            });
        }
        Ok(Self {
            candidate_schema_version: 1,
            candidate_id: giga_nonempty("candidate_id", candidate_id)?,
            event_id: giga_nonempty("event_id", event_id)?,
            room,
            session_id: giga_nonempty("session_id", session_id)?,
            kind,
            source_refs,
            proof_refs,
            scores,
            project_keys,
            thread_keys: giga_strings("thread_keys", thread_keys)?,
            entity_hints: giga_strings("entity_hints", entity_hints)?,
            retrieval_terms: giga_strings("retrieval_terms", retrieval_terms)?,
            proposed_title,
            gist,
            rationale,
            scope,
            authority,
            review_state,
            classifier,
            created_at: giga_rfc3339("created_at", created_at)?,
            expires_at: expires_at
                .map(|value| giga_rfc3339("expires_at", value))
                .transpose()?,
            promotion_refs,
        })
    }
    pub const fn candidate_schema_version(&self) -> u8 {
        self.candidate_schema_version
    }
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub const fn kind(&self) -> GigaCandidateKind {
        self.kind
    }
    pub fn source_refs(&self) -> &[GigaSourceRef] {
        &self.source_refs
    }
    pub fn proof_refs(&self) -> &[String] {
        &self.proof_refs
    }
    pub const fn scores(&self) -> GigaScores {
        self.scores
    }
    pub fn project_keys(&self) -> &[String] {
        &self.project_keys
    }
    pub fn thread_keys(&self) -> &[String] {
        &self.thread_keys
    }
    pub fn entity_hints(&self) -> &[String] {
        &self.entity_hints
    }
    pub fn retrieval_terms(&self) -> &[String] {
        &self.retrieval_terms
    }
    pub fn proposed_title(&self) -> &str {
        &self.proposed_title
    }
    pub fn gist(&self) -> &str {
        &self.gist
    }
    pub fn rationale(&self) -> &str {
        &self.rationale
    }
    pub fn scope(&self) -> &GigaScope {
        &self.scope
    }
    pub const fn authority(&self) -> GigaAuthority {
        self.authority
    }
    pub const fn review_state(&self) -> GigaReviewState {
        self.review_state
    }
    pub fn classifier(&self) -> &GigaClassifierIdentity {
        &self.classifier
    }
    pub fn created_at(&self) -> &str {
        &self.created_at
    }
    pub fn expires_at(&self) -> Option<&str> {
        self.expires_at.as_deref()
    }
    pub fn promotion_refs(&self) -> &[String] {
        &self.promotion_refs
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GigaResonance {
    event_id: String,
    score: f64,
    classifier: GigaClassifierIdentity,
    source_refs: Vec<GigaSourceRef>,
}
impl GigaResonance {
    pub fn new(
        event_id: String,
        score: f64,
        classifier: GigaClassifierIdentity,
        source_refs: Vec<GigaSourceRef>,
    ) -> Result<Self, DomainError> {
        if !score.is_finite() || !(0.0..=1.0).contains(&score) {
            return Err(DomainError::InvalidGigaScore {
                field: "resonance_score".into(),
                value: score,
            });
        }
        if source_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "resonance.source_refs".into(),
                message: "must not be empty".into(),
            });
        }
        Ok(Self {
            event_id: giga_nonempty("resonance.event_id", event_id)?,
            score,
            classifier,
            source_refs,
        })
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub const fn score(&self) -> f64 {
        self.score
    }
    pub fn classifier(&self) -> &GigaClassifierIdentity {
        &self.classifier
    }
    pub fn source_refs(&self) -> &[GigaSourceRef] {
        &self.source_refs
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct GigaReviewAction {
    candidate_id: String,
    reviewer_id: String,
    previous_state: GigaReviewState,
    new_state: GigaReviewState,
    reason: String,
    authorization_basis: String,
    source_refs: Vec<GigaSourceRef>,
    promotion_target: Option<String>,
    merge_target: Option<String>,
    merge_source_candidates: Vec<String>,
    resonance: Option<GigaResonance>,
    reviewed_at: String,
}
impl GigaReviewAction {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        candidate_id: String,
        reviewer_id: String,
        previous_state: GigaReviewState,
        new_state: GigaReviewState,
        reason: String,
        authorization_basis: String,
        source_refs: Vec<GigaSourceRef>,
        promotion_target: Option<String>,
        merge_target: Option<String>,
        merge_source_candidates: Vec<String>,
        resonance: Option<GigaResonance>,
        reviewed_at: String,
    ) -> Result<Self, DomainError> {
        if !previous_state.can_transition(new_state) {
            return Err(DomainError::InvalidGigaTransition {
                from: previous_state.as_str().into(),
                to: new_state.as_str().into(),
            });
        }
        if source_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "source_refs".into(),
                message: "review must retain exact sources".into(),
            });
        }
        let promotion_target = promotion_target
            .map(|value| giga_nonempty("promotion_target", value))
            .transpose()?;
        let merge_target = merge_target
            .map(|value| giga_nonempty("merge_target", value))
            .transpose()?;
        let merge_source_candidates =
            giga_strings("merge_source_candidates", merge_source_candidates)?;
        match new_state {
            GigaReviewState::Promoted if promotion_target.is_none()=>return Err(DomainError::InvalidGiga{field:"promotion_target".into(),message:"required for promotion".into()}),
            GigaReviewState::Merged if merge_target.is_none()||merge_source_candidates.len()<2||!merge_source_candidates.iter().any(|source|source==&candidate_id)||!merge_source_candidates.iter().any(|source|source!=&candidate_id)=>return Err(DomainError::InvalidGiga{field:"merge_target".into(),message:"merge target and all distinct source candidates, including this candidate, are required".into()}),
            GigaReviewState::Corrected|GigaReviewState::Superseded if promotion_target.is_none()||source_refs.len()<2=>return Err(DomainError::InvalidGiga{field:"promotion_target".into(),message:"target and exact new/old source references are required".into()}),
            _=>{}
        }
        if new_state != GigaReviewState::Merged
            && (merge_target.is_some() || !merge_source_candidates.is_empty())
        {
            return Err(DomainError::InvalidGiga {
                field: "merge_target".into(),
                message: "only valid for merged reviews".into(),
            });
        }
        if !matches!(
            new_state,
            GigaReviewState::Promoted | GigaReviewState::Corrected | GigaReviewState::Superseded
        ) && promotion_target.is_some()
        {
            return Err(DomainError::InvalidGiga {
                field: "promotion_target".into(),
                message: "not valid for this transition".into(),
            });
        }
        let resonance_transition =
            previous_state == GigaReviewState::Curio && new_state == GigaReviewState::InReview;
        if resonance_transition != resonance.is_some() {
            return Err(DomainError::InvalidGiga {
                field: "resonance".into(),
                message: "required only for curio resonance to in_review".into(),
            });
        }
        Ok(Self {
            candidate_id: giga_nonempty("candidate_id", candidate_id)?,
            reviewer_id: giga_nonempty("reviewer_id", reviewer_id)?,
            previous_state,
            new_state,
            reason: giga_nonempty("reason", reason)?,
            authorization_basis: giga_nonempty("authorization_basis", authorization_basis)?,
            source_refs,
            promotion_target,
            merge_target,
            merge_source_candidates,
            resonance,
            reviewed_at: giga_rfc3339("reviewed_at", reviewed_at)?,
        })
    }
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }
    pub fn reviewer_id(&self) -> &str {
        &self.reviewer_id
    }
    pub const fn previous_state(&self) -> GigaReviewState {
        self.previous_state
    }
    pub const fn new_state(&self) -> GigaReviewState {
        self.new_state
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub fn authorization_basis(&self) -> &str {
        &self.authorization_basis
    }
    pub fn source_refs(&self) -> &[GigaSourceRef] {
        &self.source_refs
    }
    pub fn promotion_target(&self) -> Option<&str> {
        self.promotion_target.as_deref()
    }
    pub fn merge_target(&self) -> Option<&str> {
        self.merge_target.as_deref()
    }
    pub fn merge_source_candidates(&self) -> &[String] {
        &self.merge_source_candidates
    }
    pub fn resonance(&self) -> Option<&GigaResonance> {
        self.resonance.as_ref()
    }
    pub fn reviewed_at(&self) -> &str {
        &self.reviewed_at
    }
}

pub const GIGA_MAX_PROCESS_SOURCES: usize = 8;
pub const GIGA_MAX_PROCESS_SOURCE_BYTES: usize = 8_000;
pub const GIGA_MAX_PROCESS_WINDOW_BYTES: usize = 24_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaProcessRequest {
    event_id: String,
}

impl GigaProcessRequest {
    pub fn new(event_id: String) -> Result<Self, DomainError> {
        if event_id.is_empty() || event_id.len() > 512 || event_id.trim() != event_id {
            return Err(DomainError::InvalidGiga {
                field: "event_id".into(),
                message: "must be a trimmed identifier of at most 512 bytes".into(),
            });
        }
        Ok(Self { event_id })
    }

    pub fn event_id(&self) -> &str {
        &self.event_id
    }
}

pub const GIGA_MAX_LEASE_SECONDS: u32 = 3_600;
pub const GIGA_MAX_EVENT_ATTEMPTS: u32 = 5;
pub const GIGA_MAX_CANDIDATES_PER_EVENT: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEventClaimRequest {
    room: RoomKey,
    worker_id: String,
    lease_seconds: u32,
}

impl GigaEventClaimRequest {
    pub fn new(room: RoomKey, worker_id: String, lease_seconds: u32) -> Result<Self, DomainError> {
        if lease_seconds == 0 || lease_seconds > GIGA_MAX_LEASE_SECONDS {
            return Err(DomainError::InvalidGiga {
                field: "lease_seconds".into(),
                message: format!("must be between 1 and {GIGA_MAX_LEASE_SECONDS}"),
            });
        }
        Ok(Self {
            room,
            worker_id: giga_nonempty("worker_id", worker_id)?,
            lease_seconds,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }
    pub const fn lease_seconds(&self) -> u32 {
        self.lease_seconds
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaEventFinishOutcome {
    Succeeded,
    Retry,
    Failed,
}

impl GigaEventFinishOutcome {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "succeeded" => Ok(Self::Succeeded),
            "retry" => Ok(Self::Retry),
            "failed" => Ok(Self::Failed),
            other => Err(DomainError::UnknownGigaValue {
                field: "outcome".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Retry => "retry",
            Self::Failed => "failed",
        }
    }
}

fn giga_error_class(value: String) -> Result<String, DomainError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(DomainError::InvalidGiga {
            field: "error_class".into(),
            message: "must be a redacted ASCII class token of at most 128 bytes".into(),
        });
    }
    Ok(value)
}

fn giga_finish_candidate_count(
    outcome: GigaEventFinishOutcome,
    candidate_count: u32,
) -> Result<(), DomainError> {
    if candidate_count > GIGA_MAX_CANDIDATES_PER_EVENT {
        return Err(DomainError::InvalidGiga {
            field: "candidate_count".into(),
            message: format!("must be at most {GIGA_MAX_CANDIDATES_PER_EVENT}"),
        });
    }
    if outcome != GigaEventFinishOutcome::Succeeded && candidate_count != 0 {
        return Err(DomainError::InvalidGiga {
            field: "candidate_count".into(),
            message: "retry and failed outcomes cannot report a stored candidate".into(),
        });
    }
    Ok(())
}

fn giga_finish_fields(
    outcome: GigaEventFinishOutcome,
    candidate_count: u32,
    error_class: Option<String>,
    retry_after_seconds: Option<u32>,
) -> Result<(Option<String>, Option<u32>), DomainError> {
    giga_finish_candidate_count(outcome, candidate_count)?;
    if outcome != GigaEventFinishOutcome::Retry && retry_after_seconds.is_some() {
        return Err(DomainError::InvalidGiga {
            field: "retry_after_seconds".into(),
            message: "is valid only for a retry outcome".into(),
        });
    }
    if outcome == GigaEventFinishOutcome::Retry
        && retry_after_seconds.is_some_and(|seconds| seconds > GIGA_MAX_LEASE_SECONDS)
    {
        return Err(DomainError::InvalidGiga {
            field: "retry_after_seconds".into(),
            message: format!("must be at most {GIGA_MAX_LEASE_SECONDS}"),
        });
    }
    match outcome {
        GigaEventFinishOutcome::Succeeded if error_class.is_some() => {
            return Err(DomainError::InvalidGiga {
                field: "error_class".into(),
                message: "is not valid for a succeeded outcome".into(),
            });
        }
        GigaEventFinishOutcome::Retry | GigaEventFinishOutcome::Failed if error_class.is_none() => {
            return Err(DomainError::InvalidGiga {
                field: "error_class".into(),
                message: "is required for retry and failed outcomes".into(),
            });
        }
        _ => {}
    }
    Ok((
        error_class.map(giga_error_class).transpose()?,
        retry_after_seconds,
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEventFinishRequest {
    room: RoomKey,
    event_id: String,
    worker_id: String,
    outcome: GigaEventFinishOutcome,
    candidate_count: u32,
    error_class: Option<String>,
    retry_after_seconds: Option<u32>,
}

impl GigaEventFinishRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        room: RoomKey,
        event_id: String,
        worker_id: String,
        outcome: GigaEventFinishOutcome,
        candidate_count: u32,
        error_class: Option<String>,
        retry_after_seconds: Option<u32>,
    ) -> Result<Self, DomainError> {
        let (error_class, retry_after_seconds) =
            giga_finish_fields(outcome, candidate_count, error_class, retry_after_seconds)?;
        Ok(Self {
            room,
            event_id: giga_nonempty("event_id", event_id)?,
            worker_id: giga_nonempty("worker_id", worker_id)?,
            outcome,
            candidate_count,
            error_class,
            retry_after_seconds,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }
    pub const fn outcome(&self) -> GigaEventFinishOutcome {
        self.outcome
    }
    pub const fn candidate_count(&self) -> u32 {
        self.candidate_count
    }
    pub fn error_class(&self) -> Option<&str> {
        self.error_class.as_deref()
    }
    pub const fn retry_after_seconds(&self) -> Option<u32> {
        self.retry_after_seconds
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEventReplayRequest {
    room: RoomKey,
    event_id: String,
    operator_identity: String,
    authorization_basis: String,
}

impl GigaEventReplayRequest {
    pub fn new(
        room: RoomKey,
        event_id: String,
        operator_identity: String,
        authorization_basis: String,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            room,
            event_id: giga_nonempty("event_id", event_id)?,
            operator_identity: giga_nonempty("operator_identity", operator_identity)?,
            authorization_basis: giga_nonempty("authorization_basis", authorization_basis)?,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn operator_identity(&self) -> &str {
        &self.operator_identity
    }
    pub fn authorization_basis(&self) -> &str {
        &self.authorization_basis
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaQueueState {
    Pending,
    Running,
    Succeeded,
    Failed,
}

impl GigaQueueState {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            other => Err(DomainError::UnknownGigaValue {
                field: "queue_state".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaQueueMaintenanceOperation {
    Check,
    PurgeStuck,
}

impl GigaQueueMaintenanceOperation {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "check" => Ok(Self::Check),
            "purge_stuck" => Ok(Self::PurgeStuck),
            other => Err(DomainError::UnknownGigaValue {
                field: "operation".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::PurgeStuck => "purge_stuck",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaQueueMaintenanceScope {
    Room,
    All,
}

impl GigaQueueMaintenanceScope {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "room" => Ok(Self::Room),
            "all" => Ok(Self::All),
            other => Err(DomainError::UnknownGigaValue {
                field: "scope".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Room => "room",
            Self::All => "all",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaQueueMaintenanceRequest {
    room: RoomKey,
    operation: GigaQueueMaintenanceOperation,
    scope: GigaQueueMaintenanceScope,
}

impl GigaQueueMaintenanceRequest {
    pub const fn new(
        room: RoomKey,
        operation: GigaQueueMaintenanceOperation,
        scope: GigaQueueMaintenanceScope,
    ) -> Self {
        Self {
            room,
            operation,
            scope,
        }
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }

    pub const fn operation(&self) -> GigaQueueMaintenanceOperation {
        self.operation
    }

    pub const fn scope(&self) -> GigaQueueMaintenanceScope {
        self.scope
    }
}

fn giga_attempt_count(value: u32) -> Result<u32, DomainError> {
    if value == 0 || value > GIGA_MAX_EVENT_ATTEMPTS {
        return Err(DomainError::InvalidGiga {
            field: "attempt_count".into(),
            message: format!("must be between 1 and {GIGA_MAX_EVENT_ATTEMPTS}"),
        });
    }
    Ok(value)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEventClaimReceipt {
    room: RoomKey,
    worker_id: String,
    claimed_at: String,
    event: Option<GigaEvent>,
    lease_expires_at: Option<String>,
    attempt_count: Option<u32>,
}

impl GigaEventClaimReceipt {
    pub fn new(
        room: RoomKey,
        worker_id: String,
        claimed_at: String,
        event: Option<GigaEvent>,
        lease_expires_at: Option<String>,
        attempt_count: Option<u32>,
    ) -> Result<Self, DomainError> {
        if event.is_some() != lease_expires_at.is_some()
            || event.is_some() != attempt_count.is_some()
        {
            return Err(DomainError::InvalidGiga {
                field: "claim_receipt".into(),
                message: "event, lease_expires_at, and attempt_count must all be present or absent"
                    .into(),
            });
        }
        if event.as_ref().is_some_and(|event| event.room() != &room) {
            return Err(DomainError::GigaScopeViolation);
        }
        Ok(Self {
            room,
            worker_id: giga_nonempty("worker_id", worker_id)?,
            claimed_at: giga_rfc3339("claimed_at", claimed_at)?,
            event,
            lease_expires_at: lease_expires_at
                .map(|value| giga_rfc3339("lease_expires_at", value))
                .transpose()?,
            attempt_count: attempt_count.map(giga_attempt_count).transpose()?,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }
    pub fn claimed_at(&self) -> &str {
        &self.claimed_at
    }
    pub fn event(&self) -> Option<&GigaEvent> {
        self.event.as_ref()
    }
    pub fn lease_expires_at(&self) -> Option<&str> {
        self.lease_expires_at.as_deref()
    }
    pub const fn attempt_count(&self) -> Option<u32> {
        self.attempt_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEventFinishReceipt {
    room: RoomKey,
    event_id: String,
    worker_id: String,
    outcome: GigaEventFinishOutcome,
    queue_state: GigaQueueState,
    attempt_count: u32,
    candidate_count: u32,
    available_at: Option<String>,
    finished_at: String,
}

impl GigaEventFinishReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        room: RoomKey,
        event_id: String,
        worker_id: String,
        outcome: GigaEventFinishOutcome,
        queue_state: GigaQueueState,
        attempt_count: u32,
        candidate_count: u32,
        available_at: Option<String>,
        finished_at: String,
    ) -> Result<Self, DomainError> {
        let expected_state = match outcome {
            GigaEventFinishOutcome::Succeeded => GigaQueueState::Succeeded,
            GigaEventFinishOutcome::Retry => GigaQueueState::Pending,
            GigaEventFinishOutcome::Failed => GigaQueueState::Failed,
        };
        if queue_state != expected_state {
            return Err(DomainError::InvalidGiga {
                field: "queue_state".into(),
                message: "does not match finish outcome".into(),
            });
        }
        let attempt_count = giga_attempt_count(attempt_count)?;
        if outcome == GigaEventFinishOutcome::Retry && attempt_count == GIGA_MAX_EVENT_ATTEMPTS {
            return Err(DomainError::InvalidGiga {
                field: "outcome".into(),
                message: "the final bounded attempt must terminate as succeeded or failed".into(),
            });
        }
        giga_finish_candidate_count(outcome, candidate_count)?;
        if outcome != GigaEventFinishOutcome::Retry && available_at.is_some() {
            return Err(DomainError::InvalidGiga {
                field: "available_at".into(),
                message: "is valid only for a retry receipt".into(),
            });
        }
        let available_at = available_at
            .map(|value| giga_rfc3339("available_at", value))
            .transpose()?;
        Ok(Self {
            room,
            event_id: giga_nonempty("event_id", event_id)?,
            worker_id: giga_nonempty("worker_id", worker_id)?,
            outcome,
            queue_state,
            attempt_count,
            candidate_count,
            available_at,
            finished_at: giga_rfc3339("finished_at", finished_at)?,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }
    pub const fn outcome(&self) -> GigaEventFinishOutcome {
        self.outcome
    }
    pub const fn queue_state(&self) -> GigaQueueState {
        self.queue_state
    }
    pub const fn attempt_count(&self) -> u32 {
        self.attempt_count
    }
    pub const fn candidate_count(&self) -> u32 {
        self.candidate_count
    }
    pub fn available_at(&self) -> Option<&str> {
        self.available_at.as_deref()
    }
    pub fn finished_at(&self) -> &str {
        &self.finished_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaEventReplayReceipt {
    room: RoomKey,
    event_id: String,
    operator_identity: String,
    previous_state: GigaQueueState,
    queue_state: GigaQueueState,
    attempt_count: u32,
    replayed_at: String,
}

impl GigaEventReplayReceipt {
    pub fn new(
        room: RoomKey,
        event_id: String,
        operator_identity: String,
        previous_state: GigaQueueState,
        queue_state: GigaQueueState,
        attempt_count: u32,
        replayed_at: String,
    ) -> Result<Self, DomainError> {
        if previous_state != GigaQueueState::Failed
            || queue_state != GigaQueueState::Pending
            || attempt_count != 0
        {
            return Err(DomainError::InvalidGiga {
                field: "replay_receipt".into(),
                message: "replay must reset failed work to pending with attempt_count zero".into(),
            });
        }
        Ok(Self {
            room,
            event_id: giga_nonempty("event_id", event_id)?,
            operator_identity: giga_nonempty("operator_identity", operator_identity)?,
            previous_state,
            queue_state,
            attempt_count,
            replayed_at: giga_rfc3339("replayed_at", replayed_at)?,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn operator_identity(&self) -> &str {
        &self.operator_identity
    }
    pub const fn previous_state(&self) -> GigaQueueState {
        self.previous_state
    }
    pub const fn queue_state(&self) -> GigaQueueState {
        self.queue_state
    }
    pub const fn attempt_count(&self) -> u32 {
        self.attempt_count
    }
    pub fn replayed_at(&self) -> &str {
        &self.replayed_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaPromotionKind {
    Memory,
    CodingLesson,
    ProjectLesson,
}

impl GigaPromotionKind {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "memory" => Ok(Self::Memory),
            "coding_lesson" => Ok(Self::CodingLesson),
            "project_lesson" => Ok(Self::ProjectLesson),
            other => Err(DomainError::UnknownGigaValue {
                field: "durable_kind".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::CodingLesson => "coding_lesson",
            Self::ProjectLesson => "project_lesson",
        }
    }

    pub const fn accepts(self, candidate_kind: GigaCandidateKind) -> bool {
        matches!(
            (self, candidate_kind),
            (Self::Memory, GigaCandidateKind::Memory)
                | (Self::CodingLesson, GigaCandidateKind::CodingLesson)
                | (Self::ProjectLesson, GigaCandidateKind::ProjectLesson)
        )
    }
}

fn giga_edited_text(field: &str, value: String) -> Result<String, DomainError> {
    giga_nonempty(field, value)
}

fn giga_optional_edited_text(
    field: &str,
    value: Option<String>,
) -> Result<Option<String>, DomainError> {
    value
        .map(|value| giga_edited_text(field, value))
        .transpose()
}

fn giga_promotion_strings(field: &str, values: Vec<String>) -> Result<Vec<String>, DomainError> {
    if values.len() > MAX_ARRAY_VALUES {
        return Err(DomainError::InvalidGiga {
            field: field.into(),
            message: format!("must contain at most {MAX_ARRAY_VALUES} values"),
        });
    }
    giga_strings(field, values)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaMemoryPromotionPayload {
    title: String,
    body: String,
    threads: Vec<String>,
}

impl GigaMemoryPromotionPayload {
    pub fn new(title: String, body: String, threads: Vec<String>) -> Result<Self, DomainError> {
        Ok(Self {
            title: giga_edited_text("target.payload.title", title)?,
            body: giga_edited_text("target.payload.body", body)?,
            threads: giga_promotion_strings("target.payload.threads", threads)?,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn threads(&self) -> &[String] {
        &self.threads
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaCodingLessonPromotionPayload {
    title: String,
    body: String,
    shape: Option<String>,
    proof_pattern: String,
    trigger_context: String,
    tags: Vec<String>,
}

impl GigaCodingLessonPromotionPayload {
    pub fn new(
        title: String,
        body: String,
        shape: Option<String>,
        proof_pattern: String,
        trigger_context: String,
        tags: Vec<String>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            title: giga_edited_text("target.payload.title", title)?,
            body: giga_edited_text("target.payload.body", body)?,
            shape: giga_optional_edited_text("target.payload.shape", shape)?,
            proof_pattern: giga_edited_text("target.payload.proof_pattern", proof_pattern)?,
            trigger_context: giga_edited_text("target.payload.trigger_context", trigger_context)?,
            tags: giga_promotion_strings("target.payload.tags", tags)?,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn shape(&self) -> Option<&str> {
        self.shape.as_deref()
    }
    pub fn proof_pattern(&self) -> &str {
        &self.proof_pattern
    }
    pub fn trigger_context(&self) -> &str {
        &self.trigger_context
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaProjectLessonPromotionPayload {
    title: String,
    body: String,
    project: String,
    proof_pattern: String,
    trigger_context: String,
    tags: Vec<String>,
}

impl GigaProjectLessonPromotionPayload {
    pub fn new(
        title: String,
        body: String,
        project: String,
        proof_pattern: String,
        trigger_context: String,
        tags: Vec<String>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            title: giga_edited_text("target.payload.title", title)?,
            body: giga_edited_text("target.payload.body", body)?,
            project: giga_edited_text("target.payload.project", project)?,
            proof_pattern: giga_edited_text("target.payload.proof_pattern", proof_pattern)?,
            trigger_context: giga_edited_text("target.payload.trigger_context", trigger_context)?,
            tags: giga_promotion_strings("target.payload.tags", tags)?,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn project(&self) -> &str {
        &self.project
    }
    pub fn proof_pattern(&self) -> &str {
        &self.proof_pattern
    }
    pub fn trigger_context(&self) -> &str {
        &self.trigger_context
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GigaPromotionPayload {
    Memory(GigaMemoryPromotionPayload),
    CodingLesson(GigaCodingLessonPromotionPayload),
    ProjectLesson(GigaProjectLessonPromotionPayload),
}

impl GigaPromotionPayload {
    pub const fn kind(&self) -> GigaPromotionKind {
        match self {
            Self::Memory(_) => GigaPromotionKind::Memory,
            Self::CodingLesson(_) => GigaPromotionKind::CodingLesson,
            Self::ProjectLesson(_) => GigaPromotionKind::ProjectLesson,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GigaPublicationConsent {
    operator_approved: bool,
    reviewer_approved: bool,
}

impl GigaPublicationConsent {
    pub fn new(operator_approved: bool, reviewer_approved: bool) -> Result<Self, DomainError> {
        if !operator_approved || !reviewer_approved {
            return Err(DomainError::InvalidGiga {
                field: "publication_consent".into(),
                message: "project publication requires operator and governing-spirit approval"
                    .into(),
            });
        }
        Ok(Self {
            operator_approved,
            reviewer_approved,
        })
    }

    pub const fn operator_approved(&self) -> bool {
        self.operator_approved
    }
    pub const fn reviewer_approved(&self) -> bool {
        self.reviewer_approved
    }
}

fn giga_exact_source_set(left: &[GigaSourceRef], right: &[GigaSourceRef]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .enumerate()
            .all(|(index, source)| !left[..index].contains(source) && right.contains(source))
        && right
            .iter()
            .enumerate()
            .all(|(index, source)| !right[..index].contains(source) && left.contains(source))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaPromotionRequest {
    candidate_id: String,
    room: RoomKey,
    reviewer_id: String,
    operator_identity: String,
    authorization_basis: String,
    source_refs: Vec<GigaSourceRef>,
    payload: GigaPromotionPayload,
    publication_consent: Option<GigaPublicationConsent>,
    reviewed_at: String,
}

impl GigaPromotionRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        candidate_id: String,
        room: RoomKey,
        reviewer_id: String,
        operator_identity: String,
        authorization_basis: String,
        source_refs: Vec<GigaSourceRef>,
        payload: GigaPromotionPayload,
        publication_consent: Option<GigaPublicationConsent>,
        reviewed_at: String,
    ) -> Result<Self, DomainError> {
        if source_refs.is_empty() {
            return Err(DomainError::InvalidGiga {
                field: "source_refs".into(),
                message: "promotion must retain exact sources".into(),
            });
        }
        for (index, source) in source_refs.iter().enumerate() {
            if source_refs[..index]
                .iter()
                .any(|known| known.source_id() == source.source_id())
            {
                return Err(DomainError::InvalidGiga {
                    field: "source_refs".into(),
                    message: "source IDs must be unique".into(),
                });
            }
            if source.scope().visibility() == GigaVisibility::Private
                && source.scope().room() != Some(&room)
            {
                return Err(DomainError::GigaScopeViolation);
            }
        }
        match payload.kind() {
            GigaPromotionKind::ProjectLesson if publication_consent.is_none() => {
                return Err(DomainError::InvalidGiga {
                    field: "publication_consent".into(),
                    message: "is required for project_lesson promotion".into(),
                });
            }
            GigaPromotionKind::Memory | GigaPromotionKind::CodingLesson
                if publication_consent.is_some() =>
            {
                return Err(DomainError::InvalidGiga {
                    field: "publication_consent".into(),
                    message: "is valid only for project_lesson promotion".into(),
                });
            }
            _ => {}
        }
        Ok(Self {
            candidate_id: giga_nonempty("candidate_id", candidate_id)?,
            room,
            reviewer_id: giga_nonempty("reviewer_id", reviewer_id)?,
            operator_identity: giga_nonempty("operator_identity", operator_identity)?,
            authorization_basis: giga_nonempty("authorization_basis", authorization_basis)?,
            source_refs,
            payload,
            publication_consent,
            reviewed_at: giga_rfc3339("reviewed_at", reviewed_at)?,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn validate_candidate(
        &self,
        candidate_id: &str,
        room: &RoomKey,
        kind: GigaCandidateKind,
        review_state: GigaReviewState,
        source_refs: &[GigaSourceRef],
        project_keys: &[String],
        scope: &GigaScope,
    ) -> Result<(), DomainError> {
        if candidate_id != self.candidate_id {
            return Err(DomainError::InvalidGiga {
                field: "candidate_id".into(),
                message: "does not match the locked candidate".into(),
            });
        }
        if room != &self.room {
            return Err(DomainError::GigaScopeViolation);
        }
        if review_state != GigaReviewState::InReview {
            return Err(DomainError::InvalidGiga {
                field: "review_state".into(),
                message: "promotion requires an in_review candidate".into(),
            });
        }
        if !self.payload.kind().accepts(kind) {
            return Err(DomainError::InvalidGiga {
                field: "target.kind".into(),
                message: "does not match candidate kind or is not promotable".into(),
            });
        }
        if !giga_exact_source_set(&self.source_refs, source_refs) {
            return Err(DomainError::InvalidGiga {
                field: "source_refs".into(),
                message: "must exactly match the locked candidate source set and hashes".into(),
            });
        }
        match &self.payload {
            GigaPromotionPayload::Memory(_) | GigaPromotionPayload::CodingLesson(_) => {
                if scope.visibility() != GigaVisibility::Private || scope.room() != Some(&self.room)
                {
                    return Err(DomainError::GigaScopeViolation);
                }
            }
            GigaPromotionPayload::ProjectLesson(payload) => {
                if project_keys.len() != 1
                    || project_keys[0] != payload.project
                    || scope.project() != Some(payload.project())
                {
                    return Err(DomainError::GigaScopeViolation);
                }
            }
        }
        Ok(())
    }

    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn reviewer_id(&self) -> &str {
        &self.reviewer_id
    }
    pub fn operator_identity(&self) -> &str {
        &self.operator_identity
    }
    pub fn authorization_basis(&self) -> &str {
        &self.authorization_basis
    }
    pub fn source_refs(&self) -> &[GigaSourceRef] {
        &self.source_refs
    }
    pub fn payload(&self) -> &GigaPromotionPayload {
        &self.payload
    }
    pub const fn publication_consent(&self) -> Option<GigaPublicationConsent> {
        self.publication_consent
    }
    pub fn reviewed_at(&self) -> &str {
        &self.reviewed_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GigaPromotionAuthority {
    Full,
}

impl GigaPromotionAuthority {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "full" => Ok(Self::Full),
            other => Err(DomainError::UnknownGigaValue {
                field: "promotion_authority".into(),
                value: other.into(),
            }),
        }
    }

    pub const fn as_str(self) -> &'static str {
        "full"
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GigaPromotionReceiptCommon {
    candidate_id: String,
    durable_id: u64,
    reviewer_id: String,
    operator_identity: String,
    reviewed_at: String,
    committed_at: String,
}

impl GigaPromotionReceiptCommon {
    fn new(
        candidate_id: String,
        durable_id: u64,
        reviewer_id: String,
        operator_identity: String,
        reviewed_at: String,
        committed_at: String,
    ) -> Result<Self, DomainError> {
        if durable_id == 0 {
            return Err(DomainError::InvalidGiga {
                field: "durable_id".into(),
                message: "must be positive".into(),
            });
        }
        Ok(Self {
            candidate_id: giga_nonempty("candidate_id", candidate_id)?,
            durable_id,
            reviewer_id: giga_nonempty("reviewer_id", reviewer_id)?,
            operator_identity: giga_nonempty("operator_identity", operator_identity)?,
            reviewed_at: giga_rfc3339("reviewed_at", reviewed_at)?,
            committed_at: giga_rfc3339("committed_at", committed_at)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaMemoryPromotionReceipt {
    common: GigaPromotionReceiptCommon,
    room: RoomKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaCodingLessonPromotionReceipt {
    common: GigaPromotionReceiptCommon,
    scope: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GigaProjectLessonPromotionReceipt {
    common: GigaPromotionReceiptCommon,
    project: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GigaPromotionReceipt {
    Memory(GigaMemoryPromotionReceipt),
    CodingLesson(GigaCodingLessonPromotionReceipt),
    ProjectLesson(GigaProjectLessonPromotionReceipt),
}

impl GigaPromotionReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn memory(
        candidate_id: String,
        memory_id: u64,
        room: RoomKey,
        reviewer_id: String,
        operator_identity: String,
        reviewed_at: String,
        committed_at: String,
    ) -> Result<Self, DomainError> {
        Ok(Self::Memory(GigaMemoryPromotionReceipt {
            common: GigaPromotionReceiptCommon::new(
                candidate_id,
                memory_id,
                reviewer_id,
                operator_identity,
                reviewed_at,
                committed_at,
            )?,
            room,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn coding_lesson(
        candidate_id: String,
        coding_lesson_id: u64,
        scope: String,
        reviewer_id: String,
        operator_identity: String,
        reviewed_at: String,
        committed_at: String,
    ) -> Result<Self, DomainError> {
        Ok(Self::CodingLesson(GigaCodingLessonPromotionReceipt {
            common: GigaPromotionReceiptCommon::new(
                candidate_id,
                coding_lesson_id,
                reviewer_id,
                operator_identity,
                reviewed_at,
                committed_at,
            )?,
            scope: giga_nonempty("scope", scope)?,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn project_lesson(
        candidate_id: String,
        project_lesson_id: u64,
        project: String,
        reviewer_id: String,
        operator_identity: String,
        reviewed_at: String,
        committed_at: String,
    ) -> Result<Self, DomainError> {
        Ok(Self::ProjectLesson(GigaProjectLessonPromotionReceipt {
            common: GigaPromotionReceiptCommon::new(
                candidate_id,
                project_lesson_id,
                reviewer_id,
                operator_identity,
                reviewed_at,
                committed_at,
            )?,
            project: giga_nonempty("project", project)?,
        }))
    }

    fn common(&self) -> &GigaPromotionReceiptCommon {
        match self {
            Self::Memory(receipt) => &receipt.common,
            Self::CodingLesson(receipt) => &receipt.common,
            Self::ProjectLesson(receipt) => &receipt.common,
        }
    }

    pub fn candidate_id(&self) -> &str {
        &self.common().candidate_id
    }
    pub const fn review_state(&self) -> GigaReviewState {
        GigaReviewState::Promoted
    }
    pub const fn durable_kind(&self) -> GigaPromotionKind {
        match self {
            Self::Memory(_) => GigaPromotionKind::Memory,
            Self::CodingLesson(_) => GigaPromotionKind::CodingLesson,
            Self::ProjectLesson(_) => GigaPromotionKind::ProjectLesson,
        }
    }
    pub fn durable_id(&self) -> u64 {
        self.common().durable_id
    }
    pub const fn durable(&self) -> bool {
        true
    }
    pub const fn authority(&self) -> GigaPromotionAuthority {
        GigaPromotionAuthority::Full
    }
    pub fn reviewer_id(&self) -> &str {
        &self.common().reviewer_id
    }
    pub fn operator_identity(&self) -> &str {
        &self.common().operator_identity
    }
    pub fn reviewed_at(&self) -> &str {
        &self.common().reviewed_at
    }
    pub fn committed_at(&self) -> &str {
        &self.common().committed_at
    }
}

impl GigaMemoryPromotionReceipt {
    pub fn memory_id(&self) -> u64 {
        self.common.durable_id
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
}

impl GigaCodingLessonPromotionReceipt {
    pub fn coding_lesson_id(&self) -> u64 {
        self.common.durable_id
    }
    pub fn scope(&self) -> &str {
        &self.scope
    }
}

impl GigaProjectLessonPromotionReceipt {
    pub fn project_lesson_id(&self) -> u64 {
        self.common.durable_id
    }
    pub fn project(&self) -> &str {
        &self.project
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_room_keys_and_reserved_house() {
        assert!(RoomKey::new("living-room2").is_ok());
        for invalid in ["", "Living", "-room", "room-", "two--rooms", "house"] {
            assert!(RoomKey::new(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn shared_house_doors_stay_purpose_scoped() {
        assert!(RoomKey::for_anamnesis("house").is_ok());
        assert!(RoomKey::for_memory_write("house").is_ok());
        assert!(RoomKey::for_memory_write("living-room2").is_ok());
        assert!(RoomKey::for_anamnesis("Living").is_err());
        assert!(RoomKey::for_memory_write("Living").is_err());
        assert!(RoomKey::new("house").is_err());
    }

    #[test]
    fn recall_constructor_compatibility_defaults_decay_off_and_builder_opts_in() {
        let request =
            RecallRequest::new(RoomKey::new("lab").unwrap(), "alpha".into(), 8, 0.5, 8, 0.3)
                .unwrap();
        assert!(!request.temporal_decay());

        let decayed = request.with_temporal_decay(true);
        assert!(decayed.temporal_decay());
    }

    #[test]
    fn accepts_canonical_room_keys_longer_than_63_bytes() {
        let room = "a".repeat(64);
        assert!(RoomKey::new(room).is_ok());
    }

    #[test]
    fn committed_receipt_requires_source_path_and_is_postgres_durable() {
        let room = RoomKey::new("lab").unwrap();
        assert!(RememberReceipt::committed(1, room.clone(), " ".into(), vec![]).is_err());
        let receipt = RememberReceipt::committed(1, room, "memory.md".into(), vec![]).unwrap();
        assert_eq!(receipt.source_path(), "memory.md");
        assert!(receipt.durable());
        assert_eq!(receipt.authority(), Authority::Full);
    }

    #[test]
    fn validates_memory_request_invariants() {
        let room = RoomKey::new("lab").unwrap();
        assert_eq!(
            RememberRequest::new_memory(
                room.clone(),
                " ".into(),
                "body".into(),
                RememberMemoryDetails {
                    source_path: None,
                    threads: vec![],
                    continues: vec![],
                    supersedes: vec![],
                    backup: true,
                },
            ),
            Err(DomainError::EmptyTitle)
        );
        assert_eq!(
            RememberRequest::new_memory(
                room,
                "title".into(),
                "\n".into(),
                RememberMemoryDetails {
                    source_path: None,
                    threads: vec![],
                    continues: vec![],
                    supersedes: vec![],
                    backup: true,
                },
            ),
            Err(DomainError::EmptyBody)
        );
    }

    #[test]
    fn validates_memory_continuations_per_thread() {
        let accepted = RememberRequest::new_memory(
            RoomKey::new("lab").unwrap(),
            "decision".into(),
            "new decision".into(),
            RememberMemoryDetails {
                source_path: None,
                threads: vec![" work / page ".into()],
                continues: vec![ThreadContinuation {
                    thread: "work / page".into(),
                    previous_memory_id: 41,
                }],
                supersedes: vec![],
                backup: false,
            },
        )
        .unwrap();
        assert_eq!(accepted.threads(), &["work / page"]);
        assert_eq!(
            accepted.continues(),
            &[ThreadContinuation {
                thread: "work / page".into(),
                previous_memory_id: 41,
            }]
        );

        let missing_membership = RememberRequest::new_memory(
            RoomKey::new("lab").unwrap(),
            "decision".into(),
            "new decision".into(),
            RememberMemoryDetails {
                source_path: None,
                threads: vec!["other".into()],
                continues: vec![ThreadContinuation {
                    thread: "work / page".into(),
                    previous_memory_id: 41,
                }],
                supersedes: vec![],
                backup: false,
            },
        );
        assert_eq!(
            missing_membership,
            Err(DomainError::ContinuationThreadNotMember(
                "work / page".into()
            ))
        );

        let duplicate = RememberRequest::new_memory(
            RoomKey::new("lab").unwrap(),
            "decision".into(),
            "new decision".into(),
            RememberMemoryDetails {
                source_path: None,
                threads: vec!["work / page".into()],
                continues: vec![
                    ThreadContinuation {
                        thread: "work / page".into(),
                        previous_memory_id: 41,
                    },
                    ThreadContinuation {
                        thread: " work / page ".into(),
                        previous_memory_id: 42,
                    },
                ],
                supersedes: vec![],
                backup: false,
            },
        );
        assert_eq!(
            duplicate,
            Err(DomainError::DuplicateContinuationThread(
                "work / page".into()
            ))
        );
    }

    #[test]
    fn anamnesis_append_allows_no_source_paths_but_rejects_blank_paths() {
        let room = RoomKey::for_anamnesis("tuner").unwrap();
        let rep = || {
            AnamnesisSeedRep::new(
                1,
                None,
                "worked".into(),
                "old pull".into(),
                "lighter".into(),
            )
            .unwrap()
        };
        assert!(
            AnamnesisAppendRequest::new(room.clone(), "cycle".into(), rep(), Vec::new()).is_ok()
        );
        assert!(
            AnamnesisAppendRequest::new(room, "cycle".into(), rep(), vec![" ".into()]).is_err()
        );
    }

    #[test]
    fn full_unhealthy_never_falls_back_to_base() {
        let result = authorize(
            HouseMode::Full,
            HealthVerdict::Unhealthy {
                reason: "db down".into(),
            },
        );
        assert_eq!(
            result,
            Err(DomainError::FullUnhealthy {
                reason: "db down".into()
            })
        );
        assert_eq!(
            authorize(HouseMode::Degraded, HealthVerdict::Healthy),
            Err(DomainError::DegradedUnavailable)
        );
    }
    #[test]
    fn cluster_staleness_boundaries_require_unseen_chunks() {
        let fresh = ClusterStaleness::new(Some("2026-07-20T00:00:00Z".into()), 0, 0.05).unwrap();
        assert!(!fresh.is_stale(30));
        assert!(
            ClusterStaleness::new(Some("2026-07-20T00:00:00Z".into()), 1, 0.05)
                .unwrap()
                .is_stale(0)
        );
        assert!(
            ClusterStaleness::new(Some("2026-07-20T00:00:00Z".into()), 250, 0.0)
                .unwrap()
                .is_stale(0)
        );
        assert!(
            ClusterStaleness::new(Some("2026-07-20T00:00:00Z".into()), 1, 0.0)
                .unwrap()
                .is_stale(7)
        );
        assert!(ClusterStaleness::new(None, 0, 0.0).unwrap().is_stale(0));
    }

    #[test]
    fn cluster_request_rejects_invalid_k_and_check_options() {
        let room = RoomKey::new("lab").unwrap();
        assert!(
            ClusterMaintenanceRequest::new(
                room.clone(),
                ClusterMaintenanceOperation::Rebuild,
                false,
                false,
                0
            )
            .is_err()
        );
        assert!(
            ClusterMaintenanceRequest::new(
                room,
                ClusterMaintenanceOperation::Check,
                true,
                false,
                8
            )
            .is_err()
        );
    }
    fn giga_test_source(source_id: &str, hash_digit: char, scope: GigaScope) -> GigaSourceRef {
        GigaSourceRef::new(
            GigaSourceType::Turn,
            source_id.into(),
            "user".into(),
            "2026-07-24T12:00:00Z".into(),
            hash_digit.to_string().repeat(64),
            scope,
            None,
        )
        .unwrap()
    }

    fn giga_private_scope() -> GigaScope {
        GigaScope::new(Some("lab".into()), None, GigaVisibility::Private, false).unwrap()
    }

    fn giga_project_scope() -> GigaScope {
        GigaScope::new(
            Some("lab".into()),
            Some("athanor".into()),
            GigaVisibility::Private,
            true,
        )
        .unwrap()
    }

    #[test]
    fn giga_queue_claim_finish_and_replay_requests_preserve_authorized_fields() {
        let room = RoomKey::new("lab").unwrap();
        let claim = GigaEventClaimRequest::new(room.clone(), "agents-a1".into(), 60).unwrap();
        assert_eq!(claim.room(), &room);
        assert_eq!(claim.worker_id(), "agents-a1");
        assert_eq!(claim.lease_seconds(), 60);
        let source = giga_test_source("turn-1", 'a', giga_private_scope());
        let event = GigaEvent::new(
            "event-1".into(),
            GigaEventType::ConversationWindow,
            room.clone(),
            "session-1".into(),
            vec![],
            vec![source.clone()],
            GigaLifecycle::conversation_window(),
            "2026-07-24T11:59:00Z".into(),
        )
        .unwrap();
        let claim_receipt = GigaEventClaimReceipt::new(
            room.clone(),
            "agents-a1".into(),
            "2026-07-24T12:00:00Z".into(),
            Some(event),
            Some("2026-07-24T12:01:00Z".into()),
            Some(1),
        )
        .unwrap();
        assert_eq!(claim_receipt.attempt_count(), Some(1));
        assert_eq!(claim_receipt.event().unwrap().source_refs(), &[source]);

        let succeeded = GigaEventFinishRequest::new(
            room.clone(),
            "event-1".into(),
            "agents-a1".into(),
            GigaEventFinishOutcome::Succeeded,
            1,
            None,
            None,
        )
        .unwrap();
        assert_eq!(succeeded.outcome(), GigaEventFinishOutcome::Succeeded);
        assert_eq!(succeeded.candidate_count(), 1);
        assert_eq!(succeeded.error_class(), None);

        let retry = GigaEventFinishRequest::new(
            room.clone(),
            "event-2".into(),
            "agents-a1".into(),
            GigaEventFinishOutcome::Retry,
            0,
            Some("model_timeout".into()),
            Some(60),
        )
        .unwrap();
        assert_eq!(retry.error_class(), Some("model_timeout"));
        assert_eq!(retry.retry_after_seconds(), Some(60));

        let failed = GigaEventFinishRequest::new(
            room.clone(),
            "event-3".into(),
            "agents-a1".into(),
            GigaEventFinishOutcome::Failed,
            0,
            Some("invalid_output".into()),
            None,
        )
        .unwrap();
        assert_eq!(failed.outcome(), GigaEventFinishOutcome::Failed);

        let replay = GigaEventReplayRequest::new(
            room,
            "event-3".into(),
            "sol".into(),
            "operator requested replay after prompt repair".into(),
        )
        .unwrap();
        assert_eq!(replay.operator_identity(), "sol");
        assert_eq!(
            replay.authorization_basis(),
            "operator requested replay after prompt repair"
        );
    }

    #[test]
    fn giga_queue_enforces_lease_bounds_retry_ceiling_and_receipt_shapes() {
        let room = RoomKey::new("lab").unwrap();
        for lease_seconds in [1, GIGA_MAX_LEASE_SECONDS] {
            assert!(
                GigaEventClaimRequest::new(room.clone(), "agents-a1".into(), lease_seconds).is_ok()
            );
        }
        for lease_seconds in [0, GIGA_MAX_LEASE_SECONDS + 1] {
            assert!(matches!(
                GigaEventClaimRequest::new(
                    room.clone(),
                    "agents-a1".into(),
                    lease_seconds,
                ),
                Err(DomainError::InvalidGiga { field, .. }) if field == "lease_seconds"
            ));
        }

        for (outcome, retry_after_seconds) in [
            (GigaEventFinishOutcome::Retry, Some(1)),
            (GigaEventFinishOutcome::Failed, None),
        ] {
            assert!(matches!(
                GigaEventFinishRequest::new(
                    room.clone(),
                    "event-1".into(),
                    "agents-a1".into(),
                    outcome,
                    0,
                    None,
                    retry_after_seconds,
                ),
                Err(DomainError::InvalidGiga { field, .. }) if field == "error_class"
            ));
        }

        assert!(
            GigaEventClaimReceipt::new(
                room.clone(),
                "agents-a1".into(),
                "2026-07-24T12:00:00Z".into(),
                None,
                None,
                None,
            )
            .is_ok()
        );
        assert!(matches!(
            GigaEventClaimReceipt::new(
                room.clone(),
                "agents-a1".into(),
                "2026-07-24T12:00:00Z".into(),
                None,
                Some("2026-07-24T12:01:00Z".into()),
                Some(1),
            ),
            Err(DomainError::InvalidGiga { field, .. }) if field == "claim_receipt"
        ));

        assert!(
            GigaEventFinishReceipt::new(
                room.clone(),
                "event-1".into(),
                "agents-a1".into(),
                GigaEventFinishOutcome::Retry,
                GigaQueueState::Pending,
                GIGA_MAX_EVENT_ATTEMPTS - 1,
                0,
                Some("2026-07-24T12:02:00Z".into()),
                "2026-07-24T12:01:00Z".into(),
            )
            .is_ok()
        );
        assert!(matches!(
            GigaEventFinishReceipt::new(
                room.clone(),
                "event-1".into(),
                "agents-a1".into(),
                GigaEventFinishOutcome::Retry,
                GigaQueueState::Pending,
                GIGA_MAX_EVENT_ATTEMPTS,
                0,
                Some("2026-07-24T12:02:00Z".into()),
                "2026-07-24T12:01:00Z".into(),
            ),
            Err(DomainError::InvalidGiga { field, .. }) if field == "outcome"
        ));
        assert!(
            GigaEventFinishReceipt::new(
                room.clone(),
                "event-1".into(),
                "agents-a1".into(),
                GigaEventFinishOutcome::Failed,
                GigaQueueState::Failed,
                GIGA_MAX_EVENT_ATTEMPTS,
                0,
                None,
                "2026-07-24T12:01:00Z".into(),
            )
            .is_ok()
        );
        assert!(
            GigaEventFinishReceipt::new(
                room,
                "event-1".into(),
                "agents-a1".into(),
                GigaEventFinishOutcome::Succeeded,
                GigaQueueState::Pending,
                1,
                1,
                None,
                "2026-07-24T12:01:00Z".into(),
            )
            .is_err()
        );
    }

    #[test]
    fn giga_replay_requires_explicit_authorization_and_exact_reset_receipt() {
        let room = RoomKey::new("lab").unwrap();
        for (operator_identity, authorization_basis) in
            [("", "operator requested replay"), ("sol", " ")]
        {
            assert!(
                GigaEventReplayRequest::new(
                    room.clone(),
                    "event-1".into(),
                    operator_identity.into(),
                    authorization_basis.into(),
                )
                .is_err()
            );
        }

        let receipt = GigaEventReplayReceipt::new(
            room.clone(),
            "event-1".into(),
            "sol".into(),
            GigaQueueState::Failed,
            GigaQueueState::Pending,
            0,
            "2026-07-24T12:03:00Z".into(),
        )
        .unwrap();
        assert_eq!(receipt.previous_state(), GigaQueueState::Failed);
        assert_eq!(receipt.queue_state(), GigaQueueState::Pending);
        assert_eq!(receipt.attempt_count(), 0);

        for (previous, current, attempts) in [
            (GigaQueueState::Running, GigaQueueState::Pending, 0),
            (GigaQueueState::Failed, GigaQueueState::Running, 0),
            (GigaQueueState::Failed, GigaQueueState::Pending, 1),
        ] {
            assert!(
                GigaEventReplayReceipt::new(
                    room.clone(),
                    "event-1".into(),
                    "sol".into(),
                    previous,
                    current,
                    attempts,
                    "2026-07-24T12:03:00Z".into(),
                )
                .is_err()
            );
        }
    }

    #[test]
    fn giga_memory_coding_and_project_promotions_validate_their_candidates() {
        let room = RoomKey::new("lab").unwrap();
        let private_scope = giga_private_scope();
        let private_source = giga_test_source("turn-1", 'a', private_scope.clone());

        let memory = GigaPromotionRequest::new(
            "candidate-memory".into(),
            room.clone(),
            "kintsu".into(),
            "sol".into(),
            "reviewed exact source".into(),
            vec![private_source.clone()],
            GigaPromotionPayload::Memory(
                GigaMemoryPromotionPayload::new(
                    "Edited memory".into(),
                    "Durable human-edited body".into(),
                    vec!["consent".into()],
                )
                .unwrap(),
            ),
            None,
            "2026-07-24T12:04:00Z".into(),
        )
        .unwrap();
        memory
            .validate_candidate(
                "candidate-memory",
                &room,
                GigaCandidateKind::Memory,
                GigaReviewState::InReview,
                &[private_source.clone()],
                &[],
                &private_scope,
            )
            .unwrap();

        let coding = GigaPromotionRequest::new(
            "candidate-coding".into(),
            room.clone(),
            "kintsu".into(),
            "sol".into(),
            "proof reviewed".into(),
            vec![private_source.clone()],
            GigaPromotionPayload::CodingLesson(
                GigaCodingLessonPromotionPayload::new(
                    "Sanitize inherited state".into(),
                    "Clear inherited variables before invoking tools.".into(),
                    Some("process".into()),
                    "failure then passing proof".into(),
                    "inherited environment state reaches a child tool process".into(),
                    vec!["environment".into()],
                )
                .unwrap(),
            ),
            None,
            "2026-07-24T12:04:00Z".into(),
        )
        .unwrap();
        coding
            .validate_candidate(
                "candidate-coding",
                &room,
                GigaCandidateKind::CodingLesson,
                GigaReviewState::InReview,
                &[private_source],
                &[],
                &private_scope,
            )
            .unwrap();

        let project_scope = giga_project_scope();
        let project_source = giga_test_source("turn-2", 'b', project_scope.clone());
        let project = GigaPromotionRequest::new(
            "candidate-project".into(),
            room.clone(),
            "kintsu".into(),
            "sol".into(),
            "operator and spirit approved publication".into(),
            vec![project_source.clone()],
            GigaPromotionPayload::ProjectLesson(
                GigaProjectLessonPromotionPayload::new(
                    "Stable Athanor rule".into(),
                    "Keep queue mutations transactional.".into(),
                    "athanor".into(),
                    "rollback observed".into(),
                    "queue work crosses a durable transaction boundary".into(),
                    vec!["queue".into()],
                )
                .unwrap(),
            ),
            Some(GigaPublicationConsent::new(true, true).unwrap()),
            "2026-07-24T12:04:00Z".into(),
        )
        .unwrap();
        project
            .validate_candidate(
                "candidate-project",
                &room,
                GigaCandidateKind::ProjectLesson,
                GigaReviewState::InReview,
                &[project_source],
                &["athanor".into()],
                &project_scope,
            )
            .unwrap();
    }

    #[test]
    fn giga_promotion_requires_exact_source_refs_and_matching_target_kind() {
        let room = RoomKey::new("lab").unwrap();
        let scope = giga_private_scope();
        let first = giga_test_source("turn-1", 'a', scope.clone());
        let second = giga_test_source("turn-2", 'b', scope.clone());
        let request = GigaPromotionRequest::new(
            "candidate-memory".into(),
            room.clone(),
            "kintsu".into(),
            "sol".into(),
            "reviewed exact sources".into(),
            vec![first.clone(), second.clone()],
            GigaPromotionPayload::Memory(
                GigaMemoryPromotionPayload::new(
                    "Edited title".into(),
                    "Edited body".into(),
                    vec![],
                )
                .unwrap(),
            ),
            None,
            "2026-07-24T12:04:00Z".into(),
        )
        .unwrap();

        request
            .validate_candidate(
                "candidate-memory",
                &room,
                GigaCandidateKind::Memory,
                GigaReviewState::InReview,
                &[second.clone(), first.clone()],
                &[],
                &scope,
            )
            .unwrap();
        let rehashed = giga_test_source("turn-2", 'c', scope.clone());
        for candidate_sources in [vec![first.clone()], vec![first.clone(), rehashed]] {
            assert!(matches!(
                request.validate_candidate(
                    "candidate-memory",
                    &room,
                    GigaCandidateKind::Memory,
                    GigaReviewState::InReview,
                    &candidate_sources,
                    &[],
                    &scope,
                ),
                Err(DomainError::InvalidGiga { field, .. }) if field == "source_refs"
            ));
        }

        for kind in [
            GigaCandidateKind::CodingLesson,
            GigaCandidateKind::ProjectLesson,
            GigaCandidateKind::Correction,
            GigaCandidateKind::Supersession,
            GigaCandidateKind::EntityUpdate,
            GigaCandidateKind::ThreadUpdate,
        ] {
            assert!(matches!(
                request.validate_candidate(
                    "candidate-memory",
                    &room,
                    kind,
                    GigaReviewState::InReview,
                    &[first.clone(), second.clone()],
                    &[],
                    &scope,
                ),
                Err(DomainError::InvalidGiga { field, .. }) if field == "target.kind"
            ));
        }

        let cross_room_source = giga_test_source(
            "turn-3",
            'd',
            GigaScope::new(
                Some("other-room".into()),
                None,
                GigaVisibility::Private,
                false,
            )
            .unwrap(),
        );
        assert_eq!(
            GigaPromotionRequest::new(
                "candidate-memory".into(),
                room,
                "kintsu".into(),
                "sol".into(),
                "reviewed exact source".into(),
                vec![cross_room_source],
                GigaPromotionPayload::Memory(
                    GigaMemoryPromotionPayload::new(
                        "Edited title".into(),
                        "Edited body".into(),
                        vec![],
                    )
                    .unwrap(),
                ),
                None,
                "2026-07-24T12:04:00Z".into(),
            ),
            Err(DomainError::GigaScopeViolation)
        );
    }

    #[test]
    fn giga_project_promotion_requires_dual_consent_and_exact_project_scope() {
        for (operator_approved, reviewer_approved) in [(false, true), (true, false), (false, false)]
        {
            assert!(matches!(
                GigaPublicationConsent::new(operator_approved, reviewer_approved),
                Err(DomainError::InvalidGiga { field, .. }) if field == "publication_consent"
            ));
        }
        let consent = GigaPublicationConsent::new(true, true).unwrap();
        assert!(consent.operator_approved());
        assert!(consent.reviewer_approved());

        let room = RoomKey::new("lab").unwrap();
        let scope = giga_project_scope();
        let source = giga_test_source("turn-1", 'a', scope.clone());
        let payload = || {
            GigaPromotionPayload::ProjectLesson(
                GigaProjectLessonPromotionPayload::new(
                    "Edited title".into(),
                    "Edited body".into(),
                    "athanor".into(),
                    "transaction rollback preserves the prior durable state".into(),
                    "a project rule changes coupled database writes".into(),
                    vec![],
                )
                .unwrap(),
            )
        };
        assert!(matches!(
            GigaPromotionRequest::new(
                "candidate-project".into(),
                room.clone(),
                "kintsu".into(),
                "sol".into(),
                "publication reviewed".into(),
                vec![source.clone()],
                payload(),
                None,
                "2026-07-24T12:04:00Z".into(),
            ),
            Err(DomainError::InvalidGiga { field, .. }) if field == "publication_consent"
        ));

        let request = GigaPromotionRequest::new(
            "candidate-project".into(),
            room.clone(),
            "kintsu".into(),
            "sol".into(),
            "publication reviewed".into(),
            vec![source.clone()],
            payload(),
            Some(consent),
            "2026-07-24T12:04:00Z".into(),
        )
        .unwrap();
        for projects in [
            Vec::new(),
            vec!["other-project".into()],
            vec!["athanor".into(), "other-project".into()],
        ] {
            assert_eq!(
                request.validate_candidate(
                    "candidate-project",
                    &room,
                    GigaCandidateKind::ProjectLesson,
                    GigaReviewState::InReview,
                    &[source.clone()],
                    &projects,
                    &scope,
                ),
                Err(DomainError::GigaScopeViolation)
            );
        }
    }

    #[test]
    fn giga_promotion_payloads_require_human_edited_durable_fields() {
        assert!(GigaMemoryPromotionPayload::new(" ".into(), "body".into(), vec![]).is_err());
        assert!(GigaMemoryPromotionPayload::new("title".into(), "\n".into(), vec![]).is_err());
        assert!(
            GigaCodingLessonPromotionPayload::new(
                "".into(),
                "body".into(),
                None,
                "proof".into(),
                "trigger".into(),
                vec![],
            )
            .is_err()
        );
        assert!(
            GigaCodingLessonPromotionPayload::new(
                "title".into(),
                "body".into(),
                Some(" ".into()),
                "proof".into(),
                "trigger".into(),
                vec![],
            )
            .is_err()
        );
        for (proof_pattern, trigger_context) in [(" ", "trigger context"), ("proof pattern", "\n")]
        {
            assert!(
                GigaCodingLessonPromotionPayload::new(
                    "title".into(),
                    "body".into(),
                    None,
                    proof_pattern.into(),
                    trigger_context.into(),
                    vec![],
                )
                .is_err()
            );
        }
        assert!(
            GigaProjectLessonPromotionPayload::new(
                "title".into(),
                "body".into(),
                " ".into(),
                "proof".into(),
                "trigger".into(),
                vec![],
            )
            .is_err()
        );
        for (proof_pattern, trigger_context) in [("", "trigger context"), ("proof pattern", " ")] {
            assert!(
                GigaProjectLessonPromotionPayload::new(
                    "title".into(),
                    "body".into(),
                    "project".into(),
                    proof_pattern.into(),
                    trigger_context.into(),
                    vec![],
                )
                .is_err()
            );
        }
    }

    #[test]
    fn giga_promotion_receipts_are_exhaustive_and_require_positive_typed_ids() {
        let room = RoomKey::new("lab").unwrap();
        assert!(
            GigaPromotionReceipt::memory(
                "candidate-1".into(),
                0,
                room.clone(),
                "kintsu".into(),
                "sol".into(),
                "2026-07-24T12:04:00Z".into(),
                "2026-07-24T12:05:00Z".into(),
            )
            .is_err()
        );
        assert!(matches!(
            GigaPromotionReceipt::memory(
                "candidate-1".into(),
                7,
                room,
                "kintsu".into(),
                "sol".into(),
                "2026-07-24T12:04:00Z".into(),
                "2026-07-24T12:05:00Z".into(),
            )
            .unwrap(),
            GigaPromotionReceipt::Memory(_)
        ));
        assert!(matches!(
            GigaPromotionReceipt::coding_lesson(
                "candidate-2".into(),
                8,
                "lab".into(),
                "kintsu".into(),
                "sol".into(),
                "2026-07-24T12:04:00Z".into(),
                "2026-07-24T12:05:00Z".into(),
            )
            .unwrap(),
            GigaPromotionReceipt::CodingLesson(_)
        ));
        assert!(matches!(
            GigaPromotionReceipt::project_lesson(
                "candidate-3".into(),
                9,
                "kintsu".into(),
                "kintsu".into(),
                "sol".into(),
                "2026-07-24T12:04:00Z".into(),
                "2026-07-24T12:05:00Z".into(),
            )
            .unwrap(),
            GigaPromotionReceipt::ProjectLesson(_)
        ));
    }
}

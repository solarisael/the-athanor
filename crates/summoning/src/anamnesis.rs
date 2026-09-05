use chrono::NaiveDate;
use hearth::{BackupOutcome, DomainError, RoomKey};

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
    occurred_on: Option<NaiveDate>,
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
        let occurred_on = occurred_on
            .map(|raw| {
                NaiveDate::parse_from_str(&raw, "%Y-%m-%d").map_err(|_| {
                    DomainError::InvalidAnamnesis {
                        field: "occurred_on".into(),
                        message: "must use YYYY-MM-DD".into(),
                    }
                })
            })
            .transpose()?;
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
    pub const fn occurred_on(&self) -> Option<NaiveDate> {
        self.occurred_on
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
    rep: AnamnesisSeedRep,
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
        Ok(Self {
            room,
            title,
            rep,
            source_paths,
        })
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn rep(&self) -> &AnamnesisSeedRep {
        &self.rep
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
    backup: BackupOutcome,
    warnings: Vec<String>,
}
impl AnamnesisReceipt {
    pub fn committed(
        room: RoomKey,
        title: String,
        kind: AnamnesisKind,
        backup: BackupOutcome,
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
            backup,
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
    /// The file backup that followed the PostgreSQL commit.
    pub const fn backup(&self) -> &BackupOutcome {
        &self.backup
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
    backup: BackupOutcome,
    warnings: Vec<String>,
}
impl AnamnesisAppendReceipt {
    pub fn committed(
        room: RoomKey,
        title: String,
        rep_number: u32,
        backup: BackupOutcome,
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
            backup,
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
    /// The file backup that followed the PostgreSQL commit.
    pub const fn backup(&self) -> &BackupOutcome {
        &self.backup
    }
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

/// One write door with two operations: a new drawer, or a lived repetition
/// appended to an existing cycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnamnesisWriteRequest {
    Add(AnamnesisAddRequest),
    AppendRep(AnamnesisAppendRequest),
}

use std::fmt;

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
    InvalidCanon { field: String, message: String },
    InvalidGiga { field: String, message: String },
    InvalidGigaTransition { from: String, to: String },
    InvalidGigaHash { field: String },
    InvalidGigaScore { field: String, value: f64 },
    GigaProofNotSource,
    GigaScopeViolation,
    GigaPointerOnly,
    UnknownGigaValue { field: String, value: String },
    InvalidPaperBoat { field: String, message: String },
    InvalidLessonTrigger(String),
    InvalidBackupReceipt { field: String, message: String },
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
            Self::InvalidCanon { field, message } => {
                write!(f, "invalid canon {field}: {message}")
            }
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
            Self::InvalidPaperBoat { field, message } => {
                write!(f, "invalid paper boat {field}: {message}")
            }
            Self::InvalidBackupReceipt { field, message } => {
                write!(f, "invalid backup receipt {field}: {message}")
            }
            Self::InvalidLessonTrigger(message) => write!(f, "invalid lesson trigger: {message}"),
        }
    }
}

impl std::error::Error for DomainError {}

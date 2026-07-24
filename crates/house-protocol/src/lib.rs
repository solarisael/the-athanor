//! Newline-delimited JSON wire protocol, version 1.

use house_core::{
    AnamnesisActivation, AnamnesisAddDetails, AnamnesisAddRequest, AnamnesisAppendReceipt,
    AnamnesisAppendRequest, AnamnesisFidelity, AnamnesisKind, AnamnesisReadMode,
    AnamnesisReadRequest, AnamnesisReceipt, AnamnesisSeedRep, ClusterMaintenanceOperation,
    ClusterMaintenanceRequest, RecallRequest, RememberKind, RememberLessonDetails,
    RememberMemoryDetails, RememberReceipt, RememberRequest, RoomKey,
};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{DeserializeOwned, Error as DeError},
};
use serde_json::{Map, Value};
use std::fmt;

pub const PROTOCOL_VERSION: u8 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RequestEnvelope {
    pub protocol: u8,
    pub id: String,
    pub method: String,
    pub params: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RememberParams {
    pub room: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub threads: Vec<String>,
    #[serde(default)]
    pub supersedes: Vec<String>,
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub voice: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default, rename = "proofPattern")]
    pub proof_pattern: Option<String>,
    #[serde(default, rename = "triggerContext")]
    pub trigger_context: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_backup")]
    pub backup: bool,
}

fn default_backup() -> bool {
    true
}

fn default_semantic_top_k() -> u32 {
    8
}
fn default_semantic_min_similarity() -> f64 {
    0.50
}
fn default_content_top_k() -> u32 {
    8
}
fn default_content_min_similarity() -> f64 {
    0.30
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallParams {
    pub room: String,
    pub query: String,
    #[serde(default = "default_semantic_top_k")]
    pub semantic_top_k: u32,
    #[serde(default = "default_semantic_min_similarity")]
    pub semantic_min_similarity: f64,
    #[serde(default = "default_content_top_k")]
    pub content_top_k: u32,
    #[serde(default = "default_content_min_similarity")]
    pub content_min_similarity: f64,
}

fn deserialize_unit_fraction<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = f64::deserialize(deserializer)?;
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(D::Error::custom("must be finite and between 0 and 1"))
    }
}
fn default_cluster_k() -> u32 {
    8
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ClusterMaintenanceParams {
    pub room: String,
    pub operation: String,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub if_stale: bool,
    #[serde(default = "default_cluster_k")]
    pub k: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ClusterStalenessTelemetry {
    pub built_at: Option<String>,
    pub chunks_since_build: u64,
    #[serde(deserialize_with = "deserialize_unit_fraction")]
    pub fraction_unseen: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ClusterResonanceTelemetry {
    pub profile: Vec<ClusterProfileEntry>,
    pub hot: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ClusterProfileEntry {
    pub label: String,
    #[serde(deserialize_with = "deserialize_unit_fraction")]
    pub activation: f64,
    pub member_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallResult {
    pub ok: bool,
    pub query: String,
    pub found: bool,
    pub source: String,
    #[serde(rename = "retrievalCandidates")]
    pub retrieval_candidates: Vec<Value>,
    #[serde(rename = "canonMatches")]
    pub canon_matches: Vec<Value>,
    #[serde(rename = "semanticChunks")]
    pub semantic_chunks: Vec<Value>,
    #[serde(rename = "contentChunks")]
    pub content_chunks: Vec<Value>,
    #[serde(rename = "dateMatches")]
    pub date_matches: Vec<Value>,
    #[serde(rename = "queryDates")]
    pub query_dates: Vec<Value>,
    pub taxonomy: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clusters: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "clusterStaleness")]
    pub cluster_staleness: Option<ClusterStalenessTelemetry>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "clusterResonance")]
    pub cluster_resonance: Option<ClusterResonanceTelemetry>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "memoryHandle")]
    pub memory_handle: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClusterMaintenanceStalenessResult {
    pub built_at: Option<String>,
    pub clusters: u64,
    pub chunks_total: u64,
    pub chunks_since_build: u64,
    pub fraction_unseen: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClusterSummaryResult {
    pub cluster_id: i64,
    pub label: String,
    pub member_count: u64,
    pub accepted: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClusterMaintenanceStatusResult {
    pub stale: bool,
    pub reason: String,
    pub staleness: ClusterMaintenanceStalenessResult,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClusterMaintenanceResultWire {
    pub ok: bool,
    pub operation: String,
    pub dry_run: bool,
    pub rebuilt: bool,
    pub status: ClusterMaintenanceStatusResult,
    pub clusters: Vec<ClusterSummaryResult>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(bound(deserialize = "T: DeserializeOwned"))]
pub struct ResponseEnvelope<T> {
    pub protocol: u8,
    pub id: String,
    #[serde(flatten)]
    pub payload: ResponsePayload<T>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(untagged)]
pub enum ResponsePayload<T> {
    Result { result: T },
    Error { error: ProtocolErrorBody },
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for ResponsePayload<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut object = Map::<String, Value>::deserialize(deserializer)?;
        let result = object.remove("result");
        let error = object.remove("error");
        if !object.is_empty() || result.is_some() == error.is_some() {
            return Err(D::Error::custom(
                "response must contain exactly one result or error branch",
            ));
        }
        match (result, error) {
            (Some(value), None) => serde_json::from_value(value)
                .map(|result| Self::Result { result })
                .map_err(D::Error::custom),
            (None, Some(value)) => serde_json::from_value(value)
                .map(|error| Self::Error { error })
                .map_err(D::Error::custom),
            _ => unreachable!(),
        }
    }
}

/// A version-1-compatible error body.
///
/// `details` intentionally remains raw JSON: old peers may send arbitrary diagnostic
/// objects, and readers that only understand the legacy error shape must keep working.
/// New producers should prefer [`DiagnosticDetails`] through [`ProtocolErrorBodyBuilder`].
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProtocolErrorBody {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

/// Stable broad diagnostic category.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCategory {
    Input,
    Transport,
    Protocol,
    Configuration,
    Database,
    Embedding,
    Filesystem,
    Backup,
    Authorization,
    Operation,
    Reconciliation,
    Internal,
}

/// The precise stage at which an operation failed.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticStage {
    Validation,
    Spawn,
    Startup,
    RequestWrite,
    RequestParse,
    ConfigurationLoad,
    DatabaseConnect,
    DatabaseQuery,
    EmbeddingRequest,
    Transaction,
    Backup,
    ResponseEncode,
    Reconciliation,
    Shutdown,
}

/// The component and source location that own a failure.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticOwner {
    pub component: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

impl DiagnosticOwner {
    pub fn new(component: impl Into<String>) -> Self {
        Self {
            component: redact_diagnostic_text(component.into()),
            path: None,
            symbol: None,
        }
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(redact_diagnostic_text(path.into()));
        self
    }

    pub fn symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(redact_diagnostic_text(symbol.into()));
        self
    }
}

/// A machine-readable record supporting a diagnostic conclusion.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticEvidence {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl DiagnosticEvidence {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            summary: None,
            data: None,
        }
    }

    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(redact_diagnostic_text(summary.into()));
        self
    }

    pub fn data(mut self, data: Value) -> Self {
        self.data = Some(redact_diagnostic_value(data));
        self
    }
}

/// Kinds of exact targets an AI or operator can inspect.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticTargetKind {
    File,
    Symbol,
    Endpoint,
    Migration,
    Table,
    RequestField,
    Service,
}

/// An exact source, schema, request, or service inspection target.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticTarget {
    pub kind: DiagnosticTargetKind,
    pub value: String,
}

impl DiagnosticTarget {
    pub fn new(kind: DiagnosticTargetKind, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: redact_diagnostic_text(value.into()),
        }
    }
}

/// An ordered, machine-actionable diagnostic follow-up.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticNextCheck {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<DiagnosticTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<Value>,
}

impl DiagnosticNextCheck {
    pub fn new(action: impl Into<String>) -> Self {
        Self {
            action: redact_diagnostic_text(action.into()),
            target: None,
            expected: None,
        }
    }

    pub fn target(mut self, target: DiagnosticTarget) -> Self {
        self.target = Some(target);
        self
    }

    pub fn expected(mut self, expected: Value) -> Self {
        self.expected = Some(redact_diagnostic_value(expected));
        self
    }
}

/// The known write outcome when an operation stops.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticWriteOutcome {
    NotStarted,
    RolledBack,
    Committed,
    Unknown,
}

/// The safe retry policy for an operation.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticRetry {
    SafeNow,
    AfterChange,
    ReconcileFirst,
    Never,
}

/// Write and retry facts needed to safely handle a failed request.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticExecution {
    pub request_dispatched: bool,
    pub write_outcome: DiagnosticWriteOutcome,
    pub retry: DiagnosticRetry,
}

impl DiagnosticExecution {
    pub const fn new(
        request_dispatched: bool,
        write_outcome: DiagnosticWriteOutcome,
        retry: DiagnosticRetry,
    ) -> Self {
        Self {
            request_dispatched,
            write_outcome,
            retry,
        }
    }
}

/// Typed fields in the extensible version-1 `error.details` object.
///
/// Unknown fields are retained in `additional`, allowing old and independently
/// evolving producers to remain readable. Fact values passed through the builders
/// are redacted before they reach the wire.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct DiagnosticDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<DiagnosticCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<DiagnosticStage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<DiagnosticOwner>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub evidence: Vec<DiagnosticEvidence>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub targets: Vec<DiagnosticTarget>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub next_checks: Vec<DiagnosticNextCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<DiagnosticExecution>,
    #[serde(flatten)]
    pub additional: Map<String, Value>,
}

impl DiagnosticDetails {
    pub fn new(category: DiagnosticCategory, stage: DiagnosticStage) -> Self {
        Self {
            category: Some(category),
            stage: Some(stage),
            ..Self::default()
        }
    }

    pub fn operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = Some(redact_diagnostic_text(operation.into()));
        self
    }

    pub fn owner(mut self, owner: DiagnosticOwner) -> Self {
        self.owner = Some(owner);
        self
    }

    pub fn expected(mut self, expected: Value) -> Self {
        self.expected = Some(redact_diagnostic_value(expected));
        self
    }

    pub fn observed(mut self, observed: Value) -> Self {
        self.observed = Some(redact_diagnostic_value(observed));
        self
    }

    pub fn evidence(mut self, evidence: DiagnosticEvidence) -> Self {
        self.evidence.push(evidence);
        self
    }

    pub fn target(mut self, target: DiagnosticTarget) -> Self {
        self.targets.push(target);
        self
    }

    pub fn next_check(mut self, next_check: DiagnosticNextCheck) -> Self {
        self.next_checks.push(next_check);
        self
    }

    pub fn execution(mut self, execution: DiagnosticExecution) -> Self {
        self.execution = Some(execution);
        self
    }

    pub fn additional(mut self, key: impl Into<String>, value: Value) -> Self {
        self.additional
            .insert(key.into(), redact_diagnostic_value(value));
        self
    }
}

/// Builds a backward-compatible protocol error body without exposing raw diagnostic
/// facts accidentally.
#[derive(Clone, Debug)]
pub struct ProtocolErrorBodyBuilder {
    body: ProtocolErrorBody,
}

impl ProtocolErrorBodyBuilder {
    pub fn retryable(mut self, retryable: bool) -> Self {
        self.body.retryable = retryable;
        self
    }

    pub fn details(mut self, details: Value) -> Self {
        self.body.details = Some(redact_diagnostic_value(details));
        self
    }

    pub fn diagnostics(mut self, diagnostics: DiagnosticDetails) -> Self {
        self.body.details = Some(redact_diagnostic_value(
            serde_json::to_value(diagnostics)
                .expect("serializing diagnostic details containing JSON values cannot fail"),
        ));
        self
    }

    pub fn build(self) -> ProtocolErrorBody {
        self.body
    }
}

impl ProtocolErrorBody {
    /// Starts an application error. Application code can add typed diagnostics before
    /// calling [`ProtocolErrorBodyBuilder::build`].
    pub fn application(
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> ProtocolErrorBodyBuilder {
        ProtocolErrorBodyBuilder {
            body: Self {
                code: code.into(),
                message: message.into(),
                retryable: false,
                details: None,
            },
        }
    }

    /// Starts a builder for a protocol decoding error.
    pub fn protocol(error: ProtocolError) -> ProtocolErrorBodyBuilder {
        ProtocolErrorBodyBuilder { body: error.into() }
    }

    /// Decodes the optional typed diagnostic extension without changing legacy raw
    /// `details` handling. Unknown detail keys are preserved by `DiagnosticDetails`.
    pub fn diagnostics(&self) -> Option<Result<DiagnosticDetails, serde_json::Error>> {
        self.details
            .as_ref()
            .map(|details| serde_json::from_value(details.clone()))
    }
}

fn redact_diagnostic_text(value: String) -> String {
    let lowercase = value.to_ascii_lowercase();
    let has_authenticated_url = lowercase
        .split_once("://")
        .is_some_and(|(_, rest)| rest.split('/').next().is_some_and(|authority| authority.contains('@')));
    if has_authenticated_url
        || lowercase.starts_with("bearer ")
        || lowercase.starts_with("basic ")
        || lowercase.contains("authorization:")
        || lowercase.contains("token=")
        || lowercase.contains("password=")
    {
        "[redacted]".into()
    } else {
        value
    }
}

fn redact_diagnostic_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(redact_diagnostic_value)
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    if is_sensitive_diagnostic_key(&key) {
                        (key, Value::String("[redacted]".into()))
                    } else {
                        (key, redact_diagnostic_value(value))
                    }
                })
                .collect(),
        ),
        Value::String(value) => Value::String(redact_diagnostic_text(value)),
        value => value,
    }
}

fn is_sensitive_diagnostic_key(key: &str) -> bool {
    let normalized = key
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(char::from)
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "password"
            | "passwd"
            | "secret"
            | "token"
            | "authorization"
            | "authorizationheader"
            | "cookie"
            | "setcookie"
            | "apikey"
            | "privatekey"
            | "databaseurl"
            | "headers"
            | "body"
    ) || normalized.ends_with("password")
        || normalized.ends_with("passwd")
        || normalized.ends_with("secret")
        || normalized.ends_with("token")
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct RememberResult {
    #[serde(skip_serializing_if = "is_zero")]
    pub memory_id: u64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub room: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lesson_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub durable: bool,
    pub authority: String,
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RememberResultWire {
    #[serde(default)]
    memory_id: Option<u64>,
    #[serde(default)]
    room: Option<String>,
    #[serde(default)]
    source_path: Option<String>,
    #[serde(default)]
    lesson_id: Option<u64>,
    #[serde(default)]
    kind: Option<String>,
    durable: Option<bool>,
    authority: Option<String>,
    warnings: Option<Vec<String>>,
}

impl<'de> Deserialize<'de> for RememberResult {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RememberResultWire::deserialize(deserializer)?;
        let has_memory =
            wire.memory_id.is_some() || wire.room.is_some() || wire.source_path.is_some();
        let has_lesson = wire.lesson_id.is_some() || wire.kind.is_some();
        let durable = wire
            .durable
            .ok_or_else(|| D::Error::missing_field("durable"))?;
        let authority = wire
            .authority
            .ok_or_else(|| D::Error::missing_field("authority"))?;
        let warnings = wire
            .warnings
            .ok_or_else(|| D::Error::missing_field("warnings"))?;
        match (has_memory, has_lesson) {
            (true, true) => Err(D::Error::custom(
                "memory and lesson receipt fields cannot be mixed",
            )),
            (true, false) => Ok(Self {
                memory_id: wire
                    .memory_id
                    .ok_or_else(|| D::Error::missing_field("memory_id"))?,
                room: wire.room.ok_or_else(|| D::Error::missing_field("room"))?,
                source_path: wire
                    .source_path
                    .ok_or_else(|| D::Error::missing_field("source_path"))?,
                lesson_id: None,
                kind: None,
                durable,

                authority,
                warnings,
            }),
            (false, true) => Ok(Self {
                memory_id: 0,
                room: String::new(),
                source_path: String::new(),
                lesson_id: Some(
                    wire.lesson_id
                        .ok_or_else(|| D::Error::missing_field("lesson_id"))?,
                ),
                kind: Some(wire.kind.ok_or_else(|| D::Error::missing_field("kind"))?),
                durable,
                authority,
                warnings,
            }),
            (false, false) => Err(D::Error::custom(
                "receipt must contain memory or lesson fields",
            )),
        }
    }
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

impl TryFrom<RecallParams> for RecallRequest {
    type Error = ProtocolError;

    fn try_from(params: RecallParams) -> Result<Self, Self::Error> {
        let room =
            RoomKey::new(params.room).map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        RecallRequest::new(
            room,
            params.query,
            params.semantic_top_k,
            params.semantic_min_similarity,
            params.content_top_k,
            params.content_min_similarity,
        )
        .map_err(|e| ProtocolError::InvalidParams(e.to_string()))
    }
}
impl TryFrom<ClusterMaintenanceParams> for ClusterMaintenanceRequest {
    type Error = ProtocolError;
    fn try_from(p: ClusterMaintenanceParams) -> Result<Self, Self::Error> {
        let room = RoomKey::new(p.room).map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        let operation = ClusterMaintenanceOperation::parse(&p.operation)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        Self::new(room, operation, p.dry_run, p.if_stale, p.k)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    Malformed(String),
    ProtocolMismatch(u8),
    UnknownMethod(String),
    InvalidParams(String),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "malformed request: {message}"),
            Self::ProtocolMismatch(version) => write!(f, "unsupported protocol version: {version}"),
            Self::UnknownMethod(method) => write!(f, "unknown method: {method}"),
            Self::InvalidParams(error) => write!(f, "invalid parameters: {error}"),
        }
    }
}
impl std::error::Error for ProtocolError {}

impl From<ProtocolError> for ProtocolErrorBody {
    fn from(error: ProtocolError) -> Self {
        let (code, retryable) = match &error {
            ProtocolError::Malformed(_) => ("malformed_request", false),
            ProtocolError::ProtocolMismatch(_) => ("protocol_mismatch", false),
            ProtocolError::UnknownMethod(_) => ("unknown_method", false),
            ProtocolError::InvalidParams(_) => ("invalid_params", false),
        };
        Self {
            code: code.into(),
            message: error.to_string(),
            retryable,
            details: None,
        }
    }
}

impl TryFrom<RememberParams> for RememberRequest {
    type Error = ProtocolError;
    fn try_from(params: RememberParams) -> Result<Self, Self::Error> {
        let room =
            RoomKey::new(params.room).map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        let kind = RememberKind::parse(&params.kind)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        if kind.is_lesson()
            && (params.source_path.is_some()
                || !params.threads.is_empty()
                || !params.supersedes.is_empty())
        {
            return Err(ProtocolError::InvalidParams(
                "memory-only fields are not valid for lessons".into(),
            ));
        }
        if !kind.is_lesson()
            && (params.shape.is_some()
                || params.voice.is_some()
                || params.scope.is_some()
                || params.project.is_some()
                || params.proof_pattern.is_some()
                || params.trigger_context.is_some()
                || !params.tags.is_empty())
        {
            return Err(ProtocolError::InvalidParams(
                "lesson-only fields are not valid for memory".into(),
            ));
        }
        let mut supersedes = Vec::with_capacity(params.supersedes.len());
        for raw in params.supersedes {
            if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(ProtocolError::InvalidParams(format!(
                    "supersedes ID must be a positive PostgreSQL BIGINT decimal: {raw}"
                )));
            }
            let id = raw
                .parse::<i64>()
                .ok()
                .filter(|&id| id > 0)
                .ok_or_else(|| {
                    ProtocolError::InvalidParams(format!(
                        "supersedes ID must be a positive PostgreSQL BIGINT decimal: {raw}"
                    ))
                })? as u64;
            if !supersedes.contains(&id) {
                supersedes.push(id);
            }
        }
        let result = if kind.is_lesson() {
            RememberRequest::new_lesson(
                room,
                kind,
                params.title,
                params.body,
                RememberLessonDetails {
                    backup: params.backup,
                    shape: params.shape,
                    voice: params.voice,
                    scope: params.scope,
                    project: params.project,
                    proof_pattern: params.proof_pattern,
                    trigger_context: params.trigger_context,
                    tags: params.tags,
                },
            )
        } else {
            RememberRequest::new_memory(
                room,
                params.title,
                params.body,
                RememberMemoryDetails {
                    source_path: params.source_path,
                    threads: params.threads,
                    supersedes,
                    backup: params.backup,
                },
            )
        };
        result.map_err(|e| ProtocolError::InvalidParams(e.to_string()))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AnamnesisParams {
    pub room: String,
    pub mode: String,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default = "default_anamnesis_limit")]
    pub limit: u32,
}
fn default_anamnesis_limit() -> u32 {
    10
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AnamnesisWriteParams {
    pub operation: String,
    #[serde(default)]
    pub room: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub fidelity: Option<String>,
    #[serde(default)]
    pub activation: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub dormant: bool,
    #[serde(default)]
    pub ramp: Option<String>,
    #[serde(default)]
    pub counsel: Option<String>,
    #[serde(default)]
    pub peak: Option<String>,
    #[serde(default)]
    pub beginning: Option<String>,
    #[serde(default)]
    pub verify_note: Option<String>,
    #[serde(default)]
    pub canon: Vec<String>,
    #[serde(default)]
    pub source_paths: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub allow_empty_cycle: bool,
    #[serde(default)]
    pub seed_rep: Option<AnamnesisSeedRepParams>,
    #[serde(default)]
    pub rep_number: Option<u32>,
    #[serde(default)]
    pub occurred_on: Option<String>,
    #[serde(default)]
    pub how_it_went: Option<String>,
    #[serde(default)]
    pub portal_pull: Option<String>,
    #[serde(default)]
    pub lighter: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AnamnesisSeedRepParams {
    pub number: u32,
    #[serde(default)]
    pub occurred_on: Option<String>,
    pub how_it_went: String,
    pub portal_pull: String,
    pub lighter: String,
}

impl TryFrom<AnamnesisParams> for AnamnesisReadRequest {
    type Error = ProtocolError;
    fn try_from(p: AnamnesisParams) -> Result<Self, Self::Error> {
        let room = RoomKey::for_anamnesis(p.room)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        let mode = AnamnesisReadMode::parse(&p.mode)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        Self::new(room, mode, p.query, p.limit)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))
    }
}
impl TryFrom<AnamnesisWriteParams> for AnamnesisAddRequest {
    type Error = ProtocolError;
    fn try_from(p: AnamnesisWriteParams) -> Result<Self, Self::Error> {
        if p.operation != "add" {
            return Err(ProtocolError::InvalidParams("operation is not add".into()));
        }
        let room = RoomKey::for_anamnesis(
            p.room
                .ok_or_else(|| ProtocolError::InvalidParams("add requires room".into()))?,
        )
        .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        let kind = AnamnesisKind::parse(
            &p.kind
                .ok_or_else(|| ProtocolError::InvalidParams("add requires kind".into()))?,
        )
        .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        let fidelity = AnamnesisFidelity::parse(
            &p.fidelity
                .ok_or_else(|| ProtocolError::InvalidParams("add requires fidelity".into()))?,
        )
        .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        let activation = AnamnesisActivation::parse(
            &p.activation
                .ok_or_else(|| ProtocolError::InvalidParams("add requires activation".into()))?,
        )
        .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        if kind == AnamnesisKind::Pillar && p.seed_rep.is_some() {
            return Err(ProtocolError::InvalidParams(
                "pillars cannot include seedRep".into(),
            ));
        }
        let seed = p
            .seed_rep
            .map(|s| {
                AnamnesisSeedRep::new(
                    s.number,
                    s.occurred_on,
                    s.how_it_went,
                    s.portal_pull,
                    s.lighter,
                )
            })
            .transpose()
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        AnamnesisAddRequest::new(
            room,
            kind,
            fidelity,
            activation,
            p.title.unwrap_or_default(),
            AnamnesisAddDetails {
                shape: p.shape,
                dormant: p.dormant,
                ramp: p.ramp.unwrap_or_default(),
                counsel: p.counsel,
                peak: p.peak,
                beginning: p.beginning,
                verify_note: p.verify_note,
                canon: p.canon,
                source_paths: p.source_paths,
                tags: p.tags,
                allow_empty_cycle: p.allow_empty_cycle,
                seed_rep: seed,
            },
        )
        .map_err(|e| ProtocolError::InvalidParams(e.to_string()))
    }
}
impl AnamnesisWriteParams {
    pub fn append_request(self) -> Result<AnamnesisAppendRequest, ProtocolError> {
        if self.operation != "append-rep" {
            return Err(ProtocolError::InvalidParams(
                "operation is not append-rep".into(),
            ));
        }
        let room = RoomKey::for_anamnesis(
            self.room
                .ok_or_else(|| ProtocolError::InvalidParams("append-rep requires room".into()))?,
        )
        .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        let rep = AnamnesisSeedRep::new(
            self.rep_number.unwrap_or(0),
            self.occurred_on,
            self.how_it_went.unwrap_or_default(),
            self.portal_pull.unwrap_or_default(),
            self.lighter.unwrap_or_default(),
        )
        .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        AnamnesisAppendRequest::new(room, self.title.unwrap_or_default(), rep, self.source_paths)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))
    }
}

impl RequestEnvelope {
    pub fn remember_request(self) -> Result<RememberRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "remember" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        let params: RememberParams = serde_json::from_value(self.params)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        params.try_into()
    }
    pub fn recall_request(self) -> Result<RecallRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "recall" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        let params: RecallParams = serde_json::from_value(self.params)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        params.try_into()
    }
    pub fn cluster_maintenance_request(self) -> Result<ClusterMaintenanceRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "cluster_maintenance" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<ClusterMaintenanceParams>(self.params)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?
            .try_into()
    }
    pub fn anamnesis_request(self) -> Result<AnamnesisReadRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "anamnesis" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<AnamnesisParams>(self.params)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?
            .try_into()
    }
    pub fn anamnesis_add_request(self) -> Result<AnamnesisAddRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "anamnesis_write" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<AnamnesisWriteParams>(self.params)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?
            .try_into()
    }
    pub fn anamnesis_append_request(self) -> Result<AnamnesisAppendRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "anamnesis_write" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<AnamnesisWriteParams>(self.params)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?
            .append_request()
    }
    pub fn parse_line(line: &str) -> Result<Self, ProtocolError> {
        serde_json::from_str(line).map_err(|e| ProtocolError::Malformed(e.to_string()))
    }
}

impl From<RememberReceipt> for RememberResult {
    fn from(receipt: RememberReceipt) -> Self {
        Self {
            memory_id: receipt.memory_id(),
            room: if receipt.kind().is_lesson() {
                String::new()
            } else {
                receipt.room().to_string()
            },
            source_path: if receipt.kind().is_lesson() {
                String::new()
            } else {
                receipt.source_path().to_owned()
            },
            lesson_id: (receipt.lesson_id() != 0).then_some(receipt.lesson_id()),
            kind: receipt
                .kind()
                .is_lesson()
                .then(|| receipt.kind().as_str().to_owned()),
            durable: receipt.durable(),
            authority: "postgres".into(),
            warnings: receipt.warnings().to_vec(),
        }
    }
}

pub fn success<T>(id: impl Into<String>, result: T) -> ResponseEnvelope<T> {
    ResponseEnvelope {
        protocol: PROTOCOL_VERSION,
        id: id.into(),
        payload: ResponsePayload::Result { result },
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnamnesisResult {
    pub ok: bool,
    pub mode: String,
    pub room: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    pub found: bool,
    #[serde(default)]
    pub entries: Vec<Value>,
    #[serde(default)]
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnamnesisWriteResult {
    pub ok: bool,
    pub operation: String,
    pub room: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rep_number: Option<u32>,
    pub durable: bool,
    pub authority: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}
impl From<AnamnesisReceipt> for AnamnesisWriteResult {
    fn from(r: AnamnesisReceipt) -> Self {
        Self {
            ok: true,
            operation: "add".into(),
            room: r.room().to_string(),
            title: Some(r.title().into()),
            kind: Some(r.kind().as_str().into()),
            rep_number: None,
            durable: r.durable(),
            authority: "postgres".into(),
            warnings: r.warnings().to_vec(),
        }
    }
}
impl From<AnamnesisAppendReceipt> for AnamnesisWriteResult {
    fn from(r: AnamnesisAppendReceipt) -> Self {
        Self {
            ok: true,
            operation: "append-rep".into(),
            room: r.room().to_string(),
            title: Some(r.title().into()),
            kind: None,
            rep_number: Some(r.rep_number()),
            durable: r.durable(),
            authority: "postgres".into(),
            warnings: r.warnings().to_vec(),
        }
    }
}
pub type AnamnesisReadResult = AnamnesisResult;
pub type AnamnesisReceiptResult = AnamnesisWriteResult;

pub fn error<T>(id: impl Into<String>, error: ProtocolError) -> ResponseEnvelope<T> {
    ResponseEnvelope {
        protocol: PROTOCOL_VERSION,
        id: id.into(),
        payload: ResponsePayload::Error {
            error: error.into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_details_round_trip_with_machine_actions() {
        let target = DiagnosticTarget::new(
            DiagnosticTargetKind::File,
            "crates/house-substrate/src/lib.rs",
        );
        let details = DiagnosticDetails::new(
            DiagnosticCategory::Database,
            DiagnosticStage::Transaction,
        )
        .operation("remember")
        .owner(
            DiagnosticOwner::new("house-substrate")
                .path("crates/house-substrate/src/lib.rs")
                .symbol("Store::remember"),
        )
        .expected(serde_json::json!({"transaction": "committed"}))
        .observed(serde_json::json!({
            "transaction": "unknown",
            "password": "do-not-leak",
        }))
        .evidence(
            DiagnosticEvidence::new("database_error")
                .summary("commit result was not returned")
                .data(serde_json::json!({"sqlstate": "08006"})),
        )
        .target(target.clone())
        .next_check(
            DiagnosticNextCheck::new("inspect")
                .target(target)
                .expected(serde_json::json!({"symbol_exists": true})),
        )
        .execution(DiagnosticExecution::new(
            true,
            DiagnosticWriteOutcome::Unknown,
            DiagnosticRetry::ReconcileFirst,
        ));
        let error = ProtocolErrorBody::application("database_failure", "write outcome unknown")
            .retryable(false)
            .diagnostics(details)
            .build();
        let response = ResponseEnvelope::<Value> {
            protocol: PROTOCOL_VERSION,
            id: "d1".into(),
            payload: ResponsePayload::Error { error },
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["protocol"], PROTOCOL_VERSION);
        assert_eq!(json["error"]["details"]["category"], "database");
        assert_eq!(json["error"]["details"]["stage"], "transaction");
        assert_eq!(
            json["error"]["details"]["execution"]["write_outcome"],
            "unknown"
        );
        assert_eq!(
            json["error"]["details"]["execution"]["retry"],
            "reconcile_first"
        );
        assert_eq!(
            json["error"]["details"]["observed"]["password"],
            "[redacted]"
        );

        let decoded: ResponseEnvelope<Value> = serde_json::from_value(json).unwrap();
        let ResponsePayload::Error { error } = decoded.payload else {
            panic!("expected error response");
        };
        let details = error.diagnostics().unwrap().unwrap();
        assert_eq!(details.category, Some(DiagnosticCategory::Database));
        assert_eq!(details.stage, Some(DiagnosticStage::Transaction));
        assert_eq!(details.targets.len(), 1);
        assert_eq!(details.next_checks.len(), 1);
        assert_eq!(
            details.execution,
            Some(DiagnosticExecution::new(
                true,
                DiagnosticWriteOutcome::Unknown,
                DiagnosticRetry::ReconcileFirst,
            ))
        );
    }

    #[test]
    fn diagnostic_redaction_preserves_environment_presence_without_secret_values() {
        let details = DiagnosticDetails::new(
            DiagnosticCategory::Configuration,
            DiagnosticStage::ConfigurationLoad,
        )
        .observed(serde_json::json!({
            "environment": {
                "PGHOST": "present",
                "PGPASSWORD": "secret-value",
                "API_TOKEN": "secret-value",
            },
            "env": {
                "DATABASE_URL": "postgres://user:password@localhost/database",
                "PGDATABASE": "present",
            },
        }));

        let observed = details.observed.unwrap();
        assert_eq!(observed["environment"]["PGHOST"], "present");
        assert_eq!(observed["environment"]["PGPASSWORD"], "[redacted]");
        assert_eq!(observed["environment"]["API_TOKEN"], "[redacted]");
        assert_eq!(observed["env"]["DATABASE_URL"], "[redacted]");
        assert_eq!(observed["env"]["PGDATABASE"], "present");
    }

    #[test]
    fn diagnostic_details_are_optional_and_old_details_remain_compatible() {
        let omitted = ProtocolErrorBody::application("app_failure", "failed").build();
        assert!(
            !serde_json::to_value(&omitted)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("details")
        );
        assert!(omitted.diagnostics().is_none());
        let protocol = ProtocolErrorBody::protocol(ProtocolError::InvalidParams("bad".into()))
            .build();
        assert_eq!(protocol.code, "invalid_params");
        assert_eq!(protocol.message, "invalid parameters: bad");
        assert!(!protocol.retryable);
        assert!(protocol.details.is_none());

        let legacy: ResponseEnvelope<Value> = serde_json::from_value(serde_json::json!({
            "protocol": 1,
            "id": "legacy",
            "error": {
                "code": "legacy_failure",
                "message": "old producer",
                "retryable": true,
                "details": {
                    "legacy_pointer": "subsystem/old",
                    "unrecognized_fact": 7
                }
            }
        }))
        .unwrap();
        let ResponsePayload::Error { error } = legacy.payload else {
            panic!("expected error response");
        };
        assert_eq!(error.code, "legacy_failure");
        assert_eq!(error.message, "old producer");
        assert!(error.retryable);
        let diagnostics = error.diagnostics().unwrap().unwrap();
        assert_eq!(
            diagnostics.additional["legacy_pointer"],
            serde_json::json!("subsystem/old")
        );
        assert_eq!(diagnostics.additional["unrecognized_fact"], 7);

        let arbitrary_details: ResponseEnvelope<Value> = serde_json::from_value(serde_json::json!({
            "protocol": 1,
            "id": "older",
            "error": {
                "code": "old_failure",
                "message": "old details can be any JSON",
                "retryable": false,
                "details": ["legacy", "array"]
            }
        }))
        .unwrap();
        let ResponsePayload::Error { error } = arbitrary_details.payload else {
            panic!("expected error response");
        };
        assert_eq!(error.details, Some(serde_json::json!(["legacy", "array"])));
        assert!(error.diagnostics().unwrap().is_err());
    }

    #[test]
    fn diagnostic_execution_enum_wire_values_are_stable() {
        for (write_outcome, wire) in [
            (DiagnosticWriteOutcome::NotStarted, "not_started"),
            (DiagnosticWriteOutcome::RolledBack, "rolled_back"),
            (DiagnosticWriteOutcome::Committed, "committed"),
            (DiagnosticWriteOutcome::Unknown, "unknown"),
        ] {
            assert_eq!(serde_json::to_value(write_outcome).unwrap(), wire);
            assert_eq!(
                serde_json::from_value::<DiagnosticWriteOutcome>(serde_json::json!(wire))
                    .unwrap(),
                write_outcome
            );
        }
        for (retry, wire) in [
            (DiagnosticRetry::SafeNow, "safe_now"),
            (DiagnosticRetry::AfterChange, "after_change"),
            (DiagnosticRetry::ReconcileFirst, "reconcile_first"),
            (DiagnosticRetry::Never, "never"),
        ] {
            assert_eq!(serde_json::to_value(retry).unwrap(), wire);
            assert_eq!(
                serde_json::from_value::<DiagnosticRetry>(serde_json::json!(wire)).unwrap(),
                retry
            );
        }
    }

    #[test]
    fn response_envelope_rejects_mutually_exclusive_payloads() {
        for payload in [
            serde_json::json!({"protocol": 1, "id": "both", "result": {}, "error": {}}),
            serde_json::json!({"protocol": 1, "id": "neither"}),
        ] {
            assert!(serde_json::from_value::<ResponseEnvelope<Value>>(payload).is_err());
        }
    }

    #[test]
    fn exact_v1_request_and_response_shape() {
        let line = r#"{"protocol":1,"id":"x","method":"remember","params":{"room":"lab","kind":"memory","title":"T","body":"B","backup":true}}"#;
        let request = RequestEnvelope::parse_line(line).unwrap();
        assert_eq!(request.remember_request().unwrap().room().as_str(), "lab");
        let json = serde_json::to_string(&success(
            "x",
            RememberResult {
                memory_id: 4,
                room: "lab".into(),
                source_path: "mem.md".into(),
                lesson_id: None,
                kind: None,
                durable: true,
                authority: "postgres".into(),
                warnings: vec![],
            },
        ))
        .unwrap();
        assert_eq!(
            json,
            r#"{"protocol":1,"id":"x","result":{"memory_id":4,"room":"lab","source_path":"mem.md","durable":true,"authority":"postgres","warnings":[]}}"#
        );
    }

    #[test]
    fn anamnesis_accepts_house_and_preserves_query_in_exact_result_json() {
        let request = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"a","method":"anamnesis","params":{"room":"house","mode":"consult","query":"needle"}}"#,
        ).unwrap();
        let parsed = request.anamnesis_request().unwrap();
        assert_eq!(parsed.room().as_str(), "house");
        assert_eq!(parsed.query(), Some("needle"));
        let json = serde_json::to_string(&success(
            "a",
            AnamnesisResult {
                ok: true,
                mode: "consult".into(),
                room: "house".into(),
                query: Some("needle".into()),
                found: true,
                entries: vec![serde_json::json!({"title":"T"})],
                warnings: vec![],
            },
        ))
        .unwrap();
        assert_eq!(
            json,
            r#"{"protocol":1,"id":"a","result":{"ok":true,"mode":"consult","room":"house","query":"needle","found":true,"entries":[{"title":"T"}],"warnings":[]}}"#
        );
        let remember = RequestEnvelope {
            protocol: 1,
            id: "r".into(),
            method: "remember".into(),
            params: serde_json::json!({"room":"house","kind":"memory","title":"T","body":"B"}),
        };
        assert!(remember.remember_request().is_err());
    }

    #[test]
    fn remember_result_deserializes_memory_and_lesson_receipts_without_hybrids() {
        let memory: RememberResult = serde_json::from_value(serde_json::json!({
            "memory_id": 4, "room": "lab", "source_path": "mem.md",
            "durable": true, "authority": "postgres", "warnings": []
        }))
        .unwrap();
        assert_eq!(memory.memory_id, 4);
        assert_eq!(memory.lesson_id, None);

        let lesson: RememberResult = serde_json::from_value(serde_json::json!({
            "lesson_id": 9, "kind": "coding-lesson",
            "durable": true, "authority": "postgres", "warnings": []
        }))
        .unwrap();
        assert_eq!(lesson.lesson_id, Some(9));
        assert_eq!(lesson.kind.as_deref(), Some("coding-lesson"));
        assert_eq!(lesson.memory_id, 0);

        let hybrid = serde_json::json!({
            "memory_id": 4, "room": "lab", "source_path": "mem.md",
            "lesson_id": 9, "kind": "coding-lesson",
            "durable": true, "authority": "postgres", "warnings": []
        });
        assert!(serde_json::from_value::<RememberResult>(hybrid).is_err());
    }
    #[test]
    fn rejects_mismatch_malformed_unknown_and_bad_param_shape() {
        let mismatch = RequestEnvelope {
            protocol: 2,
            id: "x".into(),
            method: "remember".into(),
            params: Value::Null,
        };
        assert!(matches!(
            mismatch.remember_request(),
            Err(ProtocolError::ProtocolMismatch(2))
        ));
        assert!(matches!(
            RequestEnvelope::parse_line("{"),
            Err(ProtocolError::Malformed(_))
        ));
        let unknown = RequestEnvelope {
            protocol: 1,
            id: "x".into(),
            method: "recall".into(),
            params: Value::Null,
        };
        assert!(matches!(
            unknown.remember_request(),
            Err(ProtocolError::UnknownMethod(_))
        ));
        let bad = RequestEnvelope {
            protocol: 1,
            id: "x".into(),
            method: "remember".into(),
            params: serde_json::json!({"room":"lab","kind":"memory","title":"T","body":"B","threads":"x"}),
        };
        assert!(matches!(
            bad.remember_request(),
            Err(ProtocolError::InvalidParams(_))
        ));
    }

    #[test]
    fn rejects_unknown_envelope_and_params_fields() {
        let envelope = r#"{"protocol":1,"id":"x","method":"remember","params":{"room":"lab","kind":"memory","title":"T","body":"B"},"extra":true}"#;
        assert!(matches!(
            RequestEnvelope::parse_line(envelope),
            Err(ProtocolError::Malformed(_))
        ));

        let params = RequestEnvelope {
            protocol: 1,
            id: "x".into(),
            method: "remember".into(),
            params: serde_json::json!({"room":"lab","kind":"memory","title":"T","body":"B","extra":true}),
        };
        assert!(matches!(
            params.remember_request(),
            Err(ProtocolError::InvalidParams(_))
        ));
    }

    #[test]
    fn supersedes_strings_are_positive_postgres_bigints_and_deduplicated() {
        let params = RememberParams {
            room: "lab".into(),
            kind: "memory".into(),
            title: "T".into(),
            body: "B".into(),
            source_path: None,
            threads: vec![],
            supersedes: vec!["41".into(), "42".into(), "41".into()],
            shape: None,
            voice: None,
            scope: None,
            project: None,
            proof_pattern: None,
            trigger_context: None,
            tags: vec![],
            backup: true,
        };
        assert_eq!(
            RememberRequest::try_from(params).unwrap().supersedes(),
            &[41, 42]
        );
        let max = i64::MAX.to_string();
        let params = RememberParams {
            supersedes: vec![max],
            room: "lab".into(),
            kind: "memory".into(),
            title: "T".into(),
            body: "B".into(),
            source_path: None,
            threads: vec![],
            shape: None,
            voice: None,
            scope: None,
            project: None,
            proof_pattern: None,
            trigger_context: None,
            tags: vec![],
            backup: true,
        };
        assert_eq!(
            RememberRequest::try_from(params).unwrap().supersedes(),
            &[i64::MAX as u64]
        );
        for bad in ["0", "+1", "9223372036854775808", "nope"] {
            let params = RememberParams {
                supersedes: vec![bad.into()],
                room: "lab".into(),
                kind: "memory".into(),
                title: "T".into(),
                body: "B".into(),
                source_path: None,
                threads: vec![],
                shape: None,
                voice: None,
                scope: None,
                project: None,
                proof_pattern: None,
                trigger_context: None,
                tags: vec![],
                backup: true,
            };
            assert!(matches!(
                RememberRequest::try_from(params),
                Err(ProtocolError::InvalidParams(_))
            ));
        }
    }

    #[test]
    fn response_requires_exactly_one_branch() {
        let both = r#"{"protocol":1,"id":"x","result":{},"error":{}}"#;
        let neither = r#"{"protocol":1,"id":"x"}"#;
        assert!(serde_json::from_str::<ResponseEnvelope<Value>>(both).is_err());
        assert!(serde_json::from_str::<ResponseEnvelope<Value>>(neither).is_err());
    }
    #[test]
    fn recall_defaults_validate_and_round_trip() {
        let request = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"r","method":"recall","params":{"room":"lab","query":"alpha"}}"#,
        )
        .unwrap();
        let recall = request.recall_request().unwrap();
        assert_eq!(recall.semantic_top_k(), 8);
        assert_eq!(recall.semantic_min_similarity(), 0.50);
        assert_eq!(recall.content_top_k(), 8);
        assert_eq!(recall.content_min_similarity(), 0.30);

        let params: RecallParams =
            serde_json::from_value(serde_json::json!({"room":"lab","query":"alpha"})).unwrap();
        assert_eq!(
            serde_json::to_string(&params).unwrap(),
            r#"{"room":"lab","query":"alpha","semantic_top_k":8,"semantic_min_similarity":0.5,"content_top_k":8,"content_min_similarity":0.3}"#
        );
    }

    #[test]
    fn recall_rejects_bounds_nonfinite_unknown_fields_and_methods() {
        let base = |params| RequestEnvelope {
            protocol: 1,
            id: "r".into(),
            method: "recall".into(),
            params,
        };
        for key in ["semantic_top_k", "content_top_k"] {
            let mut value = serde_json::json!({"room":"lab","query":"x"});
            value[key] = serde_json::json!(0);
            assert!(matches!(
                base(value).recall_request(),
                Err(ProtocolError::InvalidParams(_))
            ));
        }
        let mut value = serde_json::json!({"room":"lab","query":"x"});
        value["semantic_min_similarity"] = serde_json::json!(1.1);
        assert!(base(value).recall_request().is_err());
        let params = RecallParams {
            room: "lab".into(),
            query: "x".into(),
            semantic_top_k: 8,
            semantic_min_similarity: f64::NAN,
            content_top_k: 8,
            content_min_similarity: 0.3,
        };
        assert!(RecallRequest::try_from(params).is_err());
        let unknown = serde_json::json!({"room":"lab","query":"x","extra":true});
        assert!(base(unknown).recall_request().is_err());
        let wrong = RequestEnvelope {
            protocol: 1,
            id: "r".into(),
            method: "other".into(),
            params: serde_json::json!({}),
        };
        assert!(matches!(
            wrong.recall_request(),
            Err(ProtocolError::UnknownMethod(_))
        ));
    }
    #[test]
    fn all_lesson_kinds_validate_defaults_and_receipt_shape() {
        for kind in [
            "coding-lesson",
            "project-lesson",
            "writing-lesson",
            "audio-lesson",
        ] {
            let mut params = serde_json::json!({"room":"lab","kind":kind,"title":"T","body":"B"});
            if kind == "project-lesson" {
                params["project"] = serde_json::json!("app");
            }
            let request = RequestEnvelope {
                protocol: 1,
                id: "l".into(),
                method: "remember".into(),
                params,
            };
            let parsed = request.remember_request().unwrap();
            assert_eq!(parsed.kind().as_str(), kind);
            let receipt = RememberReceipt::committed_lesson(
                9,
                parsed.kind(),
                RoomKey::new("lab").unwrap(),
                vec![],
            )
            .unwrap();
            let json = serde_json::to_string(&success("l", RememberResult::from(receipt))).unwrap();
            assert_eq!(
                json,
                format!(
                    r#"{{"protocol":1,"id":"l","result":{{"lesson_id":9,"kind":"{kind}","durable":true,"authority":"postgres","warnings":[]}}}}"#
                )
            );
        }
    }

    #[test]
    fn lessons_reject_memory_fields_and_require_project() {
        let base = |params| RequestEnvelope {
            protocol: 1,
            id: "l".into(),
            method: "remember".into(),
            params,
        };
        assert!(
            base(serde_json::json!({"room":"lab","kind":"project-lesson","title":"T","body":"B"}))
                .remember_request()
                .is_err()
        );
        assert!(base(serde_json::json!({"room":"lab","kind":"coding-lesson","title":"T","body":"B","threads":["x"]})).remember_request().is_err());
        assert!(base(serde_json::json!({"room":"lab","kind":"writing-lesson","title":"T","body":"B","project":"x"})).remember_request().is_err());
        assert!(base(serde_json::json!({"room":"lab","kind":"project-lesson","title":"T","body":"B","project":"x","shape":"process"})).remember_request().is_ok());
        assert!(base(serde_json::json!({"room":"lab","kind":"audio-lesson","title":"T","body":"B","voice":"narrator"})).remember_request().is_err());
        assert!(
            base(
                serde_json::json!({"room":"lab","kind":"memory","title":"T","body":"B","shape":"x"})
            )
            .remember_request()
            .is_err()
        );
    }
    #[test]
    fn cluster_maintenance_is_strict_and_camel_case() {
        let request = RequestEnvelope::parse_line(r#"{"protocol":1,"id":"c","method":"cluster_maintenance","params":{"room":"lab","operation":"rebuild","dryRun":true,"ifStale":true,"k":40}}"#).unwrap();
        let parsed = request.cluster_maintenance_request().unwrap();
        assert_eq!(parsed.operation(), ClusterMaintenanceOperation::Rebuild);
        assert!(parsed.dry_run());
        assert!(parsed.if_stale());
        assert_eq!(
            serde_json::to_string(&ClusterMaintenanceParams {
                room: "lab".into(),
                operation: "rebuild".into(),
                dry_run: true,
                if_stale: true,
                k: 40
            })
            .unwrap(),
            r#"{"room":"lab","operation":"rebuild","dryRun":true,"ifStale":true,"k":40}"#
        );
        for bad in [
            serde_json::json!({"room":"lab","operation":"rebuild","k":0}),
            serde_json::json!({"room":"lab","operation":"rebuild","k":129}),
            serde_json::json!({"room":"lab","operation":"other","k":4}),
            serde_json::json!({"room":"lab","operation":"rebuild","extra":true}),
        ] {
            assert!(
                RequestEnvelope {
                    protocol: 1,
                    id: "c".into(),
                    method: "cluster_maintenance".into(),
                    params: bad
                }
                .cluster_maintenance_request()
                .is_err()
            );
        }
        assert!(
            RequestEnvelope {
                protocol: 1,
                id: "c".into(),
                method: "recall".into(),
                params: serde_json::json!({})
            }
            .cluster_maintenance_request()
            .is_err()
        );
    }

    #[test]
    fn recall_cluster_telemetry_is_optional_and_compatible() {
        let telemetry: ClusterStalenessTelemetry = serde_json::from_value(
            serde_json::json!({"built_at":null,"chunks_since_build":250,"fraction_unseen":0.05}),
        )
        .unwrap();
        assert_eq!(telemetry.built_at, None);
        let resonance: ClusterResonanceTelemetry = serde_json::from_value(serde_json::json!({"profile":[{"label":"x","activation":0.9,"member_count":2}],"hot":["chunk"]})).unwrap();
        assert_eq!(resonance.profile[0].member_count, 2);
        assert!(serde_json::from_value::<ClusterStalenessTelemetry>(serde_json::json!({"built_at":null,"chunks_since_build":1,"fraction_unseen":0.1,"bad":true})).is_err());
    }
}

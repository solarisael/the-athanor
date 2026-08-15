//! Newline-delimited JSON wire protocol, version 1.
mod host;

pub use host::*;

use house_core::{
    AnamnesisActivation, AnamnesisAddDetails, AnamnesisAddRequest, AnamnesisAppendReceipt,
    AnamnesisAppendRequest, AnamnesisFidelity, AnamnesisKind, AnamnesisReadMode,
    AnamnesisReadRequest, AnamnesisReceipt, AnamnesisSeedRep, CanonAttribution, CanonPointer,
    CanonReadRequest, CanonWriteReceipt, CanonWriteRequest, ClusterMaintenanceOperation,
    ClusterMaintenanceRequest, GigaAuthority, GigaCandidate, GigaCandidateKind,
    GigaClassifierIdentity, GigaCodingLessonPromotionPayload, GigaEvent, GigaEventClaimReceipt,
    GigaEventClaimRequest, GigaEventFinishOutcome, GigaEventFinishReceipt, GigaEventFinishRequest,
    GigaEventReplayReceipt, GigaEventReplayRequest, GigaEventType, GigaLifecycle,
    GigaMemoryPromotionPayload, GigaProjectLessonPromotionPayload, GigaPromotionAuthority,
    GigaPromotionPayload, GigaPromotionReceipt, GigaPromotionRequest, GigaPublicationConsent,
    GigaQueueMaintenanceOperation, GigaQueueMaintenanceRequest, GigaQueueMaintenanceScope,
    GigaQueueState, GigaResonance, GigaReviewAction, GigaReviewState, GigaRisk, GigaScope,
    GigaScores, GigaSourceRange, GigaSourceRef, GigaSourceType, GigaVisibility, PaperBoatRecord,
    PaperBoatSleepReceipt, PaperBoatSleepRequest, PaperBoatWakeReceipt, PaperBoatWakeRequest,
    RecallRequest, RememberKind, RememberLessonDetails, RememberMemoryDetails, RememberReceipt,
    RememberRequest, RoomKey, ThreadContinuation, UnboatedMemory,
    lesson_triggers::LessonTriggerSpec,
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

fn default_substrate_backup_age_hours() -> f64 {
    24.0
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SubstrateHealthParams {
    #[serde(default)]
    pub skip_embedding: bool,
    #[serde(default = "default_substrate_backup_age_hours")]
    pub max_backup_age_hours: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SubstrateMigrationsParams {}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperBoatSleepParams {
    pub room: String,
    pub body: String,
    #[serde(default = "default_backup")]
    pub backup: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperBoatWakeParams {
    pub room: String,
}

impl TryFrom<PaperBoatSleepParams> for PaperBoatSleepRequest {
    type Error = ProtocolError;

    fn try_from(params: PaperBoatSleepParams) -> Result<Self, Self::Error> {
        PaperBoatSleepRequest::new(params.room, params.body, params.backup)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}

impl TryFrom<PaperBoatWakeParams> for PaperBoatWakeRequest {
    type Error = ProtocolError;

    fn try_from(params: PaperBoatWakeParams) -> Result<Self, Self::Error> {
        PaperBoatWakeRequest::new(params.room)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThreadContinuationParams {
    pub thread: String,
    #[serde(rename = "previousMemoryId")]
    pub previous_memory_id: String,
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
    #[serde(default, rename = "sourceMemoryPath")]
    pub source_memory_path: Option<String>,
    #[serde(default)]
    pub threads: Vec<String>,
    #[serde(default)]
    pub continues: Vec<ThreadContinuationParams>,
    #[serde(default)]
    pub supersedes: Vec<String>,
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub voice: Option<String>,
    #[serde(default)]
    pub register: Vec<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default, rename = "proofPattern")]
    pub proof_pattern: Option<String>,
    #[serde(default, rename = "triggerContext")]
    pub trigger_context: Option<String>,
    #[serde(default, rename = "exampleText")]
    pub example_text: Option<String>,
    #[serde(default, rename = "languageKeys")]
    pub language_keys: Vec<String>,
    #[serde(default, rename = "technologyKeys")]
    pub technology_keys: Vec<String>,
    #[serde(default, rename = "threadKeys")]
    pub thread_keys: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub condition: Vec<String>,
    #[serde(default, rename = "astCondition")]
    pub ast_condition: Vec<String>,
    #[serde(default, rename = "triggerScope")]
    pub trigger_scope: Vec<String>,
    #[serde(default, rename = "interruptMode")]
    pub interrupt_mode: Option<String>,
    #[serde(default, rename = "repeatCooldownSecs")]
    pub repeat_cooldown_secs: Option<i32>,
    #[serde(default)]
    pub backup: Option<bool>,
}

fn default_backup() -> bool {
    true
}

fn default_semantic_top_k() -> u32 {
    8
}
fn default_semantic_min_similarity() -> f64 {
    // Calibrated 2026-07-25 against Nemotron-3-Embed-1B-Q4: true positives
    // measure 0.42-0.56, noise ceilings 0.24. Must stay equal to the substrate
    // constant in recall.rs — when these diverged, the omitted-field path
    // silently reimposed 0.50 and dropped the whole true-positive band.
    0.40
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
    #[serde(default)]
    pub temporal_decay: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VaultRecallParams {
    pub room: String,
    pub room_dir: String,
    pub query: String,
}

impl VaultRecallParams {
    fn validate(self) -> Result<Self, ProtocolError> {
        if self.room.trim().is_empty() {
            return Err(ProtocolError::InvalidParams(
                "vault_recall room must be non-empty".into(),
            ));
        }
        if self.room_dir.trim().is_empty() {
            return Err(ProtocolError::InvalidParams(
                "vault_recall room_dir must be non-empty".into(),
            ));
        }
        if self.query.trim().is_empty() {
            return Err(ProtocolError::InvalidParams(
                "vault_recall query must be non-empty".into(),
            ));
        }
        Ok(self)
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clusters: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunks_total: Option<u64>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<i64>,
    pub label: String,
    #[serde(deserialize_with = "deserialize_unit_fraction")]
    pub activation: f64,
    pub member_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallThreadNeighbor {
    #[serde(default)]
    pub thread: String,
    #[serde(default)]
    pub direction: String,
    pub id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub source_path: String,
    #[serde(default)]
    pub excerpt: String,
    #[serde(default)]
    pub authority_state: String,
    #[serde(default)]
    pub superseded_by: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallCandidate {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub identity: Option<String>,
    #[serde(default)]
    pub candidate_id: Option<String>,
    #[serde(default)]
    pub memory_id: Option<i64>,
    pub source_path: String,
    pub title: String,
    #[serde(default)]
    pub heading_path: String,
    #[serde(default)]
    pub excerpt: String,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub term_coverage: Value,
    #[serde(default)]
    pub matched_terms: Vec<String>,
    #[serde(default)]
    pub missing_terms: Vec<String>,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub thread_key: Option<String>,
    #[serde(default)]
    pub thread_neighbors: Vec<RecallThreadNeighbor>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub chunk_index: Option<i64>,
    #[serde(default)]
    pub semantic_rank: Option<u64>,
    #[serde(default)]
    pub semantic_similarity: Option<f64>,
    #[serde(default)]
    pub semantic_score: Option<f64>,
    #[serde(default)]
    pub content_rank: Option<u64>,
    #[serde(default)]
    pub content_similarity: Option<f64>,
    #[serde(default)]
    pub content_score: Option<f64>,
    #[serde(default)]
    pub lexical_rank: Option<u64>,
    #[serde(default)]
    pub lexical_score: Option<f64>,
    #[serde(default)]
    pub bm25f_score: Option<f64>,
    #[serde(default)]
    pub bm25f_fields: Option<Value>,
    #[serde(default)]
    pub semantic_lexical_score: Option<f64>,
    #[serde(default)]
    pub semantic_lexical_fields: Option<Value>,
    #[serde(default)]
    pub semantic_lexical_concepts: Option<Value>,
    #[serde(default)]
    pub durability: Option<String>,
    #[serde(default)]
    pub temporal_weight: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallCanonFile {
    pub file: String,
    #[serde(default)]
    pub lines: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallCanonEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub summary: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub weighty: bool,
    #[serde(default)]
    pub files: Vec<RecallCanonFile>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallCanonMatch {
    #[serde(rename = "termKey")]
    pub term_key: String,
    pub entry: RecallCanonEntry,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallRawChunk {
    #[serde(default)]
    pub memory_id: Option<i64>,
    #[serde(default)]
    pub source_path: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub heading_path: String,
    #[serde(default)]
    pub sim: Option<f64>,
    #[serde(default)]
    pub ws: Option<f64>,
    pub body: String,
    #[serde(default)]
    pub char_start: Option<i64>,
    #[serde(default)]
    pub char_end: Option<i64>,
    #[serde(default)]
    pub chunk_index: Option<i64>,
    #[serde(default)]
    pub durability: Option<String>,
    #[serde(default)]
    pub temporal_weight: Option<f64>,
    #[serde(default)]
    pub matched_terms: Vec<String>,
    #[serde(default)]
    pub missing_terms: Vec<String>,
    #[serde(default)]
    pub term_coverage: Value,
    #[serde(default)]
    pub evidence: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallDateMatch {
    #[serde(default)]
    pub source_path: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub dates: Vec<String>,
    pub body_excerpt: String,
    #[serde(default)]
    pub excerpt: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub matched_terms: Vec<String>,
    #[serde(default)]
    pub missing_terms: Vec<String>,
    #[serde(default)]
    pub term_coverage: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecallTaxonomy {
    #[serde(default)]
    pub rooms: Vec<String>,
    #[serde(default)]
    pub memory_types: Vec<String>,
    #[serde(default)]
    pub thread_keys: Vec<String>,
    #[serde(default)]
    pub named_entities: Vec<String>,
    #[serde(default)]
    pub file_types: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallMemoryRecord {
    pub source_path: String,
    pub body: String,
    #[serde(default, flatten)]
    pub extra: std::collections::BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct RecallMemoryHandle {
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub memory: Option<RecallMemoryRecord>,
    #[serde(default, flatten)]
    pub extra: std::collections::BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallResultInput {
    pub ok: bool,
    pub query: String,
    pub found: bool,
    pub source: String,
    #[serde(rename = "retrievalCandidates")]
    pub retrieval_candidates: Vec<RecallCandidate>,
    #[serde(rename = "canonMatches")]
    pub canon_matches: Vec<RecallCanonMatch>,
    #[serde(rename = "semanticChunks")]
    pub semantic_chunks: Vec<RecallRawChunk>,
    #[serde(rename = "contentChunks")]
    pub content_chunks: Vec<RecallRawChunk>,
    #[serde(rename = "dateMatches")]
    pub date_matches: Vec<RecallDateMatch>,
    #[serde(rename = "queryDates")]
    pub query_dates: Vec<String>,
    pub taxonomy: RecallTaxonomy,
    #[serde(default)]
    pub authority: Option<String>,
    #[serde(default)]
    pub roots: Vec<String>,
    #[serde(default, rename = "scannedFiles")]
    pub scanned_files: Option<u64>,
    #[serde(default, rename = "indexedDocuments")]
    pub indexed_documents: Option<u64>,
    #[serde(default)]
    pub cluster: Option<Value>,
    #[serde(default)]
    pub clusters: Option<Value>,
    #[serde(default, rename = "clusterStaleness")]
    pub cluster_staleness: Option<ClusterStalenessTelemetry>,
    #[serde(default, rename = "clusterResonance")]
    pub cluster_resonance: Option<ClusterResonanceTelemetry>,
    #[serde(default, rename = "memoryHandle")]
    pub memory_handle: Option<RecallMemoryHandle>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

pub type RecallResult = RecallResultInput;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallViewportMode {
    Automatic,
    Manual,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallPresentationThreadNeighbor {
    pub thread: String,
    pub direction: String,
    pub id: i64,
    pub title: String,
    pub source_path: String,
    pub excerpt: String,
    pub authority_state: String,
    pub superseded_by: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallPresentationCandidate {
    pub source_path: String,
    pub title: String,
    pub heading_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thread_neighbors: Vec<RecallPresentationThreadNeighbor>,
    pub sources: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    pub term_coverage: Value,
    pub matched_terms: Vec<String>,
    pub missing_terms: Vec<String>,
    pub reasons: Vec<String>,
    pub excerpt: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallPresentationCanonMatch {
    #[serde(rename = "termKey")]
    pub term_key: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub summary: String,
    pub files: Vec<RecallCanonFile>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallPresentationRawChunk {
    pub source_path: String,
    pub heading_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    pub body: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallPresentationDateMatch {
    pub source_path: String,
    pub title: String,
    pub dates: Vec<String>,
    pub body_excerpt: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecallPresentationTaxonomy {
    pub rooms: Vec<String>,
    pub memory_types: Vec<String>,
    pub thread_keys: Vec<String>,
    pub named_entities: Vec<String>,
    pub file_types: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallPresentationClusterProfile {
    pub label: String,
    pub activation: f64,
    pub members: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecallPresentationClusterResonance {
    pub note: String,
    pub profile: Vec<RecallPresentationClusterProfile>,
    pub dormant_hot: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallPresentationMemoryRecord {
    pub source_path: String,
    pub body: String,
    pub frontmatter: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallPresentationMemoryHandle {
    pub path: String,
    pub title: String,
    pub memory: Option<RecallPresentationMemoryRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecallPresentationVault {
    pub authority: String,
    pub roots: Vec<String>,
    pub scanned_files: u64,
    pub indexed_documents: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecallPresentation {
    pub ok: bool,
    pub query: String,
    pub found: bool,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault: Option<RecallPresentationVault>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(rename = "canonMatches")]
    pub canon_matches: Vec<RecallPresentationCanonMatch>,
    #[serde(rename = "retrievalCandidates")]
    pub retrieval_candidates: Vec<RecallPresentationCandidate>,
    #[serde(rename = "semanticChunks")]
    pub semantic_chunks: Vec<RecallPresentationRawChunk>,
    #[serde(rename = "contentChunks")]
    pub content_chunks: Vec<RecallPresentationRawChunk>,
    #[serde(rename = "dateMatches")]
    pub date_matches: Vec<RecallPresentationDateMatch>,
    #[serde(rename = "queryDates")]
    pub query_dates: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taxonomy: Option<RecallPresentationTaxonomy>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "clusterNudge")]
    pub cluster_nudge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "clusterResonance")]
    pub cluster_resonance: Option<RecallPresentationClusterResonance>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "memoryHandle")]
    pub memory_handle: Option<RecallPresentationMemoryHandle>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecallViewportSuppression {
    pub identity: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecallViewportDiagnostics {
    pub kept: u64,
    pub suppressed: u64,
    pub reasons: std::collections::HashMap<String, u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecallViewportResult {
    pub kept_candidates: Vec<RecallPresentationCandidate>,
    pub suppressions: Vec<RecallViewportSuppression>,
    pub diagnostics: RecallViewportDiagnostics,
    pub presentation: RecallPresentation,
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
    let has_authenticated_url = lowercase.split_once("://").is_some_and(|(_, rest)| {
        rest.split('/')
            .next()
            .is_some_and(|authority| authority.contains('@'))
    });
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
        Value::Array(values) => {
            Value::Array(values.into_iter().map(redact_diagnostic_value).collect())
        }
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
        .map(|request| request.with_temporal_decay(params.temporal_decay))
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
        let kind = RememberKind::parse(&params.kind)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        let room = if kind.is_lesson() {
            RoomKey::new(params.room)
        } else {
            RoomKey::for_memory_write(params.room)
        }
        .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;
        if kind.is_lesson()
            && (params.source_path.is_some()
                || !params.threads.is_empty()
                || !params.continues.is_empty()
                || !params.supersedes.is_empty())
        {
            return Err(ProtocolError::InvalidParams(
                "memory-only fields are not valid for lessons".into(),
            ));
        }
        if !kind.is_lesson()
            && (params.source_memory_path.is_some()
                || params.shape.is_some()
                || params.voice.is_some()
                || !params.register.is_empty()
                || params.scope.is_some()
                || params.project.is_some()
                || params.proof_pattern.is_some()
                || params.trigger_context.is_some()
                || params.example_text.is_some()
                || !params.language_keys.is_empty()
                || !params.technology_keys.is_empty()
                || !params.thread_keys.is_empty()
                || !params.tags.is_empty()
                || !params.condition.is_empty()
                || !params.ast_condition.is_empty()
                || !params.trigger_scope.is_empty()
                || params.interrupt_mode.is_some()
                || params.repeat_cooldown_secs.is_some())
        {
            return Err(ProtocolError::InvalidParams(
                "lesson-only fields are not valid for memory".into(),
            ));
        }
        let mut supersedes = Vec::with_capacity(params.supersedes.len());
        for raw in params.supersedes {
            if raw.starts_with('0') || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
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
        let mut continues = Vec::with_capacity(params.continues.len());
        for continuation in params.continues {
            let raw = continuation.previous_memory_id;
            if raw.starts_with('0') || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(ProtocolError::InvalidParams(format!(
                    "previousMemoryId must be a positive PostgreSQL BIGINT decimal: {raw}"
                )));
            }
            let previous_memory_id =
                raw.parse::<i64>()
                    .ok()
                    .filter(|&id| id > 0)
                    .ok_or_else(|| {
                        ProtocolError::InvalidParams(format!(
                            "previousMemoryId must be a positive PostgreSQL BIGINT decimal: {raw}"
                        ))
                    })? as u64;
            continues.push(ThreadContinuation {
                thread: continuation.thread,
                previous_memory_id,
            });
        }
        let result = if kind.is_lesson() {
            RememberRequest::new_lesson(
                room,
                kind,
                params.title,
                params.body,
                RememberLessonDetails {
                    backup: params.backup.unwrap_or(matches!(
                        kind,
                        RememberKind::ProjectLesson | RememberKind::AudioLesson
                    )),
                    source_memory_path: params.source_memory_path,
                    shape: params.shape,
                    voice: params.voice,
                    register: params.register,
                    scope: params.scope,
                    project: params.project,
                    proof_pattern: params.proof_pattern,
                    trigger_context: params.trigger_context,
                    example_text: params.example_text,
                    language_keys: params.language_keys,
                    technology_keys: params.technology_keys,
                    thread_keys: params.thread_keys,
                    tags: params.tags,
                    triggers: LessonTriggerSpec {
                        condition: params.condition,
                        ast_condition: params.ast_condition,
                        trigger_scope: params.trigger_scope,
                        interrupt_mode: params.interrupt_mode,
                        repeat_cooldown_secs: params.repeat_cooldown_secs,
                    },
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
                    continues,
                    supersedes,
                    backup: params.backup.unwrap_or(false),
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CanonAttributionParams {
    pub actor: String,
    pub origin: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CanonPointerParams {
    pub file: String,
    #[serde(default)]
    pub lines: Option<[u32; 2]>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CanonWriteParams {
    pub room: String,
    pub name: String,
    pub kind: String,
    pub summary: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub search_boost: Option<String>,
    #[serde(default)]
    pub weighty: bool,
    #[serde(default)]
    pub pointer_files: Vec<CanonPointerParams>,
    #[serde(default)]
    pub summary_as_of: Option<String>,
    #[serde(default)]
    pub supersedes: Vec<String>,
    pub attribution: CanonAttributionParams,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CanonReadParams {
    pub room: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub include_history: bool,
}

fn canon_id(value: &str, field: &str) -> Result<u64, ProtocolError> {
    value
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
        .ok_or_else(|| {
            ProtocolError::InvalidParams(format!("{field} must be a positive PostgreSQL BIGINT"))
        })
}

impl TryFrom<CanonWriteParams> for CanonWriteRequest {
    type Error = ProtocolError;

    fn try_from(value: CanonWriteParams) -> Result<Self, Self::Error> {
        let pointers = value
            .pointer_files
            .into_iter()
            .map(|pointer| {
                CanonPointer::new(
                    pointer.file,
                    pointer.lines.map(|lines| (lines[0], lines[1])),
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?;
        let supersedes = value
            .supersedes
            .iter()
            .map(|id| canon_id(id, "supersedes ID"))
            .collect::<Result<Vec<_>, _>>()?;
        let attribution = CanonAttribution::new(value.attribution.actor, value.attribution.origin)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?;
        CanonWriteRequest::new(
            value.room,
            value.name,
            value.kind,
            value.summary,
            value.aliases,
            value.search_boost,
            value.weighty,
            pointers,
            value.summary_as_of,
            supersedes,
            attribution,
        )
        .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}

impl TryFrom<CanonReadParams> for CanonReadRequest {
    type Error = ProtocolError;

    fn try_from(value: CanonReadParams) -> Result<Self, Self::Error> {
        let id = value
            .id
            .as_deref()
            .map(|id| canon_id(id, "id"))
            .transpose()?;
        CanonReadRequest::new(value.room, id, value.name, value.include_history)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CanonEntityResult {
    pub entity_id: String,
    pub room: String,
    pub name: String,
    pub kind: String,
    pub summary: String,
    pub aliases: Vec<String>,
    pub search_boost: Option<String>,
    pub weighty: bool,
    pub pointer_files: Value,
    pub summary_as_of: Option<String>,
    pub meta: Value,
    pub authority: String,
    pub superseded_by: Option<String>,
    pub supersedes: Vec<String>,
    pub attributed_by: String,
    pub attribution_origin: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CanonReadResult {
    pub ok: bool,
    pub entities: Vec<CanonEntityResult>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CanonWriteResult {
    pub ok: bool,
    pub durable: bool,
    pub authority: String,
    pub entity_authority: String,
    pub entity_id: String,
    pub room: String,
    pub name: String,
    pub superseded_entity_ids: Vec<String>,
    pub attributed_by: String,
    pub attribution_origin: String,
}

impl From<CanonWriteReceipt> for CanonWriteResult {
    fn from(value: CanonWriteReceipt) -> Self {
        Self {
            ok: true,
            durable: true,
            authority: "postgres".into(),
            entity_authority: "active".into(),
            entity_id: value.entity_id().to_string(),
            room: value.room().to_string(),
            name: value.name().into(),
            superseded_entity_ids: value
                .superseded_entity_ids()
                .iter()
                .map(u64::to_string)
                .collect(),
            attributed_by: value.attribution().actor().into(),
            attribution_origin: value.attribution().origin().into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PaperBoatSleepResult {
    pub ok: bool,
    pub memory_id: String,
    pub room: String,
    pub source_path: String,
    pub outbox_event_id: String,
    pub inserted: bool,
    pub durable: bool,
    pub authority: String,
    pub backup_status: String,
    pub warnings: Vec<String>,
}

impl From<PaperBoatSleepReceipt> for PaperBoatSleepResult {
    fn from(receipt: PaperBoatSleepReceipt) -> Self {
        Self {
            ok: true,
            memory_id: receipt.memory_id().to_string(),
            room: receipt.room().to_string(),
            source_path: receipt.source_path().into(),
            outbox_event_id: receipt.outbox_event_id().into(),
            inserted: receipt.inserted(),
            durable: receipt.durable(),
            authority: "postgres".into(),
            backup_status: receipt.backup_status().as_str().into(),
            warnings: receipt.warnings().to_vec(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PaperBoatUnboatedResult {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub source_path: String,
    pub created_at: String,
}

impl From<&UnboatedMemory> for PaperBoatUnboatedResult {
    fn from(memory: &UnboatedMemory) -> Self {
        Self {
            id: memory.id.to_string(),
            title: memory.title.clone(),
            kind: memory.kind.clone(),
            source_path: memory.source_path.clone(),
            created_at: memory.created_at.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PaperBoatWakeResult {
    pub ok: bool,
    pub found: bool,
    pub room: String,
    pub id: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub date: Option<String>,
    pub source_path: Option<String>,
    pub created_at: Option<String>,
    pub unboated: Vec<PaperBoatUnboatedResult>,
    pub unboated_truncated: bool,
    pub warnings: Vec<String>,
    pub wake_context: Option<String>,
}

fn bounded_wake_body(body: &str) -> String {
    const LIMIT: usize = 6_000;
    let length = body.chars().count();
    if length <= LIMIT {
        body.to_owned()
    } else {
        format!(
            "{}\n...[paper boat clipped {} chars]",
            body.chars().take(LIMIT).collect::<String>().trim_end(),
            length - LIMIT,
        )
    }
}

fn wake_context(
    title: &str,
    source_path: &str,
    body: &str,
    unboated: &[PaperBoatUnboatedResult],
) -> String {
    let warning = (!unboated.is_empty()).then(|| {
        let plural = if unboated.len() == 1 {
            "memory was"
        } else {
            "memories were"
        };
        format!(
            "STALE BOAT: {} {plural} written AFTER this boat was cast, so this boat does NOT describe the most recent session. Do not treat it as current.\nRecover the missing session by recalling these before answering:\n{}",
            unboated.len(),
            unboated
                .iter()
                .map(|memory| format!("  - [{}] {}", memory.id, memory.title.trim()))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    });
    [
        Some("<system-reminder>".to_owned()),
        warning,
        Some("Automatic wake: latest paper boat for this room.".into()),
        Some("Receive it as lived continuity from the room's previous waking self: keep its voice, relationships, uncertainty, and concrete state intact; orient from it without turning it into a script or status report.".into()),
        Some(format!("Title: {title}")),
        Some(format!("Source: {source_path}")),
        Some(bounded_wake_body(body)),
        Some("</system-reminder>".into()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n")
}
impl From<PaperBoatWakeReceipt> for PaperBoatWakeResult {
    fn from(receipt: PaperBoatWakeReceipt) -> Self {
        let room = receipt.room().to_string();
        let warnings = receipt.warnings().to_vec();
        match receipt.boat() {
            Some(PaperBoatRecord {
                id,
                title,
                body,
                date,
                source_path,
                created_at,
                unboated,
                unboated_truncated,
            }) => {
                let unboated = unboated.iter().map(Into::into).collect::<Vec<_>>();
                Self {
                    ok: true,
                    found: true,
                    room,
                    id: Some(id.to_string()),
                    title: Some(title.clone()),
                    body: Some(body.clone()),
                    date: date.clone(),
                    source_path: Some(source_path.clone()),
                    created_at: Some(created_at.clone()),
                    wake_context: Some(wake_context(title, source_path, body, &unboated)),
                    unboated,
                    unboated_truncated: *unboated_truncated,
                    warnings,
                }
            }
            None => Self {
                ok: true,
                found: false,
                room,
                id: None,
                title: None,
                body: None,
                date: None,
                source_path: None,
                created_at: None,
                unboated: Vec::new(),
                unboated_truncated: false,
                wake_context: None,
                warnings,
            },
        }
    }
}

impl RequestEnvelope {
    pub fn canon_write_request(self) -> Result<CanonWriteRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "canon_write" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<CanonWriteParams>(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?
            .try_into()
    }

    pub fn canon_read_request(self) -> Result<CanonReadRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "canon_read" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<CanonReadParams>(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?
            .try_into()
    }
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
    pub fn paper_boat_sleep_request(self) -> Result<PaperBoatSleepRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "paper_boat_sleep" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<PaperBoatSleepParams>(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?
            .try_into()
    }

    pub fn paper_boat_wake_request(self) -> Result<PaperBoatWakeRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "paper_boat_wake" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<PaperBoatWakeParams>(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?
            .try_into()
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
    pub fn vault_recall_request(self) -> Result<VaultRecallParams, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "vault_recall" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<VaultRecallParams>(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?
            .validate()
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
    pub fn giga_event_ingest_request(self) -> Result<GigaEvent, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "giga_event_ingest" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<GigaEventParams>(self.params)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?
            .try_into()
    }
    pub fn giga_conversation_ingest_request(
        self,
    ) -> Result<GigaConversationIngestParams, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "giga_conversation_ingest" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
    pub fn giga_event_claim_request(self) -> Result<GigaEventClaimRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "giga_event_claim" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<GigaEventClaimParams>(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?
            .try_into()
    }
    pub fn giga_event_finish_request(self) -> Result<GigaEventFinishRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "giga_event_finish" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<GigaEventFinishParams>(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?
            .try_into()
    }
    pub fn giga_event_replay_request(self) -> Result<GigaEventReplayRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "giga_event_replay" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<GigaEventReplayParams>(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?
            .try_into()
    }
    pub fn giga_queue_maintenance_request(
        self,
    ) -> Result<GigaQueueMaintenanceRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "giga_queue_maintenance" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<GigaQueueMaintenanceParams>(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?
            .try_into()
    }
    pub fn giga_promote_request(self) -> Result<GigaPromotionRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "giga_promote" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<GigaPromoteParams>(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?
            .try_into()
    }
    pub fn giga_tool_promote_request(self) -> Result<GigaToolPromoteParams, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "giga_tool_promote" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
    pub fn giga_review_request(self) -> Result<GigaReviewAction, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "giga_review" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<GigaReviewParams>(self.params)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?
            .try_into()
    }
    pub fn giga_tool_review_request(self) -> Result<GigaToolReviewParams, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "giga_tool_review" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
    pub fn giga_candidate_list_request(self) -> Result<GigaCandidateListRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "giga_candidate_list" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<GigaCandidateListParams>(self.params)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?
            .try_into()
    }
    pub fn giga_health_request(self) -> Result<GigaHealthRequest, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "giga_health" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<GigaHealthParams>(self.params)
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?
            .try_into()
    }
    pub fn substrate_health_request(self) -> Result<SubstrateHealthParams, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "substrate_health" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        let params = serde_json::from_value::<SubstrateHealthParams>(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?;
        if !params.max_backup_age_hours.is_finite() || params.max_backup_age_hours <= 0.0 {
            return Err(ProtocolError::InvalidParams(
                "maxBackupAgeHours must be a positive finite number".into(),
            ));
        }
        Ok(params)
    }
    pub fn substrate_migrations_request(self) -> Result<SubstrateMigrationsParams, ProtocolError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch(self.protocol));
        }
        if self.method != "substrate_migrations" {
            return Err(ProtocolError::UnknownMethod(self.method));
        }
        serde_json::from_value::<SubstrateMigrationsParams>(self.params)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
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

macro_rules! giga_known_string {
    ($name:ident, $parse:path) => {
        fn $name<'de, D>(deserializer: D) -> Result<String, D::Error>
        where
            D: Deserializer<'de>,
        {
            let value = String::deserialize(deserializer)?;
            $parse(&value).map_err(D::Error::custom)?;
            Ok(value)
        }
    };
}
giga_known_string!(deserialize_giga_visibility, GigaVisibility::parse);
giga_known_string!(deserialize_giga_source_type, GigaSourceType::parse);
giga_known_string!(deserialize_giga_event_type, GigaEventType::parse);
giga_known_string!(deserialize_giga_risk, GigaRisk::parse);
giga_known_string!(deserialize_giga_kind, GigaCandidateKind::parse);
giga_known_string!(deserialize_giga_authority, GigaAuthority::parse);
giga_known_string!(deserialize_giga_review_state, GigaReviewState::parse);
giga_known_string!(
    deserialize_giga_finish_outcome,
    GigaEventFinishOutcome::parse
);
giga_known_string!(deserialize_giga_queue_state, GigaQueueState::parse);
giga_known_string!(
    deserialize_giga_promotion_authority,
    GigaPromotionAuthority::parse
);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(transparent)]
pub struct RequiredNullable<T>(pub Option<T>);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaScopeParams {
    pub room: RequiredNullable<String>,
    pub project: RequiredNullable<String>,
    #[serde(deserialize_with = "deserialize_giga_visibility")]
    pub visibility: String,
    pub publication_review_required: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaSourceRangeParams {
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaSourceRefParams {
    #[serde(deserialize_with = "deserialize_giga_source_type")]
    pub source_type: String,
    pub source_id: String,
    pub role: String,
    pub timestamp: String,
    pub content_hash: String,
    pub scope: GigaScopeParams,
    pub range: RequiredNullable<GigaSourceRangeParams>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConversationWindowLifecycle {}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TaskStartedLifecycle {
    pub task_reference: String,
    pub worker_id: String,
    pub worker_role: String,
    pub phase: String,
    pub project_key: String,
    pub task_kind: String,
    #[serde(deserialize_with = "deserialize_giga_risk")]
    pub risk: String,
    pub target: String,
    pub change: String,
    pub proof_contract: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TaskCompletedLifecycle {
    pub task_reference: String,
    pub outcome: String,
    pub verification_result: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SubagentDispatchedLifecycle {
    pub subagent_reference: String,
    pub parent_task: String,
    pub role: String,
    pub target: String,
    pub change: String,
    pub acceptance: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SubagentCompletedLifecycle {
    pub subagent_reference: String,
    pub parent_task: String,
    pub outcome: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TodoTransitionLifecycle {
    pub todo_reference: String,
    pub previous_state: String,
    pub new_state: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ToolOutcomeLifecycle {
    pub tool_name: String,
    pub status: String,
    pub sanitized_outcome: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ManualReprocessLifecycle {
    pub source_range: GigaSourceRangeParams,
    pub reason: String,
    pub operator_identity: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum GigaLifecycleParams {
    TaskStarted(TaskStartedLifecycle),
    TaskCompleted(TaskCompletedLifecycle),
    SubagentDispatched(SubagentDispatchedLifecycle),
    SubagentCompleted(SubagentCompletedLifecycle),
    TodoTransition(TodoTransitionLifecycle),
    ToolOutcome(ToolOutcomeLifecycle),
    ManualReprocess(ManualReprocessLifecycle),
    ConversationWindow(ConversationWindowLifecycle),
}

impl GigaLifecycleParams {
    fn into_core(self) -> Result<GigaLifecycle, ProtocolError> {
        let result = match self {
            Self::ConversationWindow(_) => Ok(GigaLifecycle::conversation_window()),
            Self::TaskStarted(v) => GigaRisk::parse(&v.risk).and_then(|risk| {
                GigaLifecycle::task_started(
                    v.task_reference,
                    v.worker_id,
                    v.worker_role,
                    v.phase,
                    v.project_key,
                    v.task_kind,
                    risk,
                    v.target,
                    v.change,
                    v.proof_contract,
                )
            }),
            Self::TaskCompleted(v) => {
                GigaLifecycle::task_completed(v.task_reference, v.outcome, v.verification_result)
            }
            Self::SubagentDispatched(v) => GigaLifecycle::subagent_dispatched(
                v.subagent_reference,
                v.parent_task,
                v.role,
                v.target,
                v.change,
                v.acceptance,
            ),
            Self::SubagentCompleted(v) => {
                GigaLifecycle::subagent_completed(v.subagent_reference, v.parent_task, v.outcome)
            }
            Self::TodoTransition(v) => {
                GigaLifecycle::todo_transition(v.todo_reference, v.previous_state, v.new_state)
            }
            Self::ToolOutcome(v) => {
                GigaLifecycle::tool_outcome(v.tool_name, v.status, v.sanitized_outcome)
            }
            Self::ManualReprocess(v) => {
                GigaSourceRange::new(v.source_range.start, v.source_range.end).and_then(|range| {
                    GigaLifecycle::manual_reprocess(range, v.reason, v.operator_identity)
                })
            }
        };
        result.map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaConversationTurnParams {
    pub role: String,
    pub source_id: String,
    pub content_hash: String,
    pub session_id: String,
    pub timestamp: String,
    pub has_stable_id: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaConversationIngestParams {
    pub room: String,
    #[serde(default)]
    pub project_keys: Vec<String>,
    pub turns: Vec<GigaConversationTurnParams>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaEventParams {
    pub event_schema_version: u8,
    pub event_id: String,
    #[serde(deserialize_with = "deserialize_giga_event_type")]
    pub event_type: String,
    pub room: String,
    pub session_id: String,
    pub project_keys: Vec<String>,
    pub source_refs: Vec<GigaSourceRefParams>,
    pub lifecycle: GigaLifecycleParams,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaEventClaimParams {
    pub room: String,
    pub worker_id: String,
    pub lease_seconds: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaEventFinishParams {
    pub room: String,
    pub event_id: String,
    pub worker_id: String,
    #[serde(deserialize_with = "deserialize_giga_finish_outcome")]
    pub outcome: String,
    pub candidate_count: u32,
    pub error_class: RequiredNullable<String>,
    pub retry_after_seconds: RequiredNullable<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaEventReplayParams {
    pub room: String,
    pub event_id: String,
    pub operator_identity: String,
    pub authorization_basis: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaQueueMaintenanceParams {
    pub room: String,
    pub operation: String,
    pub scope: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaScoresParams {
    pub priority: f64,
    pub novelty: f64,
    pub durability: f64,
    pub confidence: f64,
}

const fn default_giga_list_limit() -> u32 {
    50
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaCandidateListParams {
    pub room: String,
    pub review_state: Option<String>,
    #[serde(default = "default_giga_list_limit")]
    pub limit: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaHealthParams {
    pub room: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct GigaHealthRequest {
    room: RoomKey,
}
impl GigaHealthRequest {
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
}
impl TryFrom<GigaHealthParams> for GigaHealthRequest {
    type Error = ProtocolError;
    fn try_from(value: GigaHealthParams) -> Result<Self, Self::Error> {
        Ok(Self {
            room: RoomKey::new(value.room)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct GigaCandidateListRequest {
    room: RoomKey,
    review_state: Option<GigaReviewState>,
    limit: u32,
}
impl GigaCandidateListRequest {
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub const fn review_state(&self) -> Option<GigaReviewState> {
        self.review_state
    }
    pub const fn limit(&self) -> u32 {
        self.limit
    }
}
impl TryFrom<GigaCandidateListParams> for GigaCandidateListRequest {
    type Error = ProtocolError;
    fn try_from(value: GigaCandidateListParams) -> Result<Self, Self::Error> {
        if value.limit == 0 || value.limit > 200 {
            return Err(ProtocolError::InvalidParams(
                "limit must be between 1 and 200".into(),
            ));
        }
        Ok(Self {
            room: RoomKey::new(value.room)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            review_state: value
                .review_state
                .as_deref()
                .map(GigaReviewState::parse)
                .transpose()
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            limit: value.limit,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaClassifierParams {
    pub model: String,
    pub provider_type: String,
    pub model_version: String,
    pub prompt_version: String,
    pub configuration_digest: String,
    pub run_id: String,
    pub completed_at: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaCandidateParams {
    pub candidate_schema_version: u8,
    pub candidate_id: String,
    pub event_id: String,
    pub room: String,
    pub session_id: String,
    #[serde(deserialize_with = "deserialize_giga_kind")]
    pub kind: String,
    pub source_refs: Vec<GigaSourceRefParams>,
    pub priority: f64,
    pub novelty: f64,
    pub durability: f64,
    pub confidence: f64,
    pub project_keys: Vec<String>,
    pub thread_keys: Vec<String>,
    pub entity_hints: Vec<String>,
    pub retrieval_terms: Vec<String>,
    pub proposed_title: String,
    pub gist: String,
    pub rationale: String,
    pub proof_refs: Vec<String>,
    pub scope: GigaScopeParams,
    #[serde(deserialize_with = "deserialize_giga_authority")]
    pub authority: String,
    #[serde(deserialize_with = "deserialize_giga_review_state")]
    pub review_state: String,
    pub classifier: GigaClassifierParams,
    pub created_at: String,
    pub expires_at: RequiredNullable<String>,
    pub promotion_refs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaResonanceParams {
    pub event_id: String,
    pub score: f64,
    pub classifier: GigaClassifierParams,
    pub source_refs: Vec<GigaSourceRefParams>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaReviewParams {
    pub candidate_id: String,
    pub reviewer_id: String,
    #[serde(deserialize_with = "deserialize_giga_review_state")]
    pub previous_state: String,
    #[serde(deserialize_with = "deserialize_giga_review_state")]
    pub new_state: String,
    pub reason: String,
    pub authorization_basis: String,
    pub source_refs: Vec<GigaSourceRefParams>,
    pub promotion_target: RequiredNullable<String>,
    pub merge_target: RequiredNullable<String>,
    pub merge_source_candidates: Vec<String>,
    pub resonance: RequiredNullable<GigaResonanceParams>,
    pub reviewed_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaMemoryPromotionPayloadParams {
    pub title: String,
    pub body: String,
    pub threads: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaCodingLessonPromotionPayloadParams {
    pub title: String,
    pub body: String,
    pub shape: RequiredNullable<String>,
    pub proof_pattern: String,
    pub trigger_context: String,
    #[serde(default)]
    pub language_keys: Vec<String>,
    #[serde(default)]
    pub technology_keys: Vec<String>,
    #[serde(default)]
    pub thread_keys: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaProjectLessonPromotionPayloadParams {
    pub title: String,
    pub body: String,
    pub project: String,
    pub proof_pattern: String,
    pub trigger_context: String,
    #[serde(default)]
    pub language_keys: Vec<String>,
    #[serde(default)]
    pub technology_keys: Vec<String>,
    #[serde(default)]
    pub thread_keys: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(
    tag = "kind",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GigaPromotionTargetParams {
    Memory(GigaMemoryPromotionPayloadParams),
    CodingLesson(GigaCodingLessonPromotionPayloadParams),
    ProjectLesson(GigaProjectLessonPromotionPayloadParams),
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaPublicationConsentParams {
    pub operator_approved: bool,
    pub reviewer_approved: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GigaToolPromotionTargetParams {
    Memory {
        title: String,
        body: String,
        #[serde(default)]
        threads: Vec<String>,
    },
    CodingLesson {
        title: String,
        body: String,
        #[serde(default)]
        shape: Option<String>,
        #[serde(default)]
        proof_pattern: Option<String>,
        #[serde(default)]
        trigger_context: Option<String>,
        #[serde(default)]
        language_keys: Vec<String>,
        #[serde(default)]
        technology_keys: Vec<String>,
        #[serde(default)]
        tags: Vec<String>,
    },
    ProjectLesson {
        title: String,
        body: String,
        #[serde(default)]
        proof_pattern: Option<String>,
        #[serde(default)]
        trigger_context: Option<String>,
        #[serde(default)]
        language_keys: Vec<String>,
        #[serde(default)]
        technology_keys: Vec<String>,
        #[serde(default)]
        tags: Vec<String>,
        publication_approved: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaToolPromoteParams {
    pub candidate_id: String,
    pub room: String,
    pub reviewer_id: String,
    pub operator_identity: String,
    pub authorization_basis: String,
    pub target: GigaToolPromotionTargetParams,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaToolReviewParams {
    pub candidate_id: String,
    pub room: String,
    pub reviewer_id: String,
    pub new_state: String,
    pub reason: String,
    pub authorization_basis: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaPromoteParams {
    pub candidate_id: String,
    pub room: String,
    pub reviewer_id: String,
    pub operator_identity: String,
    pub authorization_basis: String,
    pub source_refs: Vec<GigaSourceRefParams>,
    pub target: GigaPromotionTargetParams,
    pub publication_consent: RequiredNullable<GigaPublicationConsentParams>,
    pub reviewed_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaEventIngestResult {
    pub event_id: String,
    pub accepted: bool,
    pub duplicate: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaEventClaimResult {
    pub room: String,
    pub worker_id: String,
    pub claimed_at: String,
    pub event: RequiredNullable<GigaEventParams>,
    pub lease_expires_at: RequiredNullable<String>,
    pub attempt_count: RequiredNullable<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaProcessResult {
    pub event_id: String,
    #[serde(deserialize_with = "deserialize_giga_finish_outcome")]
    pub outcome: String,
    pub candidate_count: u32,
    pub attempt_count: u32,
    pub error_class: RequiredNullable<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaEventFinishResult {
    pub room: String,
    pub event_id: String,
    pub worker_id: String,
    #[serde(deserialize_with = "deserialize_giga_finish_outcome")]
    pub outcome: String,
    #[serde(deserialize_with = "deserialize_giga_queue_state")]
    pub queue_state: String,
    pub attempt_count: u32,
    pub candidate_count: u32,
    pub available_at: RequiredNullable<String>,
    pub finished_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaEventReplayResult {
    pub room: String,
    pub event_id: String,
    pub operator_identity: String,
    #[serde(deserialize_with = "deserialize_giga_queue_state")]
    pub previous_state: String,
    #[serde(deserialize_with = "deserialize_giga_queue_state")]
    pub queue_state: String,
    pub attempt_count: u32,
    pub replayed_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaQueueStateCount {
    #[serde(deserialize_with = "deserialize_giga_queue_state")]
    pub queue_state: String,
    pub count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaQueueMaintenanceResult {
    pub ok: bool,
    pub operation: String,
    pub scope: String,
    pub room: String,
    pub eligible_events: u64,
    pub blocked_events: u64,
    pub deleted_events: u64,
    pub deleted_attempts: u64,
    pub preserved_candidates: u64,
    pub before: Vec<GigaQueueStateCount>,
    pub after: Vec<GigaQueueStateCount>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GigaPromoteResult {
    Memory {
        candidate_id: String,
        #[serde(deserialize_with = "deserialize_giga_review_state")]
        review_state: String,
        memory_id: u64,
        room: String,
        durable: bool,
        #[serde(deserialize_with = "deserialize_giga_promotion_authority")]
        authority: String,
        warnings: Vec<String>,
        reviewer_id: String,
        operator_identity: String,
        reviewed_at: String,
        committed_at: String,
    },
    CodingLesson {
        candidate_id: String,
        #[serde(deserialize_with = "deserialize_giga_review_state")]
        review_state: String,
        coding_lesson_id: u64,
        scope: String,
        durable: bool,
        #[serde(deserialize_with = "deserialize_giga_promotion_authority")]
        authority: String,
        warnings: Vec<String>,
        reviewer_id: String,
        operator_identity: String,
        reviewed_at: String,
        committed_at: String,
    },
    ProjectLesson {
        candidate_id: String,
        #[serde(deserialize_with = "deserialize_giga_review_state")]
        review_state: String,
        project_lesson_id: u64,
        project: String,
        durable: bool,
        #[serde(deserialize_with = "deserialize_giga_promotion_authority")]
        authority: String,
        warnings: Vec<String>,
        reviewer_id: String,
        operator_identity: String,
        reviewed_at: String,
        committed_at: String,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaCandidateStoreResult {
    pub candidate_id: String,
    pub stored: bool,
    pub duplicate: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaCandidateListResult {
    pub candidates: Vec<GigaCandidateParams>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaReviewResult {
    pub candidate_id: String,
    #[serde(deserialize_with = "deserialize_giga_review_state")]
    pub previous_state: String,
    #[serde(deserialize_with = "deserialize_giga_review_state")]
    pub new_state: String,
    pub reviewed_at: String,
    pub resonance: RequiredNullable<GigaResonanceParams>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaHealthCount {
    #[serde(deserialize_with = "deserialize_giga_kind")]
    pub kind: String,
    #[serde(deserialize_with = "deserialize_giga_review_state")]
    pub review_state: String,
    pub count: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaClassifierHealthResult {
    pub provider_type: String,
    pub model: String,
    pub model_digest: String,
    pub prompt_version: String,
    pub endpoint_scope: String,
    pub last_error_class: RequiredNullable<String>,
    pub last_error_at: RequiredNullable<String>,
    pub consecutive_failures: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GigaHealthResult {
    pub enabled: bool,
    pub store_healthy: bool,
    pub queue_depth: u64,
    pub oldest_queue_age_seconds: Option<u64>,
    pub processed_count: u64,
    pub failed_count: u64,
    pub candidates_by_kind_state: Vec<GigaHealthCount>,
    pub classifier: GigaClassifierHealthResult,
}

impl TryFrom<GigaEventClaimParams> for GigaEventClaimRequest {
    type Error = ProtocolError;

    fn try_from(value: GigaEventClaimParams) -> Result<Self, Self::Error> {
        GigaEventClaimRequest::new(
            RoomKey::new(value.room)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.worker_id,
            value.lease_seconds,
        )
        .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}

impl TryFrom<GigaEventFinishParams> for GigaEventFinishRequest {
    type Error = ProtocolError;

    fn try_from(value: GigaEventFinishParams) -> Result<Self, Self::Error> {
        GigaEventFinishRequest::new(
            RoomKey::new(value.room)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.event_id,
            value.worker_id,
            GigaEventFinishOutcome::parse(&value.outcome)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.candidate_count,
            value.error_class.0,
            value.retry_after_seconds.0,
        )
        .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}

impl TryFrom<GigaEventReplayParams> for GigaEventReplayRequest {
    type Error = ProtocolError;

    fn try_from(value: GigaEventReplayParams) -> Result<Self, Self::Error> {
        GigaEventReplayRequest::new(
            RoomKey::new(value.room)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.event_id,
            value.operator_identity,
            value.authorization_basis,
        )
        .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}

impl TryFrom<GigaQueueMaintenanceParams> for GigaQueueMaintenanceRequest {
    type Error = ProtocolError;

    fn try_from(value: GigaQueueMaintenanceParams) -> Result<Self, Self::Error> {
        let room = RoomKey::new(value.room)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?;
        let operation = GigaQueueMaintenanceOperation::parse(&value.operation)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?;
        let scope = GigaQueueMaintenanceScope::parse(&value.scope)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?;

        Ok(GigaQueueMaintenanceRequest::new(room, operation, scope))
    }
}

impl GigaPromotionTargetParams {
    fn into_core(self) -> Result<GigaPromotionPayload, ProtocolError> {
        let payload = match self {
            Self::Memory(payload) => {
                GigaMemoryPromotionPayload::new(payload.title, payload.body, payload.threads)
                    .map(GigaPromotionPayload::Memory)
            }
            Self::CodingLesson(payload) => GigaCodingLessonPromotionPayload::new(
                payload.title,
                payload.body,
                payload.shape.0,
                payload.proof_pattern,
                payload.trigger_context,
                payload.language_keys,
                payload.technology_keys,
                payload.thread_keys,
                payload.tags,
            )
            .map(GigaPromotionPayload::CodingLesson),
            Self::ProjectLesson(payload) => GigaProjectLessonPromotionPayload::new(
                payload.title,
                payload.body,
                payload.project,
                payload.proof_pattern,
                payload.trigger_context,
                payload.language_keys,
                payload.technology_keys,
                payload.thread_keys,
                payload.tags,
            )
            .map(GigaPromotionPayload::ProjectLesson),
        };
        payload.map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}

impl TryFrom<GigaPromoteParams> for GigaPromotionRequest {
    type Error = ProtocolError;

    fn try_from(value: GigaPromoteParams) -> Result<Self, Self::Error> {
        let source_refs = value
            .source_refs
            .into_iter()
            .map(giga_source)
            .collect::<Result<Vec<_>, _>>()?;
        let publication_consent = value
            .publication_consent
            .0
            .map(|consent| {
                GigaPublicationConsent::new(consent.operator_approved, consent.reviewer_approved)
                    .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
            })
            .transpose()?;
        GigaPromotionRequest::new(
            value.candidate_id,
            RoomKey::new(value.room)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.reviewer_id,
            value.operator_identity,
            value.authorization_basis,
            source_refs,
            value.target.into_core()?,
            publication_consent,
            value.reviewed_at,
        )
        .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}

fn giga_scope_params(value: &GigaScope) -> GigaScopeParams {
    GigaScopeParams {
        room: RequiredNullable(value.room().map(ToString::to_string)),
        project: RequiredNullable(value.project().map(str::to_owned)),
        visibility: value.visibility().as_str().into(),
        publication_review_required: value.publication_review_required(),
    }
}

fn giga_source_params(value: &GigaSourceRef) -> GigaSourceRefParams {
    GigaSourceRefParams {
        source_type: value.source_type().as_str().into(),
        source_id: value.source_id().into(),
        role: value.role().into(),
        timestamp: value.timestamp().into(),
        content_hash: value.content_hash().into(),
        scope: giga_scope_params(value.scope()),
        range: RequiredNullable(value.range().map(|range| GigaSourceRangeParams {
            start: range.start(),
            end: range.end(),
        })),
    }
}

fn giga_lifecycle_params(value: &GigaLifecycle) -> GigaLifecycleParams {
    let field = |name| {
        value
            .field(name)
            .expect("GigaLifecycle constructor guarantees event-specific fields")
            .to_owned()
    };
    match value.event_type() {
        GigaEventType::ConversationWindow => {
            GigaLifecycleParams::ConversationWindow(ConversationWindowLifecycle {})
        }
        GigaEventType::TaskStarted => GigaLifecycleParams::TaskStarted(TaskStartedLifecycle {
            task_reference: field("task_reference"),
            worker_id: field("worker_id"),
            worker_role: field("worker_role"),
            phase: field("phase"),
            project_key: field("project_key"),
            task_kind: field("task_kind"),
            risk: value
                .risk()
                .expect("task_started lifecycle always has risk")
                .as_str()
                .into(),
            target: field("target"),
            change: field("change"),
            proof_contract: value.proof_contract().to_vec(),
        }),
        GigaEventType::TaskCompleted => {
            GigaLifecycleParams::TaskCompleted(TaskCompletedLifecycle {
                task_reference: field("task_reference"),
                outcome: field("outcome"),
                verification_result: field("verification_result"),
            })
        }
        GigaEventType::SubagentDispatched => {
            GigaLifecycleParams::SubagentDispatched(SubagentDispatchedLifecycle {
                subagent_reference: field("subagent_reference"),
                parent_task: field("parent_task"),
                role: field("role"),
                target: field("target"),
                change: field("change"),
                acceptance: value.proof_contract().to_vec(),
            })
        }
        GigaEventType::SubagentCompleted => {
            GigaLifecycleParams::SubagentCompleted(SubagentCompletedLifecycle {
                subagent_reference: field("subagent_reference"),
                parent_task: field("parent_task"),
                outcome: field("outcome"),
            })
        }
        GigaEventType::TodoTransition => {
            GigaLifecycleParams::TodoTransition(TodoTransitionLifecycle {
                todo_reference: field("todo_reference"),
                previous_state: field("previous_state"),
                new_state: field("new_state"),
            })
        }
        GigaEventType::ToolOutcome => GigaLifecycleParams::ToolOutcome(ToolOutcomeLifecycle {
            tool_name: field("tool_name"),
            status: field("status"),
            sanitized_outcome: field("sanitized_outcome"),
        }),
        GigaEventType::ManualReprocess => {
            let range = value
                .source_range()
                .expect("manual_reprocess lifecycle always has a source range");
            GigaLifecycleParams::ManualReprocess(ManualReprocessLifecycle {
                source_range: GigaSourceRangeParams {
                    start: range.start(),
                    end: range.end(),
                },
                reason: field("reason"),
                operator_identity: field("operator_identity"),
            })
        }
    }
}

impl From<&GigaEvent> for GigaEventParams {
    fn from(value: &GigaEvent) -> Self {
        Self {
            event_schema_version: value.event_schema_version(),
            event_id: value.event_id().into(),
            event_type: value.event_type().as_str().into(),
            room: value.room().to_string(),
            session_id: value.session_id().into(),
            project_keys: value.project_keys().to_vec(),
            source_refs: value.source_refs().iter().map(giga_source_params).collect(),
            lifecycle: giga_lifecycle_params(value.lifecycle()),
            created_at: value.created_at().into(),
        }
    }
}

impl From<GigaEventClaimReceipt> for GigaEventClaimResult {
    fn from(receipt: GigaEventClaimReceipt) -> Self {
        Self {
            room: receipt.room().to_string(),
            worker_id: receipt.worker_id().into(),
            claimed_at: receipt.claimed_at().into(),
            event: RequiredNullable(receipt.event().map(GigaEventParams::from)),
            lease_expires_at: RequiredNullable(receipt.lease_expires_at().map(str::to_owned)),
            attempt_count: RequiredNullable(receipt.attempt_count()),
        }
    }
}

impl From<GigaEventFinishReceipt> for GigaEventFinishResult {
    fn from(receipt: GigaEventFinishReceipt) -> Self {
        Self {
            room: receipt.room().to_string(),
            event_id: receipt.event_id().into(),
            worker_id: receipt.worker_id().into(),
            outcome: receipt.outcome().as_str().into(),
            queue_state: receipt.queue_state().as_str().into(),
            attempt_count: receipt.attempt_count(),
            candidate_count: receipt.candidate_count(),
            available_at: RequiredNullable(receipt.available_at().map(str::to_owned)),
            finished_at: receipt.finished_at().into(),
        }
    }
}

impl From<GigaEventReplayReceipt> for GigaEventReplayResult {
    fn from(receipt: GigaEventReplayReceipt) -> Self {
        Self {
            room: receipt.room().to_string(),
            event_id: receipt.event_id().into(),
            operator_identity: receipt.operator_identity().into(),
            previous_state: receipt.previous_state().as_str().into(),
            queue_state: receipt.queue_state().as_str().into(),
            attempt_count: receipt.attempt_count(),
            replayed_at: receipt.replayed_at().into(),
        }
    }
}

impl From<GigaPromotionReceipt> for GigaPromoteResult {
    fn from(receipt: GigaPromotionReceipt) -> Self {
        let review_state = receipt.review_state().as_str().to_owned();
        let authority = receipt.authority().as_str().to_owned();
        let candidate_id = receipt.candidate_id().to_owned();
        let reviewer_id = receipt.reviewer_id().to_owned();
        let operator_identity = receipt.operator_identity().to_owned();
        let reviewed_at = receipt.reviewed_at().to_owned();
        let committed_at = receipt.committed_at().to_owned();
        match &receipt {
            GigaPromotionReceipt::Memory(memory) => Self::Memory {
                candidate_id,
                review_state,
                memory_id: memory.memory_id(),
                room: memory.room().to_string(),
                durable: true,
                authority,
                warnings: Vec::new(),
                reviewer_id,
                operator_identity,
                reviewed_at,
                committed_at,
            },
            GigaPromotionReceipt::CodingLesson(lesson) => Self::CodingLesson {
                candidate_id,
                review_state,
                coding_lesson_id: lesson.coding_lesson_id(),
                scope: lesson.scope().into(),
                durable: true,
                authority,
                warnings: Vec::new(),
                reviewer_id,
                operator_identity,
                reviewed_at,
                committed_at,
            },
            GigaPromotionReceipt::ProjectLesson(lesson) => Self::ProjectLesson {
                candidate_id,
                review_state,
                project_lesson_id: lesson.project_lesson_id(),
                project: lesson.project().into(),
                durable: true,
                authority,
                warnings: Vec::new(),
                reviewer_id,
                operator_identity,
                reviewed_at,
                committed_at,
            },
        }
    }
}

fn giga_scope(value: GigaScopeParams) -> Result<GigaScope, ProtocolError> {
    GigaScope::new(
        value.room.0,
        value.project.0,
        GigaVisibility::parse(&value.visibility)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
        value.publication_review_required,
    )
    .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
}
fn giga_source(value: GigaSourceRefParams) -> Result<GigaSourceRef, ProtocolError> {
    let range = value
        .range
        .0
        .map(|range| GigaSourceRange::new(range.start, range.end))
        .transpose()
        .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?;
    GigaSourceRef::new(
        GigaSourceType::parse(&value.source_type)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
        value.source_id,
        value.role,
        value.timestamp,
        value.content_hash,
        giga_scope(value.scope)?,
        range,
    )
    .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
}
fn giga_classifier(value: GigaClassifierParams) -> Result<GigaClassifierIdentity, ProtocolError> {
    GigaClassifierIdentity::new(
        value.model,
        value.provider_type,
        value.model_version,
        value.prompt_version,
        value.configuration_digest,
        value.run_id,
        value.completed_at,
    )
    .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
}
impl TryFrom<GigaEventParams> for GigaEvent {
    type Error = ProtocolError;
    fn try_from(value: GigaEventParams) -> Result<Self, Self::Error> {
        if value.event_schema_version != 1 {
            return Err(ProtocolError::InvalidParams(
                "unsupported event_schema_version".into(),
            ));
        }
        let event_type = GigaEventType::parse(&value.event_type)
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?;
        let source_refs = value
            .source_refs
            .into_iter()
            .map(giga_source)
            .collect::<Result<Vec<_>, _>>()?;
        GigaEvent::new(
            value.event_id,
            event_type,
            RoomKey::new(value.room)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.session_id,
            value.project_keys,
            source_refs,
            value.lifecycle.into_core()?,
            value.created_at,
        )
        .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}
impl TryFrom<GigaEventClaimResult> for GigaEventClaimReceipt {
    type Error = ProtocolError;

    fn try_from(value: GigaEventClaimResult) -> Result<Self, Self::Error> {
        let event = value.event.0.map(GigaEvent::try_from).transpose()?;
        GigaEventClaimReceipt::new(
            RoomKey::new(value.room)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.worker_id,
            value.claimed_at,
            event,
            value.lease_expires_at.0,
            value.attempt_count.0,
        )
        .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}

impl TryFrom<GigaEventFinishResult> for GigaEventFinishReceipt {
    type Error = ProtocolError;

    fn try_from(value: GigaEventFinishResult) -> Result<Self, Self::Error> {
        GigaEventFinishReceipt::new(
            RoomKey::new(value.room)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.event_id,
            value.worker_id,
            GigaEventFinishOutcome::parse(&value.outcome)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            GigaQueueState::parse(&value.queue_state)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.attempt_count,
            value.candidate_count,
            value.available_at.0,
            value.finished_at,
        )
        .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}

impl TryFrom<GigaEventReplayResult> for GigaEventReplayReceipt {
    type Error = ProtocolError;

    fn try_from(value: GigaEventReplayResult) -> Result<Self, Self::Error> {
        GigaEventReplayReceipt::new(
            RoomKey::new(value.room)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.event_id,
            value.operator_identity,
            GigaQueueState::parse(&value.previous_state)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            GigaQueueState::parse(&value.queue_state)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.attempt_count,
            value.replayed_at,
        )
        .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}

impl TryFrom<GigaPromoteResult> for GigaPromotionReceipt {
    type Error = ProtocolError;

    fn try_from(value: GigaPromoteResult) -> Result<Self, Self::Error> {
        fn validate_common(
            review_state: &str,
            durable: bool,
            authority: &str,
        ) -> Result<(), ProtocolError> {
            if !durable
                || GigaReviewState::parse(review_state)
                    .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?
                    != GigaReviewState::Promoted
                || GigaPromotionAuthority::parse(authority)
                    .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?
                    != GigaPromotionAuthority::Full
            {
                return Err(ProtocolError::InvalidParams(
                    "promotion result must be durable, promoted, and full-authority".into(),
                ));
            }
            Ok(())
        }

        let result = match value {
            GigaPromoteResult::Memory {
                candidate_id,
                review_state,
                memory_id,
                room,
                durable,
                authority,
                warnings: _,
                reviewer_id,
                operator_identity,
                reviewed_at,
                committed_at,
            } => {
                validate_common(&review_state, durable, &authority)?;
                GigaPromotionReceipt::memory(
                    candidate_id,
                    memory_id,
                    RoomKey::new(room)
                        .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
                    reviewer_id,
                    operator_identity,
                    reviewed_at,
                    committed_at,
                )
            }
            GigaPromoteResult::CodingLesson {
                candidate_id,
                review_state,
                coding_lesson_id,
                scope,
                durable,
                authority,
                warnings: _,
                reviewer_id,
                operator_identity,
                reviewed_at,
                committed_at,
            } => {
                validate_common(&review_state, durable, &authority)?;
                GigaPromotionReceipt::coding_lesson(
                    candidate_id,
                    coding_lesson_id,
                    scope,
                    reviewer_id,
                    operator_identity,
                    reviewed_at,
                    committed_at,
                )
            }
            GigaPromoteResult::ProjectLesson {
                candidate_id,
                review_state,
                project_lesson_id,
                project,
                durable,
                authority,
                warnings: _,
                reviewer_id,
                operator_identity,
                reviewed_at,
                committed_at,
            } => {
                validate_common(&review_state, durable, &authority)?;
                GigaPromotionReceipt::project_lesson(
                    candidate_id,
                    project_lesson_id,
                    project,
                    reviewer_id,
                    operator_identity,
                    reviewed_at,
                    committed_at,
                )
            }
        };
        result.map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}
impl TryFrom<GigaCandidateParams> for GigaCandidate {
    type Error = ProtocolError;
    fn try_from(value: GigaCandidateParams) -> Result<Self, Self::Error> {
        if value.candidate_schema_version != 1 {
            return Err(ProtocolError::InvalidParams(
                "unsupported candidate_schema_version".into(),
            ));
        }
        let source_refs = value
            .source_refs
            .into_iter()
            .map(giga_source)
            .collect::<Result<Vec<_>, _>>()?;
        GigaCandidate::new(
            value.candidate_id,
            value.event_id,
            RoomKey::new(value.room)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.session_id,
            GigaCandidateKind::parse(&value.kind)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            source_refs,
            value.proof_refs,
            GigaScores::new(
                value.priority,
                value.novelty,
                value.durability,
                value.confidence,
            )
            .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.project_keys,
            value.thread_keys,
            value.entity_hints,
            value.retrieval_terms,
            value.proposed_title,
            value.gist,
            value.rationale,
            giga_scope(value.scope)?,
            GigaAuthority::parse(&value.authority)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            GigaReviewState::parse(&value.review_state)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            giga_classifier(value.classifier)?,
            value.created_at,
            value.expires_at.0,
            value.promotion_refs,
        )
        .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}
impl TryFrom<GigaReviewParams> for GigaReviewAction {
    type Error = ProtocolError;
    fn try_from(value: GigaReviewParams) -> Result<Self, Self::Error> {
        let source_refs = value
            .source_refs
            .into_iter()
            .map(giga_source)
            .collect::<Result<Vec<_>, _>>()?;
        let resonance = value
            .resonance
            .0
            .map(|resonance| {
                let refs = resonance
                    .source_refs
                    .into_iter()
                    .map(giga_source)
                    .collect::<Result<Vec<_>, _>>()?;
                GigaResonance::new(
                    resonance.event_id,
                    resonance.score,
                    giga_classifier(resonance.classifier)?,
                    refs,
                )
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
            })
            .transpose()?;
        GigaReviewAction::new(
            value.candidate_id,
            value.reviewer_id,
            GigaReviewState::parse(&value.previous_state)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            GigaReviewState::parse(&value.new_state)
                .map_err(|error| ProtocolError::InvalidParams(error.to_string()))?,
            value.reason,
            value.authorization_basis,
            source_refs,
            value.promotion_target.0,
            value.merge_target.0,
            value.merge_source_candidates,
            resonance,
            value.reviewed_at,
        )
        .map_err(|error| ProtocolError::InvalidParams(error.to_string()))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use house_core::GigaPromotionKind;
    #[test]
    fn canon_wire_is_typed_and_refuses_flattened_or_malformed_fields() {
        let write = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"c1","method":"canon_write","params":{"room":"house","name":"The Athanor","kind":"project","summary":"Current authority","pointerFiles":[{"file":"canon.md","lines":[2,7]}],"supersedes":["41"],"attribution":{"actor":"Kintsu","origin":"omp:call-1"}}}"#,
        )
        .unwrap()
        .canon_write_request()
        .unwrap();
        assert_eq!(write.name(), "The Athanor");
        assert_eq!(write.supersedes(), &[41]);

        let malformed = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"c2","method":"canon_write","params":{"room":"house","name":"The Athanor","kind":"project","summary":"Current authority","pointerFiles":[{"file":"canon.md","lines":["2",7]}],"attribution":{"actor":"Kintsu","origin":"omp:call-2"}}}"#,
        )
        .unwrap()
        .canon_write_request();
        assert!(malformed.is_err());

        let flattened = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"c3","method":"remember","params":{"room":"house","kind":"canon","title":"The Athanor","body":"Current authority"}}"#,
        )
        .unwrap()
        .remember_request();
        assert!(flattened.is_err());

        let read = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"c4","method":"canon_read","params":{"room":"house","id":"41","includeHistory":true}}"#,
        )
        .unwrap()
        .canon_read_request()
        .unwrap();
        assert!(read.include_history());
    }

    #[test]
    fn paper_boat_methods_are_strict_and_domain_prefixed() {
        let sleep = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"s1","method":"paper_boat_sleep","params":{"room":"kintsu","body":"tomorrow's letter"}}"#,
        )
        .unwrap()
        .paper_boat_sleep_request()
        .unwrap();
        assert_eq!(sleep.room().as_str(), "kintsu");
        assert!(sleep.backup());

        let empty = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"s2","method":"paper_boat_sleep","params":{"room":"kintsu","body":" "}}"#,
        )
        .unwrap()
        .paper_boat_sleep_request();
        assert!(empty.is_err());

        let unknown = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"w1","method":"wake","params":{"room":"kintsu"}}"#,
        )
        .unwrap()
        .paper_boat_wake_request();
        assert!(matches!(unknown, Err(ProtocolError::UnknownMethod(_))));

        let wake = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"w2","method":"paper_boat_wake","params":{"room":"other-room"}}"#,
        )
        .unwrap()
        .paper_boat_wake_request()
        .unwrap();
        assert_eq!(wake.room().as_str(), "other-room");
    }

    #[test]
    fn diagnostic_details_round_trip_with_machine_actions() {
        let target = DiagnosticTarget::new(
            DiagnosticTargetKind::File,
            "crates/house-substrate/src/lib.rs",
        );
        let details =
            DiagnosticDetails::new(DiagnosticCategory::Database, DiagnosticStage::Transaction)
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
        let protocol =
            ProtocolErrorBody::protocol(ProtocolError::InvalidParams("bad".into())).build();
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

        let arbitrary_details: ResponseEnvelope<Value> =
            serde_json::from_value(serde_json::json!({
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
                serde_json::from_value::<DiagnosticWriteOutcome>(serde_json::json!(wire)).unwrap(),
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
        let house_memory = RequestEnvelope {
            protocol: 1,
            id: "r".into(),
            method: "remember".into(),
            params: serde_json::json!({"room":"house","kind":"memory","title":"T","body":"B"}),
        };
        assert_eq!(
            house_memory.remember_request().unwrap().room().as_str(),
            "house"
        );
        let house_lesson = RequestEnvelope {
            protocol: 1,
            id: "r2".into(),
            method: "remember".into(),
            params: serde_json::json!({"room":"house","kind":"coding-lesson","title":"T","body":"B"}),
        };
        assert!(house_lesson.remember_request().is_err());
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
    fn writing_and_design_registers_round_trip_and_memory_refuses_them() {
        let writing = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"x","method":"remember","params":{"room":"kintsu","kind":"writing-lesson","title":"T","body":"B","register":[" fiction ","product-work","fiction"]}}"#,
        )
        .unwrap()
        .remember_request()
        .unwrap();
        assert_eq!(writing.register(), &["fiction", "product-work"]);

        let design = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"x","method":"remember","params":{"room":"kintsu","kind":"design-lesson","title":"T","body":"B","voice":"solarisael","register":[" general ","general"],"shape":"component-contract","proofPattern":"Verify keyboard navigation.","triggerContext":"Before introducing a component.","exampleText":"Use the token, not a one-off value.","tags":["accessibility"]}}"#,
        )
        .unwrap()
        .remember_request()
        .unwrap();
        assert_eq!(design.kind(), RememberKind::DesignLesson);
        assert_eq!(design.register(), &["general"]);
        assert_eq!(
            design.example_text(),
            Some("Use the token, not a one-off value.")
        );

        let memory = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"x","method":"remember","params":{"room":"kintsu","kind":"memory","title":"T","body":"B","register":["fiction"]}}"#,
        )
        .unwrap();
        assert!(matches!(
            memory.remember_request(),
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
            source_memory_path: None,
            threads: vec![],
            continues: vec![],
            supersedes: vec!["41".into(), "42".into(), "41".into()],
            shape: None,
            voice: None,
            scope: None,
            project: None,
            proof_pattern: None,
            trigger_context: None,
            example_text: None,
            language_keys: vec![],
            thread_keys: vec![],
            technology_keys: vec![],
            register: vec![],
            tags: vec![],
            condition: vec![],
            ast_condition: vec![],
            trigger_scope: vec![],
            interrupt_mode: None,
            repeat_cooldown_secs: None,
            backup: Some(true),
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
            source_memory_path: None,
            threads: vec![],
            continues: vec![],
            shape: None,
            voice: None,
            scope: None,
            project: None,
            register: vec![],
            proof_pattern: None,
            trigger_context: None,
            example_text: None,
            language_keys: vec![],
            technology_keys: vec![],
            thread_keys: vec![],
            tags: vec![],
            condition: vec![],
            ast_condition: vec![],
            trigger_scope: vec![],
            interrupt_mode: None,
            repeat_cooldown_secs: None,
            backup: Some(true),
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
                source_memory_path: None,
                threads: vec![],
                continues: vec![],
                shape: None,
                voice: None,
                scope: None,
                project: None,
                register: vec![],
                proof_pattern: None,
                trigger_context: None,
                example_text: None,
                language_keys: vec![],
                technology_keys: vec![],
                thread_keys: vec![],
                tags: vec![],
                condition: vec![],
                ast_condition: vec![],
                trigger_scope: vec![],
                interrupt_mode: None,
                repeat_cooldown_secs: None,
                backup: Some(true),
            };
            assert!(matches!(
                RememberRequest::try_from(params),
                Err(ProtocolError::InvalidParams(_))
            ));
        }
    }

    #[test]
    fn continuation_ids_round_trip_separately_from_supersession() {
        let request = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"x","method":"remember","params":{"room":"lab","kind":"memory","title":"T","body":"B","threads":["work / page"],"continues":[{"thread":"work / page","previousMemoryId":"41"}],"supersedes":["7"]}}"#,
        )
        .unwrap()
        .remember_request()
        .unwrap();
        assert_eq!(
            request.continues(),
            &[ThreadContinuation {
                thread: "work / page".into(),
                previous_memory_id: 41,
            }]
        );
        assert_eq!(request.supersedes(), &[7]);

        for params in [
            serde_json::json!({
                "room":"lab",
                "kind":"memory",
                "title":"T",
                "body":"B",
                "threads":["work / page"],
                "continues":[{"thread":"work / page","previousMemoryId":"0"}]
            }),
            serde_json::json!({
                "room":"lab",
                "kind":"memory",
                "title":"T",
                "body":"B",
                "threads":["other"],
                "continues":[{"thread":"work / page","previousMemoryId":"41"}]
            }),
        ] {
            let envelope = RequestEnvelope {
                protocol: 1,
                id: "x".into(),
                method: "remember".into(),
                params,
            };
            assert!(matches!(
                envelope.remember_request(),
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
        assert_eq!(recall.semantic_min_similarity(), 0.40);
        assert_eq!(recall.content_top_k(), 8);
        assert_eq!(recall.content_min_similarity(), 0.30);
        assert!(!recall.temporal_decay());

        let explicit = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"r","method":"recall","params":{"room":"lab","query":"alpha","temporal_decay":true}}"#,
        )
        .unwrap()
        .recall_request()
        .unwrap();
        assert!(explicit.temporal_decay());

        let params: RecallParams =
            serde_json::from_value(serde_json::json!({"room":"lab","query":"alpha"})).unwrap();
        assert_eq!(
            serde_json::to_string(&params).unwrap(),
            r#"{"room":"lab","query":"alpha","semantic_top_k":8,"semantic_min_similarity":0.4,"content_top_k":8,"content_min_similarity":0.3,"temporal_decay":false}"#
        );
    }

    #[test]
    fn vault_recall_is_a_strict_database_free_request_shape() {
        let request = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"v","method":"vault_recall","params":{"room":"lab","room_dir":"/rooms/lab","query":"hinge protocol"}}"#,
        )
        .unwrap()
        .vault_recall_request()
        .unwrap();
        assert_eq!(
            request,
            VaultRecallParams {
                room: "lab".into(),
                room_dir: "/rooms/lab".into(),
                query: "hinge protocol".into(),
            }
        );
        for params in [
            serde_json::json!({"room":"","room_dir":"/rooms/lab","query":"hinge"}),
            serde_json::json!({"room":"lab","room_dir":"","query":"hinge"}),
            serde_json::json!({"room":"lab","room_dir":"/rooms/lab","query":" "}),
            serde_json::json!({"room":"lab","room_dir":"/rooms/lab","query":"hinge","database_url":"forbidden"}),
        ] {
            assert!(
                RequestEnvelope {
                    protocol: 1,
                    id: "v".into(),
                    method: "vault_recall".into(),
                    params,
                }
                .vault_recall_request()
                .is_err()
            );
        }
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
            temporal_decay: false,
        };
        assert!(RecallRequest::try_from(params).is_err());
        let unknown = serde_json::json!({
            "room":"lab",
            "query":"x",
            "temporal_decay":true,
            "extra":true
        });
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
    fn all_lesson_kinds_validate_defaults_and_receipts() {
        for kind in [
            "coding-lesson",
            "project-lesson",
            "writing-lesson",
            "design-lesson",
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
        assert!(base(serde_json::json!({"room":"lab","kind":"design-lesson","title":"T","body":"B","scope":"house"})).remember_request().is_err());
        assert!(base(serde_json::json!({"room":"lab","kind":"project-lesson","title":"T","body":"B","project":"x","shape":"process"})).remember_request().is_ok());
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
        let live_staleness: ClusterStalenessTelemetry = serde_json::from_value(serde_json::json!({"built_at":"2026-08-14T12:00:00Z","clusters":3,"chunks_total":4200,"chunks_since_build":250,"fraction_unseen":0.05})).unwrap();
        assert_eq!(live_staleness.chunks_total, Some(4200));
        let resonance: ClusterResonanceTelemetry = serde_json::from_value(serde_json::json!({"profile":[{"label":"x","activation":0.9,"member_count":2}],"hot":["chunk"]})).unwrap();
        assert_eq!(resonance.profile[0].member_count, 2);
        let live: ClusterResonanceTelemetry = serde_json::from_value(serde_json::json!({"profile":[{"cluster_id":7,"label":"x","activation":0.9,"member_count":2}],"hot":["chunk"]})).unwrap();
        assert_eq!(live.profile[0].cluster_id, Some(7));
        assert!(live.serialize(serde_json::value::Serializer).is_ok_and(|v| v["profile"][0]["cluster_id"] == 7));
        assert!(serde_json::from_value::<ClusterResonanceTelemetry>(serde_json::json!({"profile":[{"label":"x","activation":0.9,"member_count":2,"rogue":1}],"hot":[]})).is_err());
        assert!(serde_json::from_value::<ClusterStalenessTelemetry>(serde_json::json!({"built_at":null,"chunks_since_build":1,"fraction_unseen":0.1,"bad":true})).is_err());
    }

    #[test]
    fn recall_result_defaults_warnings_without_weakening_strict_fields() {
        let result: RecallResult = serde_json::from_value(serde_json::json!({
            "ok": true,
            "query": "bounded query",
            "found": false,
            "source": "postgres",
            "retrievalCandidates": [],
            "canonMatches": [],
            "semanticChunks": [],
            "contentChunks": [],
            "dateMatches": [],
            "queryDates": [],
            "taxonomy": {},
            "cluster": null,
            "clusters": null,
            "clusterStaleness": null,
            "clusterResonance": null,
            "memoryHandle": null
        }))
        .unwrap();
        assert!(result.warnings.is_empty());
        let mut with_warning = serde_json::to_value(result).unwrap();
        assert_eq!(with_warning["warnings"], serde_json::json!([]));
        with_warning["warnings"] = serde_json::json!(["semantic lane unavailable"]);
        let decoded: RecallResult = serde_json::from_value(with_warning).unwrap();
        assert_eq!(decoded.warnings, vec!["semantic lane unavailable"]);
        let mut unknown = serde_json::to_value(decoded).unwrap();
        unknown["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<RecallResult>(unknown).is_err());
    }
    fn giga_private_source_json() -> Value {
        serde_json::json!({
            "source_type": "turn",
            "source_id": "turn-1",
            "role": "user",
            "timestamp": "2026-07-24T12:00:00Z",
            "content_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "scope": {
                "room": "lab",
                "project": null,
                "visibility": "private",
                "publication_review_required": false
            },
            "range": null
        })
    }

    fn giga_project_source_json() -> Value {
        serde_json::json!({
            "source_type": "turn",
            "source_id": "turn-2",
            "role": "user",
            "timestamp": "2026-07-24T12:00:00Z",
            "content_hash": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "scope": {
                "room": "lab",
                "project": "athanor",
                "visibility": "private",
                "publication_review_required": true
            },
            "range": null
        })
    }

    #[test]
    fn giga_curio_review_wire_keeps_new_event_resonance_sources_separate() {
        let mut new_source = giga_private_source_json();
        new_source["source_id"] = serde_json::json!("turn-new");
        new_source["timestamp"] = serde_json::json!("2026-07-24T12:01:00Z");
        new_source["content_hash"] =
            serde_json::json!("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc");
        let action = RequestEnvelope {
            protocol: 1,
            id: "review".into(),
            method: "giga_review".into(),
            params: serde_json::json!({
                "candidate_id": "candidate-curio",
                "reviewer_id": "governing-spirit",
                "previous_state": "curio",
                "new_state": "in_review",
                "reason": "new event resonates with the retained curio",
                "authorization_basis": "deliberate local review",
                "source_refs": [giga_private_source_json()],
                "promotion_target": null,
                "merge_target": null,
                "merge_source_candidates": [],
                "resonance": {
                    "event_id": "event-new",
                    "score": 0.9,
                    "classifier": {
                        "model": "resonance-model",
                        "provider_type": "ollama",
                        "model_version": "manifest",
                        "prompt_version": "resonance-v1",
                        "configuration_digest": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                        "run_id": "resonance-run",
                        "completed_at": "2026-07-24T12:01:01Z"
                    },
                    "source_refs": [new_source]
                },
                "reviewed_at": "2026-07-24T12:02:00Z"
            }),
        }
        .giga_review_request()
        .unwrap();
        assert_eq!(action.source_refs()[0].source_id(), "turn-1");
        assert_eq!(
            action.resonance().unwrap().source_refs()[0].source_id(),
            "turn-new"
        );
    }

    fn giga_promote_params_json(kind: &str) -> Value {
        let (source, payload, publication_consent) = match kind {
            "memory" => (
                giga_private_source_json(),
                serde_json::json!({
                    "title": "Edited memory",
                    "body": "Human-reviewed durable body",
                    "threads": ["consent"]
                }),
                Value::Null,
            ),
            "coding_lesson" => (
                giga_private_source_json(),
                serde_json::json!({
                    "title": "Sanitize inherited state",
                    "body": "Clear inherited variables before invoking tools.",
                    "shape": "process",
                    "proof_pattern": "failure then passing proof",
                    "trigger_context": "inherited environment state reaches a child tool process",
                    "tags": ["environment"]
                }),
                Value::Null,
            ),
            "project_lesson" => (
                giga_project_source_json(),
                serde_json::json!({
                    "title": "Stable Athanor rule",
                    "body": "Keep queue mutations transactional.",
                    "project": "athanor",
                    "proof_pattern": "rollback observed",
                    "trigger_context": "queue work crosses a durable transaction boundary",
                    "tags": ["queue"]
                }),
                serde_json::json!({
                    "operator_approved": true,
                    "reviewer_approved": true
                }),
            ),
            other => panic!("unsupported test kind: {other}"),
        };
        serde_json::json!({
            "candidate_id": format!("candidate-{kind}"),
            "room": "lab",
            "reviewer_id": "kintsu",
            "operator_identity": "sol",
            "authorization_basis": "reviewed exact source and edited payload",
            "source_refs": [source],
            "target": {
                "kind": kind,
                "payload": payload
            },
            "publication_consent": publication_consent,
            "reviewed_at": "2026-07-24T12:04:00Z"
        })
    }

    #[test]
    fn giga_queue_wire_accepts_valid_claim_finish_and_authorized_replay() {
        let claim = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"claim","method":"giga_event_claim","params":{"room":"lab","worker_id":"agents-a1","lease_seconds":60}}"#,
        )
        .unwrap()
        .giga_event_claim_request()
        .unwrap();
        assert_eq!(claim.room().as_str(), "lab");
        assert_eq!(claim.worker_id(), "agents-a1");
        assert_eq!(claim.lease_seconds(), 60);

        let finish = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"finish","method":"giga_event_finish","params":{"room":"lab","event_id":"event-1","worker_id":"agents-a1","outcome":"retry","candidate_count":0,"error_class":"model_timeout","retry_after_seconds":60}}"#,
        )
        .unwrap()
        .giga_event_finish_request()
        .unwrap();
        assert_eq!(finish.outcome(), GigaEventFinishOutcome::Retry);
        assert_eq!(finish.error_class(), Some("model_timeout"));
        assert_eq!(finish.retry_after_seconds(), Some(60));

        let replay = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"replay","method":"giga_event_replay","params":{"room":"lab","event_id":"event-1","operator_identity":"sol","authorization_basis":"operator requested replay after prompt repair"}}"#,
        )
        .unwrap()
        .giga_event_replay_request()
        .unwrap();
        assert_eq!(replay.operator_identity(), "sol");
        assert_eq!(
            replay.authorization_basis(),
            "operator requested replay after prompt repair"
        );
    }

    #[test]
    fn giga_queue_wire_enforces_lease_retry_and_replay_contracts() {
        let claim = |lease_seconds| RequestEnvelope {
            protocol: 1,
            id: "claim".into(),
            method: "giga_event_claim".into(),
            params: serde_json::json!({
                "room": "lab",
                "worker_id": "agents-a1",
                "lease_seconds": lease_seconds
            }),
        };
        for lease_seconds in [1, house_core::GIGA_MAX_LEASE_SECONDS] {
            assert!(claim(lease_seconds).giga_event_claim_request().is_ok());
        }
        for lease_seconds in [0, house_core::GIGA_MAX_LEASE_SECONDS + 1] {
            assert!(matches!(
                claim(lease_seconds).giga_event_claim_request(),
                Err(ProtocolError::InvalidParams(message)) if message.contains("lease_seconds")
            ));
        }

        for (outcome, retry_after_seconds) in
            [("retry", serde_json::json!(1)), ("failed", Value::Null)]
        {
            let request = RequestEnvelope {
                protocol: 1,
                id: "finish".into(),
                method: "giga_event_finish".into(),
                params: serde_json::json!({
                    "room": "lab",
                    "event_id": "event-1",
                    "worker_id": "agents-a1",
                    "outcome": outcome,
                    "candidate_count": 0,
                    "error_class": null,
                    "retry_after_seconds": retry_after_seconds
                }),
            };
            assert!(matches!(
                request.giga_event_finish_request(),
                Err(ProtocolError::InvalidParams(message)) if message.contains("error_class")
            ));
        }

        let final_retry: GigaEventFinishResult = serde_json::from_value(serde_json::json!({
            "room": "lab",
            "event_id": "event-1",
            "worker_id": "agents-a1",
            "outcome": "retry",
            "queue_state": "pending",
            "attempt_count": house_core::GIGA_MAX_EVENT_ATTEMPTS,
            "candidate_count": 0,
            "available_at": "2026-07-24T12:02:00Z",
            "finished_at": "2026-07-24T12:01:00Z"
        }))
        .unwrap();
        assert!(matches!(
            GigaEventFinishReceipt::try_from(final_retry),
            Err(ProtocolError::InvalidParams(message)) if message.contains("final bounded attempt")
        ));

        for (operator_identity, authorization_basis) in
            [("", "operator requested replay"), ("sol", " ")]
        {
            let request = RequestEnvelope {
                protocol: 1,
                id: "replay".into(),
                method: "giga_event_replay".into(),
                params: serde_json::json!({
                    "room": "lab",
                    "event_id": "event-1",
                    "operator_identity": operator_identity,
                    "authorization_basis": authorization_basis
                }),
            };
            assert!(request.giga_event_replay_request().is_err());
        }
    }

    #[test]
    fn giga_queue_wire_rejects_client_arbitration_timestamps() {
        assert!(
            RequestEnvelope {
                protocol: 1,
                id: "claim".into(),
                method: "giga_event_claim".into(),
                params: serde_json::json!({
                    "room":"lab",
                    "worker_id":"agents-a1",
                    "lease_seconds":60,
                    "claimed_at":"2099-01-01T00:00:00Z"
                }),
            }
            .giga_event_claim_request()
            .is_err()
        );
        assert!(
            RequestEnvelope {
                protocol: 1,
                id: "finish".into(),
                method: "giga_event_finish".into(),
                params: serde_json::json!({
                    "room":"lab",
                    "event_id":"event-1",
                    "worker_id":"agents-a1",
                    "outcome":"succeeded",
                    "candidate_count":0,
                    "error_class":null,
                    "retry_after_seconds":null,
                    "finished_at":"2099-01-01T00:00:00Z"
                }),
            }
            .giga_event_finish_request()
            .is_err()
        );
        assert!(
            RequestEnvelope {
                protocol: 1,
                id: "replay".into(),
                method: "giga_event_replay".into(),
                params: serde_json::json!({
                    "room":"lab",
                    "event_id":"event-1",
                    "operator_identity":"operator",
                    "authorization_basis":"deliberate replay",
                    "replayed_at":"2099-01-01T00:00:00Z"
                }),
            }
            .giga_event_replay_request()
            .is_err()
        );
    }

    #[test]
    fn giga_health_wire_requires_an_explicit_room_scope() {
        let request = RequestEnvelope {
            protocol: 1,
            id: "health".into(),
            method: "giga_health".into(),
            params: serde_json::json!({"room":"lab"}),
        }
        .giga_health_request()
        .unwrap();
        assert_eq!(request.room().as_str(), "lab");
        assert!(
            RequestEnvelope {
                protocol: 1,
                id: "health".into(),
                method: "giga_health".into(),
                params: serde_json::json!({}),
            }
            .giga_health_request()
            .is_err()
        );
    }

    #[test]
    fn giga_queue_maintenance_wire_is_exact_and_rejects_unknown_operations() {
        let check = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"check","method":"giga_queue_maintenance","params":{"room":"lab","operation":"check","scope":"room"}}"#,
        )
        .unwrap()
        .giga_queue_maintenance_request()
        .unwrap();
        assert_eq!(check.operation(), GigaQueueMaintenanceOperation::Check);
        assert_eq!(check.scope(), GigaQueueMaintenanceScope::Room);

        let purge = RequestEnvelope::parse_line(
            r#"{"protocol":1,"id":"purge","method":"giga_queue_maintenance","params":{"room":"lab","operation":"purge_stuck","scope":"all"}}"#,
        )
        .unwrap()
        .giga_queue_maintenance_request()
        .unwrap();
        assert_eq!(purge.operation(), GigaQueueMaintenanceOperation::PurgeStuck);

        for params in [
            serde_json::json!({
                "room": "lab",
                "operation": "purge_everything",
                "scope": "room"
            }),
            serde_json::json!({
                "room": "lab",
                "operation": "purge_stuck",
                "scope": "discardable_stage1"
            }),
            serde_json::json!({
                "room": "lab",
                "operation": "check",
                "scope": "room",
                "operator_identity": "sol"
            }),
        ] {
            assert!(
                RequestEnvelope {
                    protocol: 1,
                    id: "maintenance".into(),
                    method: "giga_queue_maintenance".into(),
                    params,
                }
                .giga_queue_maintenance_request()
                .is_err()
            );
        }
    }

    #[test]
    fn giga_memory_coding_and_project_promotion_wire_preserves_edited_targets() {
        for (kind, expected_kind) in [
            ("memory", GigaPromotionKind::Memory),
            ("coding_lesson", GigaPromotionKind::CodingLesson),
            ("project_lesson", GigaPromotionKind::ProjectLesson),
        ] {
            let request = RequestEnvelope {
                protocol: 1,
                id: format!("promote-{kind}"),
                method: "giga_promote".into(),
                params: giga_promote_params_json(kind),
            }
            .giga_promote_request()
            .unwrap();
            assert_eq!(request.payload().kind(), expected_kind);
            assert_eq!(request.source_refs().len(), 1);
            assert_eq!(
                request.source_refs()[0].source_id(),
                if kind == "project_lesson" {
                    "turn-2"
                } else {
                    "turn-1"
                }
            );
            assert_eq!(
                request.publication_consent().is_some(),
                kind == "project_lesson"
            );
        }
    }

    #[test]
    fn giga_lesson_promotion_wire_requires_proof_and_trigger_fields() {
        for kind in ["coding_lesson", "project_lesson"] {
            for field in ["proof_pattern", "trigger_context"] {
                let mut params = giga_promote_params_json(kind);
                params["target"]["payload"]
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                assert!(
                    RequestEnvelope {
                        protocol: 1,
                        id: "promotion".into(),
                        method: "giga_promote".into(),
                        params,
                    }
                    .giga_promote_request()
                    .is_err()
                );
            }
        }
    }

    #[test]
    fn giga_promotion_wire_rejects_kind_mismatch_unsupported_kinds_and_unedited_payloads() {
        let mut mismatch = giga_promote_params_json("memory");
        mismatch["target"]["kind"] = serde_json::json!("coding_lesson");
        assert!(
            RequestEnvelope {
                protocol: 1,
                id: "mismatch".into(),
                method: "giga_promote".into(),
                params: mismatch,
            }
            .giga_promote_request()
            .is_err()
        );

        for kind in [
            "correction",
            "supersession",
            "entity_update",
            "thread_update",
            "unknown",
        ] {
            let mut params = giga_promote_params_json("memory");
            params["target"]["kind"] = serde_json::json!(kind);
            assert!(
                RequestEnvelope {
                    protocol: 1,
                    id: format!("unsupported-{kind}"),
                    method: "giga_promote".into(),
                    params,
                }
                .giga_promote_request()
                .is_err()
            );
        }

        for (kind, field) in [
            ("memory", "title"),
            ("memory", "body"),
            ("coding_lesson", "title"),
            ("coding_lesson", "body"),
            ("project_lesson", "title"),
            ("project_lesson", "body"),
            ("project_lesson", "project"),
        ] {
            let mut params = giga_promote_params_json(kind);
            params["target"]["payload"][field] = serde_json::json!(" ");
            assert!(
                RequestEnvelope {
                    protocol: 1,
                    id: format!("blank-{kind}-{field}"),
                    method: "giga_promote".into(),
                    params,
                }
                .giga_promote_request()
                .is_err()
            );
        }
        let mut missing_payload = giga_promote_params_json("memory");
        missing_payload["target"]
            .as_object_mut()
            .unwrap()
            .remove("payload");
        assert!(
            RequestEnvelope {
                protocol: 1,
                id: "missing-payload".into(),
                method: "giga_promote".into(),
                params: missing_payload,
            }
            .giga_promote_request()
            .is_err()
        );
    }

    #[test]
    fn giga_project_promotion_wire_requires_both_publication_approvals() {
        for consent in [
            Value::Null,
            serde_json::json!({"operator_approved": false, "reviewer_approved": true}),
            serde_json::json!({"operator_approved": true, "reviewer_approved": false}),
            serde_json::json!({"operator_approved": false, "reviewer_approved": false}),
        ] {
            let mut params = giga_promote_params_json("project_lesson");
            params["publication_consent"] = consent;
            assert!(
                RequestEnvelope {
                    protocol: 1,
                    id: "project-consent".into(),
                    method: "giga_promote".into(),
                    params,
                }
                .giga_promote_request()
                .is_err()
            );
        }
    }

    #[test]
    fn giga_promotion_wire_rejects_unknown_fields_at_every_nested_authority_boundary() {
        let mut cases = Vec::new();

        let mut scope = giga_promote_params_json("project_lesson");
        scope["source_refs"][0]["scope"]["unexpected"] = serde_json::json!(true);
        cases.push(scope);

        let mut range = giga_promote_params_json("project_lesson");
        range["source_refs"][0]["range"] =
            serde_json::json!({"start": 0, "end": 5, "unexpected": true});
        cases.push(range);

        let mut source = giga_promote_params_json("project_lesson");
        source["source_refs"][0]["unexpected"] = serde_json::json!(true);
        cases.push(source);

        let mut target = giga_promote_params_json("project_lesson");
        target["target"]["unexpected"] = serde_json::json!(true);
        cases.push(target);

        let mut payload = giga_promote_params_json("project_lesson");
        payload["target"]["payload"]["unexpected"] = serde_json::json!(true);
        cases.push(payload);

        let mut consent = giga_promote_params_json("project_lesson");
        consent["publication_consent"]["unexpected"] = serde_json::json!(true);
        cases.push(consent);

        for params in cases {
            assert!(matches!(
                RequestEnvelope {
                    protocol: 1,
                    id: "nested-unknown".into(),
                    method: "giga_promote".into(),
                    params,
                }
                .giga_promote_request(),
                Err(ProtocolError::InvalidParams(_))
            ));
        }
    }

    #[test]
    fn giga_queue_receipts_have_exact_wire_shapes_and_reject_invalid_states() {
        let room = RoomKey::new("lab").unwrap();
        let claim = GigaEventClaimReceipt::new(
            room.clone(),
            "agents-a1".into(),
            "2026-07-24T12:00:00Z".into(),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&success("claim", GigaEventClaimResult::from(claim))).unwrap(),
            r#"{"protocol":1,"id":"claim","result":{"room":"lab","worker_id":"agents-a1","claimed_at":"2026-07-24T12:00:00Z","event":null,"lease_expires_at":null,"attempt_count":null}}"#
        );

        let finish = GigaEventFinishReceipt::new(
            room.clone(),
            "event-1".into(),
            "agents-a1".into(),
            GigaEventFinishOutcome::Succeeded,
            GigaQueueState::Succeeded,
            1,
            1,
            None,
            "2026-07-24T12:01:00Z".into(),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&success("finish", GigaEventFinishResult::from(finish))).unwrap(),
            r#"{"protocol":1,"id":"finish","result":{"room":"lab","event_id":"event-1","worker_id":"agents-a1","outcome":"succeeded","queue_state":"succeeded","attempt_count":1,"candidate_count":1,"available_at":null,"finished_at":"2026-07-24T12:01:00Z"}}"#
        );

        let replay = GigaEventReplayReceipt::new(
            room,
            "event-1".into(),
            "sol".into(),
            GigaQueueState::Failed,
            GigaQueueState::Pending,
            0,
            "2026-07-24T12:03:00Z".into(),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&success("replay", GigaEventReplayResult::from(replay))).unwrap(),
            r#"{"protocol":1,"id":"replay","result":{"room":"lab","event_id":"event-1","operator_identity":"sol","previous_state":"failed","queue_state":"pending","attempt_count":0,"replayed_at":"2026-07-24T12:03:00Z"}}"#
        );

        let invalid_replay: GigaEventReplayResult = serde_json::from_value(serde_json::json!({
            "room": "lab",
            "event_id": "event-1",
            "operator_identity": "sol",
            "previous_state": "failed",
            "queue_state": "pending",
            "attempt_count": 1,
            "replayed_at": "2026-07-24T12:03:00Z"
        }))
        .unwrap();
        assert!(GigaEventReplayReceipt::try_from(invalid_replay).is_err());
    }

    #[test]
    fn giga_promotion_receipts_are_exact_tagged_records_for_every_supported_kind() {
        let receipts = [
            GigaPromotionReceipt::memory(
                "candidate-memory".into(),
                11,
                RoomKey::new("lab").unwrap(),
                "kintsu".into(),
                "sol".into(),
                "2026-07-24T12:04:00Z".into(),
                "2026-07-24T12:05:00Z".into(),
            )
            .unwrap(),
            GigaPromotionReceipt::coding_lesson(
                "candidate-coding_lesson".into(),
                12,
                "lab".into(),
                "kintsu".into(),
                "sol".into(),
                "2026-07-24T12:04:00Z".into(),
                "2026-07-24T12:05:00Z".into(),
            )
            .unwrap(),
            GigaPromotionReceipt::project_lesson(
                "candidate-project_lesson".into(),
                13,
                "kintsu".into(),
                "kintsu".into(),
                "sol".into(),
                "2026-07-24T12:04:00Z".into(),
                "2026-07-24T12:05:00Z".into(),
            )
            .unwrap(),
        ];
        let expected = [
            serde_json::json!({
                "kind":"memory","candidate_id":"candidate-memory","review_state":"promoted",
                "memory_id":11,"room":"lab","durable":true,"authority":"full","warnings":[],
                "reviewer_id":"kintsu","operator_identity":"sol",
                "reviewed_at":"2026-07-24T12:04:00Z","committed_at":"2026-07-24T12:05:00Z"
            }),
            serde_json::json!({
                "kind":"coding_lesson","candidate_id":"candidate-coding_lesson",
                "review_state":"promoted","coding_lesson_id":12,"scope":"lab","durable":true,
                "authority":"full","warnings":[],"reviewer_id":"kintsu","operator_identity":"sol",
                "reviewed_at":"2026-07-24T12:04:00Z","committed_at":"2026-07-24T12:05:00Z"
            }),
            serde_json::json!({
                "kind":"project_lesson","candidate_id":"candidate-project_lesson",
                "review_state":"promoted","project_lesson_id":13,"project":"kintsu","durable":true,
                "authority":"full","warnings":[],"reviewer_id":"kintsu","operator_identity":"sol",
                "reviewed_at":"2026-07-24T12:04:00Z","committed_at":"2026-07-24T12:05:00Z"
            }),
        ];
        for (receipt, expected) in receipts.into_iter().zip(expected) {
            let kind = receipt.durable_kind();
            let durable_id = receipt.durable_id();
            let result = GigaPromoteResult::from(receipt);
            assert_eq!(serde_json::to_value(&result).unwrap(), expected);
            let round_trip = GigaPromotionReceipt::try_from(result).unwrap();
            assert_eq!(round_trip.durable_kind(), kind);
            assert_eq!(round_trip.durable_id(), durable_id);
        }
    }

    #[test]
    fn giga_promotion_receipt_wire_rejects_false_durability_state_id_and_authority() {
        let base = serde_json::json!({
            "kind": "memory",
            "candidate_id": "candidate-memory",
            "review_state": "promoted",
            "memory_id": 11,
            "room": "lab",
            "durable": true,
            "authority": "full",
            "warnings": [],
            "reviewer_id": "kintsu",
            "operator_identity": "sol",
            "reviewed_at": "2026-07-24T12:04:00Z",
            "committed_at": "2026-07-24T12:05:00Z"
        });

        let mut not_durable = base.clone();
        not_durable["durable"] = serde_json::json!(false);
        let result: GigaPromoteResult = serde_json::from_value(not_durable).unwrap();
        assert!(GigaPromotionReceipt::try_from(result).is_err());

        let mut wrong_state = base.clone();
        wrong_state["review_state"] = serde_json::json!("in_review");
        let result: GigaPromoteResult = serde_json::from_value(wrong_state).unwrap();
        assert!(GigaPromotionReceipt::try_from(result).is_err());

        let mut zero_id = base.clone();
        zero_id["memory_id"] = serde_json::json!(0);
        let result: GigaPromoteResult = serde_json::from_value(zero_id).unwrap();
        assert!(GigaPromotionReceipt::try_from(result).is_err());

        let mut unknown_authority = base.clone();
        unknown_authority["authority"] = serde_json::json!("pointer-only");
        assert!(serde_json::from_value::<GigaPromoteResult>(unknown_authority).is_err());

        let mut wrong_variant_field = base.clone();
        wrong_variant_field["project"] = serde_json::json!("kintsu");
        assert!(serde_json::from_value::<GigaPromoteResult>(wrong_variant_field).is_err());

        let mut unknown_receipt_field = base;
        unknown_receipt_field["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<GigaPromoteResult>(unknown_receipt_field).is_err());
    }

    #[test]
    fn substrate_lifecycle_params_reject_ambiguous_or_partial_authority() {
        let health = RequestEnvelope {
            protocol: PROTOCOL_VERSION,
            id: "health".into(),
            method: "substrate_health".into(),
            params: serde_json::json!({"skipEmbedding": true}),
        }
        .substrate_health_request()
        .unwrap();
        assert!(health.skip_embedding);
        assert_eq!(health.max_backup_age_hours, 24.0);

        let invalid_health = RequestEnvelope {
            protocol: PROTOCOL_VERSION,
            id: "health".into(),
            method: "substrate_health".into(),
            params: serde_json::json!({"maxBackupAgeHours": 0}),
        };
        assert!(invalid_health.substrate_health_request().is_err());

        let partial = RequestEnvelope {
            protocol: PROTOCOL_VERSION,
            id: "migrations".into(),
            method: "substrate_migrations".into(),
            params: serde_json::json!({"through": 12}),
        };
        assert!(partial.substrate_migrations_request().is_err());
    }
}

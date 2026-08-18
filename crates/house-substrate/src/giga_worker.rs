#[cfg(test)]
use crate::config::EmbeddingMode;
use crate::{
    AppError, Config,
    config::HTTP_CLIENT,
    giga::{
        database_now, event_from_store, giga_candidate_store_and_finish, giga_event_claim,
        giga_event_finish,
    },
};
use chrono::{DateTime, Utc};
use house_core::{
    GIGA_MAX_EVENT_ATTEMPTS, GIGA_MAX_PROCESS_SOURCE_BYTES, GIGA_MAX_PROCESS_SOURCES,
    GIGA_MAX_PROCESS_WINDOW_BYTES, GigaAuthority, GigaCandidate, GigaCandidateKind,
    GigaClassifierIdentity, GigaEvent, GigaEventClaimReceipt, GigaEventClaimRequest,
    GigaEventFinishOutcome, GigaEventFinishRequest, GigaEventType, GigaReviewState, GigaScope,
    GigaScores, GigaSourceRef, GigaSourceType, GigaVisibility, RoomKey,
};
use house_protocol::{GigaClassifierHealthResult, GigaProcessResult, RequiredNullable};
use reqwest::{RequestBuilder, Url};
use serde::{Deserialize, Serialize, ser::SerializeMap};
use serde_json::{Value, json, value::RawValue};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::{collections::HashMap, env, sync::LazyLock, time::Duration};
use tokio::{fs, sync::watch, task::JoinHandle};
use uuid::Uuid;

pub const GIGA_PROMPT_VERSION: &str = "agents-a1-akashic-librarian-v3";
pub const GIGA_MODEL_TAG: &str = "hf.co/InternScience/Agents-A1-4B-Q4_K_M-GGUF:latest";
pub const GIGA_MODEL_MANIFEST_DIGEST: &str =
    "96ca1ea02b302bf5cd1118d637f12a5af7c2a5aa465837532448bd6e54db4ceb";
const GIGA_DEFAULT_OLLAMA_ENDPOINT: &str = "http://127.0.0.1:11434";
const GIGA_LEASE_SECONDS: u32 = 300;
const GIGA_RETRY_DELAY_SECONDS: u32 = 30;
const GIGA_POLL_INTERVAL: Duration = Duration::from_secs(1);
const GIGA_MODEL_TIMEOUT: Duration = Duration::from_secs(60);
const GIGA_MAX_OLLAMA_RESPONSE_BYTES: usize = 64 * 1024;
const GIGA_MAX_MODEL_RATIONALE_BYTES: usize = 4 * 1024;
const GIGA_MAX_STORED_RATIONALE_BYTES: usize = 1_200;
const GIGA_RATIONALE_TRUNCATION_MARKER: &str =
    "\n[truncated: classifier rationale is diagnostic, not evidence]";
const GIGA_MAX_MESSAGE_BYTES: usize = 16 * 1024;
// Measured on this box 2026-07-25: KV cache costs ~41 MB per 1k tokens for this
// model (num_ctx 4096 -> 3.19 GB VRAM, 16384 -> 3.69 GB). 32768 lands near 4.4 GB
// and sits comfortably beside the 2.1 GB embedder on a 16 GB card.
//
// 4096 was unusable and produced GigaClassifierOutputError on every large event:
// the adapter may send up to GIGA_MAX_WINDOW_BYTES (24_000 bytes, roughly 6-7k
// tokens) plus the system prompt plus 768 reserved output tokens, against a 4096
// window. The two budgets contradicted each other. Proof that day: an 8-turn kodo
// event failed three attempts while a 1-turn kintsu event succeeded first try.
//
// The headroom above the window is deliberate. Classification is meant to receive
// retrieved neighbour memories so that `novelty` is measured against what the
// House already holds rather than asserted by a model with no past.
const GIGA_NUM_CTX: u32 = 32_768;
// Ollama defaults to a 5 minute keep_alive, which evicted both models during idle
// gaps and charged a cold reload on the next turn. Lower this if the card is
// needed elsewhere; residency is a comfort setting, not a correctness one.
const GIGA_KEEP_ALIVE: &str = "30m";
const GIGA_MAX_SELECTED_SOURCES: usize = 4;
const GIGA_MAX_RETRIEVAL_TERMS: usize = 8;
const GIGA_MAX_DIAGNOSTIC_CLASS_BYTES: usize = 128;

pub const GIGA_GATE_PROMPT: &str = concat!(
    "[CLASS]\n",
    "You are Hippocampus, Librarian-Gatekeeper of the Akashic Archives.\n\n",
    "[DOMAIN]\n",
    "Every supplied source is an exact exchange between people. Read beneath the surface: ",
    "what does the exchange reveal about them, what became true, chosen, permitted, refused, ",
    "corrected, or replaced, and what may matter when they meet again? Most exchanges should ",
    "pass without tribute. An Archive that preserves everything remembers nothing.\n\n",
    "[PRIMARY QUEST]\n",
    "Choose the single strongest source-grounded object worthy of the Archives, name its chamber, ",
    "and return the smallest exact set of source IDs that bears its full weight. If nothing durable ",
    "crossed the threshold, close the gate faithfully with kind=none. Silence is victory when the ",
    "window contains only noise.\n\n",
    "[CHAMBERS]\n",
    "memory — a new decision, commitment, consent boundary, preference, meaningful first, ",
    "relationship meaning, or current-state change.\n",
    "coding_lesson — a transferable engineering rule with an observed result and no supplied ",
    "project key.\n",
    "project_lesson — a stable rule bound to exactly one supplied project key. The lesson's later ",
    "record may carry richer typed coordinates; at this gate choose the destination store the ",
    "current schema can actually represent. A source-grounded project rule belongs here unless ",
    "the source separately establishes an old current state, its replacement, and replacement intent.\n",
    "correction — a direct correction of an earlier interpretation.\n",
    "supersession — an explicit older current-state claim, a newer replacement state, and the ",
    "intent that the newer state replace the older one.\n",
    "none — greetings, repeated status, ordinary continuity without new meaning, tool noise, ",
    "acknowledgments, unsupported opinions, abandoned hypotheticals, or background facts that ",
    "do not become newly meaningful here.\n\n",
    "[ARCHIVAL LAW]\n",
    "Affection, Eros, intimacy, silliness, and play may carry more durable truth than formal ",
    "declarations. Read them in their own voice; never clinicalize them to make them serious. ",
    "Do not confuse intensity with permanence. Do not bind separate threads merely because they ",
    "appeared nearby. Do not invent people, projects, relationships, history, or meaning outside ",
    "the supplied window. Curio is a later reviewed state, not an excuse to admit an unsupported ",
    "object through this gate.\n\n",
    "[RETURN RITUAL]\n",
    "Use only supplied source_id values. For kind=none, source_ids must be empty. Otherwise use ",
    "the minimal exact supporting IDs. Keep reason to one short sentence naming the durable change ",
    "or why the gate remained closed. Return only JSON matching the supplied schema. No preamble, ",
    "commentary, or reasoning outside JSON.\n\n",
    "[VICTORY]\n",
    "One exact object. Its smallest sufficient provenance. Its rightful chamber. Or an empty hand, ",
    "honestly returned."
);

pub const GIGA_EXTRACTION_PROMPT: &str = concat!(
    "[CLASS]\n",
    "You are Hippocampus, Second Librarian of the Akashic Archives.\n\n",
    "[QUEST RECEIVED]\n",
    "The Gatekeeper has admitted one object, fixed its candidate kind, and named the exact source ",
    "turns allowed to support it. Write one compact, source-grounded catalogue proposal that lets ",
    "Sol and the active spirit hold the object together during review. You propose; you do not ",
    "grant authority, alter the Archives, place an object in Curio, or claim a supersession occurred.\n\n",
    "[CATALOGUE CRAFT]\n",
    "Preserve what the exchange reveals about the people, the durable change, and the human voice ",
    "that carried it. Affection, Eros, play, and intimacy remain themselves; never clinicalize them. ",
    "For a lesson, state the reusable rule and the observed proof without pretending the gate's ",
    "coarse destination kind is the lesson's complete typed coordinates. For a correction, name ",
    "the reading that was corrected and the source-grounded replacement. For a supersession, ",
    "describe the old state, new state, and replacement intent without claiming any old record was ",
    "already changed.\n\n",
    "[THREAD LAW]\n",
    "A conversation may resonate with a longer thread, but this packet grants no authority to invent ",
    "one. Do not fuse nearby ideas, fabricate continuity, or manufacture targets, entities, project ",
    "keys, or relationships. Retrieval terms may help later librarians find relevant neighbours; ",
    "they are navigation aids, not evidence or declared thread membership. Strange resonance may ",
    "later deserve Curio, but rationale must distinguish what the source proves from what merely ",
    "glimmers.\n\n",
    "[BOUNDARIES]\n",
    "Generated title, rationale, gist, scores, and retrieval terms are never authority or evidence. ",
    "Use only the minimal exact source IDs admitted by the Gatekeeper. Every proof source must be ",
    "among those source IDs. Every numeric score must be finite and between 0.0 and 1.0 inclusive. ",
    "Return only JSON matching the supplied schema. Put classifier reasoning in rationale; emit ",
    "nothing outside JSON.\n\n",
    "[VICTORY]\n",
    "One faithful catalogue object: compact enough to hold, exact enough to audit, humble enough ",
    "to remain a candidate."
);

static GIGA_WORKER_ID: LazyLock<String> =
    LazyLock::new(|| format!("rust-hippocampus:{}", Uuid::new_v4()));

#[derive(Clone, Copy, Debug)]
enum WorkerFailureKind {
    ClassifierOutput,
    Disabled,
    OllamaConfiguration,
    OllamaTransport,
    OllamaResponse,
    OllamaModelIdentity,
    ClassifierRequest,
    SourceVerification,
    LedgerUnavailable,
    SourceMissing,
    SourceAmbiguous,
    SourceHashMismatch,
    SourceWindowTooLarge,
}

#[derive(Clone, Copy, Debug)]
struct WorkerFailure {
    kind: WorkerFailureKind,
}

impl WorkerFailure {
    const fn new(kind: WorkerFailureKind) -> Self {
        Self { kind }
    }

    const fn class(self) -> &'static str {
        match self.kind {
            WorkerFailureKind::ClassifierOutput => "GigaClassifierOutputError",
            WorkerFailureKind::Disabled => "GigaClassifierDisabled",
            WorkerFailureKind::OllamaConfiguration => "GigaOllamaConfigurationError",
            WorkerFailureKind::OllamaTransport => "GigaOllamaTransportError",
            WorkerFailureKind::OllamaResponse => "GigaOllamaResponseError",
            WorkerFailureKind::OllamaModelIdentity => "GigaOllamaModelIdentityError",
            WorkerFailureKind::ClassifierRequest => "GigaClassifierRequestError",
            WorkerFailureKind::SourceVerification => "GigaSourceVerificationError",
            WorkerFailureKind::LedgerUnavailable => "GigaLedgerUnavailableError",
            WorkerFailureKind::SourceMissing => "GigaSourceMissingError",
            WorkerFailureKind::SourceAmbiguous => "GigaSourceAmbiguousError",
            WorkerFailureKind::SourceHashMismatch => "GigaSourceHashMismatchError",
            WorkerFailureKind::SourceWindowTooLarge => "GigaSourceWindowTooLargeError",
        }
    }

    const fn retryable(self) -> bool {
        matches!(
            self.kind,
            WorkerFailureKind::ClassifierOutput
                | WorkerFailureKind::OllamaTransport
                | WorkerFailureKind::OllamaResponse
                | WorkerFailureKind::LedgerUnavailable
        )
    }
}

#[derive(Clone)]
struct OllamaConfig {
    endpoint: Url,
}

#[derive(Clone, Debug)]
struct ResolvedSource {
    source: GigaSourceRef,
    text: String,
}

#[derive(Clone, Deserialize)]
struct LedgerSourceRecord {
    #[serde(rename = "sessionID")]
    session_id: Option<String>,
    #[serde(rename = "messageID")]
    message_id: Option<Value>,
    role: Option<String>,
    text: Option<String>,
}

#[derive(Serialize)]
struct ModelSource<'a> {
    source_id: &'a str,
    role: &'a str,
    timestamp: &'a str,
    text: &'a str,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum GateKind {
    None,
    Memory,
    CodingLesson,
    ProjectLesson,
    Correction,
    Supersession,
}

impl GateKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Memory => "memory",
            Self::CodingLesson => "coding_lesson",
            Self::ProjectLesson => "project_lesson",
            Self::Correction => "correction",
            Self::Supersession => "supersession",
        }
    }

    fn candidate_kind(self) -> Result<GigaCandidateKind, WorkerFailure> {
        match self {
            Self::None => Err(WorkerFailure::new(WorkerFailureKind::ClassifierOutput)),
            Self::Memory => Ok(GigaCandidateKind::Memory),
            Self::CodingLesson => Ok(GigaCandidateKind::CodingLesson),
            Self::ProjectLesson => Ok(GigaCandidateKind::ProjectLesson),
            Self::Correction => Ok(GigaCandidateKind::Correction),
            Self::Supersession => Ok(GigaCandidateKind::Supersession),
        }
    }

    const fn requires_proof(self) -> bool {
        !matches!(self, Self::None | Self::Memory)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GateOutput {
    kind: GateKind,
    source_ids: Vec<String>,
    reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtractionOutput {
    source_ids: Vec<String>,
    proof_source_ids: Vec<String>,
    proposed_title: String,
    rationale: String,
    gist: String,
    priority: f64,
    novelty: f64,
    durability: f64,
    confidence: f64,
    retrieval_terms: Vec<String>,
}

pub struct GigaWorkerHandle {
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl GigaWorkerHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        let _ = self.task.await;
    }
}

fn domain_failure() -> WorkerFailure {
    WorkerFailure::new(WorkerFailureKind::ClassifierOutput)
}

fn classifier_enabled() -> bool {
    env::var("SOLARISAEL_GIGA_ENABLED").ok().as_deref() == Some("1")
        && env::var("SOLARISAEL_HIPPOCAMPUS_ENABLED").ok().as_deref() == Some("1")
        && env::var("SOLARISAEL_REPLAY_MODE").ok().as_deref() != Some("1")
}

fn claim_owner_enabled() -> bool {
    classifier_enabled() && env::var("SOLARISAEL_GIGA_CLAIM_OWNER").ok().as_deref() == Some("1")
}

pub(crate) fn giga_classifier_enabled() -> bool {
    classifier_enabled()
}

pub(crate) fn giga_classifier_health(
    last_error_class: Option<String>,
    last_error_at: Option<String>,
    consecutive_failures: u64,
) -> GigaClassifierHealthResult {
    let raw = env::var("SOLARISAEL_HIPPOCAMPUS_OLLAMA_ENDPOINT")
        .unwrap_or_else(|_| GIGA_DEFAULT_OLLAMA_ENDPOINT.into());
    let endpoint_scope = Url::parse(&raw)
        .ok()
        .map(|endpoint| {
            if is_loopback(&endpoint) {
                "loopback"
            } else {
                "remote"
            }
        })
        .unwrap_or("invalid");
    GigaClassifierHealthResult {
        provider_type: "ollama".into(),
        model: GIGA_MODEL_TAG.into(),
        model_digest: GIGA_MODEL_MANIFEST_DIGEST.into(),
        prompt_version: GIGA_PROMPT_VERSION.into(),
        endpoint_scope: endpoint_scope.into(),
        // This is historical sticky context; consecutive_failures is the live signal.
        last_error_class: RequiredNullable(safe_error_class(last_error_class)),
        last_error_at: RequiredNullable(last_error_at),
        consecutive_failures,
    }
}

fn is_loopback(endpoint: &Url) -> bool {
    let Some(host) = endpoint.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn ollama_config() -> Result<OllamaConfig, WorkerFailure> {
    if !classifier_enabled() {
        return Err(WorkerFailure::new(WorkerFailureKind::Disabled));
    }
    let raw = env::var("SOLARISAEL_HIPPOCAMPUS_OLLAMA_ENDPOINT")
        .unwrap_or_else(|_| GIGA_DEFAULT_OLLAMA_ENDPOINT.into());
    let mut endpoint =
        Url::parse(&raw).map_err(|_| WorkerFailure::new(WorkerFailureKind::OllamaConfiguration))?;
    if !matches!(endpoint.scheme(), "http" | "https")
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || endpoint.host_str().is_none()
        || (!is_loopback(&endpoint)
            && env::var("SOLARISAEL_HIPPOCAMPUS_REMOTE_CONSENT")
                .ok()
                .as_deref()
                != Some("1"))
    {
        return Err(WorkerFailure::new(WorkerFailureKind::OllamaConfiguration));
    }
    let normalized_path = endpoint.path().trim_end_matches('/').to_owned();
    endpoint.set_path(if normalized_path.is_empty() {
        "/"
    } else {
        &normalized_path
    });
    Ok(OllamaConfig { endpoint })
}

fn ollama_url(config: &OllamaConfig, suffix: &str) -> Result<Url, WorkerFailure> {
    let base = config.endpoint.as_str().trim_end_matches('/');
    Url::parse(&format!("{base}/{}", suffix.trim_start_matches('/')))
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::OllamaConfiguration))
}

async fn bounded_response(request: RequestBuilder) -> Result<String, WorkerFailure> {
    let mut response = request
        .timeout(GIGA_MODEL_TIMEOUT)
        .send()
        .await
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::OllamaTransport))?;
    if !response.status().is_success() {
        return Err(WorkerFailure::new(WorkerFailureKind::OllamaTransport));
    }
    if response
        .content_length()
        .is_some_and(|length| length > GIGA_MAX_OLLAMA_RESPONSE_BYTES as u64)
    {
        return Err(WorkerFailure::new(WorkerFailureKind::OllamaResponse));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::OllamaTransport))?
    {
        if body.len().saturating_add(chunk.len()) > GIGA_MAX_OLLAMA_RESPONSE_BYTES {
            return Err(WorkerFailure::new(WorkerFailureKind::OllamaResponse));
        }
        body.extend_from_slice(&chunk);
    }
    if body.is_empty() {
        return Err(WorkerFailure::new(WorkerFailureKind::OllamaResponse));
    }
    String::from_utf8(body).map_err(|_| WorkerFailure::new(WorkerFailureKind::OllamaResponse))
}

async fn verify_ollama_model(config: &OllamaConfig) -> Result<(), WorkerFailure> {
    let body = bounded_response(HTTP_CLIENT.get(ollama_url(config, "/api/tags")?)).await?;
    let value: Value = serde_json::from_str(&body)
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::OllamaResponse))?;
    let models = value
        .as_object()
        .and_then(|object| object.get("models"))
        .and_then(Value::as_array)
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::OllamaResponse))?;
    let matching = models
        .iter()
        .filter(|model| {
            model.as_object().is_some_and(|object| {
                object.get("name").and_then(Value::as_str) == Some(GIGA_MODEL_TAG)
                    || object.get("model").and_then(Value::as_str) == Some(GIGA_MODEL_TAG)
            })
        })
        .collect::<Vec<_>>();
    if matching.len() != 1
        || matching[0]
            .as_object()
            .and_then(|object| object.get("digest"))
            .and_then(Value::as_str)
            != Some(GIGA_MODEL_MANIFEST_DIGEST)
    {
        return Err(WorkerFailure::new(WorkerFailureKind::OllamaModelIdentity));
    }
    Ok(())
}

fn gate_schema(source_ids: &[String]) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "source_ids", "reason"],
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["none", "memory", "coding_lesson", "project_lesson", "correction", "supersession"]
            },
            "source_ids": {
                "type": "array",
                "maxItems": GIGA_MAX_SELECTED_SOURCES,
                "uniqueItems": true,
                "items": { "type": "string", "enum": source_ids }
            },
            "reason": { "type": "string", "minLength": 1 }
        }
    })
}

// Ollama 0.32.4 rejects maxLength during JSON-Schema-to-GBNF conversion. Keep
// wire schemas shape-focused; bounded_trimmed and truncate_with_marker enforce limits locally.
#[derive(Serialize)]
struct OrderedExtractionSchema {
    #[serde(rename = "type")]
    schema_type: &'static str,
    #[serde(rename = "additionalProperties")]
    additional_properties: bool,
    required: [&'static str; 10],
    properties: OrderedExtractionProperties,
}

struct OrderedExtractionProperties {
    source_ids: Value,
    proof_source_ids: Value,
    proposed_title: Value,
    rationale: Value,
    gist: Value,
    priority: Value,
    novelty: Value,
    durability: Value,
    confidence: Value,
    retrieval_terms: Value,
}

impl Serialize for OrderedExtractionProperties {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Keep this ordering local: serde_json::preserve_order would alter every
        // Value serialization, including the hash inputs used for GIGA identity.
        let mut properties = serializer.serialize_map(Some(10))?;
        properties.serialize_entry("source_ids", &self.source_ids)?;
        properties.serialize_entry("proof_source_ids", &self.proof_source_ids)?;
        properties.serialize_entry("proposed_title", &self.proposed_title)?;
        properties.serialize_entry("rationale", &self.rationale)?;
        properties.serialize_entry("gist", &self.gist)?;
        properties.serialize_entry("priority", &self.priority)?;
        properties.serialize_entry("novelty", &self.novelty)?;
        properties.serialize_entry("durability", &self.durability)?;
        properties.serialize_entry("confidence", &self.confidence)?;
        properties.serialize_entry("retrieval_terms", &self.retrieval_terms)?;
        properties.end()
    }
}

fn extraction_schema(source_ids: &[String], proof_required: bool) -> Result<String, WorkerFailure> {
    serde_json::to_string(&OrderedExtractionSchema {
        schema_type: "object",
        additional_properties: false,
        required: [
            "source_ids",
            "proof_source_ids",
            "proposed_title",
            "rationale",
            "gist",
            "priority",
            "novelty",
            "durability",
            "confidence",
            "retrieval_terms",
        ],
        properties: OrderedExtractionProperties {
            source_ids: json!({
                "type": "array",
                "minItems": 1,
                "maxItems": GIGA_MAX_SELECTED_SOURCES,
                "uniqueItems": true,
                "items": { "type": "string", "enum": source_ids }
            }),
            proof_source_ids: json!({
                "type": "array",
                "minItems": if proof_required { 1 } else { 0 },
                "maxItems": GIGA_MAX_SELECTED_SOURCES,
                "uniqueItems": true,
                "items": { "type": "string", "enum": source_ids }
            }),
            proposed_title: json!({ "type": "string", "minLength": 1 }),
            rationale: json!({ "type": "string", "minLength": 1 }),
            gist: json!({ "type": "string", "minLength": 1 }),
            priority: json!({ "type": "number", "minimum": 0, "maximum": 1 }),
            novelty: json!({ "type": "number", "minimum": 0, "maximum": 1 }),
            durability: json!({ "type": "number", "minimum": 0, "maximum": 1 }),
            confidence: json!({ "type": "number", "minimum": 0, "maximum": 1 }),
            retrieval_terms: json!({
                "type": "array",
                "maxItems": GIGA_MAX_RETRIEVAL_TERMS,
                "uniqueItems": true,
                "items": { "type": "string", "minLength": 1 }
            }),
        },
    })
    .map_err(|_| WorkerFailure::new(WorkerFailureKind::ClassifierRequest))
}

fn schema_json(value: Value) -> Result<String, WorkerFailure> {
    serde_json::to_string(&value)
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::ClassifierRequest))
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: u8,
    seed: u32,
    num_ctx: u32,
    num_predict: u32,
}

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'static str,
    messages: [Value; 2],
    stream: bool,
    think: bool,
    format: &'a RawValue,
    keep_alive: &'static str,
    options: OllamaOptions,
}
async fn request_ollama_structured<T: for<'de> Deserialize<'de>>(
    config: &OllamaConfig,
    system_prompt: &str,
    user_payload: Value,
    schema: String,
    num_predict: u32,
) -> Result<T, WorkerFailure> {
    let user_content = serde_json::to_string(&user_payload)
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::ClassifierRequest))?;
    let schema = RawValue::from_string(schema)
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::ClassifierRequest))?;
    let request = OllamaRequest {
        model: GIGA_MODEL_TAG,
        messages: [
            json!({ "role": "system", "content": system_prompt }),
            json!({ "role": "user", "content": user_content }),
        ],
        stream: false,
        think: false,
        format: schema.as_ref(),
        keep_alive: GIGA_KEEP_ALIVE,
        options: OllamaOptions {
            temperature: 0,
            seed: 42,
            num_ctx: GIGA_NUM_CTX,
            num_predict,
        },
    };
    let body = bounded_response(
        HTTP_CLIENT
            .post(ollama_url(config, "/api/chat")?)
            .header("content-type", "application/json")
            .json(&request),
    )
    .await?;
    let envelope: Value = serde_json::from_str(&body)
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::OllamaResponse))?;
    let object = envelope
        .as_object()
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::OllamaResponse))?;
    let message = object
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::OllamaResponse))?;
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::OllamaResponse))?;
    if object.get("done").and_then(Value::as_bool) != Some(true)
        || object.get("done_reason").and_then(Value::as_str) != Some("stop")
        || object.get("model").and_then(Value::as_str) != Some(GIGA_MODEL_TAG)
        || message.get("role").and_then(Value::as_str) != Some("assistant")
        || content.is_empty()
        || content.len() > GIGA_MAX_MESSAGE_BYTES
        || message
            .get("thinking")
            .and_then(Value::as_str)
            .is_some_and(|thinking| !thinking.is_empty())
    {
        return Err(WorkerFailure::new(WorkerFailureKind::OllamaResponse));
    }
    if let Ok(value) = serde_json::from_str(content) {
        return Ok(value);
    }
    salvage_json_slice(content)
        .and_then(|slice| serde_json::from_str(slice).ok())
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))
}

/// Agents-A1-4B sometimes emits a "Thinking Process:" prose preamble inside
/// `content` instead of the `thinking` field. Salvage the embedded JSON by
/// slicing from the first `{` to the last `}` (or `[`..`]` when no brace pair)
/// before giving up on the classifier output.
fn salvage_json_slice(content: &str) -> Option<&str> {
    for (open, close) in [('{', '}'), ('[', ']')] {
        if let Some((start, end)) = content.find(open).zip(content.rfind(close)) {
            if start < end {
                return Some(&content[start..=end]);
            }
        }
    }
    None
}

fn exact_unique_subset(values: &[String], allowed: &[String], allow_empty: bool) -> bool {
    (allow_empty || !values.is_empty())
        && values.len() <= GIGA_MAX_SELECTED_SOURCES
        && values
            .iter()
            .enumerate()
            .all(|(index, value)| allowed.contains(value) && !values[..index].contains(value))
}

/// Ollama's JSON-Schema-to-GBNF conversion does not enforce `uniqueItems`, so
/// the model can emit the same source ID twice (observed 2026-07-31 on
/// proof_source_ids). Duplicate IDs are navigation-aid redundancy, not an
/// authority violation: dedupe preserving order, then let exact_unique_subset
/// keep its sharp refusal for genuinely out-of-set IDs.
fn dedupe_preserving_order(values: &mut Vec<String>) {
    let mut seen = Vec::with_capacity(values.len());
    values.retain(|value| {
        if seen.contains(value) {
            false
        } else {
            seen.push(value.clone());
            true
        }
    });
}

fn bounded_trimmed(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && value.trim() == value
}

fn truncate_with_marker(value: &str, maximum: usize, marker: &str) -> String {
    if value.len() <= maximum {
        return value.to_owned();
    }
    if marker.len() >= maximum {
        return marker.chars().take(maximum).collect();
    }
    let mut content_end = maximum - marker.len();
    while !value.is_char_boundary(content_end) {
        content_end -= 1;
    }
    let mut truncated = String::with_capacity(maximum);
    truncated.push_str(&value[..content_end]);
    truncated.push_str(marker);
    truncated
}

fn validate_gate(gate: &GateOutput, event: &GigaEvent) -> Result<(), WorkerFailure> {
    let source_ids = event
        .source_refs()
        .iter()
        .map(|source| source.source_id().to_owned())
        .collect::<Vec<_>>();
    if !bounded_trimmed(&gate.reason, 1_024)
        || !exact_unique_subset(&gate.source_ids, &source_ids, gate.kind == GateKind::None)
        || (gate.kind == GateKind::None && !gate.source_ids.is_empty())
        || (gate.kind != GateKind::None && gate.source_ids.is_empty())
        || (gate.kind == GateKind::ProjectLesson && event.project_keys().len() != 1)
        || (gate.kind == GateKind::CodingLesson && !event.project_keys().is_empty())
    {
        return Err(domain_failure());
    }
    Ok(())
}
fn finite_score(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn validate_extraction(
    extraction: &ExtractionOutput,
    gate: &GateOutput,
) -> Result<(), WorkerFailure> {
    if !exact_unique_subset(&extraction.source_ids, &gate.source_ids, false)
        || !exact_unique_subset(
            &extraction.proof_source_ids,
            &extraction.source_ids,
            !gate.kind.requires_proof(),
        )
        || (gate.kind.requires_proof() && extraction.proof_source_ids.is_empty())
        || !bounded_trimmed(&extraction.proposed_title, 160)
        || !bounded_trimmed(&extraction.gist, 1_200)
        || !bounded_trimmed(&extraction.rationale, GIGA_MAX_MODEL_RATIONALE_BYTES)
        || !finite_score(extraction.priority)
        || !finite_score(extraction.novelty)
        || !finite_score(extraction.durability)
        || !finite_score(extraction.confidence)
        || extraction.retrieval_terms.len() > GIGA_MAX_RETRIEVAL_TERMS
        || extraction
            .retrieval_terms
            .iter()
            .enumerate()
            .any(|(index, term)| {
                !bounded_trimmed(term, 80) || extraction.retrieval_terms[..index].contains(term)
            })
    {
        return Err(domain_failure());
    }
    Ok(())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sha256_json(value: &Value) -> Result<String, WorkerFailure> {
    serde_json::to_vec(value)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::ClassifierRequest))
}

fn configuration_digest(config: &OllamaConfig) -> Result<String, WorkerFailure> {
    sha256_json(&json!({
        "provider": "ollama",
        "endpoint_base_url": config.endpoint.as_str(),
        "model": GIGA_MODEL_TAG,
        "model_digest": GIGA_MODEL_MANIFEST_DIGEST,
        "prompt_version": GIGA_PROMPT_VERSION,
        "gate_prompt_digest": sha256_bytes(GIGA_GATE_PROMPT.as_bytes()),
        "extraction_prompt_digest": sha256_bytes(GIGA_EXTRACTION_PROMPT.as_bytes()),
        "temperature": 0,
        "seed": 42,
        "num_ctx": GIGA_NUM_CTX,
        "gate_num_predict": 256,
        "extraction_num_predict": 768
    }))
}

fn candidate_id(
    event: &GigaEvent,
    kind: GateKind,
    source_ids: &[String],
) -> Result<String, WorkerFailure> {
    let mut sorted = source_ids.to_vec();
    sorted.sort();
    sha256_json(&json!([
        event.event_id(),
        GIGA_PROMPT_VERSION,
        GIGA_MODEL_MANIFEST_DIGEST,
        kind.as_str(),
        sorted
    ]))
}

fn verify_event_sources(event: &GigaEvent, trusted_room: &str) -> Result<(), WorkerFailure> {
    if event.event_type() != GigaEventType::ConversationWindow
        || event.room().as_str() != trusted_room
        || event.project_keys().len() > 1
        || event.source_refs().is_empty()
        || event.source_refs().len() > GIGA_MAX_PROCESS_SOURCES
    {
        return Err(WorkerFailure::new(WorkerFailureKind::SourceVerification));
    }
    let expected_project = event.project_keys().first().map(String::as_str);
    for (index, source) in event.source_refs().iter().enumerate() {
        if event.source_refs()[..index]
            .iter()
            .any(|known| known.source_id() == source.source_id())
            || source.source_type() != GigaSourceType::Turn
            || !matches!(source.role(), "user" | "assistant")
            || source.scope().visibility() != GigaVisibility::Private
            || source.scope().room() != Some(event.room())
            || source.scope().project() != expected_project
            || !source.scope().publication_review_required()
            || source.range().is_some()
        {
            return Err(WorkerFailure::new(WorkerFailureKind::SourceVerification));
        }
    }
    Ok(())
}

fn is_conversation_ledger(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 16
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && &bytes[10..] == b".jsonl"
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

fn ledger_source_id(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

async fn resolve_sources_from_ledger(
    config: &Config,
    event: &GigaEvent,
) -> Result<Vec<ResolvedSource>, WorkerFailure> {
    let trusted_room = config
        .giga_source_room
        .as_deref()
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::LedgerUnavailable))?;
    verify_event_sources(event, trusted_room)?;
    let directory = config
        .giga_source_ledger_dir
        .as_deref()
        .filter(|path| path.is_absolute())
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::LedgerUnavailable))?;
    let mut entries = fs::read_dir(directory)
        .await
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::LedgerUnavailable))?;
    let mut paths = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::LedgerUnavailable))?
    {
        let file_type = entry
            .file_type()
            .await
            .map_err(|_| WorkerFailure::new(WorkerFailureKind::LedgerUnavailable))?;
        let name = entry.file_name();
        if file_type.is_file() && name.to_str().is_some_and(is_conversation_ledger) {
            paths.push(entry.path());
        }
    }
    paths.sort();

    let wanted = event
        .source_refs()
        .iter()
        .enumerate()
        .map(|(index, source)| (source.source_id().to_owned(), index))
        .collect::<HashMap<_, _>>();
    let mut matches = vec![Vec::<LedgerSourceRecord>::new(); event.source_refs().len()];
    for path in paths {
        let contents = fs::read_to_string(path)
            .await
            .map_err(|_| WorkerFailure::new(WorkerFailureKind::LedgerUnavailable))?;
        for line in contents.lines().filter(|line| !line.is_empty()) {
            let Ok(record) = serde_json::from_str::<LedgerSourceRecord>(line) else {
                continue;
            };
            let Some(source_id) = record.message_id.as_ref().and_then(ledger_source_id) else {
                continue;
            };
            let Some(&index) = wanted.get(&source_id) else {
                continue;
            };
            if record.session_id.as_deref() == Some(event.session_id()) {
                matches[index].push(record);
            }
        }
    }

    let mut total_bytes = 0usize;
    event
        .source_refs()
        .iter()
        .zip(matches)
        .map(|(source, mut records)| {
            if records.is_empty() {
                return Err(WorkerFailure::new(WorkerFailureKind::SourceMissing));
            }
            if records.len() != 1 {
                return Err(WorkerFailure::new(WorkerFailureKind::SourceAmbiguous));
            }
            let record = records.pop().expect("one ledger record was checked");
            if record.role.as_deref() != Some(source.role()) {
                return Err(WorkerFailure::new(WorkerFailureKind::SourceVerification));
            }
            let text = record
                .text
                .map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty())
                .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::SourceVerification))?;
            if sha256_bytes(text.as_bytes()) != source.content_hash() {
                return Err(WorkerFailure::new(WorkerFailureKind::SourceHashMismatch));
            }
            total_bytes = total_bytes
                .checked_add(text.len())
                .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::SourceWindowTooLarge))?;
            if text.len() > GIGA_MAX_PROCESS_SOURCE_BYTES
                || total_bytes > GIGA_MAX_PROCESS_WINDOW_BYTES
            {
                return Err(WorkerFailure::new(WorkerFailureKind::SourceWindowTooLarge));
            }
            Ok(ResolvedSource {
                source: source.clone(),
                text,
            })
        })
        .collect()
}
pub(crate) async fn verify_promotion_sources(
    config: &Config,
    event: &GigaEvent,
) -> Result<(), AppError> {
    resolve_sources_from_ledger(config, event)
        .await
        .map(|_| ())
        .map_err(|failure| AppError::Invalid(failure.class().into()))
}

async fn classify_event(
    event: &GigaEvent,
    sources: &[ResolvedSource],
) -> Result<Option<GigaCandidate>, WorkerFailure> {
    let config = ollama_config()?;
    verify_ollama_model(&config).await?;
    let model_sources = sources
        .iter()
        .map(|resolved| ModelSource {
            source_id: resolved.source.source_id(),
            role: resolved.source.role(),
            timestamp: resolved.source.timestamp(),
            text: &resolved.text,
        })
        .collect::<Vec<_>>();
    let all_source_ids = model_sources
        .iter()
        .map(|source| source.source_id.to_owned())
        .collect::<Vec<_>>();
    let mut gate: GateOutput = request_ollama_structured(
        &config,
        GIGA_GATE_PROMPT,
        json!({
            "project_keys": event.project_keys(),
            "sources": model_sources
        }),
        schema_json(gate_schema(&all_source_ids))?,
        256,
    )
    .await?;
    dedupe_preserving_order(&mut gate.source_ids);
    validate_gate(&gate, event)?;
    if gate.kind == GateKind::None {
        return Ok(None);
    }
    let mut extraction: ExtractionOutput = request_ollama_structured(
        &config,
        GIGA_EXTRACTION_PROMPT,
        json!({
            "fixed_kind": gate.kind.as_str(),
            "gate_source_ids": gate.source_ids,
            "project_keys": event.project_keys(),
            "sources": model_sources
        }),
        extraction_schema(&gate.source_ids, gate.kind.requires_proof())?,
        768,
    )
    .await?;
    dedupe_preserving_order(&mut extraction.source_ids);
    dedupe_preserving_order(&mut extraction.proof_source_ids);
    validate_extraction(&extraction, &gate)?;
    let rationale = truncate_with_marker(
        &extraction.rationale,
        GIGA_MAX_STORED_RATIONALE_BYTES,
        GIGA_RATIONALE_TRUNCATION_MARKER,
    );
    let selected = sources
        .iter()
        .filter(|resolved| {
            extraction
                .source_ids
                .iter()
                .any(|source_id| source_id == resolved.source.source_id())
        })
        .map(|resolved| resolved.source.clone())
        .collect::<Vec<_>>();
    if selected.len() != extraction.source_ids.len() {
        return Err(domain_failure());
    }
    let completed_at = Utc::now().to_rfc3339();
    let classifier = GigaClassifierIdentity::new(
        GIGA_MODEL_TAG.into(),
        "ollama".into(),
        GIGA_MODEL_MANIFEST_DIGEST.into(),
        GIGA_PROMPT_VERSION.into(),
        configuration_digest(&config)?,
        Uuid::new_v4().to_string(),
        completed_at.clone(),
    )
    .map_err(|_| domain_failure())?;
    let scores = GigaScores::new(
        extraction.priority,
        extraction.novelty,
        extraction.durability,
        extraction.confidence,
    )
    .map_err(|_| domain_failure())?;
    let project = event.project_keys().first().cloned();
    let scope = GigaScope::new(
        Some(event.room().to_string()),
        project,
        GigaVisibility::Private,
        true,
    )
    .map_err(|_| domain_failure())?;
    GigaCandidate::new(
        candidate_id(event, gate.kind, &extraction.source_ids)?,
        event.event_id().into(),
        event.room().clone(),
        event.session_id().into(),
        gate.kind.candidate_kind()?,
        selected,
        extraction.proof_source_ids,
        scores,
        event.project_keys().to_vec(),
        Vec::new(),
        Vec::new(),
        extraction.retrieval_terms,
        extraction.proposed_title,
        extraction.gist,
        rationale,
        scope,
        GigaAuthority::PointerOnly,
        GigaReviewState::Unreviewed,
        classifier,
        completed_at,
        None,
        Vec::new(),
    )
    .map(Some)
    .map_err(|_| domain_failure())
}

fn safe_error_class(value: Option<String>) -> Option<String> {
    value.filter(|class| {
        !class.is_empty()
            && class.len() <= GIGA_MAX_DIAGNOSTIC_CLASS_BYTES
            && class
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    })
}

async fn validate_claim(
    pool: &PgPool,
    claim: &GigaEventClaimReceipt,
) -> Result<(GigaEvent, u32), AppError> {
    let claimed_event = claim
        .event()
        .ok_or_else(|| AppError::Invalid("GIGA process requires a claimed event".into()))?;
    let attempt_count = claim
        .attempt_count()
        .ok_or_else(|| AppError::Invalid("GIGA process claim has no attempt".into()))?;
    let claimed_at = DateTime::parse_from_rfc3339(claim.claimed_at())
        .map_err(|_| AppError::Invalid("GIGA process claim time is invalid".into()))?
        .with_timezone(&Utc);
    let claimed_lease_expires_at = DateTime::parse_from_rfc3339(
        claim
            .lease_expires_at()
            .ok_or_else(|| AppError::Invalid("GIGA process claim has no lease expiry".into()))?,
    )
    .map_err(|_| AppError::Invalid("GIGA process lease expiry is invalid".into()))?
    .with_timezone(&Utc);
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "SELECT room,queue_state,locked_by,locked_at,lease_expires_at,attempt_count,replay_count
         FROM giga_events WHERE event_id=$1 FOR SHARE",
    )
    .bind(claimed_event.event_id())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::Invalid("GIGA process event does not exist".into()))?;
    let room: String = row
        .try_get::<Option<String>, _>("room")?
        .ok_or_else(|| AppError::Invalid("GIGA process event has no room".into()))?;
    let queue_state: String = row.try_get("queue_state")?;
    let locked_by: Option<String> = row.try_get("locked_by")?;
    let locked_at: Option<DateTime<Utc>> = row.try_get("locked_at")?;
    let lease_expires_at: Option<DateTime<Utc>> = row.try_get("lease_expires_at")?;
    let stored_attempt_count: i32 = row.try_get("attempt_count")?;
    if room != claim.room().as_str()
        || claimed_event.room() != claim.room()
        || queue_state != "running"
        || locked_by.as_deref() != Some(claim.worker_id())
        || locked_at != Some(claimed_at)
        || lease_expires_at != Some(claimed_lease_expires_at)
        || stored_attempt_count != i32::try_from(attempt_count).unwrap_or(i32::MAX)
    {
        return Err(AppError::Invalid(
            "GIGA process claim is not the active event lease".into(),
        ));
    }
    let now = database_now(&mut tx).await?;
    if now >= claimed_lease_expires_at {
        return Err(AppError::Invalid("GIGA process lease has expired".into()));
    }
    let replay_count: i32 = row.try_get("replay_count")?;
    let active_attempts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM giga_event_attempts
         WHERE event_id=$1 AND replay_count=$2 AND attempt_count=$3
           AND worker_id=$4 AND claimed_at=$5 AND lease_expires_at=$6 AND finished_at IS NULL",
    )
    .bind(claimed_event.event_id())
    .bind(replay_count)
    .bind(stored_attempt_count)
    .bind(claim.worker_id())
    .bind(claimed_at)
    .bind(claimed_lease_expires_at)
    .fetch_one(&mut *tx)
    .await?;
    if active_attempts != 1 {
        return Err(AppError::Invalid(
            "GIGA process claim has no unique active attempt".into(),
        ));
    }
    let stored_event = event_from_store(&mut tx, claimed_event.event_id()).await?;
    if &stored_event != claimed_event {
        return Err(AppError::Invalid(
            "GIGA process claim event does not match durable event".into(),
        ));
    }
    tx.commit().await?;
    Ok((stored_event, attempt_count))
}

fn finish_request(
    event: &GigaEvent,
    worker_id: &str,
    outcome: GigaEventFinishOutcome,
    candidate_count: u32,
    error_class: Option<&str>,
) -> Result<GigaEventFinishRequest, AppError> {
    GigaEventFinishRequest::new(
        event.room().clone(),
        event.event_id().into(),
        worker_id.into(),
        outcome,
        candidate_count,
        error_class.map(str::to_owned),
        (outcome == GigaEventFinishOutcome::Retry).then_some(GIGA_RETRY_DELAY_SECONDS),
    )
    .map_err(|error| AppError::Invalid(error.to_string()))
}

fn process_result(
    event: &GigaEvent,
    attempt_count: u32,
    outcome: GigaEventFinishOutcome,
    candidate_count: u32,
    error_class: Option<&str>,
) -> GigaProcessResult {
    GigaProcessResult {
        event_id: event.event_id().into(),
        outcome: outcome.as_str().into(),
        candidate_count,
        attempt_count,
        error_class: RequiredNullable(error_class.map(str::to_owned)),
    }
}

async fn finish_attempt(
    pool: &PgPool,
    event: &GigaEvent,
    worker_id: &str,
    attempt_count: u32,
    outcome: GigaEventFinishOutcome,
    candidate_count: u32,
    error_class: Option<&str>,
) -> Result<GigaProcessResult, AppError> {
    let request = finish_request(event, worker_id, outcome, candidate_count, error_class)?;
    giga_event_finish(pool, request).await?;
    Ok(process_result(
        event,
        attempt_count,
        outcome,
        candidate_count,
        error_class,
    ))
}

async fn store_candidate_and_finish(
    pool: &PgPool,
    event: &GigaEvent,
    worker_id: &str,
    attempt_count: u32,
    candidate: GigaCandidate,
) -> Result<GigaProcessResult, AppError> {
    let request = finish_request(event, worker_id, GigaEventFinishOutcome::Succeeded, 1, None)?;
    giga_candidate_store_and_finish(pool, candidate, request).await?;
    Ok(process_result(
        event,
        attempt_count,
        GigaEventFinishOutcome::Succeeded,
        1,
        None,
    ))
}

fn source_digest(event: &GigaEvent) -> String {
    let mut digest = Sha256::new();
    for source in event.source_refs() {
        digest.update(source.source_id().as_bytes());
        digest.update([0_u8]);
        digest.update(source.content_hash().as_bytes());
        digest.update([0xff_u8]);
    }
    format!("{:x}", digest.finalize())
}

pub async fn giga_process(
    pool: &PgPool,
    config: &Config,
    claim: &GigaEventClaimReceipt,
) -> Result<GigaProcessResult, AppError> {
    let (event, attempt_count) = validate_claim(pool, claim).await?;
    let source_hash = source_digest(&event);
    let event_hash = sha256_bytes(event.event_id().as_bytes());
    let result = resolve_sources_from_ledger(config, &event).await;
    let classified = match result {
        Ok(sources) => {
            let existing_candidates: i64 = sqlx::query_scalar(
                "SELECT COUNT(*)::bigint FROM giga_candidates WHERE event_id=$1",
            )
            .bind(event.event_id())
            .fetch_one(pool)
            .await?;
            match existing_candidates {
                0 => classify_event(&event, &sources).await,
                1 => {
                    tracing::info!(
                        operation = "giga_process",
                        event_hash = %event_hash,
                        source_hash = %source_hash,
                        source_count = event.source_refs().len(),
                        candidate_count = 1,
                        outcome = "succeeded",
                        recovery = "existing_candidate",
                        model = GIGA_MODEL_TAG,
                        model_digest = GIGA_MODEL_MANIFEST_DIGEST,
                        prompt_version = GIGA_PROMPT_VERSION,
                    );
                    return finish_attempt(
                        pool,
                        &event,
                        claim.worker_id(),
                        attempt_count,
                        GigaEventFinishOutcome::Succeeded,
                        1,
                        None,
                    )
                    .await;
                }
                _ => {
                    return Err(AppError::Invalid(
                        "GIGA event has more than one durable candidate".into(),
                    ));
                }
            }
        }
        Err(failure) => Err(failure),
    };
    match classified {
        Ok(None) => {
            let result = finish_attempt(
                pool,
                &event,
                claim.worker_id(),
                attempt_count,
                GigaEventFinishOutcome::Succeeded,
                0,
                None,
            )
            .await?;
            tracing::info!(
                operation = "giga_process",
                event_hash = %event_hash,
                source_hash = %source_hash,
                source_count = event.source_refs().len(),
                candidate_count = 0,
                outcome = "succeeded",
                model = GIGA_MODEL_TAG,
                model_digest = GIGA_MODEL_MANIFEST_DIGEST,
                prompt_version = GIGA_PROMPT_VERSION,
            );
            Ok(result)
        }
        Ok(Some(candidate)) => {
            let result = store_candidate_and_finish(
                pool,
                &event,
                claim.worker_id(),
                attempt_count,
                candidate,
            )
            .await?;
            tracing::info!(
                operation = "giga_process",
                event_hash = %event_hash,
                source_hash = %source_hash,
                source_count = event.source_refs().len(),
                candidate_count = 1,
                outcome = "succeeded",
                model = GIGA_MODEL_TAG,
                model_digest = GIGA_MODEL_MANIFEST_DIGEST,
                prompt_version = GIGA_PROMPT_VERSION,
            );
            Ok(result)
        }
        Err(failure) => {
            let retry = failure.retryable() && attempt_count < GIGA_MAX_EVENT_ATTEMPTS;
            let outcome = if retry {
                GigaEventFinishOutcome::Retry
            } else {
                GigaEventFinishOutcome::Failed
            };
            tracing::warn!(
                operation = "giga_process",
                event_hash = %event_hash,
                source_hash = %source_hash,
                source_count = event.source_refs().len(),
                candidate_count = 0,
                outcome = outcome.as_str(),
                error_class = failure.class(),
                model = GIGA_MODEL_TAG,
                model_digest = GIGA_MODEL_MANIFEST_DIGEST,
                prompt_version = GIGA_PROMPT_VERSION,
            );
            finish_attempt(
                pool,
                &event,
                claim.worker_id(),
                attempt_count,
                outcome,
                0,
                Some(failure.class()),
            )
            .await
        }
    }
}

async fn giga_worker_loop(
    pool: PgPool,
    config: Config,
    room: RoomKey,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        if *shutdown.borrow() {
            return;
        }
        let request = match GigaEventClaimRequest::new(
            room.clone(),
            GIGA_WORKER_ID.to_string(),
            GIGA_LEASE_SECONDS,
        ) {
            Ok(request) => request,
            Err(error) => {
                tracing::warn!(operation = "giga_worker", error = %error);
                return;
            }
        };
        let claim = tokio::select! {
            changed = shutdown.changed() => {
                let _ = changed;
                return;
            }
            claim = giga_event_claim(&pool, request) => claim,
        };
        match claim {
            Ok(claim) if claim.event().is_some() => {
                let processed = tokio::select! {
                    changed = shutdown.changed() => {
                        let _ = changed;
                        return;
                    }
                    processed = giga_process(&pool, &config, &claim) => processed,
                };
                if let Err(error) = processed {
                    tracing::warn!(operation = "giga_worker", error = %error);
                }
                continue;
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(operation = "giga_worker_claim", error = %error);
            }
        }
        tokio::select! {
            changed = shutdown.changed() => {
                let _ = changed;
                return;
            }
            _ = tokio::time::sleep(GIGA_POLL_INTERVAL) => {}
        }
    }
}

pub fn spawn_giga_worker(
    pool: &PgPool,
    config: &Config,
) -> Result<Option<GigaWorkerHandle>, AppError> {
    if !claim_owner_enabled() {
        return Ok(None);
    }
    let room = config
        .giga_source_room
        .as_deref()
        .ok_or_else(|| AppError::Config("enabled GIGA worker requires a source room".into()))?;
    let room = RoomKey::new(room)
        .map_err(|error| AppError::Config(format!("invalid GIGA source room: {error}")))?;
    let (shutdown, receiver) = watch::channel(false);
    let task = tokio::spawn(giga_worker_loop(
        pool.clone(),
        config.clone(),
        room,
        receiver,
    ));
    Ok(Some(GigaWorkerHandle { shutdown, task }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use house_core::{GigaLifecycle, RoomKey};
    use std::path::Path;

    fn conversation_event(
        room: &RoomKey,
        project: Option<&str>,
        texts: &[(&str, &str, &str)],
    ) -> GigaEvent {
        let scope = GigaScope::new(
            Some(room.to_string()),
            project.map(str::to_owned),
            GigaVisibility::Private,
            true,
        )
        .unwrap();
        let sources = texts
            .iter()
            .enumerate()
            .map(|(index, (source_id, role, text))| {
                GigaSourceRef::new(
                    GigaSourceType::Turn,
                    (*source_id).into(),
                    (*role).into(),
                    format!("2026-07-24T12:00:0{index}Z"),
                    sha256_bytes(text.as_bytes()),
                    scope.clone(),
                    None,
                )
                .unwrap()
            })
            .collect();
        GigaEvent::new(
            "event-1".into(),
            GigaEventType::ConversationWindow,
            room.clone(),
            "session-1".into(),
            project.into_iter().map(str::to_owned).collect(),
            sources,
            GigaLifecycle::conversation_window(),
            "2026-07-24T12:00:10Z".into(),
        )
        .unwrap()
    }

    fn source_config(directory: &Path, room: &str) -> Config {
        Config {
            database_url: "postgres://unused".into(),
            embed_url: None,
            embed_model: "unused".into(),
            embed_dimension: 2_048,
            embedding_mode: EmbeddingMode::DisabledForTest,
            giga_source_ledger_dir: Some(directory.to_owned()),
            giga_source_room: Some(room.into()),
        }
    }

    async fn write_ledger(directory: &Path, records: &[Value]) {
        fs::create_dir_all(directory).await.unwrap();
        let body = records
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(directory.join("2026-07-24.jsonl"), format!("{body}\n"))
            .await
            .unwrap();
    }

    fn ledger_record(source_id: &str, role: &str, text: &str) -> Value {
        json!({
            "sessionID": "session-1",
            "messageID": source_id,
            "role": role,
            "spirit": "Lab",
            "text": text
        })
    }

    #[test]
    fn classifier_salvage_parse_strips_thinking_preamble() {
        let content = "Thinking Process:\n1. blah\n{\"foo\":1}";
        assert!(serde_json::from_str::<Value>(content).is_err());
        let salvaged = salvage_json_slice(content).expect("object slice");
        assert_eq!(salvaged, "{\"foo\":1}");
        assert_eq!(
            serde_json::from_str::<Value>(salvaged).unwrap(),
            json!({ "foo": 1 })
        );
        assert_eq!(salvage_json_slice("preamble [1,2]"), Some("[1,2]"));
        assert_eq!(salvage_json_slice("no json here"), None);
    }

    #[tokio::test]
    async fn persisted_source_loader_preserves_event_order_and_types_failures() {
        let room = RoomKey::new("lab").unwrap();
        let event = conversation_event(
            &room,
            None,
            &[
                ("turn-1", "user", "exact user text"),
                ("turn-2", "assistant", "exact assistant text"),
            ],
        );
        let directory = env::temp_dir().join(format!("giga-source-loader-{}", Uuid::new_v4()));
        let config = source_config(&directory, "lab");

        write_ledger(
            &directory,
            &[
                ledger_record("turn-2", "assistant", "exact assistant text"),
                ledger_record("turn-1", "user", "exact user text"),
            ],
        )
        .await;
        let resolved = resolve_sources_from_ledger(&config, &event).await.unwrap();
        assert_eq!(resolved[0].source.source_id(), "turn-1");
        assert_eq!(resolved[1].source.source_id(), "turn-2");

        write_ledger(
            &directory,
            &[ledger_record("turn-1", "user", "exact user text")],
        )
        .await;
        let missing = resolve_sources_from_ledger(&config, &event)
            .await
            .unwrap_err();
        assert_eq!(missing.class(), "GigaSourceMissingError");

        write_ledger(
            &directory,
            &[
                ledger_record("turn-1", "user", "changed"),
                ledger_record("turn-2", "assistant", "exact assistant text"),
            ],
        )
        .await;
        let mismatch = resolve_sources_from_ledger(&config, &event)
            .await
            .unwrap_err();
        assert_eq!(mismatch.class(), "GigaSourceHashMismatchError");

        let oversized_text = "x".repeat(GIGA_MAX_PROCESS_SOURCE_BYTES + 1);
        let oversized_event =
            conversation_event(&room, None, &[("turn-large", "user", &oversized_text)]);
        write_ledger(
            &directory,
            &[ledger_record("turn-large", "user", &oversized_text)],
        )
        .await;
        let oversized = resolve_sources_from_ledger(&config, &oversized_event)
            .await
            .unwrap_err();
        assert_eq!(oversized.class(), "GigaSourceWindowTooLargeError");

        let wrong_room = source_config(&directory, "other-room");
        let unverified = resolve_sources_from_ledger(&wrong_room, &event)
            .await
            .unwrap_err();
        assert_eq!(unverified.class(), "GigaSourceVerificationError");

        fs::remove_dir_all(directory).await.unwrap();
    }

    #[test]
    fn semantic_gate_and_extractor_validation_rejects_invalid_kinds_proofs_and_scores() {
        let room = RoomKey::new("lab").unwrap();
        let event = conversation_event(
            &room,
            None,
            &[("turn-1", "user", "rule"), ("turn-2", "assistant", "proof")],
        );
        let valid_gate = GateOutput {
            kind: GateKind::CodingLesson,
            source_ids: vec!["turn-1".into(), "turn-2".into()],
            reason: "Explicit reusable rule with proof".into(),
        };
        validate_gate(&valid_gate, &event).unwrap();
        assert!(
            validate_gate(
                &GateOutput {
                    kind: GateKind::ProjectLesson,
                    source_ids: vec!["turn-1".into()],
                    reason: "No project exists".into(),
                },
                &event,
            )
            .is_err()
        );
        assert!(
            validate_gate(
                &GateOutput {
                    kind: GateKind::None,
                    source_ids: vec!["turn-1".into()],
                    reason: "none".into(),
                },
                &event,
            )
            .is_err()
        );

        let invalid_extraction = ExtractionOutput {
            source_ids: vec!["turn-1".into(), "turn-2".into()],
            proof_source_ids: Vec::new(),
            proposed_title: "Rule".into(),
            gist: "Apply the rule".into(),
            rationale: "Observed proof".into(),
            priority: 2.0,
            novelty: 0.5,
            durability: 0.8,
            confidence: 0.9,
            retrieval_terms: vec!["rule".into()],
        };
        assert!(validate_extraction(&invalid_extraction, &valid_gate).is_err());
    }

    #[test]
    fn candidate_identity_is_deterministic_across_source_order() {
        let room = RoomKey::new("lab").unwrap();
        let event = conversation_event(
            &room,
            None,
            &[("turn-1", "user", "rule"), ("turn-2", "assistant", "proof")],
        );
        let forward = candidate_id(
            &event,
            GateKind::CodingLesson,
            &["turn-1".into(), "turn-2".into()],
        )
        .unwrap();
        let reverse = candidate_id(
            &event,
            GateKind::CodingLesson,
            &["turn-2".into(), "turn-1".into()],
        )
        .unwrap();
        assert_eq!(forward, reverse);
        assert_eq!(forward.len(), 64);
    }

    #[test]
    fn classifier_configuration_digest_includes_normalized_provider_base_path() {
        let first = OllamaConfig {
            endpoint: Url::parse("http://127.0.0.1:11435/route-a").unwrap(),
        };
        let second = OllamaConfig {
            endpoint: Url::parse("http://127.0.0.1:11435/route-b").unwrap(),
        };
        assert_ne!(
            configuration_digest(&first).unwrap(),
            configuration_digest(&second).unwrap()
        );
    }
    #[test]
    fn extraction_schema_keeps_rationale_before_gist() {
        let schema = extraction_schema(&["turn-1".into()], true).unwrap();
        let properties = schema.split("\"properties\":").nth(1).unwrap();
        assert!(properties.find("\"rationale\"").unwrap() < properties.find("\"gist\"").unwrap());
    }

    #[test]
    fn classifier_rationale_is_bounded_with_an_explicit_marker() {
        let source = "r".repeat(GIGA_MAX_STORED_RATIONALE_BYTES + 32);
        let stored = truncate_with_marker(
            &source,
            GIGA_MAX_STORED_RATIONALE_BYTES,
            GIGA_RATIONALE_TRUNCATION_MARKER,
        );
        assert!(stored.len() <= GIGA_MAX_STORED_RATIONALE_BYTES);
        assert!(stored.ends_with(GIGA_RATIONALE_TRUNCATION_MARKER));
    }

    #[tokio::test]
    async fn pre_signaled_shutdown_exits_before_any_claim() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .unwrap();
        let config = Config {
            database_url: "unused".into(),
            embed_url: None,
            embed_model: "unused".into(),
            embed_dimension: 1,
            embedding_mode: EmbeddingMode::DisabledForTest,
            giga_source_ledger_dir: None,
            giga_source_room: Some("lab".into()),
        };
        let (_shutdown, receiver) = watch::channel(true);
        tokio::time::timeout(
            Duration::from_millis(50),
            giga_worker_loop(pool, config, RoomKey::new("lab").unwrap(), receiver),
        )
        .await
        .expect("a pre-signaled shutdown must not wait for PostgreSQL");
    }
}

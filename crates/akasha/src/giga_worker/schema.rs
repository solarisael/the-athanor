use super::failure::{WorkerFailure, WorkerFailureKind};
use hearth::GigaCandidateKind;
use serde::{Deserialize, Serialize, ser::SerializeMap};
use serde_json::{Value, json};

pub(super) const GIGA_MAX_SELECTED_SOURCES: usize = 4;
pub(super) const GIGA_MAX_RETRIEVAL_TERMS: usize = 8;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum GateKind {
    None,
    Memory,
    CodingLesson,
    ProjectLesson,
    Correction,
    Supersession,
}

impl GateKind {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Memory => "memory",
            Self::CodingLesson => "coding_lesson",
            Self::ProjectLesson => "project_lesson",
            Self::Correction => "correction",
            Self::Supersession => "supersession",
        }
    }

    pub(super) fn candidate_kind(self) -> Result<GigaCandidateKind, WorkerFailure> {
        match self {
            Self::None => Err(WorkerFailure::new(WorkerFailureKind::ClassifierOutput)),
            Self::Memory => Ok(GigaCandidateKind::Memory),
            Self::CodingLesson => Ok(GigaCandidateKind::CodingLesson),
            Self::ProjectLesson => Ok(GigaCandidateKind::ProjectLesson),
            Self::Correction => Ok(GigaCandidateKind::Correction),
            Self::Supersession => Ok(GigaCandidateKind::Supersession),
        }
    }

    pub(super) const fn requires_proof(self) -> bool {
        !matches!(self, Self::None | Self::Memory)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GateOutput {
    pub(super) kind: GateKind,
    pub(super) source_ids: Vec<String>,
    pub(super) reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExtractionOutput {
    pub(super) source_ids: Vec<String>,
    pub(super) proof_source_ids: Vec<String>,
    pub(super) proposed_title: String,
    pub(super) rationale: String,
    pub(super) gist: String,
    pub(super) priority: f64,
    pub(super) novelty: f64,
    pub(super) durability: f64,
    pub(super) confidence: f64,
    pub(super) retrieval_terms: Vec<String>,
}

pub(super) fn gate_schema(source_ids: &[String]) -> Value {
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

pub(super) fn extraction_schema(
    source_ids: &[String],
    proof_required: bool,
) -> Result<String, WorkerFailure> {
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

pub(super) fn schema_json(value: Value) -> Result<String, WorkerFailure> {
    serde_json::to_string(&value)
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::ClassifierRequest))
}

use super::context::context_policy;
use super::failure::{WorkerFailure, WorkerFailureKind};
use super::typesafe::GigaClassifierConfig;
use crate::typesafe::{TYPESAFE_ENDPOINT, TYPESAFE_MODEL};
use hearth::{GigaCandidateKind, GigaEvent};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const GIGA_PROMPT_VERSION: &str = "jev-giga-classifier-v2-context";
pub const GIGA_MODEL_TAG: &str = TYPESAFE_MODEL;

pub(super) fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sha256_json(value: &Value) -> Result<String, WorkerFailure> {
    serde_json::to_vec(value)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::ClassifierRequest))
}

pub(super) fn configuration_digest(config: &GigaClassifierConfig) -> Result<String, WorkerFailure> {
    sha256_json(&json!({
        "provider": "typesafe",
        "endpoint": TYPESAFE_ENDPOINT,
        "model": TYPESAFE_MODEL,
        "policy_revision": config.policy_revision(),
        "prompt_version": GIGA_PROMPT_VERSION,
        "temperature": "provider-default",
        "question_set": ["kind", "priority", "durability", "novelty", "lookup"],
        "novelty": "measured-against-completed-recall",
        "context": context_policy(config)
    }))
}

pub(super) fn candidate_id(
    event: &GigaEvent,
    kind: GigaCandidateKind,
    source_ids: &[String],
) -> Result<String, WorkerFailure> {
    let mut sorted = source_ids.to_vec();
    sorted.sort();
    sha256_json(&json!([
        event.event_id(),
        GIGA_PROMPT_VERSION,
        TYPESAFE_MODEL,
        kind.as_str(),
        sorted
    ]))
}

pub(super) fn source_digest(event: &GigaEvent) -> String {
    let mut digest = Sha256::new();
    for source in event.source_refs() {
        digest.update(source.source_id().as_bytes());
        digest.update([0_u8]);
        digest.update(source.content_hash().as_bytes());
        digest.update([0xff_u8]);
    }
    format!("{:x}", digest.finalize())
}

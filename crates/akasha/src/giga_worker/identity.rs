use hearth::GigaEvent;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use super::failure::{WorkerFailure, WorkerFailureKind};
use super::ollama::{GIGA_NUM_CTX, OllamaConfig};
use super::prompts::{GIGA_EXTRACTION_PROMPT, GIGA_GATE_PROMPT};
use super::schema::GateKind;

pub const GIGA_PROMPT_VERSION: &str = "agents-a1-akashic-librarian-v3";
pub const GIGA_MODEL_TAG: &str = "hf.co/InternScience/Agents-A1-4B-Q4_K_M-GGUF:latest";
pub const GIGA_MODEL_MANIFEST_DIGEST: &str =
    "96ca1ea02b302bf5cd1118d637f12a5af7c2a5aa465837532448bd6e54db4ceb";

pub(super) fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sha256_json(value: &Value) -> Result<String, WorkerFailure> {
    serde_json::to_vec(value)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::ClassifierRequest))
}

pub(super) fn configuration_digest(config: &OllamaConfig) -> Result<String, WorkerFailure> {
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

pub(super) fn candidate_id(
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

use crate::config::HTTP_CLIENT;
use reqwest::{RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json, value::RawValue};
use std::{env, time::Duration};
use super::enablement::classifier_enabled;
use super::failure::{WorkerFailure, WorkerFailureKind};
use super::identity::{GIGA_MODEL_MANIFEST_DIGEST, GIGA_MODEL_TAG};

pub(super) const GIGA_DEFAULT_OLLAMA_ENDPOINT: &str = "http://127.0.0.1:11434";
const GIGA_MODEL_TIMEOUT: Duration = Duration::from_secs(60);
const GIGA_MAX_OLLAMA_RESPONSE_BYTES: usize = 64 * 1024;
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
pub(super) const GIGA_NUM_CTX: u32 = 32_768;

// Ollama defaults to a 5 minute keep_alive, which evicted both models during idle
// gaps and charged a cold reload on the next turn. Lower this if the card is
// needed elsewhere; residency is a comfort setting, not a correctness one.
const GIGA_KEEP_ALIVE: &str = "30m";

#[derive(Clone)]
pub(super) struct OllamaConfig {
    pub(super) endpoint: Url,
}

pub(super) fn is_loopback(endpoint: &Url) -> bool {
    let Some(host) = endpoint.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

pub(super) fn ollama_config() -> Result<OllamaConfig, WorkerFailure> {
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

pub(super) async fn verify_ollama_model(config: &OllamaConfig) -> Result<(), WorkerFailure> {
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

pub(super) async fn request_ollama_structured<T: for<'de> Deserialize<'de>>(
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
pub(super) fn salvage_json_slice(content: &str) -> Option<&str> {
    for (open, close) in [('{', '}'), ('[', ']')] {
        if let Some((start, end)) = content.find(open).zip(content.rfind(close)) {
            if start < end {
                return Some(&content[start..=end]);
            }
        }
    }
    None
}

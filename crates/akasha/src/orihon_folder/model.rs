//! The folder model binding and the local Ollama call. Hosted calls go through the OMP gateway
//! child in `gateway`; both return raw replies that `engine` judges and records.

use crate::AppError;
use hearth::orihon::FolderRuntime;
use origami::orihon::live::store::Tool;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use std::{net::IpAddr, sync::LazyLock, time::Duration};

/// Output budget of one reply, in tokens. A 512-byte line needs far fewer; the headroom lets an
/// over-long line finish, so its real size is measured and corrected instead of cut.
pub(super) const NUM_PREDICT: usize = 768;
/// The longest local response envelope the engine reads.
const RESPONSE_BYTES: usize = 64 * 1024;
/// Chat-template overhead reserved beside the prompt in the local admission budget.
const TEMPLATE_RESERVE: usize = 1024;

// The shared client follows redirects and proxies, which can send source data off loopback.
static LOCAL_CLIENT: LazyLock<Result<Client, reqwest::Error>> = LazyLock::new(|| {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .resolve("localhost", ([127, 0, 0, 1], 0).into())
        .build()
});

#[derive(Clone)]
pub struct FolderConfig {
    pub endpoint: Url,
    pub model: String,
    pub timeout: Duration,
    pub num_ctx: u32,
    pub hosted: Option<FolderRuntime>,
}

impl FolderConfig {
    pub fn local(model: impl Into<String>) -> Self {
        Self {
            endpoint: Url::parse("http://127.0.0.1:11434").expect("static local Ollama URL"),
            model: model.into(),
            timeout: Duration::from_secs(120),
            num_ctx: 32768,
            hosted: None,
        }
    }

    pub fn hosted(model: String, runtime: FolderRuntime) -> Self {
        let mut config = Self::local(model);
        config.num_ctx = 131_072;
        config.hosted = Some(runtime);
        config
    }
}

/// The local chat endpoint: plain loopback HTTP(S) without credentials, query, or fragment.
pub(super) fn chat_url(config: &FolderConfig) -> Result<Url, AppError> {
    let endpoint = &config.endpoint;
    let loopback = endpoint.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });

    if !matches!(endpoint.scheme(), "http" | "https")
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || !loopback
    {
        return Err(AppError::Config(
            "Orihon requires an HTTP(S) loopback endpoint without credentials, query, or fragment"
                .into(),
        ));
    }
    if config.num_ctx == 0 {
        return Err(AppError::Config(
            "Orihon local folding requires a positive context size".into(),
        ));
    }

    let base = endpoint.as_str().trim_end_matches('/');

    Url::parse(&format!("{base}/api/chat"))
        .map_err(|_| AppError::Config("Orihon chat URL is invalid".into()))
}

/// One finished local reply. `done_reason` is `stop` for a whole line, `length` for a cut one.
pub(super) struct LocalReply {
    pub text: String,
    pub done_reason: String,
    pub prompt_eval_count: Option<u64>,
    pub eval_count: Option<u64>,
    /// The model asked for a tool. The compactor runs none, so the attempt fails.
    pub tool_call: bool,
}

/// Why a local call produced no reply. `sent` is false when no request left this process.
pub(super) struct LocalFailure {
    pub error: String,
    pub sent: bool,
}

impl LocalFailure {
    fn unsent(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            sent: false,
        }
    }

    fn sent(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            sent: true,
        }
    }
}

/// The system prompt goes first as Ollama's one system message; the conversation follows.
struct Messages<'a> {
    system: &'a str,
    conversation: &'a [Value],
}

impl Serialize for Messages<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum Entry<'a> {
            System {
                role: &'static str,
                content: &'a str,
            },
            Turn(&'a Value),
        }

        serializer.collect_seq(
            std::iter::once(Entry::System {
                role: "system",
                content: self.system,
            })
            .chain(self.conversation.iter().map(Entry::Turn)),
        )
    }
}

/// The harness tool definitions in Ollama's function shape. They are sent, never executed.
struct Functions<'a>(&'a [Tool]);

impl Functions<'_> {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for Functions<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Function<'a> {
            name: &'a str,
            description: &'a str,
            parameters: &'a Value,
        }

        #[derive(Serialize)]
        struct Definition<'a> {
            r#type: &'static str,
            function: Function<'a>,
        }

        serializer.collect_seq(self.0.iter().map(|tool| Definition {
            r#type: "function",
            function: Function {
                name: &tool.name,
                description: &tool.description,
                parameters: &tool.input_schema,
            },
        }))
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Messages<'a>,
    #[serde(skip_serializing_if = "Functions::is_empty")]
    tools: Functions<'a>,
    stream: bool,
    think: bool,
    options: ChatOptions,
}

#[derive(Serialize)]
struct ChatOptions {
    temperature: u8,
    seed: u32,
    num_ctx: u32,
    num_predict: usize,
}

#[derive(Deserialize)]
struct Envelope {
    model: String,
    done: bool,
    done_reason: String,
    message: Message,
    prompt_eval_count: Option<u64>,
    eval_count: Option<u64>,
}

#[derive(Deserialize)]
struct Message {
    role: String,
    content: String,
    thinking: Option<String>,
    tool_calls: Option<Vec<Value>>,
}

/// Content bytes of every message string the local model reads.
fn prompt_bytes(system: &str, conversation: &[Value]) -> usize {
    conversation
        .iter()
        .map(|message| match &message["content"] {
            Value::String(text) => text.len(),
            _ => 0,
        })
        .fold(system.len(), usize::saturating_add)
}

struct ByteCount(usize);

impl std::io::Write for ByteCount {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Serialized size of the tool definitions, counted without building them.
fn json_bytes(value: &impl Serialize) -> usize {
    let mut count = ByteCount(0);
    let _ = serde_json::to_writer(&mut count, value);
    count.0
}

/// One local chat call. Admission counts one token per content byte, so Ollama never silently
/// truncates a prompt that does not fit `num_ctx`; such a prompt refuses before any request.
pub(super) async fn local_chat(
    config: &FolderConfig,
    url: &Url,
    system: &str,
    tools: &[Tool],
    conversation: &[Value],
) -> Result<LocalReply, LocalFailure> {
    let required = prompt_bytes(system, conversation)
        .saturating_add(json_bytes(&Functions(tools)))
        .saturating_add(TEMPLATE_RESERVE + NUM_PREDICT);
    if required > config.num_ctx as usize {
        return Err(LocalFailure::unsent(format!(
            "the local prompt needs a conservative context of {required} tokens (one per byte); num_ctx is {}",
            config.num_ctx
        )));
    }
    let client = LOCAL_CLIENT
        .as_ref()
        .map_err(|_| LocalFailure::unsent("the local HTTP client could not be initialized"))?;

    let mut response = client
        .post(url.clone())
        .timeout(config.timeout)
        .json(&ChatRequest {
            model: &config.model,
            messages: Messages {
                system,
                conversation,
            },
            tools: Functions(tools),
            stream: false,
            think: false,
            options: ChatOptions {
                temperature: 0,
                seed: 42,
                num_ctx: config.num_ctx,
                num_predict: NUM_PREDICT,
            },
        })
        .send()
        .await
        .map_err(|error| {
            if error.is_connect() {
                LocalFailure::unsent("the local model endpoint refused the connection")
            } else if error.is_timeout() {
                LocalFailure::sent("the local model did not answer within the folder timeout")
            } else {
                LocalFailure::sent("the local model request failed")
            }
        })?;

    if !response.status().is_success() {
        return Err(LocalFailure::sent(format!(
            "the local model answered HTTP status {}",
            response.status().as_u16()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > RESPONSE_BYTES as u64)
    {
        return Err(LocalFailure::sent(
            "the local model response exceeds 64 KiB",
        ));
    }

    let mut body = Vec::new();

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| LocalFailure::sent("the local model response was interrupted"))?
    {
        if body.len().saturating_add(chunk.len()) > RESPONSE_BYTES {
            return Err(LocalFailure::sent(
                "the local model response exceeds 64 KiB",
            ));
        }

        body.extend_from_slice(&chunk);
    }

    let envelope: Envelope = serde_json::from_slice(&body)
        .map_err(|_| LocalFailure::sent("the local model response envelope is invalid"))?;
    let message = envelope.message;

    if !envelope.done
        || !matches!(envelope.done_reason.as_str(), "stop" | "length")
        || envelope.model != config.model
        || message.role != "assistant"
        || message
            .thinking
            .is_some_and(|thinking| !thinking.is_empty())
    {
        return Err(LocalFailure::sent(
            "the local model reply is unfinished, from another model, or carries thinking",
        ));
    }

    Ok(LocalReply {
        text: message.content,
        done_reason: envelope.done_reason,
        prompt_eval_count: envelope.prompt_eval_count,
        eval_count: envelope.eval_count,
        tool_call: message.tool_calls.is_some_and(|calls| !calls.is_empty()),
    })
}

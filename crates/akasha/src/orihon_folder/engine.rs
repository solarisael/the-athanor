//! The Orihon compactor. The Host opens one engine per room prefix and folder model, calls
//! `summarize` once per node, and closes it when the prefix or folder changes or at shutdown.
//! The engine owns the backend, the wire shape, the size rule, the prefix-writer gate, and the
//! job receipts. The Host owns which nodes run, their prompts, and storage.
//!
//! A compaction is a turn's twin (UniiChat §4): the harness's exact system parts and tool
//! definitions (never executed), then one user message whose text parts are the compactor view's
//! whole 4-line blocks, then one user message holding the view's tail and the task. Through OMP's
//! Anthropic provider that shape carries four cache marks: the last tool, the stable system head,
//! the last whole block, and the request end.
//!
//! A line over 512 bytes gets the same-conversation "Too long" correction. After five replies the
//! shortest complete one stands: "a few bytes over is fine, because the view measures real
//! sizes". A reply cut by the output budget is never kept. A failed call ends its job.
//! The Host retries failed work after 60 seconds while a registered fold session participates.
//! A message boundary can retry sooner. Loss of consent or room ownership stops automatic retries.
//!
//! Hosted calls use `omp auth-gateway stdio`, `POST /v1/pi/stream` with `stream: false`. Its body
//! is OMP's final `AssistantMessage`: the token buckets and `usage.cost.total`, the price-table
//! cost OMP's HTTP route sends as `x-litellm-response-cost`. The stdio wire forwards no headers,
//! so the body is the only place the price can come from.

use super::{
    gateway::{Gateway, hosted_route},
    model::{self, FolderConfig, LocalFailure, LocalReply, NUM_PREDICT},
};
use crate::AppError;
use chrono::{SecondsFormat, Utc};
use hearth::orihon::live::too_long;
use origami::orihon::live::{
    LINE_BYTES, MAX_JOBS, MODEL_LINE_CEILING, NodeKey, Rendered, sha256_hex,
    store::{Attempt, Job, Outcome, Prefix, Tool, Usage},
};
use reqwest::Url;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{Semaphore, watch};
use tokio_util::sync::CancellationToken;

/// UniiChat: "Try at most 5 times, and keep the shortest line."
const MAX_ATTEMPTS: u32 = 5;
/// Anthropic keeps a short-retention cache entry five minutes from its last use.
const CACHE_LIFETIME: Duration = Duration::from_secs(5 * 60);
const LOCAL_PRICE: &str = "the local Ollama backend reports no price";
/// The gate's exact wait, worded for status surfaces.
const GATE_NOTE: &str = "first-writer barrier: a compaction whose cached prefix another call is \
writing waits for that call's whole reply, because OMP stdio shows no response start; no cold \
prefix is written twice, at about one call's latency per cold prefix";

/// One node to build. The Host writes `prompt` with `hearth::orihon::live::compress_task` or
/// `merge_task`; `context` is the compactor view's built lines before the node, rendered.
pub struct Task {
    pub key: NodeKey,
    pub context: Rendered,
    pub prompt: String,
}

/// The accepted line, if any, and the job receipt in every case.
pub struct Completion {
    pub text: Option<String>,
    pub job: Job,
}

/// One compactor for one room prefix and folder model. Clones share it; one `close` ends it.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

struct Inner {
    config: FolderConfig,
    backend: Backend,
    prefix: Prefix,
    prefix_digest: String,
    cache_scope: String,
    provider: String,
    /// Each running job holds one permit from admission to its receipt.
    permits: Semaphore,
    closed: AtomicBool,
}

enum Backend {
    Local {
        url: Url,
        /// The system parts as Ollama's one system message.
        system: String,
    },
    Hosted {
        /// The model id OMP must report serving: the selector after its provider.
        id: String,
        /// Anthropic only: other providers bill no cache writes.
        gate: Option<Gate>,
        gateway: tokio::sync::Mutex<Option<Arc<Gateway>>>,
    },
}

impl Engine {
    /// Opens the backend. A hosted engine starts its gateway child, reads the handshake, and
    /// proves the model is in the bound chat catalog before it returns; a local one checks its
    /// loopback endpoint. An error leaves nothing running.
    pub async fn open(config: FolderConfig, prefix: Prefix) -> Result<Self, AppError> {
        if config.model.trim().is_empty()
            || config.timeout.is_zero()
            || Instant::now().checked_add(config.timeout).is_none()
        {
            return Err(AppError::Config(
                "Orihon requires a folder model and a finite positive timeout".into(),
            ));
        }
        let prefix_digest = prefix.digest();
        let (backend, provider, binding) = match &config.hosted {
            Some(runtime) => {
                let (provider, _) = hosted_route(&config.model)?;
                let id = config.model[provider.len() + 1..].to_owned();
                let gateway = Gateway::start(runtime, &config.model, config.timeout).await?;
                (
                    Backend::Hosted {
                        id,
                        gate: (provider == "anthropic").then(Gate::default),
                        gateway: tokio::sync::Mutex::new(Some(Arc::new(gateway))),
                    },
                    provider.to_owned(),
                    json!({ "runtime": runtime }),
                )
            }
            None => (
                Backend::Local {
                    url: model::chat_url(&config)?,
                    system: prefix.system.join("\n\n"),
                },
                "ollama".to_owned(),
                json!({ "endpoint": config.endpoint.as_str(), "numCtx": config.num_ctx }),
            ),
        };
        // Cache entries belong to one backend, provider, model, account binding, and prefix.
        // Nothing here claims two scopes share an entry.
        let cache_scope = sha256_hex(
            json!({
                "backend": backend.name(),
                "provider": provider,
                "model": config.model,
                "binding": binding,
                "prefixDigest": prefix_digest,
            })
            .to_string()
            .as_bytes(),
        );
        Ok(Self {
            inner: Arc::new(Inner {
                config,
                backend,
                prefix,
                prefix_digest,
                cache_scope,
                provider,
                permits: Semaphore::new(MAX_JOBS),
                closed: AtomicBool::new(false),
            }),
        })
    }

    /// Builds one node line. It never fails: every outcome is a `Completion` whose job lists
    /// each attempt with its usage and price, or why they are unknown. `text` is `Some` exactly
    /// when the job is accepted.
    ///
    /// Cancellation starts no further attempt and ends any wait for a call slot or the gate. A
    /// hosted request already queued to the gateway cannot be withdrawn, so its reply is awaited
    /// (bounded by the folder timeout) and recorded; a line within 512 bytes from it is still
    /// accepted. A local request is aborted at once.
    pub async fn summarize(&self, task: Task, cancellation: CancellationToken) -> Completion {
        let inner = &*self.inner;
        let Task {
            key,
            context,
            prompt,
        } = task;
        let mut run = Run {
            job: inner.receipt(key, &context, &prompt),
            attempts: Vec::new(),
        };
        if inner.closed.load(Ordering::Acquire) {
            return run.finish(Err(Stop::Failed("the Orihon engine is closed".into())));
        }
        let permit = tokio::select! {
            biased;
            () = cancellation.cancelled() => {
                let stop = Stop::Cancelled("cancelled before a call slot opened".into());
                return run.finish(Err(stop));
            }
            permit = inner.permits.acquire() => permit,
        };
        let (Ok(_permit), false) = (permit, inner.closed.load(Ordering::Acquire)) else {
            return run.finish(Err(Stop::Failed("the Orihon engine is closed".into())));
        };
        let chain = inner
            .gate()
            .map(|_| chain(&inner.cache_scope, &context.blocks));
        let mut conversation = inner.backend.opening(context, &prompt);
        let result = inner
            .converse(&mut run, &mut conversation, chain.as_deref(), &cancellation)
            .await;
        run.finish(result)
    }

    /// How this engine coordinates cache writers, for status notes: `Some` for hosted Anthropic.
    pub fn cache_gate(&self) -> Option<&'static str> {
        self.inner.gate().map(|_| GATE_NOTE)
    }

    /// Ends the engine: new calls fail at once, running ones finish first, then the gateway child
    /// gets stdin EOF, an exit wait, a kill after the exit timeout, and its overlay is removed.
    /// The Host cancels its tokens and awaits its calls before this; a second close is `Ok`.
    pub async fn close(self) -> Result<(), AppError> {
        let inner = &*self.inner;
        inner.closed.store(true, Ordering::Release);
        match inner.permits.acquire_many(MAX_JOBS as u32).await {
            Ok(permits) => {
                inner.permits.close();
                drop(permits);
            }
            Err(_) => return Ok(()),
        }
        match &inner.backend {
            Backend::Hosted { gateway, .. } => match gateway.lock().await.take() {
                Some(gateway) => gateway.close().await,
                None => Ok(()),
            },
            Backend::Local { .. } => Ok(()),
        }
    }
}

impl Inner {
    fn gate(&self) -> Option<&Gate> {
        match &self.backend {
            Backend::Hosted { gate, .. } => gate.as_ref(),
            Backend::Local { .. } => None,
        }
    }

    fn receipt(&self, key: NodeKey, context: &Rendered, prompt: &str) -> Job {
        let mut digest = Sha256::new();
        for block in &context.blocks {
            digest.update(block.as_bytes());
        }
        digest.update(context.tail.as_bytes());
        Job {
            id: None,
            key,
            outcome: Outcome::Failed,
            backend: self.backend.name().to_owned(),
            provider: self.provider.clone(),
            model: self.config.model.clone(),
            cache_scope: self.cache_scope.clone(),
            prefix_digest: self.prefix_digest.clone(),
            context_digest: format!("{:x}", digest.finalize()),
            context_lines: count(context.lines),
            context_bytes: count(context.bytes),
            input_bytes: count(prompt.len()),
            attempts: Vec::new(),
            usage: Usage::default(),
            cost_usd: None,
            cost_reason: None,
            error: None,
            started_at: now(),
            finished_at: String::new(),
        }
    }

    async fn converse(
        &self,
        run: &mut Run,
        conversation: &mut Vec<Value>,
        chain: Option<&[String]>,
        cancellation: &CancellationToken,
    ) -> Result<String, Stop> {
        // Complete replies over the limit, as (attempt index, line), for the shortest rule.
        let mut over = Vec::new();
        for number in 1..=MAX_ATTEMPTS {
            if cancellation.is_cancelled() {
                return Err(Stop::Cancelled(format!(
                    "cancelled before attempt {number}"
                )));
            }
            // Corrections extend this job's own prefix, so only the first attempt can collide
            // with another call's cache write.
            let admission = match (number, self.gate(), chain) {
                (1, Some(gate), Some(chain)) => match gate.admit(chain, cancellation).await {
                    Some(admission) => Some(admission),
                    None => {
                        return Err(Stop::Cancelled(
                            "cancelled while another call wrote the shared cache prefix".into(),
                        ));
                    }
                },
                _ => None,
            };
            let started = Instant::now();
            let call = self.call(conversation, cancellation).await;
            if let Some(admission) = admission {
                admission.settle(call.served);
            }
            let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let reply = match call.result {
                Ok(reply) => reply,
                Err(Stop::Cancelled(reason)) => {
                    run.push(
                        number,
                        "cancelled",
                        None,
                        call.receipt,
                        Some(&reason),
                        elapsed_ms,
                    );
                    return Err(Stop::Cancelled(reason));
                }
                Err(Stop::Failed(reason)) => {
                    run.push(
                        number,
                        "failed",
                        None,
                        call.receipt,
                        Some(&reason),
                        elapsed_ms,
                    );
                    return Err(Stop::Failed(format!("attempt {number}: {reason}")));
                }
            };
            let line = reply.text.trim();
            let bytes = line.len();
            if line.is_empty() || bytes > MODEL_LINE_CEILING {
                let reason = if line.is_empty() {
                    "the reply is empty"
                } else {
                    "the reply exceeds 16 KiB"
                };
                run.push(
                    number,
                    "failed",
                    Some(bytes),
                    call.receipt,
                    Some(reason),
                    elapsed_ms,
                );
                return Err(Stop::Failed(format!("attempt {number}: {reason}")));
            }
            if bytes <= LINE_BYTES {
                if !reply.complete {
                    // The budget cut it, yet it fits: "Too long" would be false, and a cut line
                    // is never kept.
                    let reason = "the reply reached the output budget within 512 bytes";
                    run.push(
                        number,
                        "failed",
                        Some(bytes),
                        call.receipt,
                        Some(reason),
                        elapsed_ms,
                    );
                    return Err(Stop::Failed(format!("attempt {number}: {reason}")));
                }
                let line = line.to_owned();
                run.push(
                    number,
                    "accepted",
                    Some(bytes),
                    call.receipt,
                    None,
                    elapsed_ms,
                );
                return Ok(line);
            }
            run.push(
                number,
                "too_long",
                Some(bytes),
                call.receipt,
                None,
                elapsed_ms,
            );
            if reply.complete {
                over.push((run.attempts.len() - 1, line.to_owned()));
            }
            if number < MAX_ATTEMPTS {
                let correction = too_long(bytes, cut(line, LINE_BYTES));
                self.backend.correct(conversation, reply, correction);
            }
        }
        // The first of equally short lines stands.
        match over.into_iter().min_by_key(|(_, line)| line.len()) {
            Some((index, line)) => {
                run.attempts[index].outcome = "accepted".into();
                Ok(line)
            }
            None => Err(Stop::Failed(format!(
                "no complete line in {MAX_ATTEMPTS} replies: each one reached the output budget"
            ))),
        }
    }

    async fn call(&self, conversation: &[Value], cancellation: &CancellationToken) -> Call {
        match &self.backend {
            Backend::Local { url, system } => tokio::select! {
                biased;
                () = cancellation.cancelled() => Call::stopped(
                    Receipt::unobserved(LOCAL_PRICE),
                    Stop::Cancelled("cancelled; the local request was aborted".into()),
                ),
                reply = model::local_chat(&self.config, url, system, &self.prefix.tools, conversation) => {
                    local_call(reply)
                }
            },
            Backend::Hosted { id, gateway, .. } => {
                self.hosted_call(id, gateway, conversation, cancellation)
                    .await
            }
        }
    }

    async fn hosted_call(
        &self,
        id: &str,
        slot: &tokio::sync::Mutex<Option<Arc<Gateway>>>,
        conversation: &[Value],
        cancellation: &CancellationToken,
    ) -> Call {
        let gateway = match self.gateway(slot).await {
            Ok(gateway) => gateway,
            Err(error) => return Call::stopped(Receipt::unsent(), Stop::Failed(error.to_string())),
        };
        if cancellation.is_cancelled() {
            return Call::stopped(
                Receipt::unsent(),
                Stop::Cancelled("cancelled before the request was sent".into()),
            );
        }
        let request = PiRequest {
            model: &self.config.model,
            context: PiContext {
                system_prompt: &self.prefix.system,
                messages: conversation,
                tools: Tools(&self.prefix.tools),
            },
            options: PiOptions {
                max_tokens: NUM_PREDICT,
                temperature: 0,
                disable_reasoning: true,
                cache_retention: "short",
                tool_choice: "none",
                session_id: &self.cache_scope,
            },
            stream: false,
        };
        let (request_id, reply) = match gateway.send("POST", "/v1/pi/stream", Some(&request)).await
        {
            Ok(queued) => queued,
            Err(error) => return Call::stopped(Receipt::unsent(), Stop::Failed(error)),
        };
        // stdio cannot withdraw a queued request: even a cancelled job waits here, because the
        // reply's usage and price are real spending that belongs in the receipt.
        match tokio::time::timeout(self.config.timeout, reply).await {
            Ok(Ok(line)) => hosted_reply(line, &self.provider, id),
            Ok(Err(_)) => Call::stopped(
                Receipt::unobserved(
                    "the gateway exited before replying; usage and price unobserved",
                ),
                Stop::Failed(
                    "the OMP gateway exited after the request was sent; outcome unknown".into(),
                ),
            ),
            Err(_) => {
                gateway.forget(request_id);
                Call::stopped(
                    Receipt::unobserved(
                        "no reply within the folder timeout; usage and price unobserved",
                    ),
                    Stop::Failed(format!(
                        "no OMP gateway reply within {} s after the request was sent; outcome unknown",
                        self.config.timeout.as_secs()
                    )),
                )
            }
        }
    }

    /// The live gateway. A child that exited or broke its stream is retired and replaced here,
    /// so jobs the Host retries at a later boundary meet a working gateway.
    async fn gateway(
        &self,
        slot: &tokio::sync::Mutex<Option<Arc<Gateway>>>,
    ) -> Result<Arc<Gateway>, AppError> {
        let mut slot = slot.lock().await;
        if let Some(gateway) = slot.as_ref().filter(|gateway| gateway.alive()) {
            return Ok(Arc::clone(gateway));
        }
        if let Some(retired) = slot.take()
            && let Err(error) = retired.close().await
        {
            tracing::warn!(%error, "Orihon OMP gateway did not end cleanly after it failed");
        }
        let Some(runtime) = &self.config.hosted else {
            return Err(AppError::Config(
                "Orihon hosted folding lost its OMP binding".into(),
            ));
        };
        let gateway =
            Arc::new(Gateway::start(runtime, &self.config.model, self.config.timeout).await?);
        *slot = Some(Arc::clone(&gateway));
        Ok(gateway)
    }
}

impl Backend {
    fn name(&self) -> &'static str {
        match self {
            Self::Local { .. } => "local",
            Self::Hosted { .. } => "hosted",
        }
    }

    /// The first request: whole blocks as separate text parts of one user message, then the
    /// tail and the task. With no whole block, only the second message is sent.
    fn opening(&self, context: Rendered, prompt: &str) -> Vec<Value> {
        let Rendered { blocks, tail, .. } = context;
        let ending = format!("{tail}\n{prompt}");
        let mut conversation = Vec::with_capacity(2);
        match self {
            Self::Local { .. } => {
                if !blocks.is_empty() {
                    conversation.push(local_message("user", blocks.concat()));
                }
                conversation.push(local_message("user", ending));
            }
            Self::Hosted { .. } => {
                let timestamp = Utc::now().timestamp_millis();
                if !blocks.is_empty() {
                    let parts = blocks.into_iter().map(text_part).collect();
                    conversation.push(pi_user(parts, timestamp));
                }
                conversation.push(pi_user(vec![text_part(ending)], timestamp));
            }
        }
        conversation
    }

    /// The same-conversation correction: the reply as the model gave it, then "Too long".
    fn correct(&self, conversation: &mut Vec<Value>, reply: Reply, correction: String) {
        conversation.push(match reply.replay {
            Some(message) => message,
            None => local_message("assistant", reply.text),
        });
        conversation.push(match self {
            Self::Local { .. } => local_message("user", correction),
            Self::Hosted { .. } => {
                pi_user(vec![text_part(correction)], Utc::now().timestamp_millis())
            }
        });
    }
}

/// What one model call returned, before the size rule judges it.
struct Call {
    receipt: Receipt,
    /// The provider answered, so a cache write may have happened.
    served: bool,
    result: Result<Reply, Stop>,
}

impl Call {
    fn stopped(receipt: Receipt, stop: Stop) -> Self {
        Self {
            receipt,
            served: false,
            result: Err(stop),
        }
    }
}

struct Reply {
    text: String,
    /// False when the reply stopped at the output budget: a cut line is never kept.
    complete: bool,
    /// OMP's own message, replayed verbatim by a hosted correction.
    replay: Option<Value>,
}

enum Stop {
    Failed(String),
    Cancelled(String),
}

/// One attempt's spending. `None` is unreported, never zero; a price is either reported or has
/// a reason.
struct Receipt {
    usage: Usage,
    cost_usd: Option<f64>,
    cost_reason: Option<String>,
    stop_reason: Option<String>,
}

impl Receipt {
    /// No request reached the gateway, so nothing was used or billed.
    fn unsent() -> Self {
        Self {
            usage: Usage {
                input_tokens: Some(0),
                output_tokens: Some(0),
                cache_read_tokens: Some(0),
                cache_write_tokens: Some(0),
            },
            cost_usd: Some(0.0),
            cost_reason: None,
            stop_reason: None,
        }
    }

    fn unobserved(reason: &str) -> Self {
        Self {
            usage: Usage::default(),
            cost_usd: None,
            cost_reason: Some(reason.to_owned()),
            stop_reason: None,
        }
    }

    fn hosted(message: &Value, provider: &str) -> Self {
        let usage = &message["usage"];
        let tokens = |name: &str| usage[name].as_u64();
        let (cost_usd, cost_reason) = match usage["cost"]["total"].as_f64() {
            Some(total) if total.is_finite() && total >= 0.0 => (Some(total), None),
            _ => (
                None,
                Some("OMP reported no finite nonnegative cost for this reply".to_owned()),
            ),
        };
        Self {
            usage: Usage {
                input_tokens: tokens("input"),
                output_tokens: tokens("output"),
                cache_read_tokens: tokens("cacheRead"),
                // OpenAI reports no cache-write count; OMP's zero there is not a provider report.
                cache_write_tokens: (provider == "anthropic")
                    .then(|| tokens("cacheWrite"))
                    .flatten(),
            },
            cost_usd,
            cost_reason,
            stop_reason: message["stopReason"].as_str().map(str::to_owned),
        }
    }
}

fn local_call(reply: Result<LocalReply, LocalFailure>) -> Call {
    match reply {
        Ok(reply) if reply.tool_call => Call {
            receipt: Receipt {
                usage: Usage {
                    input_tokens: reply.prompt_eval_count,
                    output_tokens: reply.eval_count,
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                },
                cost_usd: None,
                cost_reason: Some(LOCAL_PRICE.to_owned()),
                stop_reason: Some(reply.done_reason),
            },
            served: true,
            result: Err(Stop::Failed(
                "the reply asked for a tool; the compactor runs no tools".into(),
            )),
        },
        Ok(reply) => Call {
            receipt: Receipt {
                usage: Usage {
                    input_tokens: reply.prompt_eval_count,
                    output_tokens: reply.eval_count,
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                },
                cost_usd: None,
                cost_reason: Some(LOCAL_PRICE.to_owned()),
                stop_reason: Some(reply.done_reason.clone()),
            },
            served: true,
            result: Ok(Reply {
                complete: reply.done_reason == "stop",
                text: reply.text,
                replay: None,
            }),
        },
        Err(failure) => {
            let mut receipt = Receipt::unobserved(LOCAL_PRICE);
            if !failure.sent {
                receipt.usage = Receipt::unsent().usage;
            }
            Call::stopped(receipt, Stop::Failed(failure.error))
        }
    }
}

fn hosted_reply(mut line: Value, provider: &str, id: &str) -> Call {
    let status = line["status"].as_u64();
    if status != Some(200) || line.get("encoding").is_some() {
        // Provider error text can carry credentials; only the gateway's error class is kept.
        let class = line["body"]["error"]["type"]
            .as_str()
            .filter(|class| {
                class.len() <= 64
                    && class
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
            })
            .unwrap_or("unclassified");
        let status = status.map_or_else(
            || "no status".to_owned(),
            |status| format!("status {status}"),
        );
        return Call::stopped(
            Receipt::unobserved("the gateway error reply carries no usage or price"),
            Stop::Failed(format!("the OMP gateway answered {status} ({class})")),
        );
    }
    let message = line
        .pointer_mut("/body/message")
        .map(Value::take)
        .unwrap_or_default();
    let receipt = Receipt::hosted(&message, provider);
    let failed = |receipt, reason: &str| Call {
        receipt,
        served: true,
        result: Err(Stop::Failed(reason.to_owned())),
    };
    if message["role"] != "assistant" || message["provider"] != provider || message["model"] != id {
        return failed(
            receipt,
            "the reply names another provider or model than the configured selector",
        );
    }
    let Some(blocks) = message["content"].as_array() else {
        return failed(receipt, "the reply has no content list");
    };
    let mut text = String::new();
    for block in blocks {
        match (block["type"].as_str(), block["text"].as_str()) {
            (Some("text"), Some(part)) => text.push_str(part),
            // Thoughts are never logged (UniiChat §1).
            (Some("thinking" | "redactedThinking"), _) => {}
            _ => {
                return failed(
                    receipt,
                    "the reply holds a tool call or other non-text content; the compactor runs no tools",
                );
            }
        }
    }
    let complete = match message["stopReason"].as_str() {
        Some("stop") => true,
        Some("length") => false,
        _ => {
            return failed(
                receipt,
                "the reply ended for a reason other than its end or its output budget",
            );
        }
    };
    Call {
        receipt,
        served: true,
        result: Ok(Reply {
            text,
            complete,
            replay: Some(message),
        }),
    }
}

/// A job's receipt while it runs.
struct Run {
    job: Job,
    attempts: Vec<Attempt>,
}

impl Run {
    fn push(
        &mut self,
        number: u32,
        outcome: &str,
        bytes: Option<usize>,
        receipt: Receipt,
        error: Option<&str>,
        elapsed_ms: u64,
    ) {
        self.attempts.push(Attempt {
            attempt: number,
            outcome: outcome.to_owned(),
            bytes,
            usage: receipt.usage,
            stop_reason: receipt.stop_reason,
            error: error.map(str::to_owned),
            elapsed_ms,
            cost_usd: receipt.cost_usd,
            cost_reason: receipt.cost_reason,
        });
    }

    /// Totals spend over every attempt. A price total exists only when each attempt reported
    /// one; otherwise the first unpriced attempt is the reason.
    fn finish(self, result: Result<String, Stop>) -> Completion {
        let Self { mut job, attempts } = self;
        job.usage = Usage::total(attempts.iter().map(|attempt| &attempt.usage));
        let mut cost = Some(0.0);
        for attempt in &attempts {
            match (cost, attempt.cost_usd) {
                (Some(total), Some(price)) => cost = Some(total + price),
                (Some(_), None) => {
                    cost = None;
                    job.cost_reason = Some(format!(
                        "attempt {}: {}",
                        attempt.attempt,
                        attempt
                            .cost_reason
                            .as_deref()
                            .unwrap_or("no price reported")
                    ));
                }
                (None, _) => {}
            }
        }
        job.cost_usd = cost;
        job.attempts = attempts;
        job.finished_at = now();
        let text = match result {
            Ok(line) => {
                job.outcome = Outcome::Accepted;
                Some(line)
            }
            Err(Stop::Failed(error)) => {
                job.outcome = Outcome::Failed;
                job.error = Some(error);
                None
            }
            Err(Stop::Cancelled(reason)) => {
                job.outcome = Outcome::Cancelled;
                job.error = Some(reason);
                None
            }
        };
        Completion { text, job }
    }
}

/// Which Anthropic cache entries of this engine are being written, or were written lately. A key
/// chains the cache scope with each whole block of a call's view, so it names one exact prefix.
///
/// A call whose prefix holds an entry another call is still writing waits for that call, then
/// reads the entry instead of paying the same cold write twice (UniiChat §3.3). OMP's stdio
/// gateway answers each request whole, so a response start is invisible here: the wait lasts
/// until the writer's reply arrives. That is the conservative form. It never doubles a write; it
/// costs each waiting call the writer's whole generation time, about one call per cold prefix.
#[derive(Default)]
struct Gate {
    entries: Mutex<HashMap<String, Entry>>,
}

enum Entry {
    /// Dropping the writer's sender wakes every waiter.
    Writing(watch::Receiver<()>),
    Written(Instant),
}

impl Gate {
    /// Admits a call once no key of its chain is being written. The call then writes the entries
    /// at its own marks, the scope and its last whole block, unless they are fresh.
    async fn admit<'a>(
        &'a self,
        chain: &'a [String],
        cancellation: &CancellationToken,
    ) -> Option<Admission<'a>> {
        loop {
            let mut writer = {
                let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
                entries.retain(|_, entry| {
                    !matches!(entry, Entry::Written(at) if at.elapsed() >= CACHE_LIFETIME)
                });
                let busy = chain.iter().find_map(|key| match entries.get(key) {
                    Some(Entry::Writing(writer)) => Some(writer.clone()),
                    _ => None,
                });
                match busy {
                    Some(writer) => writer,
                    None => {
                        let (sender, receiver) = watch::channel(());
                        let mut writes = Vec::with_capacity(2);
                        for key in [chain.first(), chain.last()].into_iter().flatten() {
                            if !writes.contains(key)
                                && !matches!(entries.get(key), Some(Entry::Written(_)))
                            {
                                entries.insert(key.clone(), Entry::Writing(receiver.clone()));
                                writes.push(key.clone());
                            }
                        }
                        return Some(Admission {
                            gate: self,
                            chain,
                            writes,
                            served: false,
                            _writing: sender,
                        });
                    }
                }
            };
            tokio::select! {
                biased;
                () = cancellation.cancelled() => return None,
                _ = writer.changed() => {}
            }
        }
    }
}

/// An admitted first attempt. Settling or dropping it records what its call wrote and wakes
/// every waiter.
struct Admission<'a> {
    gate: &'a Gate,
    chain: &'a [String],
    writes: Vec<String>,
    served: bool,
    _writing: watch::Sender<()>,
}

impl Admission<'_> {
    fn settle(mut self, served: bool) {
        self.served = served;
    }
}

impl Drop for Admission<'_> {
    fn drop(&mut self) {
        let mut entries = self
            .gate
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let now = Instant::now();
        for key in &self.writes {
            if self.served {
                entries.insert(key.clone(), Entry::Written(now));
            } else {
                entries.remove(key);
            }
        }
        if self.served {
            // A served call read the entries its prefix holds, which renews their lifetime.
            for key in self.chain {
                if let Some(Entry::Written(at)) = entries.get_mut(key) {
                    *at = now;
                }
            }
        }
    }
}

fn chain(scope: &str, blocks: &[String]) -> Vec<String> {
    let mut chain = Vec::with_capacity(blocks.len() + 1);
    chain.push(scope.to_owned());
    for block in blocks {
        let previous = chain.last().map_or("", String::as_str);
        let digest = Sha256::new()
            .chain_update(previous)
            .chain_update(block)
            .finalize();
        chain.push(format!("{digest:x}"));
    }
    chain
}

#[derive(Serialize)]
struct PiRequest<'a> {
    model: &'a str,
    context: PiContext<'a>,
    options: PiOptions<'a>,
    stream: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PiContext<'a> {
    system_prompt: &'a [String],
    messages: &'a [Value],
    #[serde(skip_serializing_if = "Tools::is_empty")]
    tools: Tools<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PiOptions<'a> {
    max_tokens: usize,
    temperature: u8,
    disable_reasoning: bool,
    cache_retention: &'static str,
    /// The compactor runs no tools; the parent's no-network probe saw `tool_choice: none` on
    /// the wire with the four cache marks intact.
    tool_choice: &'static str,
    /// Keeps every call of this engine on one OMP credential, so they share cache entries.
    session_id: &'a str,
}

/// The harness tool definitions in OMP's own `Tool` shape, in provider order.
struct Tools<'a>(&'a [Tool]);

impl Tools<'_> {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for Tools<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Definition<'a> {
            name: &'a str,
            description: &'a str,
            parameters: &'a Value,
        }

        serializer.collect_seq(self.0.iter().map(|tool| Definition {
            name: &tool.name,
            description: &tool.description,
            parameters: &tool.input_schema,
        }))
    }
}

fn text_part(text: String) -> Value {
    let mut part = Map::with_capacity(2);
    part.insert("type".into(), Value::String("text".into()));
    part.insert("text".into(), Value::String(text));
    Value::Object(part)
}

fn pi_user(content: Vec<Value>, timestamp: i64) -> Value {
    let mut message = Map::with_capacity(3);
    message.insert("role".into(), Value::String("user".into()));
    message.insert("content".into(), Value::Array(content));
    message.insert("timestamp".into(), Value::from(timestamp));
    Value::Object(message)
}

fn local_message(role: &'static str, content: String) -> Value {
    let mut message = Map::with_capacity(2);
    message.insert("role".into(), Value::String(role.into()));
    message.insert("content".into(), Value::String(content));
    Value::Object(message)
}

/// The longest prefix of `text` within `limit` bytes that ends on a character boundary.
fn cut(text: &str, limit: usize) -> &str {
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

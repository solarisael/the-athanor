use reqwest::{Client, StatusCode, redirect::Policy};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    path::Path,
    sync::LazyLock,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::time::timeout;
use uuid::Uuid;

const TYPESAFE_ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const TYPESAFE_MODEL: &str = "jev-latest";
const MAX_PACKET_BYTES: usize = 32_768;
const MAX_RESPONSE_BYTES: usize = 65_536;
const DEFAULT_DEADLINE_MS: u64 = 5_000;
const MAX_CARDS: usize = 48;
const MAX_CARD_CODEPOINTS: usize = 384;
const MESSAGE_CHARS: usize = 300;
const REPLY_CHARS: usize = 400;
const ASSISTANT_TAIL_CHARS: usize = 600;
const RECALL_TITLES_CHARS: usize = 600;
static PROVIDER_CLIENT: LazyLock<Result<Client, reqwest::Error>> =
    LazyLock::new(|| Client::builder().redirect(Policy::none()).build());

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecallGrantKind {
    Recall,
    Lessons,
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum JudgmentRequest {
    RecallPolicy {
        grant: RecallGrantKind,
    },
    ModePolicy,
    VerdictPolicy,
    RecallRerank {
        grant: RecallGrantKind,
        query: String,
        retrieval_candidates: Vec<Value>,
        #[serde(default)]
        rerank_candidates: Option<Vec<Value>>,
        #[serde(default)]
        exact_candidate_indexes: Vec<usize>,
        #[serde(default)]
        credential: Option<String>,
        #[serde(default)]
        deadline: Option<u64>,
    },
    ModeScore {
        operator_message: String,
        assistant_turn: String,
        operator_reply: String,
        #[serde(default)]
        credential: Option<String>,
        #[serde(default)]
        deadline: Option<u64>,
    },
    TurnVerdict {
        operator_message: String,
        assistant_turn: String,
        operator_reply: String,
        #[serde(default)]
        recall_titles: Vec<String>,
        #[serde(default)]
        credential: Option<String>,
        #[serde(default)]
        deadline: Option<u64>,
    },
}

#[derive(Clone, Debug)]
struct GrantPolicy {
    mode: String,
    approved: bool,
    provider: Option<String>,
    endpoint: Option<String>,
    revision: Option<String>,
    room: Option<String>,
}

struct Card {
    index: usize,
    token: String,
    text: String,
}

pub async fn execute(
    room_dir: &Path,
    room: &str,
    request: JudgmentRequest,
) -> Result<Value, String> {
    let marker = read_marker(room_dir).await;
    match request {
        JudgmentRequest::RecallPolicy { grant } => {
            Ok(recall_policy_value(&recall_policy(&marker, room, grant)))
        }
        JudgmentRequest::ModePolicy => Ok(mode_policy_value(&mode_policy(&marker, room))),
        JudgmentRequest::VerdictPolicy => Ok(verdict_policy_value(&verdict_policy(&marker, room))),
        JudgmentRequest::RecallRerank {
            grant,
            query,
            retrieval_candidates,
            rerank_candidates,
            exact_candidate_indexes,
            credential,
            deadline,
        } => Ok(rerank(
            room,
            &marker,
            grant,
            query,
            retrieval_candidates,
            rerank_candidates,
            exact_candidate_indexes,
            credential,
            deadline,
        )
        .await),
        JudgmentRequest::ModeScore {
            operator_message,
            assistant_turn,
            operator_reply,
            credential,
            deadline,
        } => Ok(score_mode(
            room,
            &marker,
            operator_message,
            assistant_turn,
            operator_reply,
            credential,
            deadline,
        )
        .await),
        JudgmentRequest::TurnVerdict {
            operator_message,
            assistant_turn,
            operator_reply,
            recall_titles,
            credential,
            deadline,
        } => Ok(score_verdict(
            room,
            &marker,
            operator_message,
            assistant_turn,
            operator_reply,
            recall_titles,
            credential,
            deadline,
        )
        .await),
    }
}

async fn read_marker(room_dir: &Path) -> Value {
    tokio::fs::read(room_dir.join(".athanor-room.json"))
        .await
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null)
}

fn text_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn grant_policy(
    marker: &Value,
    room: &str,
    key: &str,
    purpose: &str,
    flag: &str,
    allow_laya: bool,
    allowed_modes: &[&str],
) -> GrantPolicy {
    let grant = marker.get(key).unwrap_or(&Value::Null);
    let provider = text_at(grant, "provider");
    let revision = grant
        .pointer("/grant/policyRevision")
        .and_then(Value::as_str);
    let mode = text_at(grant, "mode");
    let provider_valid = provider == Some("typesafe") || (allow_laya && provider == Some("laya"));
    let endpoint = if provider == Some("typesafe") {
        Some(TYPESAFE_ENDPOINT.to_owned())
    } else if allow_laya && provider == Some("laya") {
        text_at(grant, "endpoint")
            .filter(|endpoint| is_loopback_endpoint(endpoint))
            .map(str::to_owned)
    } else {
        None
    };
    let approved = text_at(marker, "room") == Some(room)
        && mode.is_some_and(|mode| allowed_modes.contains(&mode))
        && provider_valid
        && endpoint.is_some()
        && grant.pointer("/grant/purpose").and_then(Value::as_str) == Some(purpose)
        && grant
            .pointer(&format!("/grant/{flag}"))
            .and_then(Value::as_bool)
            == Some(true)
        && revision.is_some_and(|value| !value.is_empty());
    GrantPolicy {
        mode: if approved {
            mode.unwrap().to_owned()
        } else {
            "off".to_owned()
        },
        approved,
        provider: approved.then(|| provider.unwrap().to_owned()),
        endpoint: approved.then(|| endpoint.unwrap()),
        revision: approved.then(|| revision.unwrap().to_owned()),
        room: approved.then(|| room.to_owned()),
    }
}

fn is_loopback_endpoint(value: &str) -> bool {
    let Some(endpoint) = value.strip_prefix("http://") else {
        return false;
    };
    let Some(slash) = endpoint.find('/') else {
        return false;
    };
    let authority = &endpoint[..slash];
    let path = &endpoint[slash..];
    if path.chars().any(char::is_whitespace) {
        return false;
    }
    ["127.0.0.1", "localhost", "[::1]"].iter().any(|host| {
        if authority == *host {
            return true;
        }
        let prefix = format!("{host}:");
        authority.strip_prefix(prefix.as_str()).is_some_and(|port| {
            (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit())
        })
    })
}

fn recall_policy(marker: &Value, room: &str, grant: RecallGrantKind) -> GrantPolicy {
    let (key, purpose, flag) = match grant {
        RecallGrantKind::Recall => ("jevRecall", "recall-rerank", "allowPrivateRecallPackets"),
        RecallGrantKind::Lessons => ("jevLessons", "lesson-sieve", "allowPrivateLessonPackets"),
    };
    grant_policy(
        marker,
        room,
        key,
        purpose,
        flag,
        false,
        &["active", "shadow"],
    )
}

fn mode_policy(marker: &Value, room: &str) -> GrantPolicy {
    grant_policy(
        marker,
        room,
        "jevMode",
        "recall-mode",
        "allowPrivateConversationPackets",
        true,
        &["active"],
    )
}

fn verdict_policy(marker: &Value, room: &str) -> GrantPolicy {
    grant_policy(
        marker,
        room,
        "jevVerdict",
        "turn-verdict",
        "allowPrivateConversationPackets",
        true,
        &["active"],
    )
}

fn recall_policy_value(policy: &GrantPolicy) -> Value {
    if !policy.approved {
        return json!({ "mode": "off", "approved": false });
    }
    json!({
        "mode": policy.mode,
        "approved": true,
        "revision": policy.revision,
        "room": policy.room,
    })
}

fn mode_policy_value(policy: &GrantPolicy) -> Value {
    if !policy.approved {
        return json!({ "mode": "off", "approved": false });
    }
    json!({
        "mode": "active",
        "approved": true,
        "provider": policy.provider,
        "endpoint": policy.endpoint,
        "revision": policy.revision,
        "room": policy.room,
    })
}

fn verdict_policy_value(policy: &GrantPolicy) -> Value {
    mode_policy_value(policy)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn clipped(value: &str, count: usize) -> String {
    value.chars().take(count).collect()
}

fn js_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_whitespace() || character == '\u{feff}')
}

fn js_length(value: &str) -> usize {
    value.encode_utf16().count()
}

fn clipped_tail(value: &str, count: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    let start = chars.len().saturating_sub(count);
    chars.into_iter().skip(start).collect()
}

fn is_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.contains("-----begin") {
        return true;
    }
    let bytes = value.as_bytes();
    for (index, _) in lower.match_indices("sk-") {
        if bytes[index + 3..]
            .iter()
            .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_' || **byte == b'-')
            .count()
            >= 16
        {
            return true;
        }
    }
    for token in [
        "api_key",
        "api-key",
        "apikey",
        "password",
        "secret",
        "access_token",
        "access-token",
        "authorization",
        "cookie",
    ] {
        let mut from = 0;
        while let Some(offset) = lower[from..].find(token) {
            let mut index = from + offset + token.len();
            while index < bytes.len()
                && (bytes[index].is_ascii_whitespace() || matches!(bytes[index], b'\'' | b'"'))
            {
                index += 1;
            }
            if index < bytes.len() && matches!(bytes[index], b':' | b'=') {
                return true;
            }
            from = index.max(from + offset + token.len());
            if from >= lower.len() {
                break;
            }
        }
    }
    for (index, _) in lower.match_indices("bearer") {
        let mut start = index + "bearer".len();
        if start == bytes.len() || !bytes[start].is_ascii_whitespace() {
            continue;
        }
        while start < bytes.len() && bytes[start].is_ascii_whitespace() {
            start += 1;
        }
        let token = &bytes[start..];
        if token
            .iter()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(**byte, b'.' | b'_' | b'-'))
            .count()
            >= 16
        {
            return true;
        }
    }
    false
}

fn get_excerpt(candidate: &Value) -> &str {
    for key in ["excerpt", "text", "body", "content"] {
        let Some(value) = candidate.get(key) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        return value.as_str().unwrap_or("");
    }
    ""
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}
fn deadline_at(deadline: Option<u64>) -> u64 {
    deadline.unwrap_or_else(|| now_ms().saturating_add(DEFAULT_DEADLINE_MS))
}

fn recall_receipt(status: &str, reason: Option<&str>, revision: &str, extra: Value) -> Value {
    let mut receipt = json!({
        "schemaVersion": "jev-recall-receipt.v1",
        "status": status,
        "provider": "typesafe",
        "model": TYPESAFE_MODEL,
        "policyRevision": revision,
        "fallbackUsed": status != "active",
    });
    if let Some(reason) = reason {
        receipt["reason"] = json!(reason);
    }
    if let (Some(target), Some(source)) = (receipt.as_object_mut(), extra.as_object()) {
        target.extend(source.clone());
    }
    receipt
}

fn recall_failed(
    baseline: &[Value],
    status: &str,
    reason: &str,
    revision: &str,
    started: Instant,
) -> Value {
    let receipt = recall_receipt(
        status,
        Some(reason),
        revision,
        json!({ "latencyMs": elapsed_ms(started) }),
    );
    json!({ "retrievalCandidates": baseline, "receipt": receipt })
}
fn exact_baseline_indexes(baseline: &[Value]) -> Vec<usize> {
    baseline
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            candidate.get("source").and_then(Value::as_str) == Some("exact_id")
        })
        .map(|(index, _)| index)
        .collect()
}

fn select_recall_cards<'a>(
    cards: &'a [Card],
    answers: &std::collections::HashMap<String, f64>,
) -> Vec<&'a Card> {
    let mut selected: Vec<&Card> = cards
        .iter()
        .filter(|card| answers.get(&card.token).is_some_and(|score| *score >= 0.5))
        .collect();
    selected.sort_by(|left, right| {
        answers[&right.token]
            .total_cmp(&answers[&left.token])
            .then(left.index.cmp(&right.index))
    });
    selected.truncate(8);
    selected
}

fn selected_pool_indexes(
    selected: &[&Card],
    exact_candidate_indexes: &[usize],
    pool_len: usize,
) -> Vec<usize> {
    let exact: std::collections::HashSet<usize> = exact_candidate_indexes
        .iter()
        .copied()
        .filter(|index| *index < pool_len)
        .collect();
    selected
        .iter()
        .filter(|card| !exact.contains(&card.index))
        .map(|card| card.index)
        .collect()
}

async fn rerank(
    room: &str,
    marker: &Value,
    grant: RecallGrantKind,
    query: String,
    baseline: Vec<Value>,
    pool: Option<Vec<Value>>,
    exact_candidate_indexes: Vec<usize>,
    credential: Option<String>,
    deadline: Option<u64>,
) -> Value {
    let started = Instant::now();
    let policy = recall_policy(marker, room, grant);
    let revision = policy.revision.as_deref().unwrap_or("");
    if !policy.approved {
        return recall_failed(&baseline, "disabled", "not-approved", revision, started);
    }
    let end = deadline_at(deadline);
    if end <= now_ms() {
        return recall_failed(&baseline, "refused", "deadline", revision, started);
    }
    let query = query;
    let pool = pool.as_deref().unwrap_or(&baseline);
    if query.is_empty() || is_secret(&query) {
        return recall_failed(&baseline, "refused", "privacy-refused", revision, started);
    }
    if pool.is_empty() {
        return recall_failed(&baseline, "refused", "empty-pool", revision, started);
    }
    let source: Vec<(usize, &str)> = pool
        .iter()
        .enumerate()
        .map(|(index, candidate)| (index, get_excerpt(candidate)))
        .collect();
    if source
        .iter()
        .any(|(_, excerpt)| excerpt.is_empty() || is_secret(excerpt))
    {
        return recall_failed(&baseline, "refused", "privacy-refused", revision, started);
    }
    let cards: Vec<Card> = source
        .into_iter()
        .take(MAX_CARDS)
        .map(|(index, full)| {
            let token = format!("c{}_{}", base36(index), Uuid::new_v4());
            Card {
                index,
                text: clipped(&full, MAX_CARD_CODEPOINTS),
                token,
            }
        })
        .collect();
    let packet = recall_packet(&query, &cards);
    let body = packet.to_string();
    if body.len() > MAX_PACKET_BYTES {
        return recall_failed(&baseline, "refused", "oversize", revision, started);
    }
    if credential.as_deref().unwrap_or("").is_empty() {
        return recall_failed(
            &baseline,
            "unavailable",
            "credential-unavailable",
            revision,
            started,
        );
    }
    let remaining = end.saturating_sub(now_ms());
    if remaining == 0 {
        return recall_failed(&baseline, "refused", "deadline", revision, started);
    }
    match request_provider(TYPESAFE_ENDPOINT, credential.as_deref(), body, end).await {
        Ok((status, raw)) if status.is_success() => {
            let Some(answers) = parse_recall_response(&raw, &cards) else {
                return recall_failed(
                    &baseline,
                    "failed",
                    recall_response_reason(&raw, &cards),
                    revision,
                    started,
                );
            };
            let selected = select_recall_cards(&cards, &answers);
            let status_name = policy.mode.as_str();
            let receipt = recall_receipt(
                status_name,
                None,
                revision,
                json!({
                    "latencyMs": elapsed_ms(started), "scored": cards.len(), "selected": selected.len(),
                    "model": parse_json(&raw).and_then(|value| value.get("model").and_then(Value::as_str).map(str::to_owned)).unwrap_or_default(),
                }),
            );
            let candidates = if status_name == "shadow" {
                baseline
            } else {
                let exact_indexes = exact_baseline_indexes(&baseline);
                let selected_indexes =
                    selected_pool_indexes(&selected, &exact_candidate_indexes, pool.len());
                let mut candidates =
                    Vec::with_capacity(exact_indexes.len() + selected_indexes.len());
                candidates.extend(
                    exact_indexes
                        .iter()
                        .filter_map(|index| baseline.get(*index).cloned()),
                );
                candidates.extend(
                    selected_indexes
                        .iter()
                        .filter_map(|index| pool.get(*index).cloned()),
                );
                candidates
            };
            json!({
                "retrievalCandidates": candidates,
                "receipt": receipt,
            })
        }
        Ok(_) => recall_failed(&baseline, "failed", "non-ok", revision, started),
        Err("deadline") => recall_failed(&baseline, "refused", "deadline", revision, started),
        Err("oversize") => recall_failed(&baseline, "failed", "oversize", revision, started),
        Err(_) => recall_failed(&baseline, "failed", "backend-failed", revision, started),
    }
}
fn base36(mut value: usize) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_owned();
    }
    let mut result = Vec::new();
    while value > 0 {
        result.push(DIGITS[value % 36] as char);
        value /= 36;
    }
    result.iter().rev().copied().collect()
}

fn recall_packet(query: &str, cards: &[Card]) -> Value {
    let mut questions = Map::new();
    for card in cards {
        questions.insert(card.token.clone(), json!({ "type": "noul", "instructions": "Estimate the probability that this card is relevant to the query." }));
    }
    json!({ "state": { "schemaVersion": "jev-recall.v1", "query": clipped(query, 256), "cards": cards.iter().map(|card| json!({ "token": card.token, "text": card.text })).collect::<Vec<_>>() }, "model": TYPESAFE_MODEL, "questions": questions })
}

fn parse_json(raw: &str) -> Option<Value> {
    serde_json::from_str(raw).ok()
}

fn parse_recall_response(
    raw: &str,
    cards: &[Card],
) -> Option<std::collections::HashMap<String, f64>> {
    let parsed = parse_json(raw)?;
    let model = parsed.get("model")?.as_str()?;
    if model.is_empty()
        || model.len() > 64
        || !model.to_ascii_lowercase().contains("jev")
        || !model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
    {
        return None;
    }
    let answers = parsed.get("answers")?.as_object()?;
    if answers.len() != cards.len() {
        return None;
    }
    let mut scores = std::collections::HashMap::new();
    for card in cards {
        let answer = answers.get(&card.token)?.as_object()?;
        if answer.len() != 2 || answer.get("type")?.as_str()? != "noul" {
            return None;
        }
        let score = answer.get("noul")?.as_f64()?;
        if !score.is_finite() || !(0.0..=1.0).contains(&score) {
            return None;
        }
        scores.insert(card.token.clone(), score);
    }
    Some(scores)
}
fn recall_response_reason(raw: &str, cards: &[Card]) -> &'static str {
    let Some(parsed) = parse_json(raw) else {
        return "malformed";
    };
    let Some(model) = parsed.get("model").and_then(Value::as_str) else {
        return "model-mismatch";
    };
    if model.is_empty()
        || model.len() > 64
        || !model.to_ascii_lowercase().contains("jev")
        || !model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
    {
        return "model-mismatch";
    }
    let Some(answers) = parsed.get("answers").and_then(Value::as_object) else {
        return "invalid-response";
    };
    if answers.len() != cards.len() {
        return "invalid-response";
    }
    if cards.iter().any(|card| {
        answers
            .get(&card.token)
            .and_then(Value::as_object)
            .is_none_or(|answer| {
                answer.len() != 2
                    || answer.get("type").and_then(Value::as_str) != Some("noul")
                    || answer
                        .get("noul")
                        .and_then(Value::as_f64)
                        .is_none_or(|score| !score.is_finite() || !(0.0..=1.0).contains(&score))
            })
    }) {
        return "invalid-response";
    }
    "invalid-response"
}

async fn request_provider(
    endpoint: &str,
    credential: Option<&str>,
    body: String,
    deadline: u64,
) -> Result<(StatusCode, String), &'static str> {
    let client = PROVIDER_CLIENT.as_ref().map_err(|_| "transport")?;
    let remaining = deadline.saturating_sub(now_ms());
    if remaining == 0 {
        return Err("deadline");
    }
    let mut request = client
        .post(endpoint)
        .timeout(Duration::from_millis(remaining))
        .header("accept", "application/json")
        .header("content-type", "application/json");
    if let Some(credential) = credential {
        request = request.bearer_auth(credential);
    }
    let operation = async {
        let response = request.body(body).send().await.map_err(|error| {
            if error.is_connect() || error.is_builder() {
                "network"
            } else {
                "transport"
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Ok((status, String::new()));
        }
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "transport")? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES + 3 {
                return Err("oversize");
            }
            bytes.extend_from_slice(&chunk);
        }
        // Fetch text decoding removes a leading UTF-8 BOM before the decoded-byte limit.
        if bytes.starts_with(b"\xef\xbb\xbf") {
            bytes.drain(..3);
        }
        let raw = String::from_utf8(bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned());
        if raw.len() > MAX_RESPONSE_BYTES {
            return Err("oversize");
        }
        Ok((status, raw))
    };
    match timeout(Duration::from_millis(remaining), operation).await {
        Ok(result) => result,
        Err(_) => Err("deadline"),
    }
}

fn mode_packet(
    operator_message: &str,
    assistant_turn: &str,
    operator_reply: &str,
    provider: &str,
) -> Value {
    let operator_message = clipped(js_trim(operator_message), MESSAGE_CHARS);
    let operator_reply = clipped(js_trim(operator_reply), REPLY_CHARS);
    let mut state = Map::new();
    state.insert("schemaVersion".into(), json!("jev-mode.v1"));
    state.insert(
        "assistantTurn".into(),
        json!(clipped_tail(js_trim(assistant_turn), ASSISTANT_TAIL_CHARS)),
    );
    if !operator_message.is_empty() {
        state.insert("operatorMessage".into(), json!(operator_message));
    }
    if !operator_reply.is_empty() {
        state.insert("operatorReply".into(), json!(operator_reply));
    }
    let criteria = json!({
        "conversation": "The exchange is about the operator, their people, their history, feelings, or the room's shared life",
        "work": "The exchange is about a project, code, files, a build, a deploy, or a technical decision",
        "mixed": "Both at once, or neither clearly",
    });
    let mut packet = json!({ "state": Value::Object(state), "questions": { "mode": { "type": "choice", "instructions": "A memory system chooses which retrieval mode to use for the operator's newest message. Judge the exchange below, weighing that newest message most, and pick the mode that would surface the right memories.", "criteria": criteria } } });
    if provider == "typesafe" {
        packet["model"] = json!(TYPESAFE_MODEL);
    }
    packet
}

fn verdict_packet(
    operator_message: &str,
    assistant_turn: &str,
    operator_reply: &str,
    titles: &[String],
    provider: &str,
) -> Value {
    let recall_list = clipped(
        &titles
            .iter()
            .map(|title| js_trim(title))
            .filter(|title| !title.is_empty())
            .map(|title| format!("- {title}"))
            .collect::<Vec<_>>()
            .join("\n"),
        RECALL_TITLES_CHARS,
    );
    let mut state = Map::new();
    state.insert("schemaVersion".into(), json!("jev-verdict.v2"));
    state.insert(
        "assistantTurn".into(),
        json!(clipped_tail(js_trim(assistant_turn), ASSISTANT_TAIL_CHARS)),
    );
    state.insert(
        "operatorReply".into(),
        json!(clipped(js_trim(operator_reply), REPLY_CHARS)),
    );
    let operator_message = clipped(js_trim(operator_message), MESSAGE_CHARS);
    if !operator_message.is_empty() {
        state.insert("operatorMessage".into(), json!(operator_message));
    }
    if !recall_list.is_empty() {
        state.insert("recalledMemories".into(), json!(recall_list));
    }
    let turn_criteria = json!({
        "corrected": "The operator corrects the assistant's answer, approach, tone, or assumptions",
        "continued": "The operator continues the same line of thought without correcting it",
        "gold": "The operator signals that the answer fully solved the need or was especially valuable",
    });
    let fit_criteria = json!({
        "relevant": "The retrieved memories were useful for answering this message",
        "unneeded": "The message needed no retrieved memories",
        "wrong": "The retrieved memories were irrelevant or misleading",
    });
    let use_criteria = json!({
        "used": "The assistant used the retrieved memories appropriately",
        "ignored": "The assistant did not use useful retrieved memories",
        "misled": "The retrieved memories led the assistant away from a good answer",
    });
    let mut questions = json!({
        "turn": {
            "type": "choice",
            "instructions": "An AI assistant answered the operator's message, then the operator replied. Judge the assistant's turn by how the operator reacts to it.",
            "criteria": turn_criteria
        }
    });
    if !recall_list.is_empty() {
        questions["recallFit"] = json!({
            "type": "choice",
            "instructions": "A memory system retrieved the listed memories for the operator's message, before the assistant answered. Judge the retrieval, not the assistant: did it surface what that message needed?",
            "criteria": fit_criteria
        });
        questions["recallUse"] = json!({
            "type": "choice",
            "instructions": "The assistant answered with the listed memories in its context. Judge the assistant, not the retrieval: did its turn draw on them, and to what effect?",
            "criteria": use_criteria
        });
    }
    let mut packet = json!({ "state": Value::Object(state), "questions": questions });
    if provider == "typesafe" {
        packet["model"] = json!(TYPESAFE_MODEL);
    }
    packet
}

fn mode_result(status: &str, provider: Option<&str>, reason: &str, started: Instant) -> Value {
    json!({ "status": status, "provider": provider, "reason": reason, "latencyMs": elapsed_ms(started) })
}

async fn score_mode(
    room: &str,
    marker: &Value,
    operator_message: String,
    assistant_turn: String,
    operator_reply: String,
    credential: Option<String>,
    deadline: Option<u64>,
) -> Value {
    let started = Instant::now();
    let policy = mode_policy(marker, room);
    if !policy.approved {
        return json!({ "status": "disabled", "provider": null, "reason": "not_approved", "latencyMs": 0 });
    }
    let provider = policy.provider.as_deref().unwrap_or("typesafe");
    if js_trim(&assistant_turn).is_empty() {
        return mode_result("refused", Some(provider), "empty_turn", started);
    }
    let packet = mode_packet(
        &operator_message,
        &assistant_turn,
        &operator_reply,
        provider,
    );
    let body = packet.to_string();
    if is_secret(&body) {
        return mode_result("refused", Some(provider), "secret_like", started);
    }
    let packet_bytes = body.len();
    if packet_bytes > MAX_PACKET_BYTES {
        return mode_result("refused", Some(provider), "oversize", started);
    }
    let end = deadline_at(deadline);
    if end <= now_ms() {
        return mode_result("failed", Some(provider), "deadline", started);
    }
    if provider == "typesafe" && credential.as_deref().unwrap_or("").is_empty() {
        return mode_result(
            "unavailable",
            Some(provider),
            "credential_unavailable",
            started,
        );
    }
    let credential = (provider == "typesafe")
        .then_some(credential.as_deref())
        .flatten();
    match request_provider(
        policy.endpoint.as_deref().unwrap_or(TYPESAFE_ENDPOINT),
        credential,
        body,
        end,
    )
    .await
    {
        Ok((status, raw)) if status.is_success() => {
            let (model, mode) = match parse_mode_response(&raw) {
                Ok(value) => value,
                Err(reason) => return mode_result("failed", Some(provider), reason, started),
            };
            json!({ "status": "scored", "provider": provider, "model": model, "mode": mode, "latencyMs": elapsed_ms(started), "packetBytes": packet_bytes })
        }
        Ok((_, _)) => mode_result("failed", Some(provider), "non_ok", started),
        Err("deadline") => mode_result("failed", Some(provider), "deadline", started),
        Err("network") => mode_result("failed", Some(provider), "network", started),
        Err("oversize") => mode_result("failed", Some(provider), "oversize", started),
        Err(_) => mode_result("failed", Some(provider), "transport", started),
    }
}

fn parse_mode_response(raw: &str) -> Result<(String, &'static str), &'static str> {
    let parsed: Value = serde_json::from_str(raw).map_err(|_| "malformed")?;
    let Some(model) = parsed.get("model").and_then(Value::as_str) else {
        return Err("model_mismatch");
    };
    if model.is_empty() || js_length(model) > 64 {
        return Err("model_mismatch");
    }
    let Some(answer) = parsed.pointer("/answers/mode") else {
        return Err("invalid_response");
    };
    if answer.get("type").and_then(Value::as_str) != Some("choice") {
        return Err("invalid_response");
    }
    let Some(choice) = answer.get("choice").and_then(Value::as_str) else {
        return Err("invalid_response");
    };
    let mode = match choice {
        "conversation" => "conversation",
        "work" => "work",
        "mixed" => "mixed",
        _ => return Err("invalid_response"),
    };
    Ok((model.to_owned(), mode))
}

async fn score_verdict(
    room: &str,
    marker: &Value,
    operator_message: String,
    assistant_turn: String,
    operator_reply: String,
    recall_titles: Vec<String>,
    credential: Option<String>,
    deadline: Option<u64>,
) -> Value {
    let started = Instant::now();
    let policy = verdict_policy(marker, room);
    if !policy.approved {
        return json!({ "status": "disabled", "provider": null, "reason": "not_approved", "latencyMs": 0 });
    }
    let provider = policy.provider.as_deref().unwrap_or("typesafe");
    if js_trim(&operator_reply).is_empty() || js_trim(&assistant_turn).is_empty() {
        return mode_result("refused", Some(provider), "empty_turn", started);
    }
    let packet = verdict_packet(
        &operator_message,
        &assistant_turn,
        &operator_reply,
        &recall_titles,
        provider,
    );
    let body = packet.to_string();
    if is_secret(&body) {
        return mode_result("refused", Some(provider), "secret_like", started);
    }
    let packet_bytes = body.len();
    if packet_bytes > MAX_PACKET_BYTES {
        return mode_result("refused", Some(provider), "oversize", started);
    }
    let end = deadline_at(deadline);
    if end <= now_ms() {
        return mode_result("failed", Some(provider), "deadline", started);
    }
    if provider == "typesafe" && credential.as_deref().unwrap_or("").is_empty() {
        return mode_result(
            "unavailable",
            Some(provider),
            "credential_unavailable",
            started,
        );
    }
    let asked_recall = packet.pointer("/questions/recallFit").is_some();
    let credential = (provider == "typesafe")
        .then_some(credential.as_deref())
        .flatten();
    match request_provider(
        policy.endpoint.as_deref().unwrap_or(TYPESAFE_ENDPOINT),
        credential,
        body,
        end,
    )
    .await
    {
        Ok((status, raw)) if status.is_success() => {
            let (model, turn, recall) = match parse_verdict_response(&raw, asked_recall) {
                Ok(value) => value,
                Err(reason) => return mode_result("failed", Some(provider), reason, started),
            };
            json!({ "status": "scored", "provider": provider, "model": model, "turn": turn, "recall": recall, "latencyMs": elapsed_ms(started), "packetBytes": packet_bytes })
        }
        Ok((_, _)) => mode_result("failed", Some(provider), "non_ok", started),
        Err("deadline") => mode_result("failed", Some(provider), "deadline", started),
        Err("network") => mode_result("failed", Some(provider), "network", started),
        Err("oversize") => mode_result("failed", Some(provider), "oversize", started),
        Err(_) => mode_result("failed", Some(provider), "transport", started),
    }
}

fn parse_verdict_response(
    raw: &str,
    asked_recall: bool,
) -> Result<(String, &'static str, Value), &'static str> {
    let parsed: Value = serde_json::from_str(raw).map_err(|_| "malformed")?;
    let Some(model) = parsed.get("model").and_then(Value::as_str) else {
        return Err("model_mismatch");
    };
    if model.is_empty() || js_length(model) > 64 {
        return Err("model_mismatch");
    }
    let answers = parsed.get("answers").ok_or("invalid_response")?;
    let choice =
        |key: &str, valid: &'static [&'static str]| -> Result<&'static str, &'static str> {
            let Some(answer) = answers.get(key) else {
                return Err("invalid_response");
            };
            if answer.get("type").and_then(Value::as_str) != Some("choice") {
                return Err("invalid_response");
            }
            let Some(selected) = answer.get("choice").and_then(Value::as_str) else {
                return Err("invalid_response");
            };
            valid
                .iter()
                .copied()
                .find(|value| *value == selected)
                .ok_or("invalid_response")
        };
    let turn = choice("turn", &["corrected", "continued", "gold"])?;
    if !asked_recall {
        if answers.get("recallFit").is_some() || answers.get("recallUse").is_some() {
            return Err("invalid_response");
        }
        return Ok((model.to_owned(), turn, Value::Null));
    }
    let fit = choice("recallFit", &["relevant", "unneeded", "wrong"])?;
    let usage = choice("recallUse", &["used", "ignored", "misled"])?;
    Ok((model.to_owned(), turn, json!({ "fit": fit, "use": usage })))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn recall_marker(room: &str) -> Value {
        json!({
            "room": room,
            "jevRecall": {
                "mode": "active",
                "provider": "typesafe",
                "grant": {
                    "purpose": "recall-rerank",
                    "allowPrivateRecallPackets": true,
                    "policyRevision": "recall-r1"
                }
            },
            "jevLessons": {
                "mode": "shadow",
                "provider": "typesafe",
                "grant": {
                    "purpose": "lesson-sieve",
                    "allowPrivateLessonPackets": true,
                    "policyRevision": "lessons-r1"
                }
            }
        })
    }

    #[tokio::test]
    async fn null_excerpt_uses_body_but_empty_excerpt_still_refuses_scoring() {
        let marker = recall_marker("room-a");
        for (excerpt, expected) in [(Value::Null, "unavailable"), (json!(""), "refused")] {
            let result = rerank(
                "room-a",
                &marker,
                RecallGrantKind::Recall,
                "context".into(),
                vec![json!({ "excerpt": excerpt, "body": "useful context" })],
                None,
                Vec::new(),
                None,
                None,
            )
            .await;
            assert_eq!(result["receipt"]["status"], expected);
        }
    }

    #[test]
    fn recall_and_lesson_grants_remain_separate() {
        let marker = recall_marker("room-a");

        assert_eq!(
            recall_policy_value(&recall_policy(&marker, "room-a", RecallGrantKind::Recall)),
            json!({
                "mode": "active",
                "approved": true,
                "revision": "recall-r1",
                "room": "room-a"
            })
        );
        assert_eq!(
            recall_policy_value(&recall_policy(&marker, "room-a", RecallGrantKind::Lessons)),
            json!({
                "mode": "shadow",
                "approved": true,
                "revision": "lessons-r1",
                "room": "room-a"
            })
        );
        assert_eq!(
            recall_policy_value(&recall_policy(&marker, "room-b", RecallGrantKind::Recall)),
            json!({ "mode": "off", "approved": false })
        );
    }

    #[tokio::test]
    async fn privacy_refusal_precedes_credential_unavailable() {
        let marker = recall_marker("room-a");
        let result = rerank(
            "room-a",
            &marker,
            RecallGrantKind::Recall,
            "safe query".to_owned(),
            vec![json!({
                "source": "memory",
                "excerpt": "Authorization: Bearer abcdefghijklmnop"
            })],
            None,
            Vec::new(),
            None,
            None,
        )
        .await;

        assert_eq!(result["receipt"]["status"], "refused");
        assert_eq!(result["receipt"]["reason"], "privacy-refused");
    }

    #[tokio::test]
    async fn expired_mode_deadline_stops_before_provider_access() {
        let marker = json!({
            "room": "room-a",
            "jevMode": {
                "mode": "active",
                "provider": "typesafe",
                "grant": {
                    "purpose": "recall-mode",
                    "allowPrivateConversationPackets": true,
                    "policyRevision": "mode-r1"
                }
            }
        });
        let result = score_mode(
            "room-a",
            &marker,
            "question".to_owned(),
            "answer".to_owned(),
            "reply".to_owned(),
            None,
            Some(0),
        )
        .await;

        assert_eq!(result["status"], "failed");
        assert_eq!(result["reason"], "deadline");
    }

    #[test]
    fn recall_selection_keeps_threshold_order_and_exact_exclusion() {
        let cards = vec![
            Card {
                index: 0,
                token: "c0".to_owned(),
                text: "a".to_owned(),
            },
            Card {
                index: 1,
                token: "c1".to_owned(),
                text: "b".to_owned(),
            },
            Card {
                index: 2,
                token: "c2".to_owned(),
                text: "c".to_owned(),
            },
            Card {
                index: 3,
                token: "c3".to_owned(),
                text: "d".to_owned(),
            },
        ];
        let answers = HashMap::from([
            ("c0".to_owned(), 0.9),
            ("c1".to_owned(), 0.8),
            ("c2".to_owned(), 0.5),
            ("c3".to_owned(), 0.49),
        ]);
        let selected = select_recall_cards(&cards, &answers);

        assert_eq!(
            selected.iter().map(|card| card.index).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(
            selected_pool_indexes(&selected, &[0], cards.len()),
            vec![1, 2]
        );
    }

    #[test]
    fn loopback_grant_refuses_a_foreign_endpoint() {
        let mut marker = json!({
            "room": "room-a",
            "jevMode": {
                "mode": "active",
                "provider": "laya",
                "endpoint": "http://memory.example/v1/systemone",
                "grant": {
                    "purpose": "recall-mode",
                    "allowPrivateConversationPackets": true,
                    "policyRevision": "mode-r1"
                }
            }
        });

        assert_eq!(
            mode_policy_value(&mode_policy(&marker, "room-a")),
            json!({ "mode": "off", "approved": false })
        );
        marker["jevMode"]["endpoint"] = json!("http://127.0.0.1:8790/v1/systemone");
        assert_eq!(
            mode_policy_value(&mode_policy(&marker, "room-a")),
            json!({
                "mode": "active",
                "approved": true,
                "provider": "laya",
                "endpoint": "http://127.0.0.1:8790/v1/systemone",
                "revision": "mode-r1",
                "room": "room-a"
            })
        );
    }
}

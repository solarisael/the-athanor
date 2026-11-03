use reqwest::{Client, StatusCode, redirect::Policy};
use serde_json::Value;
use std::{path::Path, sync::LazyLock, time::Duration};
use tokio::time::timeout;

pub const TYPESAFE_ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
pub const TYPESAFE_MODEL: &str = "jev-latest";
pub const TYPESAFE_MAX_PACKET_BYTES: usize = 32_768;
pub const TYPESAFE_MAX_RESPONSE_BYTES: usize = 65_536;

static PROVIDER_CLIENT: LazyLock<Result<Client, reqwest::Error>> =
    LazyLock::new(|| Client::builder().redirect(Policy::none()).build());

pub async fn request_provider(
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
            if bytes.len().saturating_add(chunk.len()) > TYPESAFE_MAX_RESPONSE_BYTES + 3 {
                return Err("oversize");
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.starts_with(b"\xef\xbb\xbf") {
            bytes.drain(..3);
        }
        let raw = String::from_utf8(bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned());
        if raw.len() > TYPESAFE_MAX_RESPONSE_BYTES {
            return Err("oversize");
        }
        Ok((status, raw))
    };
    match timeout(Duration::from_millis(remaining), operation).await {
        Ok(result) => result,
        Err(_) => Err("deadline"),
    }
}

pub fn is_secret(value: &str) -> bool {
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
        if bytes[start..]
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

pub async fn jev_giga_policy(room_dir: &Path, room: &str) -> Option<String> {
    let marker = crate::room_marker::read_room_marker_async(room_dir).await;
    jev_giga_policy_from_marker(&marker, room)
}

pub fn jev_giga_policy_from_marker(marker: &Value, room: &str) -> Option<String> {
    let grant = marker.get("jevGiga")?;
    if marker.get("room").and_then(Value::as_str) != Some(room)
        || grant.get("provider").and_then(Value::as_str) != Some("typesafe")
        || grant.get("mode").and_then(Value::as_str) != Some("active")
        || grant
            .get("grant")
            .and_then(|value| value.get("purpose"))
            .and_then(Value::as_str)
            != Some("giga-classify")
        || grant
            .pointer("/grant/allowPrivateConversationPackets")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return None;
    }
    grant
        .pointer("/grant/policyRevision")
        .and_then(Value::as_str)
        .filter(|revision| !revision.is_empty())
        .map(str::to_owned)
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}

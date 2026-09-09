//! The desktop door shares the room ring and the panel's admission boundary.

use super::{AppState, now_rfc3339, publish_chat};
use crate::chat::CHAT_MAX_TEXT_CHARS;
use axum::extract::{State, rejection::JsonRejection};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

pub(crate) const CHAT_SNAPSHOT_PATH: &str = "/athanor/v1/chat/snapshot";
pub(crate) const CHAT_SAY_PATH: &str = "/athanor/v1/chat/say";
pub(crate) const ROOM_STATE_PATH: &str = "/athanor/v1/room/state";
const MAX_SAY_ID_CHARS: usize = 256;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyRequest {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SayRequest {
    text: String,
    say_id: String,
}

pub(super) fn router(state: AppState) -> Router {
    state.panel.protect(
        Router::new()
            .route(CHAT_SNAPSHOT_PATH, post(snapshot))
            .route(CHAT_SAY_PATH, post(say))
            .route(ROOM_STATE_PATH, post(room_state))
            .with_state(state.clone()),
    )
}

fn error(status: StatusCode, reason: impl ToString) -> Response {
    (status, Json(json!({ "error": reason.to_string() }))).into_response()
}

fn payload<T>(value: Result<Json<T>, JsonRejection>) -> Result<T, Response> {
    value.map(|Json(value)| value)
        .map_err(|reason| error(StatusCode::BAD_REQUEST, reason.body_text()))
}

async fn snapshot(State(state): State<AppState>, body: Result<Json<EmptyRequest>, JsonRejection>) -> Response {
    if let Err(response) = payload(body) { return response; }
    let runtime = state.runtime.lock().await;
    Json(json!({ "room": state.config.room, "messages": runtime.chat.snapshot() })).into_response()
}

async fn say(State(state): State<AppState>, body: Result<Json<SayRequest>, JsonRejection>) -> Response {
    let request = match payload(body) { Ok(request) => request, Err(response) => return response };
    if request.text.trim().is_empty() || request.text.chars().count() > CHAT_MAX_TEXT_CHARS {
        return error(StatusCode::BAD_REQUEST, "text must contain 1 to 32768 characters");
    }
    if request.say_id.trim().is_empty() || request.say_id.chars().count() > MAX_SAY_ID_CHARS {
        return error(StatusCode::BAD_REQUEST, "sayId must contain 1 to 256 characters");
    }
    let identity = match state.room_store.identity() {
        Ok(identity) => identity,
        Err(reason) => return error(StatusCode::SERVICE_UNAVAILABLE, reason),
    };
    let mut runtime = state.runtime.lock().await;
    let message = runtime.chat.say(&identity.operator, &request.text, &request.say_id, now_rfc3339());
    if let Some(message) = &message {
        publish_chat(&state, None, message.clone(), runtime.cursor.sequence);
    }
    Json(json!({ "room": state.config.room, "accepted": true, "repeated": message.is_none(), "message": message })).into_response()
}

async fn room_state(State(state): State<AppState>, body: Result<Json<EmptyRequest>, JsonRejection>) -> Response {
    if let Err(response) = payload(body) { return response; }
    let root = match state.room_store.read_root() {
        Ok(root) if root.get("room").and_then(Value::as_str) == Some(state.config.room.as_str()) => root,
        Ok(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "room state names a foreign room"),
        Err(reason) => return error(StatusCode::SERVICE_UNAVAILABLE, reason),
    };
    let runtime = state.runtime.lock().await;
    let presences: Vec<_> = runtime.presence.frames()
        .filter(|frame| frame.binding.room == state.config.room)
        .map(|frame| json!({ "session": frame.binding.session, "spirit": frame.binding.spirit, "operator": frame.binding.operator }))
        .collect();
    let (entries, last_at) = runtime.chat.summary();
    let mut result = project_room(&state.config.room, &root);
    result["presences"] = json!(presences);
    result["chat"] = json!({ "entries": entries, "lastAt": last_at });
    Json(result).into_response()
}

fn project_room(room: &str, root: &Value) -> Value {
    let mut result = json!({
        "room": room,
        "operator": root.get("operator").and_then(Value::as_str),
        "spirit": root.get("embodiedSpirit").and_then(Value::as_str).or_else(|| root.get("agentName").and_then(Value::as_str)),
    });
    if let Some(enabled) = root.pointer("/routingMode/enabled").and_then(Value::as_bool) {
        result["routingMode"] = json!(if enabled { "on" } else { "off" });
    }
    if let Some(policy) = root.get("recallPolicy").filter(|value| value.is_object()) {
        result["recallPolicy"] = policy.clone();
    }
    if let Some(model) = root.pointer("/modelDefault/model").filter(|value| value.is_string() || value.is_null()) {
        result["modelDefault"] = model.clone();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HostConfig, KnockAutonomy};
    use super::super::Host;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use std::path::PathBuf;
    use tokio_util::{sync::CancellationToken, task::TaskTracker};
    use tower::ServiceExt;

    struct Fixture {
        host: Host,
        directory: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!("athanor-surface-{}", uuid::Uuid::new_v4()));
            let config = HostConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                bearer_token: "surface-test".into(),
                room_dir: directory.clone(),
                state_dir: directory.join("host"),
                house_id: "solarisael".into(),
                room: "kodo".into(),
                spirit: "Kodo".into(),
                session: "surface-session".into(),
                database_url: None,
                nats_url: None,
                knock_autonomy: KnockAutonomy::Off,
            };
            std::fs::create_dir_all(config.room_state_path().parent().unwrap()).unwrap();
            let root = json!({
                "room": "kodo", "operator": "Sol", "embodiedSpirit": "Kodo",
                "recallPolicy": {
                    "requestedMode": "auto", "resolvedMode": "conversation",
                    "workingSetEntries": 0, "recoveryPending": false, "recoveryTerms": [],
                    "activeProject": null, "resolutionReason": "test",
                    "lastRefreshReason": null, "lastRefreshAt": null,
                    "degraded": null, "updatedAt": null
                }
            });
            std::fs::write(config.room_state_path(), root.to_string()).unwrap();
            let host = Host::new(config, None, CancellationToken::new(), TaskTracker::new()).unwrap();
            Self { host, directory }
        }

        async fn post(&self, path: &str, body: Value, authenticated: bool) -> (StatusCode, Value) {
            let mut request = Request::builder().method("POST").uri(path).header("content-type", "application/json");
            if authenticated { request = request.header("authorization", "Bearer surface-test"); }
            let response = self.host.router().oneshot(request.body(Body::from(body.to_string())).unwrap()).await.unwrap();
            let status = response.status();
            let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            (status, serde_json::from_slice(&bytes).unwrap())
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.directory).unwrap();
        }
    }

    #[tokio::test]
    async fn say_snapshot_and_retry_share_the_ring_and_publish_one_delta() {
        let fixture = Fixture::new();
        let mut deltas = fixture.host.state.chat_deltas.subscribe();
        let body = json!({ "text": "Hello House", "sayId": "say-1" });
        let (status, accepted) = fixture.post(CHAT_SAY_PATH, body.clone(), true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(accepted["accepted"], true);
        assert_eq!(accepted["repeated"], false);
        assert_eq!(accepted["message"]["author"], "operator");
        assert_eq!(accepted["message"]["authorName"], "Sol");
        assert_eq!(accepted["message"]["turnId"], "say-1");
        let delta: Value = serde_json::from_str(&deltas.try_recv().unwrap()).unwrap();
        assert_eq!(delta["messages"][0], accepted["message"]);
        let (status, snapshot) = fixture.post(CHAT_SNAPSHOT_PATH, json!({}), true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(snapshot["messages"], json!([accepted["message"]]));
        let (status, repeated) = fixture.post(CHAT_SAY_PATH, body, true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(repeated, json!({ "room": "kodo", "accepted": true, "repeated": true, "message": null }));
        assert!(matches!(deltas.try_recv(), Err(tokio::sync::broadcast::error::TryRecvError::Empty)));
        assert_eq!(fixture.post(CHAT_SNAPSHOT_PATH, json!({}), true).await.1, snapshot);
        let (status, room) = fixture.post(ROOM_STATE_PATH, json!({}), true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(room["chat"], json!({ "entries": 1, "lastAt": accepted["message"]["at"] }));
        assert!(room.get("routingMode").is_none());
        assert!(room.get("modelDefault").is_none());
        println!("SAY {accepted}\nSNAPSHOT {snapshot}\nROOM {room}");
    }

    #[tokio::test]
    async fn surface_refuses_empty_oversized_and_unauthenticated_requests() {
        let fixture = Fixture::new();
        for body in [
            json!({ "text": "  ", "sayId": "say-1" }),
            json!({ "text": "x".repeat(CHAT_MAX_TEXT_CHARS + 1), "sayId": "say-1" }),
            json!({ "text": "hello", "sayId": "" }),
            json!({ "text": "hello", "sayId": "x".repeat(MAX_SAY_ID_CHARS + 1) }),
        ] {
            let (status, refusal) = fixture.post(CHAT_SAY_PATH, body, true).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert!(refusal["error"].is_string());
        }
        for path in [CHAT_SAY_PATH, CHAT_SNAPSHOT_PATH, ROOM_STATE_PATH] {
            assert_eq!(fixture.post(path, json!({}), false).await.0, StatusCode::UNAUTHORIZED);
        }
        assert_eq!(fixture.post(CHAT_SNAPSHOT_PATH, json!({}), true).await.1["messages"], json!([]));
    }

    #[tokio::test]
    async fn room_state_omits_unreported_options_and_reads_current_values() {
        let fixture = Fixture::new();
        let path = fixture.host.state.config.room_state_path();
        std::fs::write(&path, json!({ "room": "kodo" }).to_string()).unwrap();
        let (status, absent) = fixture.post(ROOM_STATE_PATH, json!({}), true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(absent, json!({
            "room": "kodo", "operator": null, "spirit": null,
            "presences": [], "chat": { "entries": 0, "lastAt": null }
        }));
        let root = json!({
            "room": "kodo", "operator": "Sol", "embodiedSpirit": "Kodo",
            "routingMode": { "enabled": false }, "recallPolicy": { "requestedMode": "quiet" },
            "modelDefault": { "model": null }
        });
        std::fs::write(&path, root.to_string()).unwrap();
        let (status, reported) = fixture.post(ROOM_STATE_PATH, json!({}), true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(reported, json!({
            "room": "kodo", "operator": "Sol", "spirit": "Kodo", "routingMode": "off",
            "recallPolicy": { "requestedMode": "quiet" }, "modelDefault": null,
            "presences": [], "chat": { "entries": 0, "lastAt": null }
        }));
    }
}

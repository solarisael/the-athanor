//! A refused command wears the projection that owns its type, so a reader that
//! matches the refusal kind of its own command sees the refusal (#67). A frame
//! whose type no projection owns gets a refusal kind that names no projection.

use super::{HOST_RECIPIENT, Host, process_text};
use protocol::{
    CHAT_COMMAND_REFUSED, CHAT_PROJECTION_ID, CHAT_SAY, COMMAND_REFUSED, CommandOutcomeEvent,
    RECALL_POLICY_COMMAND_REFUSED, RECALL_POLICY_PROJECTION_ID, RECALL_POLICY_SET_REQUESTED_MODE,
};
use serde_json::{Value, json};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

const CALLER_SESSION: &str = "refusal-caller-session";

struct Room {
    host: Host,
    directory: PathBuf,
}

impl Room {
    fn open() -> Self {
        let directory =
            std::env::temp_dir().join(format!("athanor-command-refusal-{}", uuid::Uuid::new_v4()));
        let config = crate::config::HostConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            bearer_token: "refusal-test".into(),
            room_dir: directory.clone(),
            state_dir: directory.join("host"),
            house_dir: None,
            house_id: "solarisael".into(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
            session: "refusal-host-session".into(),
            database_url: None,
            nats_url: None,
            nats_auth: None,
            knock_autonomy: crate::config::KnockAutonomy::Off,
            operator: None,
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

    /// An envelope that `validate_command` admits, with the payload fields added.
    fn command(&self, kind: &str, projection: &str, payload: Value) -> Value {
        let config = &self.host.state.config;
        let message_id = uuid::Uuid::new_v4().to_string();
        let mut command = json!({
            "schema_version": protocol::HOST_SCHEMA_VERSION,
            "message_id": message_id, "correlation_id": message_id,
            "idempotency_key": message_id, "causation_id": "",
            "house_id": config.house_id, "sender_room": config.room,
            "sender_spirit": config.spirit, "sender_session": CALLER_SESSION,
            "recipient": HOST_RECIPIENT, "reply_target": CALLER_SESSION,
            "scope": config.scope(), "visibility": "operator", "authority_class": "room_state",
            "created_at": chrono::Utc::now().to_rfc3339(),
            "expires_at": (chrono::Utc::now() + chrono::Duration::minutes(1)).to_rfc3339(),
            "max_hops": 1, "source_record_refs": [],
            "projection_id": projection,
            "command_or_event_type": kind,
        });
        command
            .as_object_mut()
            .unwrap()
            .extend(payload.as_object().unwrap().clone());

        command
    }

    fn say(&self, text: &str) -> Value {
        self.command(
            CHAT_SAY,
            CHAT_PROJECTION_ID,
            json!({ "chat_say": { "room": "kodo", "text": text, "sayId": "say-1" } }),
        )
    }

    async fn refusal(&self, frame: &str) -> CommandOutcomeEvent {
        let response = process_text(&self.host.state, frame).await;
        assert_eq!(response.direct.len(), 1, "one frame gets one answer");
        assert!(response.delta.is_none(), "a refusal publishes no delta");

        serde_json::from_str(&response.direct[0]).expect("a Host command outcome")
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}

fn failure_code(refusal: &CommandOutcomeEvent) -> &str {
    &refusal
        .failure
        .as_ref()
        .expect("a refusal carries its failure")
        .code
}

#[tokio::test]
async fn a_refused_chat_say_wears_the_chat_projection() {
    let room = Room::open();

    let blank = room.say("   ");
    let refused = room.refusal(&blank.to_string()).await;
    assert_eq!(refused.meta.command_or_event_type, CHAT_COMMAND_REFUSED);
    assert_eq!(refused.meta.projection_id, CHAT_PROJECTION_ID);
    assert_eq!(refused.meta.correlation_id, blank["message_id"]);
    assert_eq!(failure_code(&refused), "invalid_command");
    assert!(
        refused.state.is_none(),
        "a chat refusal carries no recall state"
    );

    // The type names the projection; the projection the caller declares does not.
    let mut declared = room.say("hello");
    declared["projection_id"] = json!(RECALL_POLICY_PROJECTION_ID);
    let refused = room.refusal(&declared.to_string()).await;
    assert_eq!(refused.meta.command_or_event_type, CHAT_COMMAND_REFUSED);
    assert_eq!(refused.meta.projection_id, CHAT_PROJECTION_ID);
    assert_eq!(failure_code(&refused), "invalid_command");

    let mut expired = room.say("hello");
    expired["expires_at"] = json!("2020-01-01T00:00:00Z");
    let refused = room.refusal(&expired.to_string()).await;
    assert_eq!(refused.meta.command_or_event_type, CHAT_COMMAND_REFUSED);
    assert_eq!(refused.meta.projection_id, CHAT_PROJECTION_ID);
    assert_eq!(refused.meta.correlation_id, expired["message_id"]);
    assert_eq!(refused.meta.recipient, CALLER_SESSION);
    assert_eq!(failure_code(&refused), "command_expired");
}

#[tokio::test]
async fn a_frame_without_a_known_command_type_names_no_projection() {
    let room = Room::open();

    let truncated = r#"{"command_or_event_type": "athanor.chat.say""#;
    let refused = room.refusal(truncated).await;
    assert_eq!(refused.meta.command_or_event_type, COMMAND_REFUSED);
    assert_eq!(refused.meta.projection_id, "");
    assert_eq!(failure_code(&refused), "malformed_command");

    let refused = room.refusal("[]").await;
    assert_eq!(refused.meta.command_or_event_type, COMMAND_REFUSED);
    assert_eq!(refused.meta.projection_id, "");
    assert_eq!(failure_code(&refused), "malformed_command");

    let unknown = room.command("athanor.chat.shout", CHAT_PROJECTION_ID, json!({}));
    let refused = room.refusal(&unknown.to_string()).await;
    assert_eq!(refused.meta.command_or_event_type, COMMAND_REFUSED);
    assert_eq!(refused.meta.projection_id, "");
    assert_eq!(refused.meta.correlation_id, unknown["message_id"]);
    assert_eq!(failure_code(&refused), "invalid_command");
    assert!(
        refused.state.is_none(),
        "a refusal that names no projection carries no recall state"
    );
}

#[tokio::test]
async fn a_malformed_recall_policy_command_keeps_the_recall_policy_refusal() {
    let room = Room::open();

    let unversioned = room.command(
        RECALL_POLICY_SET_REQUESTED_MODE,
        RECALL_POLICY_PROJECTION_ID,
        json!({ "mutations": [] }),
    );
    let refused = room.refusal(&unversioned.to_string()).await;
    assert_eq!(
        refused.meta.command_or_event_type,
        RECALL_POLICY_COMMAND_REFUSED
    );
    assert_eq!(refused.meta.projection_id, RECALL_POLICY_PROJECTION_ID);
    assert_eq!(refused.meta.correlation_id, unversioned["message_id"]);
    assert_eq!(failure_code(&refused), "invalid_command");
    assert!(
        refused.state.is_some(),
        "a recall_policy refusal carries the policy state"
    );
}

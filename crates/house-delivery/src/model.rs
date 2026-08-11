use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const EVENT_SCHEMA_VERSION: u8 = 1;
pub const EVENT_KIND: &str = "boat.ready";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoatReadyEvent {
    pub schema_version: u8,
    pub event_id: Uuid,
    pub event_kind: String,
    pub record_id: String,
    pub room: String,
    pub created_at: DateTime<Utc>,
    pub integrity_sha256: String,
}

impl BoatReadyEvent {
    pub fn parse(payload: &[u8]) -> Result<Self> {
        let event: Self =
            serde_json::from_slice(payload).context("malformed boat.ready payload")?;
        event.validate()?;
        Ok(event)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != EVENT_SCHEMA_VERSION {
            bail!("unknown boat.ready schema version {}", self.schema_version);
        }
        if self.event_kind != EVENT_KIND {
            bail!("unknown event kind {}", self.event_kind);
        }
        let record_id = self
            .record_id
            .parse::<i64>()
            .context("record_id must be a positive decimal memory identifier")?;
        if record_id <= 0 {
            bail!("record_id must be a positive decimal memory identifier");
        }
        if self.room.is_empty() || self.room.len() > 128 {
            bail!("room must contain between 1 and 128 bytes");
        }
        if !is_sha256(&self.integrity_sha256) {
            bail!("integrity_sha256 must be 64 lowercase hexadecimal characters");
        }
        Ok(())
    }

    pub fn record_id_i64(&self) -> i64 {
        self.record_id
            .parse()
            .expect("validated boat.ready record_id must remain numeric")
    }
}

pub fn classify_invalid_payload(payload: &[u8]) -> &'static str {
    let Ok(value) = serde_json::from_slice::<Value>(payload) else {
        return "malformed_payload";
    };
    let Some(object) = value.as_object() else {
        return "malformed_payload";
    };
    if contains_private_key(&value) {
        return "private_payload";
    }
    match object.get("event_kind").and_then(Value::as_str) {
        Some(EVENT_KIND) => "malformed_payload",
        _ => "unknown_event",
    }
}

fn contains_private_key(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, nested)| {
            matches!(
                key.as_str(),
                "body" | "title" | "conversation" | "conversation_body" | "message" | "content"
            ) || contains_private_key(nested)
        }),
        Value::Array(items) => items.iter().any(contains_private_key),
        _ => false,
    }
}

pub fn event_id_hint(payload: &[u8]) -> Option<Uuid> {
    serde_json::from_slice::<Value>(payload)
        .ok()?
        .get("event_id")?
        .as_str()?
        .parse()
        .ok()
}

pub fn payload_sha256(payload: &[u8]) -> String {
    hex::encode(Sha256::digest(payload))
}

pub fn body_sha256(body: &str) -> String {
    hex::encode(Sha256::digest(body.as_bytes()))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_value() -> Value {
        json!({
            "schema_version": 1,
            "event_id": "b93446b7-6a6c-4494-ac0c-97390faaca9c",
            "event_kind": "boat.ready",
            "record_id": "42",
            "room": "kintsu",
            "created_at": "2026-08-10T12:00:00Z",
            "integrity_sha256": "a".repeat(64)
        })
    }

    #[test]
    fn exact_pointer_envelope_parses() {
        let event = BoatReadyEvent::parse(&serde_json::to_vec(&valid_value()).unwrap()).unwrap();
        assert_eq!(event.record_id_i64(), 42);
    }

    #[test]
    fn private_and_unknown_fields_are_refused() {
        for field in ["body", "title", "conversation", "message", "content"] {
            let mut value = valid_value();
            value[field] = json!("must never cross the broker");
            let payload = serde_json::to_vec(&value).unwrap();
            assert!(BoatReadyEvent::parse(&payload).is_err());
            assert_eq!(classify_invalid_payload(&payload), "private_payload");
        }
        let mut nested = valid_value();
        nested["metadata"] = json!({ "conversation": "also private" });
        let payload = serde_json::to_vec(&nested).unwrap();
        assert_eq!(classify_invalid_payload(&payload), "private_payload");
        let mut unknown = valid_value();
        unknown["future_field"] = json!(true);
        assert!(BoatReadyEvent::parse(&serde_json::to_vec(&unknown).unwrap()).is_err());
    }

    #[test]
    fn unknown_event_and_bad_integrity_are_refused() {
        let mut unknown = valid_value();
        unknown["event_kind"] = json!("memory.ready");
        let payload = serde_json::to_vec(&unknown).unwrap();
        assert_eq!(classify_invalid_payload(&payload), "unknown_event");
        assert!(BoatReadyEvent::parse(&payload).is_err());

        let mut digest = valid_value();
        digest["integrity_sha256"] = json!("A".repeat(64));
        assert!(BoatReadyEvent::parse(&serde_json::to_vec(&digest).unwrap()).is_err());
    }
}

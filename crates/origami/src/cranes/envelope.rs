use crate::cranes::lanes::{Lane, RecipientKind, is_recipient_key};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const EVENT_SCHEMA_VERSION: u8 = 1;
pub use crate::boats::{
    CREASE_PATTERN as BOAT_READY_CREASE_PATTERN, EVENT_KIND as BOAT_READY_EVENT_KIND,
};
pub const MAX_EVENT_KIND_BYTES: usize = 128;
pub const MAX_CREASE_PATTERN_BYTES: usize = 128;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CraneEvent {
    pub schema_version: u8,
    pub event_id: Uuid,
    pub event_kind: String,
    pub record_id: String,
    pub room: String,
    pub created_at: DateTime<Utc>,
    pub integrity_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crease_pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient_kind: Option<RecipientKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_intent_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<Uuid>,
}

impl CraneEvent {
    pub fn parse(payload: &[u8]) -> Result<Self> {
        let event: Self = serde_json::from_slice(payload).context("malformed crane payload")?;
        event.validate()?;
        Ok(event)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != EVENT_SCHEMA_VERSION {
            bail!("unknown crane schema version {}", self.schema_version);
        }
        if !is_lane_token(&self.event_kind) {
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
        if let Some(crease_pattern) = &self.crease_pattern
            && (crease_pattern.is_empty() || crease_pattern.len() > MAX_CREASE_PATTERN_BYTES)
        {
            bail!("crease_pattern must contain between 1 and {MAX_CREASE_PATTERN_BYTES} bytes");
        }
        if self.recipient_kind.is_some() != self.recipient_key.is_some() {
            bail!("recipient_kind and recipient_key must be declared together");
        }
        if let Some(recipient_key) = &self.recipient_key
            && !is_recipient_key(recipient_key)
        {
            bail!("recipient_key must be a bounded lowercase subject token");
        }
        if self.event_kind == BOAT_READY_EVENT_KIND {
            if self.crease_pattern.is_some()
                || self.recipient_kind.is_some()
                || self.expires_at.is_some()
                || self.parent_intent_id.is_some()
                || self.correlation_id.is_some()
            {
                bail!("the boat.ready lane carries no addressing, expiry or lineage fields");
            }
        } else if self.recipient_kind.is_none() {
            bail!("event kind {} must declare its recipient", self.event_kind);
        }
        Ok(())
    }

    pub fn lane(&self) -> Lane {
        match (self.recipient_kind, &self.recipient_key) {
            (Some(recipient_kind), Some(recipient_key)) => Lane::Addressed {
                recipient_kind,
                recipient_key: recipient_key.clone(),
            },
            _ => Lane::BoatReady,
        }
    }

    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|expires_at| expires_at <= now)
    }

    pub fn record_id_i64(&self) -> i64 {
        self.record_id
            .parse()
            .expect("validated crane record_id must remain numeric")
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
        Some(BOAT_READY_EVENT_KIND) => "malformed_payload",
        Some(kind) if is_lane_token(kind) && object.contains_key("recipient_kind") => {
            "malformed_payload"
        }
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

fn is_lane_token(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_EVENT_KIND_BYTES {
        return false;
    }
    let mut segments = 0;
    for segment in value.split('.') {
        segments += 1;
        let mut bytes = segment.bytes();
        let Some(first) = bytes.next() else {
            return false;
        };
        if !first.is_ascii_lowercase() {
            return false;
        }
        if !bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit()) {
            return false;
        }
    }
    segments >= 2
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
    use crate::cranes::broker::BOAT_READY_SUBJECT;
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

    fn addressed_value() -> Value {
        json!({
            "schema_version": 1,
            "event_id": "0f6f6c1e-3d1a-4a3b-9d51-cf5c9f1c9a10",
            "event_kind": "crane.letter",
            "record_id": "42",
            "room": "kintsu",
            "created_at": "2026-08-10T12:00:00Z",
            "integrity_sha256": "a".repeat(64),
            "crease_pattern": "letter.v1",
            "recipient_kind": "room",
            "recipient_key": "kodo"
        })
    }

    #[test]
    fn exact_pointer_envelope_parses() {
        let event = CraneEvent::parse(&serde_json::to_vec(&valid_value()).unwrap()).unwrap();
        assert_eq!(event.record_id_i64(), 42);
        assert_eq!(event.lane(), Lane::BoatReady);
        assert_eq!(event.lane().subject(), BOAT_READY_SUBJECT);
        assert!(!event.is_expired(Utc::now()));
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            valid_value(),
            "the boat.ready envelope must round-trip with exactly its seven keys"
        );
    }

    #[test]
    fn private_and_unknown_fields_are_refused() {
        for field in ["body", "title", "conversation", "message", "content"] {
            let mut value = valid_value();
            value[field] = json!("must never cross the broker");
            let payload = serde_json::to_vec(&value).unwrap();
            assert!(CraneEvent::parse(&payload).is_err());
            assert_eq!(classify_invalid_payload(&payload), "private_payload");
        }
        let mut nested = valid_value();
        nested["metadata"] = json!({ "conversation": "also private" });
        let payload = serde_json::to_vec(&nested).unwrap();
        assert_eq!(classify_invalid_payload(&payload), "private_payload");
        let mut unknown = valid_value();
        unknown["future_field"] = json!(true);
        assert!(CraneEvent::parse(&serde_json::to_vec(&unknown).unwrap()).is_err());
    }

    #[test]
    fn unknown_event_and_bad_integrity_are_refused() {
        let mut unknown = valid_value();
        unknown["event_kind"] = json!("memory-ready");
        let payload = serde_json::to_vec(&unknown).unwrap();
        assert_eq!(classify_invalid_payload(&payload), "unknown_event");
        assert!(CraneEvent::parse(&payload).is_err());

        let mut digest = valid_value();
        digest["integrity_sha256"] = json!("A".repeat(64));
        assert!(CraneEvent::parse(&serde_json::to_vec(&digest).unwrap()).is_err());
    }

    #[test]
    fn the_boat_lane_refuses_the_widened_fields() {
        for field in [
            "crease_pattern",
            "recipient_kind",
            "expires_at",
            "parent_intent_id",
            "correlation_id",
        ] {
            let mut value = valid_value();
            value[field] = match field {
                "recipient_kind" => json!("room"),
                "expires_at" => json!("2026-08-11T12:00:00Z"),
                "crease_pattern" => json!("boat.ready.v1"),
                _ => json!("2ec1a1a0-9d55-4f5a-9f39-5da9cf1a1a11"),
            };
            let payload = serde_json::to_vec(&value).unwrap();
            assert!(
                CraneEvent::parse(&payload).is_err(),
                "boat.ready must refuse {field}"
            );
            assert_eq!(classify_invalid_payload(&payload), "malformed_payload");
        }
    }

    #[test]
    fn addressed_lanes_route_by_recipient() {
        let event = CraneEvent::parse(&serde_json::to_vec(&addressed_value()).unwrap()).unwrap();
        let lane = event.lane();
        assert_eq!(
            lane,
            Lane::Addressed {
                recipient_kind: RecipientKind::Room,
                recipient_key: "kodo".into()
            }
        );
        assert_eq!(lane.subject(), "athanor.crane.room.kodo");
        assert_eq!(Lane::from_subject(&lane.subject()), Some(lane));
        assert_eq!(
            Lane::from_subject(BOAT_READY_SUBJECT),
            Some(Lane::BoatReady)
        );
        assert_eq!(Lane::from_subject("athanor.crane.ghost.kodo"), None);
        assert_eq!(Lane::from_subject("athanor.crane.room"), None);
        assert_eq!(Lane::from_subject("athanor.crane.room.KODO"), None);
    }

    #[test]
    fn addressing_expiry_and_lineage_are_validated() {
        let mut half_addressed = addressed_value();
        half_addressed
            .as_object_mut()
            .unwrap()
            .remove("recipient_key");
        assert!(CraneEvent::parse(&serde_json::to_vec(&half_addressed).unwrap()).is_err());

        let mut unaddressed = addressed_value();
        for field in ["recipient_kind", "recipient_key"] {
            unaddressed.as_object_mut().unwrap().remove(field);
        }
        let payload = serde_json::to_vec(&unaddressed).unwrap();
        assert!(CraneEvent::parse(&payload).is_err());
        assert_eq!(classify_invalid_payload(&payload), "unknown_event");

        let mut expired = addressed_value();
        expired["expires_at"] = json!("2026-08-10T11:59:59Z");
        let event = CraneEvent::parse(&serde_json::to_vec(&expired).unwrap()).unwrap();
        assert!(event.is_expired("2026-08-10T12:00:00Z".parse().unwrap()));
        assert!(!event.is_expired("2026-08-10T11:59:58Z".parse().unwrap()));

        let mut lineage = addressed_value();
        lineage["parent_intent_id"] = json!("2ec1a1a0-9d55-4f5a-9f39-5da9cf1a1a11");
        lineage["correlation_id"] = json!("2ec1a1a0-9d55-4f5a-9f39-5da9cf1a1a12");
        let event = CraneEvent::parse(&serde_json::to_vec(&lineage).unwrap()).unwrap();
        assert!(event.parent_intent_id.is_some() && event.correlation_id.is_some());
        assert_eq!(serde_json::to_value(&event).unwrap(), lineage);
    }
}

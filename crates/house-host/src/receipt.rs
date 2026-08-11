use house_protocol::{
    BOAT_RECEIPT_SCHEMA_VERSION, BoatReceiptProjection, PaperBoatReceiptState,
    PaperBoatReceiptStatus,
};
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerStatus {
    Disabled,
    Connecting,
    Connected,
    Degraded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReceiptBridgeHealth {
    pub akasha_enabled: bool,
    pub broker_configured: bool,
    pub broker_status: BrokerStatus,
    pub last_error: Option<String>,
    pub latest_event_id: Option<String>,
    pub latest_original_stream_sequence: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReceiptIngest {
    Accepted(BoatReceiptProjection),
    Duplicate,
    Stale,
    ForeignRoom,
}

#[derive(Clone, Debug)]
pub struct ReceiptTracker {
    health: ReceiptBridgeHealth,
    latest: Option<BoatReceiptProjection>,
    refusal: Option<String>,
}

impl ReceiptTracker {
    pub fn new(akasha_enabled: bool, broker_configured: bool) -> Self {
        let (broker_status, last_error) = match (akasha_enabled, broker_configured) {
            (_, true) => (BrokerStatus::Connecting, None),
            (true, false) => (
                BrokerStatus::Degraded,
                Some("AKASHA delivery broker is not configured".into()),
            ),
            (false, false) => (BrokerStatus::Disabled, None),
        };
        Self {
            health: ReceiptBridgeHealth {
                akasha_enabled,
                broker_configured,
                broker_status,
                last_error,
                latest_event_id: None,
                latest_original_stream_sequence: None,
            },
            latest: None,
            refusal: None,
        }
    }

    pub fn connecting(&mut self) {
        if self.health.broker_configured {
            self.health.broker_status = BrokerStatus::Connecting;
            self.health.last_error = None;
        }
    }

    pub fn connected(&mut self) {
        if self.health.broker_configured {
            self.health.broker_status = BrokerStatus::Connected;
            self.health.last_error = None;
        }
    }

    pub fn degraded(&mut self, reason: &str) {
        if self.health.akasha_enabled || self.health.broker_configured {
            self.health.broker_status = BrokerStatus::Degraded;
            self.health.last_error = Some(bounded(reason));
        }
    }

    pub fn refuse_malformed(&mut self) {
        self.refusal = Some("broker receipt failed strict schema validation".into());
    }

    pub fn health(&self) -> ReceiptBridgeHealth {
        self.health.clone()
    }

    pub fn state(&self) -> PaperBoatReceiptState {
        if let Some(receipt) = &self.latest {
            return PaperBoatReceiptState {
                status: PaperBoatReceiptStatus::Delivered,
                receipt: Some(receipt.clone()),
                diagnostic: None,
            };
        }
        if let Some(reason) = &self.refusal {
            return PaperBoatReceiptState {
                status: PaperBoatReceiptStatus::Refused,
                receipt: None,
                diagnostic: Some(reason.clone()),
            };
        }
        match self.health.broker_status {
            BrokerStatus::Degraded => PaperBoatReceiptState {
                status: PaperBoatReceiptStatus::Degraded,
                receipt: None,
                diagnostic: self.health.last_error.clone(),
            },
            BrokerStatus::Disabled => PaperBoatReceiptState {
                status: PaperBoatReceiptStatus::Pending,
                receipt: None,
                diagnostic: Some("Paper Boat delivery receipts require AKASHA".into()),
            },
            BrokerStatus::Connecting | BrokerStatus::Connected => PaperBoatReceiptState {
                status: PaperBoatReceiptStatus::Pending,
                receipt: None,
                diagnostic: Some("waiting for a verified Paper Boat delivery receipt".into()),
            },
        }
    }

    pub fn ingest(&mut self, expected_room: &str, payload: &[u8]) -> Result<ReceiptIngest, String> {
        let projection: BoatReceiptProjection = serde_json::from_slice(payload)
            .map_err(|_| "receipt is not the exact sanitized schema".to_owned())?;
        validate_projection(&projection)?;
        if projection.room != expected_room {
            return Ok(ReceiptIngest::ForeignRoom);
        }
        if let Some(latest) = &self.latest {
            if projection.event_id == latest.event_id {
                return if projection == *latest {
                    Ok(ReceiptIngest::Duplicate)
                } else {
                    Err("receipt event_id conflicts with the accepted projection".into())
                };
            }
            if projection.original_stream_sequence <= latest.original_stream_sequence {
                return Ok(ReceiptIngest::Stale);
            }
        }
        self.health.latest_event_id = Some(projection.event_id.clone());
        self.health.latest_original_stream_sequence = Some(projection.original_stream_sequence);
        self.refusal = None;
        self.latest = Some(projection.clone());
        Ok(ReceiptIngest::Accepted(projection))
    }
}

fn validate_projection(projection: &BoatReceiptProjection) -> Result<(), String> {
    if projection.schema_version != BOAT_RECEIPT_SCHEMA_VERSION {
        return Err("unsupported receipt schema_version".into());
    }
    uuid::Uuid::parse_str(&projection.event_id).map_err(|_| "event_id is not a UUID")?;
    let record_id = projection
        .record_id
        .parse::<u64>()
        .map_err(|_| "record_id is not a positive decimal integer")?;
    if record_id == 0 || record_id.to_string() != projection.record_id {
        return Err("record_id is not a canonical positive decimal integer".into());
    }
    if projection.room.is_empty() || projection.room.len() > 128 {
        return Err("room is outside the bounded receipt contract".into());
    }
    chrono::DateTime::parse_from_rfc3339(&projection.processed_at)
        .map_err(|_| "processed_at is not RFC 3339")?;
    if projection.original_stream_sequence == 0 {
        return Err("original_stream_sequence must be positive".into());
    }
    if projection.integrity_sha256.len() != 64
        || !projection
            .integrity_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("integrity_sha256 is not lowercase SHA-256 hex".into());
    }
    Ok(())
}

fn bounded(reason: &str) -> String {
    reason.chars().take(256).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn receipt(event_id: &str, room: &str, sequence: u64) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "schema_version": 1,
            "event_id": event_id,
            "record_id": "42",
            "room": room,
            "processed_at": "2026-08-10T09:30:00Z",
            "original_stream_sequence": sequence,
            "integrity_sha256": "a".repeat(64)
        }))
        .unwrap()
    }

    #[test]
    fn valid_receipt_is_ordered_and_replay_is_confirmation_not_a_twin() {
        let mut tracker = ReceiptTracker::new(true, true);
        tracker.connected();
        let bytes = receipt("8d2c04ae-ef20-4fbc-8141-d0259cbf495f", "kintsu", 7);
        assert!(matches!(
            tracker.ingest("kintsu", &bytes).unwrap(),
            ReceiptIngest::Accepted(_)
        ));
        assert_eq!(
            tracker.ingest("kintsu", &bytes).unwrap(),
            ReceiptIngest::Duplicate
        );
        let stale = receipt("77aa2086-a56c-427f-a64e-95ed5e79c4fe", "kintsu", 6);
        assert_eq!(
            tracker.ingest("kintsu", &stale).unwrap(),
            ReceiptIngest::Stale
        );
        assert_eq!(tracker.health().latest_original_stream_sequence, Some(7));
    }

    #[test]
    fn wrong_room_and_private_or_malformed_payloads_never_become_state() {
        let mut tracker = ReceiptTracker::new(true, true);
        let foreign = receipt("8d2c04ae-ef20-4fbc-8141-d0259cbf495f", "other", 7);
        assert_eq!(
            tracker.ingest("kintsu", &foreign).unwrap(),
            ReceiptIngest::ForeignRoom
        );
        assert!(tracker.state().receipt.is_none());

        for private_field in ["body", "title"] {
            let mut value: serde_json::Value = serde_json::from_slice(&receipt(
                "8d2c04ae-ef20-4fbc-8141-d0259cbf495f",
                "kintsu",
                7,
            ))
            .unwrap();
            value[private_field] = json!("private prose");
            assert!(
                tracker
                    .ingest("kintsu", &serde_json::to_vec(&value).unwrap())
                    .is_err()
            );
            assert!(tracker.state().receipt.is_none());
        }
        assert!(tracker.ingest("kintsu", b"not-json").is_err());
    }

    #[test]
    fn missing_broker_degrades_akasha_and_reconnect_restores_transport_health() {
        let missing = ReceiptTracker::new(true, false);
        assert_eq!(missing.state().status, PaperBoatReceiptStatus::Degraded);
        assert!(missing.state().receipt.is_none());

        let vault = ReceiptTracker::new(false, false);
        assert_eq!(vault.health().broker_status, BrokerStatus::Disabled);
        assert_eq!(vault.state().status, PaperBoatReceiptStatus::Pending);

        let mut reconnect = ReceiptTracker::new(true, true);
        reconnect.degraded("connection lost");
        assert_eq!(reconnect.health().broker_status, BrokerStatus::Degraded);
        reconnect.connecting();
        reconnect.connected();
        assert_eq!(reconnect.health().broker_status, BrokerStatus::Connected);
        assert!(reconnect.state().receipt.is_none());
    }
}

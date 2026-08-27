//! Latest verified Paper Boat delivery, kept as state and rendered as the exact
//! fields the S01 ReceiptCard accepts.
//!
//! This module owns no node, no transport, no credential, and no body loading.
//! The Athanor Host authenticates and orders the snapshot; here it is only
//! accepted, refused, or degraded, then handed to `S01ChatCenter.set_receipt`.
//! PostgreSQL remains authority and NATS remains delivery only.

use godot::prelude::*;

use crate::disclosure::ABSENT;
use crate::protocol::{PaperBoatReceipt, PaperBoatReceiptSnapshot, ReceiptStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiptFeed {
    status: ReceiptStatus,
    receipt: Option<PaperBoatReceipt>,
    diagnostic: Option<String>,
    link_detail: String,
    last_event_id: Option<String>,
    last_sequence: i64,
}

impl ReceiptFeed {
    pub fn pending() -> Self {
        Self {
            status: ReceiptStatus::Pending,
            receipt: None,
            diagnostic: Some("waiting for an authenticated Athanor Host snapshot".into()),
            link_detail: "Host session not resolved yet".into(),
            last_event_id: None,
            last_sequence: 0,
        }
    }

    /// Accept one Host-ordered snapshot. `Ok(false)` means the snapshot repeated
    /// what is already displayed; an out-of-order snapshot is an error, never a
    /// silent overwrite.
    pub fn apply(&mut self, snapshot: PaperBoatReceiptSnapshot) -> Result<bool, String> {
        if snapshot.sequence < self.last_sequence {
            return Err(format!(
                "regressive Host order: {} after {}",
                snapshot.sequence, self.last_sequence
            ));
        }
        if self.last_event_id.as_deref() == Some(snapshot.event_id.as_str())
            && snapshot.status == self.status
            && snapshot.receipt == self.receipt
            && snapshot.diagnostic == self.diagnostic
        {
            return Ok(false);
        }
        self.status = snapshot.status;
        self.receipt = snapshot.receipt;
        self.diagnostic = snapshot.diagnostic;
        self.last_event_id = Some(snapshot.event_id);
        self.last_sequence = snapshot.sequence;
        Ok(true)
    }

    pub fn refuse(&mut self, reason: impl Into<String>) {
        self.status = ReceiptStatus::Refused;
        self.receipt = None;
        self.diagnostic = Some(reason.into());
    }

    pub fn degrade(&mut self, reason: impl Into<String>) {
        if self.receipt.is_none() {
            self.status = ReceiptStatus::Degraded;
            self.diagnostic = Some(reason.into());
        }
    }

    pub fn set_link_detail(&mut self, detail: impl Into<String>) {
        self.link_detail = detail.into();
    }

    /// Exactly the keys `S01ChatCenter.set_receipt` accepts. Absent evidence is
    /// written as the absent mark, never as a plausible value.
    pub fn card_fields(&self) -> VarDictionary {
        let (timestamp, delivered, record, event, sequence, sha) =
            match (self.status, self.receipt.as_ref()) {
                (ReceiptStatus::Delivered, Some(receipt)) => (
                    format!("{} · ROOM {}", receipt.processed_at, receipt.room),
                    "DELIVERED".to_string(),
                    receipt.record_id.clone(),
                    receipt.event_id.clone(),
                    receipt.original_stream_sequence.to_string(),
                    receipt.integrity_sha256.clone(),
                ),
                (status, _) => {
                    let phrase = match status {
                        ReceiptStatus::Pending => "PENDING · waiting for a verified receipt",
                        ReceiptStatus::Degraded => "DEGRADED · broker/Host unavailable",
                        ReceiptStatus::Refused => "REFUSED · invalid envelope or order",
                        ReceiptStatus::Delivered => "REFUSED · delivered without receipt",
                    };
                    let delivered = match self.diagnostic.as_deref() {
                        Some(diagnostic) => format!("{phrase} · {diagnostic}"),
                        None => phrase.to_string(),
                    };
                    (
                        format!("{ABSENT} · ROOM {ABSENT}"),
                        delivered,
                        ABSENT.to_string(),
                        ABSENT.to_string(),
                        ABSENT.to_string(),
                        ABSENT.to_string(),
                    )
                }
            };

        let mut fields = VarDictionary::new();
        fields.set(
            "title_text",
            format!("Latest Paper Boat · HOST: {}", self.link_detail),
        );
        fields.set("timestamp_text", timestamp);
        fields.set("delivered_text", delivered);
        fields.set("record_text", record);
        fields.set("event_text", event);
        fields.set("sequence_text", sequence);
        fields.set("sha_text", sha);
        fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(status: ReceiptStatus, sequence: i64) -> PaperBoatReceiptSnapshot {
        let delivered = status == ReceiptStatus::Delivered;
        PaperBoatReceiptSnapshot {
            event_id: format!("event-{sequence}"),
            sender_room: "kintsu".into(),
            sequence,
            status,
            receipt: delivered.then(|| PaperBoatReceipt {
                event_id: format!("event-{sequence}"),
                record_id: "42".into(),
                room: "kintsu".into(),
                processed_at: "2026-08-10T09:30:00Z".into(),
                original_stream_sequence: sequence,
                integrity_sha256: "a".repeat(64),
            }),
            diagnostic: (!delivered).then(|| "bounded reason".into()),
        }
    }

    #[test]
    fn gui_state_parses_pending_degraded_refused_delivered_and_replay() {
        let mut state = ReceiptFeed::pending();
        assert!(state.apply(snapshot(ReceiptStatus::Pending, 0)).unwrap());
        assert!(state.apply(snapshot(ReceiptStatus::Degraded, 0)).unwrap());
        assert!(state.apply(snapshot(ReceiptStatus::Refused, 0)).unwrap());
        assert!(state.apply(snapshot(ReceiptStatus::Delivered, 7)).unwrap());
        assert_eq!(state.receipt.as_ref().unwrap().record_id, "42");
        assert!(!state.apply(snapshot(ReceiptStatus::Delivered, 7)).unwrap());
        assert!(state.apply(snapshot(ReceiptStatus::Delivered, 6)).is_err());
    }
}

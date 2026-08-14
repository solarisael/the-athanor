//! Focused Host-backed status for the latest verified Paper Boat delivery.
//! The component consumes the root's authenticated Host session and has no
//! NATS, database, Vault, credential, or body-loading surface.

use godot::classes::{IPanelContainer, Label, Node, PanelContainer};
use godot::obj::Inherits;
use godot::prelude::*;

use crate::host_link::LinkPhase;
use crate::host_session::AthanorHostSession;
use crate::protocol::{
    self, CommandIdentity, PaperBoatReceipt, PaperBoatReceiptSnapshot, ReceiptStatus,
};

const DISCLOSURE: &str = "NO AUTHORITY · THIS PANEL SHOWS ONLY THE SANITIZED ATHANOR HOST RECEIPT · NO BODY OR TITLE IS LOADED · POSTGRESQL REMAINS AUTHORITY AND NATS REMAINS DELIVERY ONLY";
const ABSENT: &str = "—";

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReceiptPanelState {
    status: ReceiptStatus,
    receipt: Option<PaperBoatReceipt>,
    diagnostic: Option<String>,
    last_event_id: Option<String>,
    last_sequence: i64,
}

impl ReceiptPanelState {
    fn pending() -> Self {
        Self {
            status: ReceiptStatus::Pending,
            receipt: None,
            diagnostic: Some("waiting for an authenticated Athanor Host snapshot".into()),
            last_event_id: None,
            last_sequence: 0,
        }
    }

    fn apply(&mut self, snapshot: PaperBoatReceiptSnapshot) -> Result<bool, String> {
        if snapshot.sequence < self.last_sequence {
            return Err(format!(
                "regressive Host order: {} after {}",
                snapshot.sequence, self.last_sequence
            ));
        }
        if self.last_event_id.as_deref() == Some(snapshot.event_id.as_str())
            && snapshot.sequence == self.last_sequence
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

    fn refuse(&mut self, reason: impl Into<String>) {
        self.status = ReceiptStatus::Refused;
        self.receipt = None;
        self.diagnostic = Some(reason.into());
    }

    fn degrade(&mut self, reason: impl Into<String>) {
        if self.receipt.is_none() {
            self.status = ReceiptStatus::Degraded;
            self.diagnostic = Some(reason.into());
        }
    }
}

struct Bound {
    timestamp: Gd<Label>,
    status: Gd<Label>,
    disclosure: Gd<Label>,
}

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub struct AthanorPaperBoatReceipt {
    #[export]
    session_path: NodePath,
    #[export]
    timestamp_label: NodePath,
    #[export]
    status_label: NodePath,
    #[export]
    disclosure_label: NodePath,

    session: Option<Gd<AthanorHostSession>>,
    state: ReceiptPanelState,
    link_detail: String,
    bound: Option<Bound>,
    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for AthanorPaperBoatReceipt {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            session_path: NodePath::default(),
            timestamp_label: NodePath::default(),
            status_label: NodePath::default(),
            disclosure_label: NodePath::default(),
            session: None,
            state: ReceiptPanelState::pending(),
            link_detail: "Host session not resolved yet".into(),
            bound: None,
            base,
        }
    }

    fn ready(&mut self) {
        self.resolve_bindings();
        self.wire_session();
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.bind().phase() == LinkPhase::Open)
        {
            self.request_receipt();
        }
        self.render();
    }
}

impl AthanorPaperBoatReceipt {
    fn resolve<T>(&self, path: &NodePath, name: &str, missing: &mut Vec<String>) -> Option<Gd<T>>
    where
        T: GodotClass + Inherits<Node>,
    {
        if path.is_empty() {
            missing.push(name.into());
            return None;
        }
        match self.base().try_get_node_as::<T>(path) {
            Some(node) => Some(node),
            None => {
                missing.push(name.into());
                None
            }
        }
    }

    fn resolve_bindings(&mut self) {
        let mut missing = Vec::new();
        let timestamp =
            self.resolve::<Label>(&self.timestamp_label, "timestamp_label", &mut missing);
        let status = self.resolve::<Label>(&self.status_label, "status_label", &mut missing);
        let disclosure =
            self.resolve::<Label>(&self.disclosure_label, "disclosure_label", &mut missing);
        if !missing.is_empty() {
            godot_error!(
                "AthanorPaperBoatReceipt: missing scene bindings: {}",
                missing.join(", ")
            );
            return;
        }
        self.bound = Some(Bound {
            timestamp: timestamp.expect("resolved"),
            status: status.expect("resolved"),
            disclosure: disclosure.expect("resolved"),
        });
    }

    fn wire_session(&mut self) {
        let Some(mut session) = self
            .base()
            .try_get_node_as::<AthanorHostSession>(&self.session_path)
        else {
            self.state.degrade("shared Host session not found");
            self.link_detail = "Host link unavailable".into();
            return;
        };
        let this = self.to_gd();
        session.connect(
            "opened",
            &Callable::from_object_method(&this, "on_host_opened"),
        );
        session.connect(
            "closed",
            &Callable::from_object_method(&this, "on_host_closed"),
        );
        session.connect(
            "malformed",
            &Callable::from_object_method(&this, "on_host_malformed"),
        );
        session.connect(
            "unavailable",
            &Callable::from_object_method(&this, "on_host_unavailable"),
        );
        session.connect(
            "message",
            &Callable::from_object_method(&this, "on_host_message"),
        );
        self.session = Some(session);
    }

    fn request_receipt(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let message_id = session.bind_mut().new_identifier();
        let command = protocol::paper_boat_receipt_subscribe_command(&CommandIdentity {
            idempotency_key: message_id.clone(),
            message_id,
            causation_id: String::new(),
        });
        match session.bind_mut().send(&command) {
            Ok(()) => self.link_detail = "authenticated Host · requesting latest receipt".into(),
            Err(reason) => {
                self.state.degrade(reason.clone());
                self.link_detail = reason;
            }
        }
    }

    fn render(&mut self) {
        let Some(bound) = &mut self.bound else {
            return;
        };
        let (timestamp, status) = match (&self.state.status, &self.state.receipt) {
            (ReceiptStatus::Delivered, Some(receipt)) => (
                format!("{} · ROOM {}", receipt.processed_at, receipt.room),
                format!(
                    "◆ DELIVERED · RECORD {} · EVENT {} · SEQ {} · SHA256 {}",
                    receipt.record_id,
                    receipt.event_id,
                    receipt.original_stream_sequence,
                    receipt.integrity_sha256
                ),
            ),
            (ReceiptStatus::Pending, _) => (
                format!("{ABSENT} · ROOM {ABSENT}"),
                "◇ PENDING · waiting for a verified receipt".into(),
            ),
            (ReceiptStatus::Degraded, _) => (
                format!("{ABSENT} · ROOM {ABSENT}"),
                "◇ DEGRADED · broker/Host unavailable".into(),
            ),
            (ReceiptStatus::Refused, _) => (
                format!("{ABSENT} · ROOM {ABSENT}"),
                "✕ REFUSED · invalid envelope or order".into(),
            ),
            (ReceiptStatus::Delivered, None) => (
                format!("{ABSENT} · ROOM {ABSENT}"),
                "✕ REFUSED · delivered without receipt".into(),
            ),
        };
        let diagnostic = self.state.diagnostic.as_deref().unwrap_or(ABSENT);
        bound.timestamp.set_text(&timestamp);
        bound.status.set_text(&status);
        bound.disclosure.set_text(&format!(
            "{DISCLOSURE}\nHOST: {} · DETAIL: {}",
            self.link_detail, diagnostic
        ));
    }
}

#[godot_api]
impl AthanorPaperBoatReceipt {
    #[func]
    fn on_host_opened(&mut self) {
        self.request_receipt();
        self.render();
    }

    #[func]
    fn on_host_closed(&mut self, detail: GString) {
        self.state
            .degrade("Athanor Host disconnected; latest receipt was not changed");
        self.link_detail = detail.to_string();
        self.render();
    }

    #[func]
    fn on_host_malformed(&mut self, _detail: GString) {
        self.state
            .refuse("Host returned malformed JSON; nothing was displayed");
        self.link_detail = "protocol refused".into();
        self.render();
    }

    #[func]
    fn on_host_unavailable(&mut self, detail: GString) {
        self.state.degrade(detail.to_string());
        self.link_detail = "Host link unavailable".into();
        self.render();
    }

    #[func]
    fn on_host_message(&mut self, envelope: VarDictionary) {
        if protocol::event_projection_id(&envelope).ok().as_deref()
            != Some(house_protocol::PAPER_BOAT_RECEIPT_PROJECTION_ID)
        {
            return;
        }
        match protocol::parse_paper_boat_receipt(&envelope) {
            Ok(snapshot) => match self.state.apply(snapshot) {
                Ok(_) => self.link_detail = "authenticated snapshot applied".into(),
                Err(reason) => {
                    self.state.refuse(reason);
                    self.link_detail = "Host order refused".into();
                }
            },
            Err(reason) => {
                self.state.refuse(reason);
                self.link_detail = "receipt protocol refused".into();
            }
        }
        self.render();
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
        let mut state = ReceiptPanelState::pending();
        assert!(state.apply(snapshot(ReceiptStatus::Pending, 0)).unwrap());
        assert!(state.apply(snapshot(ReceiptStatus::Degraded, 0)).unwrap());
        assert!(state.apply(snapshot(ReceiptStatus::Refused, 0)).unwrap());
        assert!(state.apply(snapshot(ReceiptStatus::Delivered, 7)).unwrap());
        assert_eq!(state.receipt.as_ref().unwrap().record_id, "42");
        assert!(!state.apply(snapshot(ReceiptStatus::Delivered, 7)).unwrap());
        assert!(state.apply(snapshot(ReceiptStatus::Delivered, 6)).is_err());
    }
}

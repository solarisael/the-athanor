//! Focused Host-backed status for the latest verified Paper Boat delivery.
//! The component owns one authenticated Host link and has no NATS, database,
//! Vault, or body-loading surface.

use godot::classes::{IPanelContainer, Label, Node, Os, PanelContainer};
use godot::obj::Inherits;
use godot::prelude::*;

use crate::host_link::{HostLink, LinkEvent, LinkPhase};
use crate::protocol::{
    self, CommandIdentity, PaperBoatReceipt, PaperBoatReceiptSnapshot, ReceiptStatus,
};

const DISCLOSURE: &str = "SEM AUTORIDADE · ESTE PAINEL MOSTRA SÓ O RECIBO SANITIZADO DO ATHANOR HOST · NENHUM BODY OU TITLE É CARREGADO · POSTGRESQL CONTINUA AUTORIDADE E NATS CONTINUA SÓ ENTREGA";
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
            diagnostic: Some("aguardando snapshot autenticado do Athanor Host".into()),
            last_event_id: None,
            last_sequence: 0,
        }
    }

    fn apply(&mut self, snapshot: PaperBoatReceiptSnapshot) -> Result<bool, String> {
        if snapshot.sequence < self.last_sequence {
            return Err(format!(
                "ordem regressiva do Host: {} depois de {}",
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
    host_url: GString,
    #[export]
    timestamp_label: NodePath,
    #[export]
    status_label: NodePath,
    #[export]
    disclosure_label: NodePath,

    link: HostLink,
    state: ReceiptPanelState,
    link_detail: String,
    bound: Option<Bound>,
    dirty: bool,
    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for AthanorPaperBoatReceipt {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            host_url: GString::from(protocol::DEFAULT_HOST_URL),
            timestamp_label: NodePath::default(),
            status_label: NodePath::default(),
            disclosure_label: NodePath::default(),
            link: HostLink::new(),
            state: ReceiptPanelState::pending(),
            link_detail: "Host ainda não conectado".into(),
            bound: None,
            dirty: true,
            base,
        }
    }

    fn ready(&mut self) {
        self.resolve_bindings();
        let token = Os::singleton()
            .get_environment("ATHANOR_HOST_TOKEN")
            .to_string();
        if token.trim().is_empty() {
            self.state
                .degrade("ATHANOR_HOST_TOKEN ausente; nenhum recibo foi inventado");
            self.link_detail = "link Host indisponível".into();
            self.base_mut().set_process(false);
            self.render();
            return;
        }
        let url = self.host_url.to_string();
        match self.link.open(&url, &token) {
            Ok(()) => {
                self.link_detail = "conectando ao Athanor Host".into();
                self.base_mut().set_process(true);
            }
            Err(reason) => {
                self.state.degrade(reason.clone());
                self.link_detail = reason;
                self.base_mut().set_process(false);
            }
        }
        self.render();
    }

    fn process(&mut self, _delta: f64) {
        for event in self.link.poll() {
            self.handle_link_event(event);
        }
        if self.dirty {
            self.render();
        }
        if self.link.phase() == LinkPhase::Closed {
            self.base_mut().set_process(false);
        }
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
                "AthanorPaperBoatReceipt: vínculos de cena ausentes: {}",
                missing.join(", ")
            );
            self.bound = None;
            return;
        }
        self.bound = Some(Bound {
            timestamp: timestamp.expect("resolvido"),
            status: status.expect("resolvido"),
            disclosure: disclosure.expect("resolvido"),
        });
    }

    fn handle_link_event(&mut self, event: LinkEvent) {
        match event {
            LinkEvent::Opened => {
                self.link_detail = "Host autenticado · solicitando recibo mais recente".into();
                let message_id = self.link.new_identifier();
                let command = protocol::paper_boat_receipt_subscribe_command(&CommandIdentity {
                    idempotency_key: message_id.clone(),
                    message_id,
                    causation_id: String::new(),
                });
                if let Err(reason) = self.link.send(&command) {
                    self.state.degrade(reason.clone());
                    self.link_detail = reason;
                }
            }
            LinkEvent::Closed { detail } => {
                self.state
                    .degrade("Athanor Host desconectado; último recibo não foi alterado");
                self.link_detail = detail;
            }
            LinkEvent::Malformed { .. } => {
                self.state
                    .refuse("Host respondeu com JSON malformado; nada foi exibido");
                self.link_detail = "protocolo recusado".into();
                self.link.close();
            }
            LinkEvent::Message(envelope) => match protocol::parse_paper_boat_receipt(&envelope) {
                Ok(snapshot) => match self.state.apply(snapshot) {
                    Ok(_) => self.link_detail = "snapshot autenticado aplicado".into(),
                    Err(reason) => {
                        self.state.refuse(reason);
                        self.link_detail = "ordem Host recusada".into();
                        self.link.close();
                    }
                },
                Err(reason) => {
                    self.state.refuse(reason);
                    self.link_detail = "protocolo de recibo recusado".into();
                    self.link.close();
                }
            },
        }
        self.dirty = true;
    }

    fn render(&mut self) {
        let Some(bound) = &mut self.bound else {
            self.dirty = false;
            return;
        };
        let (timestamp, status) = match (&self.state.status, &self.state.receipt) {
            (ReceiptStatus::Delivered, Some(receipt)) => (
                format!("{} · ROOM {}", receipt.processed_at, receipt.room),
                format!(
                    "◆ ENTREGUE · RECORD {} · EVENT {} · SEQ {} · SHA256 {}",
                    receipt.record_id,
                    receipt.event_id,
                    receipt.original_stream_sequence,
                    receipt.integrity_sha256
                ),
            ),
            (ReceiptStatus::Pending, _) => (
                format!("{ABSENT} · ROOM {ABSENT}"),
                "◇ PENDENTE · aguardando recibo verificado".into(),
            ),
            (ReceiptStatus::Degraded, _) => (
                format!("{ABSENT} · ROOM {ABSENT}"),
                "◇ DEGRADADO · broker/Host indisponível".into(),
            ),
            (ReceiptStatus::Refused, _) => (
                format!("{ABSENT} · ROOM {ABSENT}"),
                "✕ RECUSADO · envelope ou ordem inválida".into(),
            ),
            (ReceiptStatus::Delivered, None) => (
                format!("{ABSENT} · ROOM {ABSENT}"),
                "✕ RECUSADO · delivered sem recibo".into(),
            ),
        };
        let diagnostic = self.state.diagnostic.as_deref().unwrap_or(ABSENT);
        bound.timestamp.set_text(&timestamp);
        bound.status.set_text(&status);
        bound.disclosure.set_text(&format!(
            "{DISCLOSURE}\nHOST: {} · DETALHE: {}",
            self.link_detail, diagnostic
        ));
        self.dirty = false;
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

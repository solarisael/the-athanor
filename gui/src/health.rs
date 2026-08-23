//! Client-side health disclosure aggregated only from real shared-session events.

use godot::classes::control::FocusMode;
use godot::classes::{Button, IPanelContainer, Label, Node, PanelContainer};
use godot::obj::Inherits;
use godot::prelude::*;

use crate::host_link::LinkPhase;
use crate::host_session::AthanorHostSession;
use crate::protocol::{
    self, CommandIdentity, HostBinding, Inbound, PaperBoatReceiptSnapshot, ProjectionCursor,
    RecallPolicyProjection, ReceiptStatus,
};

const DISCLOSURE: &str = "OBSERVATION ONLY · EACH CHANNEL REPORTS ITS OWN REAL HOST EVENT · TRANSPORT, BINDING, RECALL HEALTH, PAPER BOAT DELIVERY, AND PROTOCOL REFUSAL ARE NEVER COLLAPSED INTO ONE VERDICT";
const ABSENT: &str = "—";

struct Bound {
    disclosure: Gd<Label>,
    transport: Gd<Label>,
    binding: Gd<Label>,
    recall: Gd<Label>,
    boat: Gd<Label>,
    refusal: Gd<Label>,
    detail: Gd<Label>,
    reason: Gd<Label>,
    refresh: Gd<Button>,
}

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub struct AthanorHealth {
    #[export]
    session_path: NodePath,
    #[export]
    disclosure_label: NodePath,
    #[export]
    transport_label: NodePath,
    #[export]
    binding_label: NodePath,
    #[export]
    recall_label: NodePath,
    #[export]
    boat_label: NodePath,
    #[export]
    refusal_label: NodePath,
    #[export]
    detail_label: NodePath,
    #[export]
    unavailable_label: NodePath,
    #[export]
    refresh_button: NodePath,

    session: Option<Gd<AthanorHostSession>>,
    binding: Option<HostBinding>,
    recall_cursor: Option<ProjectionCursor>,
    recall_projection: Option<RecallPolicyProjection>,
    boat: Option<PaperBoatReceiptSnapshot>,
    last_refusal: Option<String>,
    recall_request: Option<String>,
    boat_request: Option<String>,
    detail: String,
    bound: Option<Bound>,
    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for AthanorHealth {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            session_path: NodePath::default(),
            disclosure_label: NodePath::default(),
            transport_label: NodePath::default(),
            binding_label: NodePath::default(),
            recall_label: NodePath::default(),
            boat_label: NodePath::default(),
            refusal_label: NodePath::default(),
            detail_label: NodePath::default(),
            unavailable_label: NodePath::default(),
            refresh_button: NodePath::default(),
            session: None,
            binding: None,
            recall_cursor: None,
            recall_projection: None,
            boat: None,
            last_refusal: None,
            recall_request: None,
            boat_request: None,
            detail: "waiting for the shared Host session".into(),
            bound: None,
            base,
        }
    }

    fn ready(&mut self) {
        self.resolve_bindings();
        self.wire_session();
        self.wire_controls();
        self.render();
    }
}

impl AthanorHealth {
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
        let disclosure =
            self.resolve::<Label>(&self.disclosure_label, "disclosure_label", &mut missing);
        let transport =
            self.resolve::<Label>(&self.transport_label, "transport_label", &mut missing);
        let binding = self.resolve::<Label>(&self.binding_label, "binding_label", &mut missing);
        let recall = self.resolve::<Label>(&self.recall_label, "recall_label", &mut missing);
        let boat = self.resolve::<Label>(&self.boat_label, "boat_label", &mut missing);
        let refusal = self.resolve::<Label>(&self.refusal_label, "refusal_label", &mut missing);
        let detail = self.resolve::<Label>(&self.detail_label, "detail_label", &mut missing);
        let reason =
            self.resolve::<Label>(&self.unavailable_label, "unavailable_label", &mut missing);
        let refresh = self.resolve::<Button>(&self.refresh_button, "refresh_button", &mut missing);
        if !missing.is_empty() {
            godot_error!(
                "AthanorHealth: missing scene bindings: {}",
                missing.join(", ")
            );
            return;
        }
        self.bound = Some(Bound {
            disclosure: disclosure.expect("resolved"),
            transport: transport.expect("resolved"),
            binding: binding.expect("resolved"),
            recall: recall.expect("resolved"),
            boat: boat.expect("resolved"),
            refusal: refusal.expect("resolved"),
            detail: detail.expect("resolved"),
            reason: reason.expect("resolved"),
            refresh: refresh.expect("resolved"),
        });
    }

    fn wire_session(&mut self) {
        let Some(mut session) = self
            .base()
            .try_get_node_as::<AthanorHostSession>(&self.session_path)
        else {
            self.detail = "shared Host session not found".into();
            return;
        };
        let this = self.to_gd();
        for (signal, method) in [
            ("opened", "on_host_opened"),
            ("closed", "on_host_closed"),
            ("malformed", "on_host_malformed"),
            ("unavailable", "on_host_unavailable"),
            ("message", "on_host_message"),
        ] {
            session.connect(signal, &Callable::from_object_method(&this, method));
        }
        self.session = Some(session);
    }

    fn wire_controls(&mut self) {
        let this = self.to_gd();
        let Some(bound) = &mut self.bound else {
            return;
        };
        bound.refresh.set_focus_mode(FocusMode::ALL);
        bound.refresh.connect(
            "pressed",
            &Callable::from_object_method(&this, "on_refresh_pressed"),
        );
    }

    fn disabled_reason(&self) -> Option<&'static str> {
        let connected = self
            .session
            .as_ref()
            .is_some_and(|session| session.bind().phase() == LinkPhase::Open);
        if !connected {
            Some("NO HOST CONNECTION")
        } else if self
            .session
            .as_ref()
            .and_then(|session| session.bind().binding())
            .is_none()
        {
            Some("NO AUTHENTICATED HOST BINDING")
        } else if self.recall_request.is_some() || self.boat_request.is_some() {
            Some("SNAPSHOT REQUEST ALREADY PENDING")
        } else {
            None
        }
    }

    fn request_snapshots(&mut self) {
        if self.disabled_reason().is_some() {
            return;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Some(binding) = session.bind().binding() else {
            return;
        };

        let recall_id = session.bind_mut().new_identifier();
        let recall = protocol::resync_command(
            &CommandIdentity {
                message_id: recall_id.clone(),
                idempotency_key: recall_id.clone(),
                causation_id: String::new(),
            },
            &binding,
        );
        if let Err(reason) = session.bind_mut().send(&recall) {
            self.detail = format!("recall snapshot not requested: {reason}");
            return;
        }
        self.recall_request = Some(recall_id);

        let boat_id = session.bind_mut().new_identifier();
        let boat = protocol::paper_boat_receipt_subscribe_command(&CommandIdentity {
            message_id: boat_id.clone(),
            idempotency_key: boat_id.clone(),
            causation_id: String::new(),
        });
        match session.bind_mut().send(&boat) {
            Ok(()) => {
                self.boat_request = Some(boat_id);
                self.detail =
                    "recall and Paper Boat snapshots requested through the shared Host session"
                        .into();
            }
            Err(reason) => {
                self.detail =
                    format!("recall requested; Paper Boat snapshot not requested: {reason}")
            }
        }
    }

    fn render(&mut self) {
        let disabled_reason = self.disabled_reason();
        let phase = self.session.as_ref().map(|session| session.bind().phase());
        let Some(bound) = &mut self.bound else {
            return;
        };
        bound.disclosure.set_text(DISCLOSURE);
        bound.transport.set_text(match phase {
            Some(LinkPhase::Closed) => "◇ TRANSPORT · CLOSED",
            Some(LinkPhase::Connecting) => "◇ TRANSPORT · CONNECTING",
            Some(LinkPhase::Open) => "◆ TRANSPORT · OPEN",
            None => "◇ TRANSPORT · —",
        });
        bound.binding.set_text(&self.binding.as_ref().map(|binding| format!(
            "HOST BINDING\nHOUSE {}\nROOM {}\nSPIRIT {}\nSESSION {}",
            binding.house_id, binding.room, binding.spirit, binding.session,
        )).unwrap_or_else(|| format!("HOST BINDING\nHOUSE {ABSENT}\nROOM {ABSENT}\nSPIRIT {ABSENT}\nSESSION {ABSENT}")));
        bound
            .recall
            .set_text(&match (&self.recall_cursor, &self.recall_projection) {
                (Some(cursor), Some(projection)) => format!(
                    "RECALL POLICY · VERSION {} · SEQUENCE {}\n{}",
                    cursor.version,
                    cursor.sequence,
                    projection
                        .degraded
                        .as_ref()
                        .map(|reason| format!("◇ DEGRADED · {reason}"))
                        .unwrap_or_else(|| "◆ NO DEGRADATION REPORTED".into()),
                ),
                _ => format!(
                    "RECALL POLICY · VERSION {ABSENT} · SEQUENCE {ABSENT}\nDEGRADED {ABSENT}"
                ),
            });
        bound.boat.set_text(
            &self
                .boat
                .as_ref()
                .map(|snapshot| {
                    let status = match snapshot.status {
                        ReceiptStatus::Pending => "PENDING",
                        ReceiptStatus::Delivered => "DELIVERED",
                        ReceiptStatus::Degraded => "DEGRADED",
                        ReceiptStatus::Refused => "REFUSED",
                    };
                    let integrity = snapshot
                        .receipt
                        .as_ref()
                        .map(|receipt| receipt.integrity_sha256.as_str())
                        .unwrap_or(ABSENT);
                    format!("PAPER BOAT · {status}\nINTEGRITY SHA256 {integrity}")
                })
                .unwrap_or_else(|| format!("PAPER BOAT · {ABSENT}\nINTEGRITY SHA256 {ABSENT}")),
        );
        bound.refusal.set_text(&format!(
            "LAST PROTOCOL REFUSAL\n{}",
            self.last_refusal.as_deref().unwrap_or(ABSENT)
        ));
        bound.detail.set_text(&self.detail);
        bound.refresh.set_disabled(disabled_reason.is_some());
        let reason = disabled_reason
            .map(|reason| format!("ACTION UNAVAILABLE: {reason}"))
            .unwrap_or_else(|| "ACTION AVAILABLE: REQUEST EXISTING HOST SNAPSHOTS".into());
        bound.reason.set_text(&reason);
        bound.refresh.set_tooltip_text(&reason);
    }

    fn apply_recall(&mut self, envelope: &VarDictionary) {
        match protocol::parse_inbound(envelope) {
            Ok(Inbound::Snapshot(snapshot)) => {
                self.binding = Some(snapshot.binding.clone());
                self.recall_cursor = Some(snapshot.cursor);
                self.recall_projection = Some(snapshot.projection);
                self.detail = "authenticated Recall Policy snapshot applied".into();
            }
            Ok(Inbound::Delta(delta)) => {
                if let Ok(binding) = HostBinding::parse(envelope) {
                    self.binding = Some(binding);
                }
                match (&mut self.recall_cursor, &mut self.recall_projection) {
                    (Some(cursor), Some(projection)) => {
                        match protocol::apply_delta(cursor, projection, &delta) {
                            Ok(()) => {
                                self.detail = "authenticated Recall Policy delta applied".into()
                            }
                            Err(reason) => {
                                self.last_refusal =
                                    Some(format!("Recall Policy delta refused: {reason}"))
                            }
                        }
                    }
                    _ => {
                        self.last_refusal =
                            Some("Recall Policy delta refused: no prior snapshot".into())
                    }
                }
            }
            Ok(Inbound::CommandAccepted(_))
            | Ok(Inbound::CommandRefused(_))
            | Ok(Inbound::CommandFailed(_)) => {
                if let Ok(binding) = HostBinding::parse(envelope) {
                    self.binding = Some(binding);
                }
            }
            Err(reason) => {
                self.last_refusal = Some(format!("Recall Policy envelope refused: {reason}"))
            }
        }
    }

    fn apply_boat(&mut self, envelope: &VarDictionary) {
        match protocol::parse_paper_boat_receipt(envelope) {
            Ok(snapshot) => {
                if let Ok(binding) = HostBinding::parse(envelope) {
                    self.binding = Some(binding);
                }
                self.boat = Some(snapshot);
                self.detail = "authenticated Paper Boat receipt applied".into();
            }
            Err(reason) => {
                self.last_refusal = Some(format!("Paper Boat envelope refused: {reason}"))
            }
        }
    }

    fn apply_routing(&mut self, envelope: &VarDictionary) {
        let status_error = protocol::parse_routing_status(envelope).err();
        let familiar_error = protocol::parse_familiar_status(envelope).err();
        let dispatch_error = protocol::parse_dispatch_result(envelope).err();
        if status_error.is_none() || familiar_error.is_none() || dispatch_error.is_none() {
            if let Ok(binding) = HostBinding::parse(envelope) {
                self.binding = Some(binding);
            }
            return;
        }
        self.last_refusal = Some(format!(
            "Routing envelope refused: status [{}]; familiar [{}]; dispatch [{}]",
            status_error.expect("checked"),
            familiar_error.expect("checked"),
            dispatch_error.expect("checked"),
        ));
    }
}

#[godot_api]
impl AthanorHealth {
    #[func]
    fn on_host_opened(&mut self) {
        self.detail = "Host transport opened; awaiting authenticated EventMeta".into();
        self.render();
    }

    #[func]
    fn on_host_closed(&mut self, detail: GString) {
        self.binding = None;
        self.recall_request = None;
        self.boat_request = None;
        self.detail = format!("Host disconnected: {detail}");
        self.render();
    }

    #[func]
    fn on_host_malformed(&mut self, detail: GString) {
        self.last_refusal = Some(format!("malformed JSON: {detail}"));
        self.recall_request = None;
        self.boat_request = None;
        self.detail = "Host JSON refused".into();
        self.render();
    }

    #[func]
    fn on_host_unavailable(&mut self, detail: GString) {
        self.detail = detail.to_string();
        self.render();
    }

    #[func]
    fn on_host_message(&mut self, envelope: VarDictionary) {
        let projection = protocol::event_projection_id(&envelope);
        let correlation = protocol::event_correlation_id(&envelope).ok();
        match projection.as_deref() {
            Ok(::protocol::RECALL_POLICY_PROJECTION_ID) => self.apply_recall(&envelope),
            Ok(::protocol::PAPER_BOAT_RECEIPT_PROJECTION_ID) => self.apply_boat(&envelope),
            Ok(::protocol::ROUTING_PROJECTION_ID) => self.apply_routing(&envelope),
            Ok(_) => {}
            Err(reason) => self.last_refusal = Some(format!("EventMeta refused: {reason}")),
        }
        if correlation.as_deref() == self.recall_request.as_deref() {
            self.recall_request = None;
        }
        if correlation.as_deref() == self.boat_request.as_deref() {
            self.boat_request = None;
        }
        self.render();
    }

    #[func]
    fn on_refresh_pressed(&mut self) {
        self.request_snapshots();
        self.render();
    }
}

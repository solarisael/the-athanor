//! Read-only House worker-lane status from the authenticated Athanor Host.

use godot::classes::control::FocusMode;
use godot::classes::{Button, IPanelContainer, Label, Node, PanelContainer};
use godot::obj::Inherits;
use godot::prelude::*;

use crate::disclosure::{ABSENT, ROUTING_DISCLOSURE};
use crate::host_link::LinkPhase;
use crate::host_session::AthanorHostSession;
use crate::protocol::{self, CommandIdentity, RoutingStatusProjection};

struct Bound {
    disclosure: Gd<Label>,
    state: Gd<Label>,
    lanes: Gd<Label>,
    advisor: Gd<Label>,
    detail: Gd<Label>,
    refresh: Gd<Button>,
}

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub struct AthanorRoutingStatus {
    #[export]
    session_path: NodePath,
    #[export]
    disclosure_label: NodePath,
    #[export]
    state_label: NodePath,
    #[export]
    lanes_label: NodePath,
    #[export]
    advisor_label: NodePath,
    #[export]
    detail_label: NodePath,
    #[export]
    refresh_button: NodePath,

    session: Option<Gd<AthanorHostSession>>,
    projection: Option<RoutingStatusProjection>,
    pending_correlation: Option<String>,
    detail: String,
    bound: Option<Bound>,
    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for AthanorRoutingStatus {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            session_path: NodePath::default(),
            disclosure_label: NodePath::default(),
            state_label: NodePath::default(),
            lanes_label: NodePath::default(),
            advisor_label: NodePath::default(),
            detail_label: NodePath::default(),
            refresh_button: NodePath::default(),
            session: None,
            projection: None,
            pending_correlation: None,
            detail: "waiting for the shared Host session".into(),
            bound: None,
            base,
        }
    }

    fn ready(&mut self) {
        self.resolve_bindings();
        self.wire_session();
        self.wire_controls();
        self.request_status();
        self.render();
    }
}

impl AthanorRoutingStatus {
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
        let state = self.resolve::<Label>(&self.state_label, "state_label", &mut missing);
        let lanes = self.resolve::<Label>(&self.lanes_label, "lanes_label", &mut missing);
        let advisor = self.resolve::<Label>(&self.advisor_label, "advisor_label", &mut missing);
        let detail = self.resolve::<Label>(&self.detail_label, "detail_label", &mut missing);
        let refresh = self.resolve::<Button>(&self.refresh_button, "refresh_button", &mut missing);
        if !missing.is_empty() {
            godot_error!(
                "AthanorRoutingStatus: missing scene bindings: {}",
                missing.join(", ")
            );
            return;
        }
        self.bound = Some(Bound {
            disclosure: disclosure.expect("resolved"),
            state: state.expect("resolved"),
            lanes: lanes.expect("resolved"),
            advisor: advisor.expect("resolved"),
            detail: detail.expect("resolved"),
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

    fn request_status(&mut self) {
        if self.pending_correlation.is_some() {
            return;
        }
        let Some(session) = self.session.as_mut() else {
            self.detail = "shared Host session unavailable".into();
            return;
        };
        if session.bind().phase() != LinkPhase::Open {
            self.detail = "waiting for an authenticated Host connection".into();
            return;
        }
        let Some(binding) = session.bind().binding() else {
            self.detail = "waiting for the first Host binding".into();
            return;
        };
        let message_id = session.bind_mut().new_identifier();
        let command = protocol::routing_status_command(
            &binding,
            &CommandIdentity {
                message_id: message_id.clone(),
                idempotency_key: message_id.clone(),
                causation_id: String::new(),
            },
        );
        match session.bind_mut().send(&command) {
            Ok(()) => {
                self.pending_correlation = Some(message_id);
                self.detail = "status requested from the Host".into();
            }
            Err(reason) => self.detail = format!("status not sent: {reason}"),
        }
    }

    fn render(&mut self) {
        let connected = self
            .session
            .as_ref()
            .is_some_and(|session| session.bind().phase() == LinkPhase::Open);
        let Some(bound) = &mut self.bound else {
            return;
        };
        bound.disclosure.set_text(ROUTING_DISCLOSURE);
        bound.state.set_text(if connected {
            if self.pending_correlation.is_some() {
                "◇ HOST CONNECTED · QUERY PENDING"
            } else {
                "◆ HOST CONNECTED · STATUS APPLIED"
            }
        } else {
            "◇ HOST UNAVAILABLE"
        });
        let lanes = self.projection.as_ref().map(|projection| {
            projection.lanes.iter().map(|lane| {
                format!(
                    "{} · {} · {} · {}\n  {}\n  tools: {} · context: {} · edits: {} · infers intent: {} · acceptance: {}",
                    lane.name,
                    lane.model_role,
                    lane.omp_agent,
                    if lane.can_edit { "EXECUTOR" } else { "READ ONLY" },
                    lane.description,
                    lane.tools.join(", "),
                    lane.allowed_context_modes.join(", "),
                    if lane.can_edit { "yes" } else { "no" },
                    if lane.can_infer_intent { "yes" } else { "no" },
                    if lane.requires_acceptance { "required" } else { "not required" },
                )
            }).collect::<Vec<_>>().join("\n\n")
        }).unwrap_or_else(|| format!("WORKER LANES\n{ABSENT}"));
        bound.lanes.set_text(&lanes);
        let advisor = self
            .projection
            .as_ref()
            .map(|projection| {
                format!(
                    "ADVISOR · {} · dispatchable: {}\n{}",
                    projection.advisor.name,
                    if projection.advisor.dispatchable {
                        "yes"
                    } else {
                        "no"
                    },
                    projection.advisor.description,
                )
            })
            .unwrap_or_else(|| format!("ADVISOR\n{ABSENT}"));
        bound.advisor.set_text(&advisor);
        let receipt = self
            .projection
            .as_ref()
            .map(|projection| {
                format!(
                    " · EVENT {} · SEQ {}",
                    projection.event_id, projection.sequence
                )
            })
            .unwrap_or_default();
        bound
            .detail
            .set_text(&format!("{}{}", self.detail, receipt));
        bound
            .refresh
            .set_disabled(!connected || self.pending_correlation.is_some());
        let reason = if !connected {
            "UNAVAILABLE: NO HOST CONNECTION"
        } else if self.pending_correlation.is_some() {
            "UNAVAILABLE: QUERY ALREADY PENDING"
        } else {
            "REQUEST FRESH HOST STATUS"
        };
        bound.refresh.set_tooltip_text(reason);
    }
}

#[godot_api]
impl AthanorRoutingStatus {
    #[func]
    fn on_host_opened(&mut self) {
        self.detail = "Host connected; waiting for authenticated binding".into();
        self.request_status();
        self.render();
    }

    #[func]
    fn on_host_closed(&mut self, detail: GString) {
        self.pending_correlation = None;
        self.detail = format!("Host disconnected: {detail}");
        self.render();
    }

    #[func]
    fn on_host_malformed(&mut self, detail: GString) {
        self.pending_correlation = None;
        self.detail = format!("malformed JSON refused: {detail}");
        self.render();
    }

    #[func]
    fn on_host_unavailable(&mut self, detail: GString) {
        self.pending_correlation = None;
        self.detail = detail.to_string();
        self.render();
    }

    #[func]
    fn on_host_message(&mut self, envelope: VarDictionary) {
        match protocol::event_projection_id(&envelope) {
            Ok(projection_id) if projection_id == ::protocol::ROUTING_PROJECTION_ID => {
                match protocol::parse_routing_status(&envelope) {
                    Ok(projection) => {
                        if self.pending_correlation.as_deref() != Some(&projection.correlation_id) {
                            self.detail = "unsolicited routing result was refused".into();
                        } else {
                            self.pending_correlation = None;
                            self.detail = "authenticated status applied".into();
                            self.projection = Some(projection);
                        }
                    }
                    Err(reason) => {
                        self.pending_correlation = None;
                        self.detail = format!("routing result refused: {reason}");
                    }
                }
            }
            Ok(_) => self.request_status(),
            Err(reason) => self.detail = format!("envelope without projection_id: {reason}"),
        }
        self.render();
    }

    #[func]
    fn on_refresh_pressed(&mut self) {
        self.request_status();
        self.render();
    }
}

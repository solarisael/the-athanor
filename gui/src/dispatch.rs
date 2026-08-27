//! Build-only bounded dispatch packet authoring through the shared Host session.

use godot::classes::control::FocusMode;
use godot::classes::{
    Button, IPanelContainer, Label, LineEdit, Node, OptionButton, PanelContainer, TextEdit,
};
use godot::obj::Inherits;
use godot::prelude::*;

use crate::disclosure::{ABSENT, DISPATCH_DISCLOSURE};
use crate::host_link::LinkPhase;
use crate::host_session::AthanorHostSession;
use crate::protocol::{self, CommandIdentity, DispatchProjection, SpawnPacketView};

struct Bound {
    disclosure: Gd<Label>,
    lane: Gd<LineEdit>,
    familiar: Gd<LineEdit>,
    task: Gd<TextEdit>,
    target: Gd<LineEdit>,
    acceptance: Gd<TextEdit>,
    risk: Gd<OptionButton>,
    send: Gd<Button>,
    state: Gd<Label>,
    receipt: Gd<Label>,
    diagnostics: Gd<Label>,
    dispatcher: Gd<Label>,
    packet: Gd<TextEdit>,
    detail: Gd<Label>,
    reason: Gd<Label>,
}

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub struct AthanorDispatch {
    #[export]
    session_path: NodePath,
    #[export]
    disclosure_label: NodePath,
    #[export]
    lane_field: NodePath,
    #[export]
    familiar_field: NodePath,
    #[export]
    task_field: NodePath,
    #[export]
    target_field: NodePath,
    #[export]
    acceptance_field: NodePath,
    #[export]
    risk_field: NodePath,
    #[export]
    send_button: NodePath,
    #[export]
    state_label: NodePath,
    #[export]
    receipt_label: NodePath,
    #[export]
    diagnostics_label: NodePath,
    #[export]
    dispatcher_label: NodePath,
    #[export]
    packet_field: NodePath,
    #[export]
    detail_label: NodePath,
    #[export]
    unavailable_label: NodePath,

    session: Option<Gd<AthanorHostSession>>,
    projection: Option<DispatchProjection>,
    pending_correlation: Option<String>,
    detail: String,
    bound: Option<Bound>,
    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for AthanorDispatch {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            session_path: NodePath::default(),
            disclosure_label: NodePath::default(),
            lane_field: NodePath::default(),
            familiar_field: NodePath::default(),
            task_field: NodePath::default(),
            target_field: NodePath::default(),
            acceptance_field: NodePath::default(),
            risk_field: NodePath::default(),
            send_button: NodePath::default(),
            state_label: NodePath::default(),
            receipt_label: NodePath::default(),
            diagnostics_label: NodePath::default(),
            dispatcher_label: NodePath::default(),
            packet_field: NodePath::default(),
            detail_label: NodePath::default(),
            unavailable_label: NodePath::default(),
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
        self.render();
    }
}

impl AthanorDispatch {
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
        let lane = self.resolve::<LineEdit>(&self.lane_field, "lane_field", &mut missing);
        let familiar =
            self.resolve::<LineEdit>(&self.familiar_field, "familiar_field", &mut missing);
        let task = self.resolve::<TextEdit>(&self.task_field, "task_field", &mut missing);
        let target = self.resolve::<LineEdit>(&self.target_field, "target_field", &mut missing);
        let acceptance =
            self.resolve::<TextEdit>(&self.acceptance_field, "acceptance_field", &mut missing);
        let risk = self.resolve::<OptionButton>(&self.risk_field, "risk_field", &mut missing);
        let send = self.resolve::<Button>(&self.send_button, "send_button", &mut missing);
        let state = self.resolve::<Label>(&self.state_label, "state_label", &mut missing);
        let receipt = self.resolve::<Label>(&self.receipt_label, "receipt_label", &mut missing);
        let diagnostics =
            self.resolve::<Label>(&self.diagnostics_label, "diagnostics_label", &mut missing);
        let dispatcher =
            self.resolve::<Label>(&self.dispatcher_label, "dispatcher_label", &mut missing);
        let packet = self.resolve::<TextEdit>(&self.packet_field, "packet_field", &mut missing);
        let detail = self.resolve::<Label>(&self.detail_label, "detail_label", &mut missing);
        let reason =
            self.resolve::<Label>(&self.unavailable_label, "unavailable_label", &mut missing);
        if !missing.is_empty() {
            godot_error!(
                "AthanorDispatch: missing scene bindings: {}",
                missing.join(", ")
            );
            return;
        }
        self.bound = Some(Bound {
            disclosure: disclosure.expect("resolved"),
            lane: lane.expect("resolved"),
            familiar: familiar.expect("resolved"),
            task: task.expect("resolved"),
            target: target.expect("resolved"),
            acceptance: acceptance.expect("resolved"),
            risk: risk.expect("resolved"),
            send: send.expect("resolved"),
            state: state.expect("resolved"),
            receipt: receipt.expect("resolved"),
            diagnostics: diagnostics.expect("resolved"),
            dispatcher: dispatcher.expect("resolved"),
            packet: packet.expect("resolved"),
            detail: detail.expect("resolved"),
            reason: reason.expect("resolved"),
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
        for field in [&mut bound.lane, &mut bound.familiar, &mut bound.target] {
            field.set_focus_mode(FocusMode::ALL);
        }
        bound.task.set_focus_mode(FocusMode::ALL);
        bound.acceptance.set_focus_mode(FocusMode::ALL);
        bound.risk.set_focus_mode(FocusMode::ALL);
        bound.send.set_focus_mode(FocusMode::ALL);
        bound.packet.set_focus_mode(FocusMode::ALL);
        bound.packet.set_editable(false);
        if bound.risk.get_item_count() == 0 {
            bound.risk.add_item("low");
            bound.risk.add_item("medium");
            bound.risk.add_item("high");
            bound.risk.select(0);
        }
        bound.task.connect(
            "text_changed",
            &Callable::from_object_method(&this, "on_form_changed"),
        );
        bound.send.connect(
            "pressed",
            &Callable::from_object_method(&this, "on_send_pressed"),
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
        } else if self.pending_correlation.is_some() {
            Some("PACKET REQUEST ALREADY PENDING")
        } else if self.bound.as_ref().map_or(true, |bound| {
            bound.task.get_text().to_string().trim().is_empty()
        }) {
            Some("TASK TEXT IS REQUIRED")
        } else {
            None
        }
    }

    fn send_request(&mut self) {
        if self.disabled_reason().is_some() {
            return;
        }
        let Some(bound) = &self.bound else {
            return;
        };
        let lane = bound.lane.get_text().to_string().trim().to_owned();
        let familiar = bound.familiar.get_text().to_string().trim().to_owned();
        let task = bound.task.get_text().to_string().trim().to_owned();
        let target_value = bound.target.get_text().to_string().trim().to_owned();
        let acceptance = bound
            .acceptance
            .get_text()
            .to_string()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let risk = match bound.risk.get_selected() {
            0 => "low",
            1 => "medium",
            2 => "high",
            _ => {
                self.detail = "dispatch refused locally: unknown risk selection".into();
                self.render();
                return;
            }
        };
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Some(binding) = session.bind().binding() else {
            return;
        };
        let message_id = session.bind_mut().new_identifier();
        let command = protocol::routing_dispatch_command(
            &binding,
            &CommandIdentity {
                message_id: message_id.clone(),
                idempotency_key: message_id.clone(),
                causation_id: String::new(),
            },
            &lane,
            &familiar,
            &task,
            (!target_value.is_empty()).then_some(target_value.as_str()),
            &acceptance,
            risk,
        );
        match session.bind_mut().send(&command) {
            Ok(()) => {
                self.pending_correlation = Some(message_id);
                self.detail = "bounded packet requested from the Host".into();
            }
            Err(reason) => self.detail = format!("packet request not sent: {reason}"),
        }
    }

    fn packet_text(packet: Option<&SpawnPacketView>) -> String {
        let Some(packet) = packet else {
            return ABSENT.into();
        };
        let tasks = packet
            .tasks
            .iter()
            .enumerate()
            .map(|(index, task)| {
                format!(
                    "TASK {}\nname: {}\nagent: {}\ntask:\n{}",
                    index + 1,
                    task.name,
                    task.agent.as_deref().unwrap_or(ABSENT),
                    task.task
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        format!(
            "tool: {}\n\nargs.context:\n{}\n\nargs.tasks:\n{}",
            packet.tool, packet.context, tasks
        )
    }

    fn render(&mut self) {
        let disabled_reason = self.disabled_reason();
        let connected = self
            .session
            .as_ref()
            .is_some_and(|session| session.bind().phase() == LinkPhase::Open);
        let Some(bound) = &mut self.bound else {
            return;
        };
        bound.disclosure.set_text(DISPATCH_DISCLOSURE);
        bound.state.set_text(if connected {
            if self.pending_correlation.is_some() {
                "◇ HOST CONNECTED · PACKET PENDING"
            } else {
                "◆ HOST CONNECTED · BUILDER READY"
            }
        } else {
            "◇ HOST UNAVAILABLE"
        });
        if let Some(projection) = &self.projection {
            bound.receipt.set_text(&format!(
                "{} {} · LANE {} · MODEL ROLE {} · OMP AGENT {} · EVENT {} · SEQ {}",
                if projection.ok { "◆" } else { "✕" },
                projection.status.to_uppercase(),
                projection.lane.as_deref().unwrap_or(ABSENT),
                projection.model_role.as_deref().unwrap_or(ABSENT),
                projection.omp_agent.as_deref().unwrap_or(ABSENT),
                projection.event_id,
                projection.sequence,
            ));
            bound.diagnostics.set_text(&format!(
                "ERRORS\n{}\n\nWARNINGS\n{}",
                if projection.errors.is_empty() {
                    ABSENT.into()
                } else {
                    projection.errors.join("\n")
                },
                if projection.warnings.is_empty() {
                    ABSENT.into()
                } else {
                    projection.warnings.join("\n")
                },
            ));
            bound.dispatcher.set_text(&format!(
                "DISPATCHER EXECUTED: {} · REASON: {}",
                if projection.dispatcher_executed {
                    "yes"
                } else {
                    "no"
                },
                projection.dispatcher_reason,
            ));
            bound
                .packet
                .set_text(&Self::packet_text(projection.spawn_packet.as_ref()));
        } else {
            bound.receipt.set_text(&format!(
                "STATUS {ABSENT} · LANE {ABSENT} · MODEL ROLE {ABSENT} · OMP AGENT {ABSENT}"
            ));
            bound
                .diagnostics
                .set_text(&format!("ERRORS\n{ABSENT}\n\nWARNINGS\n{ABSENT}"));
            bound
                .dispatcher
                .set_text(&format!("DISPATCHER EXECUTED: {ABSENT} · REASON: {ABSENT}"));
            bound.packet.set_text(ABSENT);
        }
        bound.detail.set_text(&self.detail);
        bound.send.set_disabled(disabled_reason.is_some());
        let reason = disabled_reason
            .map(|reason| format!("ACTION UNAVAILABLE: {reason}"))
            .unwrap_or_else(|| "ACTION AVAILABLE: ASK HOST TO BUILD PACKET".into());
        bound.reason.set_text(&reason);
        bound.send.set_tooltip_text(&reason);
    }
}

#[godot_api]
impl AthanorDispatch {
    #[func]
    fn on_host_opened(&mut self) {
        self.detail = "Host connected; waiting for authenticated binding".into();
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
        let correlation = protocol::event_correlation_id(&envelope).ok();
        if correlation.as_deref() == self.pending_correlation.as_deref() {
            match protocol::parse_dispatch_result(&envelope) {
                Ok(projection) => {
                    self.pending_correlation = None;
                    self.detail =
                        "authenticated packet receipt applied; nothing was executed".into();
                    self.projection = Some(projection);
                }
                Err(reason) => {
                    self.pending_correlation = None;
                    self.detail = format!("dispatch receipt refused: {reason}");
                }
            }
        }
        self.render();
    }

    #[func]
    fn on_form_changed(&mut self) {
        self.render();
    }

    #[func]
    fn on_send_pressed(&mut self) {
        self.send_request();
        self.render();
    }
}

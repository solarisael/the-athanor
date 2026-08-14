//! Read-only room familiar spellbook status from the shared authenticated Host.

use godot::classes::control::FocusMode;
use godot::classes::{Button, IPanelContainer, Label, Node, PanelContainer};
use godot::obj::Inherits;
use godot::prelude::*;

use crate::host_link::LinkPhase;
use crate::host_session::AthanorHostSession;
use crate::protocol::{self, CommandIdentity, FamiliarStatusProjection};

const DISCLOSURE: &str = "READ ONLY · FAMILIAR STATUS COMES FROM THE AUTHENTICATED ATHANOR HOST · THE CLIENT DOES NOT READ A SPELLBOOK PATH, INFER A ROOM, DISPATCH, SPAWN, OR EXECUTE AN AGENT";
const ABSENT: &str = "—";

struct Bound {
    disclosure: Gd<Label>,
    state: Gd<Label>,
    source: Gd<Label>,
    familiars: Gd<Label>,
    detail: Gd<Label>,
    reason: Gd<Label>,
    refresh: Gd<Button>,
}

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub struct AthanorFamiliarStatus {
    #[export]
    session_path: NodePath,
    #[export]
    disclosure_label: NodePath,
    #[export]
    state_label: NodePath,
    #[export]
    source_label: NodePath,
    #[export]
    familiars_label: NodePath,
    #[export]
    detail_label: NodePath,
    #[export]
    unavailable_label: NodePath,
    #[export]
    refresh_button: NodePath,

    session: Option<Gd<AthanorHostSession>>,
    projection: Option<FamiliarStatusProjection>,
    pending_correlation: Option<String>,
    detail: String,
    bound: Option<Bound>,
    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for AthanorFamiliarStatus {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            session_path: NodePath::default(),
            disclosure_label: NodePath::default(),
            state_label: NodePath::default(),
            source_label: NodePath::default(),
            familiars_label: NodePath::default(),
            detail_label: NodePath::default(),
            unavailable_label: NodePath::default(),
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

impl AthanorFamiliarStatus {
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
        let disclosure = self.resolve::<Label>(&self.disclosure_label, "disclosure_label", &mut missing);
        let state = self.resolve::<Label>(&self.state_label, "state_label", &mut missing);
        let source = self.resolve::<Label>(&self.source_label, "source_label", &mut missing);
        let familiars = self.resolve::<Label>(&self.familiars_label, "familiars_label", &mut missing);
        let detail = self.resolve::<Label>(&self.detail_label, "detail_label", &mut missing);
        let reason = self.resolve::<Label>(&self.unavailable_label, "unavailable_label", &mut missing);
        let refresh = self.resolve::<Button>(&self.refresh_button, "refresh_button", &mut missing);
        if !missing.is_empty() {
            godot_error!("AthanorFamiliarStatus: missing scene bindings: {}", missing.join(", "));
            return;
        }
        self.bound = Some(Bound {
            disclosure: disclosure.expect("resolved"),
            state: state.expect("resolved"),
            source: source.expect("resolved"),
            familiars: familiars.expect("resolved"),
            detail: detail.expect("resolved"),
            reason: reason.expect("resolved"),
            refresh: refresh.expect("resolved"),
        });
    }

    fn wire_session(&mut self) {
        let Some(mut session) = self.base().try_get_node_as::<AthanorHostSession>(&self.session_path) else {
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
        let Some(bound) = &mut self.bound else { return; };
        bound.refresh.set_focus_mode(FocusMode::ALL);
        bound.refresh.connect("pressed", &Callable::from_object_method(&this, "on_refresh_pressed"));
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
        let command = protocol::familiar_status_command(
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
                self.detail = "familiar status requested from the Host".into();
            }
            Err(reason) => self.detail = format!("familiar status not sent: {reason}"),
        }
    }

    fn disabled_reason(&self) -> Option<&'static str> {
        let connected = self.session.as_ref().is_some_and(|session| session.bind().phase() == LinkPhase::Open);
        if !connected {
            Some("NO HOST CONNECTION")
        } else if self.session.as_ref().and_then(|session| session.bind().binding()).is_none() {
            Some("NO AUTHENTICATED HOST BINDING")
        } else if self.pending_correlation.is_some() {
            Some("QUERY ALREADY PENDING")
        } else {
            None
        }
    }

    fn render(&mut self) {
        let disabled_reason = self.disabled_reason();
        let connected = self.session.as_ref().is_some_and(|session| session.bind().phase() == LinkPhase::Open);
        let Some(bound) = &mut self.bound else { return; };
        bound.disclosure.set_text(DISCLOSURE);
        bound.state.set_text(if connected {
            if self.pending_correlation.is_some() { "◇ HOST CONNECTED · QUERY PENDING" } else { "◆ HOST CONNECTED · STATUS SETTLED" }
        } else {
            "◇ HOST UNAVAILABLE"
        });
        let source = self.projection.as_ref().and_then(|projection| projection.source.as_deref()).unwrap_or(ABSENT);
        let source_alias = self
            .projection
            .as_ref()
            .map(|projection| {
                if projection.source_alias {
                    "yes"
                } else {
                    "no"
                }
            })
            .unwrap_or(ABSENT);
        bound
            .source
            .set_text(&format!("SOURCE {source} · SOURCE ALIAS {source_alias}"));
        let familiar_text = self.projection.as_ref().map(|projection| {
            let heading = match &projection.collective {
                Some(collective) => format!(
                    "COLLECTIVE {collective}\nCOLLECTIVE ALIASES {}\nSPELLBOOK ALIASES {}",
                    if projection.collective_aliases.is_empty() { ABSENT.into() } else { projection.collective_aliases.join(", ") },
                    if projection.spellbook_aliases.is_empty() { ABSENT.into() } else { projection.spellbook_aliases.join(", ") },
                ),
                None => format!("COLLECTIVE {ABSENT}"),
            };
            let entries = projection.familiars.iter().map(|familiar| {
                format!(
                    "{} · {} · LANE {}\n  ALIASES {}\n  {}",
                    familiar.name,
                    familiar.id,
                    familiar.lane,
                    if familiar.aliases.is_empty() { ABSENT.into() } else { familiar.aliases.join(", ") },
                    familiar.description,
                )
            }).collect::<Vec<_>>().join("\n\n");
            let errors = if projection.errors.is_empty() { String::new() } else { format!("\n\nERRORS\n{}", projection.errors.join("\n")) };
            format!("{}\n\n{}{}", heading, if entries.is_empty() { ABSENT } else { &entries }, errors)
        }).unwrap_or_else(|| format!("FAMILIARS\n{ABSENT}"));
        bound.familiars.set_text(&familiar_text);
        let receipt = self.projection.as_ref().map(|projection| {
            format!(" · {} · EVENT {} · SEQ {}", if projection.ok { "READY" } else { "REFUSED" }, projection.event_id, projection.sequence)
        }).unwrap_or_default();
        bound.detail.set_text(&format!("{}{}", self.detail, receipt));
        bound.refresh.set_disabled(disabled_reason.is_some());
        let reason = disabled_reason.map(|reason| format!("ACTION UNAVAILABLE: {reason}")).unwrap_or_else(|| "ACTION AVAILABLE: REQUEST FRESH HOST STATUS".into());
        bound.reason.set_text(&reason);
        bound.refresh.set_tooltip_text(&reason);
    }
}

#[godot_api]
impl AthanorFamiliarStatus {
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
        let correlation = protocol::event_correlation_id(&envelope).ok();
        if correlation.as_deref() == self.pending_correlation.as_deref() {
            match protocol::parse_familiar_status(&envelope) {
                Ok(projection) => {
                    self.pending_correlation = None;
                    self.detail = "authenticated familiar status applied".into();
                    self.projection = Some(projection);
                }
                Err(reason) => {
                    self.pending_correlation = None;
                    self.detail = format!("familiar status refused: {reason}");
                }
            }
        } else if self.projection.is_none() && self.pending_correlation.is_none() {
            self.request_status();
        }
        self.render();
    }

    #[func]
    fn on_refresh_pressed(&mut self) {
        self.request_status();
        self.render();
    }
}

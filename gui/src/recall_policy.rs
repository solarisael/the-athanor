//! The Recall Policy instrument: the first Host-backed operator surface.
//!
//! Behaviour lives entirely in Rust. The scene supplies composition, theme
//! tokens, and the node paths bound through the exported properties below.
//!
//! Invariants this class enforces at runtime:
//!
//! * Nothing is displayed that did not arrive in a real Host snapshot or delta.
//! * Every unavailable control carries a specific visible reason.
//! * The fixed non-authority and Host-unavailable disclosure is re-asserted on
//!   every render and can neither be replaced nor emptied by the scene.
//! * Wire values and display labels never substitute for each other.
//! * Transport phase, projection readiness, command lifecycle, and subsystem
//!   health stay on separate channels.

use godot::classes::control::FocusMode;
use godot::classes::{Button, IPanelContainer, Label, LineEdit, Node, PanelContainer};
use godot::obj::Inherits;
use godot::prelude::*;

use crate::disclosure::{ABSENT, RECALL_POLICY_DISCLOSURE};
use crate::host_link::{HostLink, LinkPhase};
use crate::host_session::AthanorHostSession;
use crate::protocol::{
    self, CommandIdentity, HostBinding, Inbound, ProjectionCursor, RecallPolicyProjection,
    RecoveryState, RequestedMode,
};

// [gui/recall/command] [runtime/timeout]
const PENDING_TIMEOUT_SECONDS: f64 = 15.0;

/// Transport and projection readiness, kept on one axis and never merged with
/// subsystem health or command lifecycle.
#[derive(Copy, Clone, PartialEq, Eq)]
enum LinkState {
    Idle,
    Connecting,
    Connected,
    Ready,
    Disconnected,
    ProtocolRefused,
}

impl LinkState {
    /// Mark plus word, so the state survives greyscale and stillness.
    fn display(self) -> &'static str {
        match self {
            LinkState::Idle => "◇ DISCONNECTED",
            LinkState::Connecting => "◈ CONNECTING",
            LinkState::Connected => "◆ CONNECTED · NO SNAPSHOT",
            LinkState::Ready => "◆ SNAPSHOT APPLIED",
            LinkState::Disconnected => "◇ DISCONNECTED",
            LinkState::ProtocolRefused => "✕ PROTOCOL REFUSED",
        }
    }
}

/// Lifecycle of the one write this instrument can author.
enum CommandPhase {
    Idle,
    Pending {
        correlation_id: String,
        idempotency_key: String,
        mode: RequestedMode,
        elapsed: f64,
    },
    Acknowledged {
        mode: RequestedMode,
    },
    Refused {
        mode: RequestedMode,
        reason: String,
    },
    Failed {
        mode: RequestedMode,
        idempotency_key: String,
        reason: String,
    },
}

/// Resolved scene bindings. Either every binding resolves or none is used, so
/// a partially wired scene cannot present half a state.
struct Bound {
    disclosure: Gd<Label>,
    url_field: Gd<LineEdit>,
    connect_button: Gd<Button>,
    disconnect_button: Gd<Button>,
    snapshot_button: Gd<Button>,
    link_state: Gd<Label>,
    link_detail: Gd<Label>,
    projection_meta: Gd<Label>,
    binding: Gd<Label>,
    requested_value: Gd<Label>,
    resolved_value: Gd<Label>,
    active_project_value: Gd<Label>,
    working_set_value: Gd<Label>,
    resolution: Gd<Label>,
    refresh: Gd<Label>,
    recovery: Gd<Label>,
    health: Gd<Label>,
    mode_buttons: Vec<Gd<Button>>,
    selection: Gd<Label>,
    apply_button: Gd<Button>,
    command_state: Gd<Label>,
    unavailable: Gd<Label>,
}

/// A control either accepts interaction with an explanatory tooltip, or refuses
/// it with the reason that must be shown to the operator.
#[derive(Clone)]
enum ControlAvailability {
    Enabled { tooltip: String },
    Disabled { reason: String },
}

impl ControlAvailability {
    fn enabled(tooltip: impl Into<String>) -> Self {
        Self::Enabled {
            tooltip: tooltip.into(),
        }
    }

    fn disabled(reason: impl Into<String>) -> Self {
        Self::Disabled {
            reason: reason.into(),
        }
    }

    fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled { .. })
    }

    fn tooltip(&self) -> &str {
        match self {
            Self::Enabled { tooltip } => tooltip,
            Self::Disabled { reason } => reason,
        }
    }
}

fn mode_controls(
    selected_mode: Option<RequestedMode>,
    modes_reason: Option<String>,
    apply_reason: Option<String>,
) -> (
    Option<RequestedMode>,
    ControlAvailability,
    ControlAvailability,
) {
    let modes = match modes_reason {
        Some(reason) => ControlAvailability::disabled(format!("UNAVAILABLE: {reason}")),
        None => {
            ControlAvailability::enabled("PROPOSE A REQUESTED MODE · APPLYING IS A SEPARATE STEP")
        }
    };
    let apply = match apply_reason {
        Some(reason) => ControlAvailability::disabled(format!("UNAVAILABLE: {reason}")),
        None => ControlAvailability::enabled("SEND THE REQUESTED-MODE COMMAND TO THE HOST"),
    };
    (selected_mode, modes, apply)
}

/// Everything the next render needs, computed without touching nodes.
struct View {
    link_state: String,
    link_detail: String,
    projection_meta: String,
    binding: String,
    requested_value: String,
    resolved_value: String,
    active_project_value: String,
    working_set_value: String,
    resolution: String,
    refresh: String,
    recovery: String,
    health: String,
    selection: String,
    command_state: String,
    unavailable: String,
    selected_mode: Option<RequestedMode>,
    url: ControlAvailability,
    connect: ControlAvailability,
    disconnect: ControlAvailability,
    snapshot: ControlAvailability,
    modes: ControlAvailability,
    apply: ControlAvailability,
}

// ---------------------------------------------------------------------------
// The instrument
// ---------------------------------------------------------------------------

#[derive(GodotClass)]
#[class(base = PanelContainer)]
pub struct AthanorRecallPolicy {
    /// Fallback Host address used when the operator field is empty.
    #[export]
    host_url: GString,
    #[export]
    session_path: NodePath,

    #[export]
    disclosure_label: NodePath,
    #[export]
    url_field: NodePath,
    #[export]
    connect_button: NodePath,
    #[export]
    disconnect_button: NodePath,
    #[export]
    snapshot_button: NodePath,
    #[export]
    link_state_label: NodePath,
    #[export]
    link_detail_label: NodePath,
    #[export]
    projection_meta_label: NodePath,
    #[export]
    binding_label: NodePath,
    #[export]
    requested_mode_value: NodePath,
    #[export]
    resolved_mode_value: NodePath,
    #[export]
    active_project_value: NodePath,
    #[export]
    working_set_value: NodePath,
    #[export]
    resolution_label: NodePath,
    #[export]
    refresh_label: NodePath,
    #[export]
    recovery_label: NodePath,
    #[export]
    health_label: NodePath,
    #[export]
    mode_auto_button: NodePath,
    #[export]
    mode_conversation_button: NodePath,
    #[export]
    mode_work_button: NodePath,
    #[export]
    mode_quiet_button: NodePath,
    #[export]
    selection_label: NodePath,
    #[export]
    apply_button: NodePath,
    #[export]
    command_state_label: NodePath,
    #[export]
    unavailable_reason_label: NodePath,

    session: Option<Gd<AthanorHostSession>>,
    link_state: LinkState,
    link_detail: String,
    host_binding: Option<HostBinding>,
    cursor: Option<ProjectionCursor>,
    projection: Option<RecallPolicyProjection>,
    last_event_id: Option<String>,
    selection: Option<RequestedMode>,
    command: CommandPhase,
    bound: Option<Bound>,
    dirty: bool,

    base: Base<PanelContainer>,
}

#[godot_api]
impl IPanelContainer for AthanorRecallPolicy {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            session_path: NodePath::default(),
            host_url: GString::new(),
            disclosure_label: NodePath::default(),
            url_field: NodePath::default(),
            connect_button: NodePath::default(),
            disconnect_button: NodePath::default(),
            snapshot_button: NodePath::default(),
            link_state_label: NodePath::default(),
            link_detail_label: NodePath::default(),
            projection_meta_label: NodePath::default(),
            binding_label: NodePath::default(),
            requested_mode_value: NodePath::default(),
            resolved_mode_value: NodePath::default(),
            active_project_value: NodePath::default(),
            working_set_value: NodePath::default(),
            resolution_label: NodePath::default(),
            refresh_label: NodePath::default(),
            recovery_label: NodePath::default(),
            health_label: NodePath::default(),
            mode_auto_button: NodePath::default(),
            mode_conversation_button: NodePath::default(),
            mode_work_button: NodePath::default(),
            mode_quiet_button: NodePath::default(),
            selection_label: NodePath::default(),
            apply_button: NodePath::default(),
            command_state_label: NodePath::default(),
            unavailable_reason_label: NodePath::default(),
            session: None,
            link_state: LinkState::Idle,
            link_detail: String::from("no connection started in this session"),
            host_binding: None,
            cursor: None,
            projection: None,
            last_event_id: None,
            selection: None,
            command: CommandPhase::Idle,
            bound: None,
            dirty: true,
            base,
        }
    }

    fn ready(&mut self) {
        self.resolve_bindings();
        self.wire_controls();
        self.wire_session();
        if self.session_phase() == LinkPhase::Open {
            self.on_host_opened();
        } else {
            self.base_mut().set_process(false);
            self.render();
        }
    }

    fn process(&mut self, delta: f64) {
        let mut ticked = false;
        let mut timed_out = false;
        if let CommandPhase::Pending { elapsed, .. } = &mut self.command {
            let previous_second = elapsed.floor();
            *elapsed += delta;
            ticked = elapsed.floor() != previous_second;
            timed_out = *elapsed >= PENDING_TIMEOUT_SECONDS;
        }
        if timed_out {
            self.fail_pending(&format!(
                "the Host did not respond within {PENDING_TIMEOUT_SECONDS:.0} s"
            ));
        } else if ticked {
            self.dirty = true;
        }

        if self.dirty {
            self.render();
        }

        if !matches!(self.command, CommandPhase::Pending { .. }) {
            self.base_mut().set_process(false);
        }
    }
}

// ---------------------------------------------------------------------------
// Scene binding
// ---------------------------------------------------------------------------

impl AthanorRecallPolicy {
    fn resolve<T>(&self, path: &NodePath, name: &str, missing: &mut Vec<String>) -> Option<Gd<T>>
    where
        T: GodotClass + Inherits<Node>,
    {
        if path.is_empty() {
            missing.push(name.to_string());
            return None;
        }
        match self.base().try_get_node_as::<T>(path) {
            Some(node) => Some(node),
            None => {
                missing.push(name.to_string());
                None
            }
        }
    }

    fn resolve_bindings(&mut self) {
        let mut missing: Vec<String> = Vec::new();

        let disclosure =
            self.resolve::<Label>(&self.disclosure_label, "disclosure_label", &mut missing);
        let url_field = self.resolve::<LineEdit>(&self.url_field, "url_field", &mut missing);
        let connect_button =
            self.resolve::<Button>(&self.connect_button, "connect_button", &mut missing);
        let disconnect_button =
            self.resolve::<Button>(&self.disconnect_button, "disconnect_button", &mut missing);
        let snapshot_button =
            self.resolve::<Button>(&self.snapshot_button, "snapshot_button", &mut missing);
        let link_state =
            self.resolve::<Label>(&self.link_state_label, "link_state_label", &mut missing);
        let link_detail =
            self.resolve::<Label>(&self.link_detail_label, "link_detail_label", &mut missing);
        let projection_meta = self.resolve::<Label>(
            &self.projection_meta_label,
            "projection_meta_label",
            &mut missing,
        );
        let binding = self.resolve::<Label>(&self.binding_label, "binding_label", &mut missing);
        let requested_value = self.resolve::<Label>(
            &self.requested_mode_value,
            "requested_mode_value",
            &mut missing,
        );
        let resolved_value = self.resolve::<Label>(
            &self.resolved_mode_value,
            "resolved_mode_value",
            &mut missing,
        );
        let active_project_value = self.resolve::<Label>(
            &self.active_project_value,
            "active_project_value",
            &mut missing,
        );
        let working_set_value =
            self.resolve::<Label>(&self.working_set_value, "working_set_value", &mut missing);
        let resolution =
            self.resolve::<Label>(&self.resolution_label, "resolution_label", &mut missing);
        let refresh = self.resolve::<Label>(&self.refresh_label, "refresh_label", &mut missing);
        let recovery = self.resolve::<Label>(&self.recovery_label, "recovery_label", &mut missing);
        let health = self.resolve::<Label>(&self.health_label, "health_label", &mut missing);
        let mode_auto =
            self.resolve::<Button>(&self.mode_auto_button, "mode_auto_button", &mut missing);
        let mode_conversation = self.resolve::<Button>(
            &self.mode_conversation_button,
            "mode_conversation_button",
            &mut missing,
        );
        let mode_work =
            self.resolve::<Button>(&self.mode_work_button, "mode_work_button", &mut missing);
        let mode_quiet =
            self.resolve::<Button>(&self.mode_quiet_button, "mode_quiet_button", &mut missing);
        let selection =
            self.resolve::<Label>(&self.selection_label, "selection_label", &mut missing);
        let apply_button = self.resolve::<Button>(&self.apply_button, "apply_button", &mut missing);
        let command_state = self.resolve::<Label>(
            &self.command_state_label,
            "command_state_label",
            &mut missing,
        );
        let unavailable = self.resolve::<Label>(
            &self.unavailable_reason_label,
            "unavailable_reason_label",
            &mut missing,
        );

        if !missing.is_empty() {
            godot_error!(
                "AthanorRecallPolicy: missing scene bindings: {}",
                missing.join(", ")
            );
            self.bound = None;
            return;
        }

        self.bound = Some(Bound {
            disclosure: disclosure.expect("resolved"),
            url_field: url_field.expect("resolved"),
            connect_button: connect_button.expect("resolved"),
            disconnect_button: disconnect_button.expect("resolved"),
            snapshot_button: snapshot_button.expect("resolved"),
            link_state: link_state.expect("resolved"),
            link_detail: link_detail.expect("resolved"),
            projection_meta: projection_meta.expect("resolved"),
            binding: binding.expect("resolved"),
            requested_value: requested_value.expect("resolved"),
            resolved_value: resolved_value.expect("resolved"),
            active_project_value: active_project_value.expect("resolved"),
            working_set_value: working_set_value.expect("resolved"),
            resolution: resolution.expect("resolved"),
            refresh: refresh.expect("resolved"),
            recovery: recovery.expect("resolved"),
            health: health.expect("resolved"),
            mode_buttons: vec![
                mode_auto.expect("resolved"),
                mode_conversation.expect("resolved"),
                mode_work.expect("resolved"),
                mode_quiet.expect("resolved"),
            ],
            selection: selection.expect("resolved"),
            apply_button: apply_button.expect("resolved"),
            command_state: command_state.expect("resolved"),
            unavailable: unavailable.expect("resolved"),
        });
    }

    fn wire_controls(&mut self) {
        let this = self.to_gd();
        let host_url = self.host_url.clone();
        let Some(bound) = self.bound.as_mut() else {
            return;
        };

        bound.disclosure.set_text(RECALL_POLICY_DISCLOSURE);

        if bound.url_field.get_text().to_string().trim().is_empty() {
            bound.url_field.set_text(&host_url);
        }
        bound
            .url_field
            .set_placeholder(protocol::HOST_URL_PLACEHOLDER);
        bound.url_field.set_focus_mode(FocusMode::ALL);
        bound.url_field.connect(
            "text_submitted",
            &Callable::from_object_method(&this, "on_url_submitted"),
        );

        let handlers: [(&mut Gd<Button>, &str); 4] = [
            (&mut bound.connect_button, "on_connect_pressed"),
            (&mut bound.disconnect_button, "on_disconnect_pressed"),
            (&mut bound.snapshot_button, "on_snapshot_pressed"),
            (&mut bound.apply_button, "on_apply_pressed"),
        ];
        for (button, method) in handlers {
            button.set_focus_mode(FocusMode::ALL);
            button.connect("pressed", &Callable::from_object_method(&this, method));
        }

        let mode_methods = [
            "on_mode_auto_pressed",
            "on_mode_conversation_pressed",
            "on_mode_work_pressed",
            "on_mode_quiet_pressed",
        ];
        for (button, method) in bound.mode_buttons.iter_mut().zip(mode_methods) {
            button.set_focus_mode(FocusMode::ALL);
            button.connect("pressed", &Callable::from_object_method(&this, method));
        }
    }

    fn wire_session(&mut self) {
        let Some(mut session) = self
            .base()
            .try_get_node_as::<AthanorHostSession>(&self.session_path)
        else {
            self.link_state = LinkState::Idle;
            self.link_detail = "shared Host session not found".into();
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

    fn session_phase(&self) -> LinkPhase {
        self.session
            .as_ref()
            .map(|session| session.bind().phase())
            .unwrap_or(LinkPhase::Closed)
    }

    fn session_url(&self) -> String {
        self.session
            .as_ref()
            .map(|session| session.bind().url().to_string())
            .unwrap_or_else(|| self.current_url())
    }

    fn new_session_identifier(&mut self) -> String {
        self.session
            .as_mut()
            .map(|session| session.bind_mut().new_identifier())
            .unwrap_or_default()
    }

    fn send_session(&mut self, envelope: &VarDictionary) -> Result<(), String> {
        self.session
            .as_mut()
            .ok_or_else(|| "shared Host session not found".to_string())?
            .bind_mut()
            .send(envelope)
    }

    fn current_url(&self) -> String {
        if let Some(bound) = self.bound.as_ref() {
            let text = bound.url_field.get_text().to_string();
            if !text.trim().is_empty() {
                return text;
            }
        }
        self.host_url.to_string()
    }
}

// ---------------------------------------------------------------------------
// Host conversation
// ---------------------------------------------------------------------------

impl AthanorRecallPolicy {
    fn new_identity(&mut self, idempotency_key: String) -> CommandIdentity {
        let message_id = self.new_session_identifier();
        let idempotency_key = if idempotency_key.trim().is_empty() {
            self.new_session_identifier()
        } else {
            idempotency_key
        };
        CommandIdentity {
            message_id,
            idempotency_key,
            causation_id: self.last_event_id.clone().unwrap_or_default(),
        }
    }

    fn drop_projection(&mut self) {
        self.host_binding = None;
        self.cursor = None;
        self.projection = None;
        self.last_event_id = None;
        self.selection = None;
    }

    fn pending_correlation(&self) -> Option<(String, String, RequestedMode)> {
        match &self.command {
            CommandPhase::Pending {
                correlation_id,
                idempotency_key,
                mode,
                ..
            } => Some((correlation_id.clone(), idempotency_key.clone(), *mode)),
            _ => None,
        }
    }

    fn fail_pending(&mut self, reason: &str) {
        if let Some((_, idempotency_key, mode)) = self.pending_correlation() {
            self.command = CommandPhase::Failed {
                mode,
                idempotency_key,
                reason: reason.to_string(),
            };
            self.dirty = true;
        }
    }

    /// Initial subscribe before a binding; explicit resync afterwards.
    fn request_snapshot(&mut self) {
        let identity = self.new_identity(String::new());
        let envelope = match self.host_binding.as_ref() {
            Some(binding) => protocol::resync_command(&identity, binding),
            None => protocol::subscribe_command(&identity, None),
        };
        match self.send_session(&envelope) {
            Ok(()) => {
                self.link_detail = format!("snapshot requested from {}", self.session_url());
            }
            Err(reason) => {
                self.link_detail = format!("snapshot request not sent: {reason}");
            }
        }
        self.dirty = true;
    }

    fn send_acknowledge(&mut self) {
        let (Some(binding), Some(cursor)) = (self.host_binding.clone(), self.cursor.clone()) else {
            return;
        };
        let identity = self.new_identity(String::new());
        let envelope = protocol::acknowledge_command(&binding, &identity, &cursor);
        if let Err(reason) = self.send_session(&envelope) {
            self.link_detail = format!("version acknowledgement not sent: {reason}");
        }
    }

    fn handle_inbound(&mut self, inbound: Inbound) {
        match inbound {
            Inbound::Snapshot(snapshot) => {
                self.host_binding = Some(snapshot.binding);
                self.last_event_id = Some(snapshot.cursor.snapshot_id.clone());
                self.cursor = Some(snapshot.cursor);
                self.projection = Some(snapshot.projection);
                self.selection = None;
                self.link_state = LinkState::Ready;
                self.link_detail = format!("snapshot received from {}", self.session_url());
                self.send_acknowledge();
            }
            Inbound::Delta(delta) => {
                let applied = match (self.cursor.as_mut(), self.projection.as_mut()) {
                    (Some(cursor), Some(projection)) => {
                        protocol::apply_delta(cursor, projection, &delta)
                    }
                    _ => Err("delta received without an applied snapshot".to_string()),
                };
                match applied {
                    Ok(()) => {
                        self.last_event_id = Some(delta.delta_id.clone());
                        self.link_state = LinkState::Ready;
                        self.link_detail = format!("delta {} aplicada", delta.delta_id);
                        self.send_acknowledge();
                    }
                    Err(reason) => {
                        self.link_detail = format!("replay pedido ao Host: {reason}");
                        self.request_snapshot();
                    }
                }
            }
            Inbound::CommandAccepted(outcome) => {
                if let Some((correlation_id, _, mode)) = self.pending_correlation() {
                    if correlation_id == outcome.correlation_id {
                        self.command = CommandPhase::Acknowledged { mode };
                    }
                }
            }
            Inbound::CommandRefused(outcome) => {
                if let Some((correlation_id, _, mode)) = self.pending_correlation() {
                    if correlation_id == outcome.correlation_id {
                        self.command = CommandPhase::Refused {
                            mode,
                            reason: outcome
                                .reason
                                .unwrap_or_else(|| "the Host did not provide a reason".to_string()),
                        };
                    }
                }
            }
            Inbound::CommandFailed(outcome) => {
                if let Some((correlation_id, idempotency_key, mode)) = self.pending_correlation() {
                    if correlation_id == outcome.correlation_id {
                        self.command = CommandPhase::Failed {
                            mode,
                            idempotency_key,
                            reason: outcome
                                .reason
                                .unwrap_or_else(|| "the Host did not provide a reason".to_string()),
                        };
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Availability: every refusal names its own reason
// ---------------------------------------------------------------------------

impl AthanorRecallPolicy {
    /// Reason the proposal controls cannot be used, if any.
    fn selection_unavailable(&self) -> Option<String> {
        if let Err(reason) = HostLink::validate_url(&self.current_url()) {
            return Some(format!("INVALID HOST ADDRESS: {reason}"));
        }
        match self.link_state {
            LinkState::Idle => return Some("NO HOST CONNECTION".to_string()),
            LinkState::Connecting => {
                return Some("WAITING FOR THE HOST CONNECTION TO OPEN".to_string());
            }
            LinkState::Disconnected => {
                return Some("CONNECTION CLOSED · RECONNECT TO OBTAIN A SNAPSHOT".to_string());
            }
            LinkState::ProtocolRefused => {
                return Some("PROTOCOL REFUSED · REQUEST A VALID SNAPSHOT".to_string());
            }
            LinkState::Connected | LinkState::Ready => {}
        }
        if self.projection.is_none() {
            return Some("WAITING FOR A REAL HOST SNAPSHOT".to_string());
        }
        if matches!(self.command, CommandPhase::Pending { .. }) {
            return Some("COMMAND PENDING · WAITING FOR HOST ACKNOWLEDGEMENT".to_string());
        }
        None
    }

    /// Reason the authorising control cannot be used, if any.
    fn write_unavailable(&self) -> Option<String> {
        if let Some(reason) = self.selection_unavailable() {
            return Some(reason);
        }
        let Some(selection) = self.selection else {
            return Some("SELECT A REQUESTED MODE BEFORE APPLYING".to_string());
        };
        if let CommandPhase::Refused { mode, reason } = &self.command {
            if *mode == selection {
                return Some(format!("THE HOST REFUSED THIS MODE: {reason}"));
            }
        }
        let projection = self.projection.as_ref()?;
        if projection.requested_mode == selection {
            return Some(format!(
                "THE REQUESTED MODE IS ALREADY {} (wire {})",
                selection.display(),
                selection.wire()
            ));
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

impl AthanorRecallPolicy {
    fn build_view(&self) -> View {
        let url_valid = HostLink::validate_url(&self.current_url());
        let phase = self.session_phase();
        let closed = phase == LinkPhase::Closed;

        let projection_meta = match (self.cursor.as_ref(), self.projection.as_ref()) {
            (Some(cursor), Some(_)) => format!(
                "PROJECTION {} · SCHEMA {} · SNAPSHOT {} · VERSION {} · SEQUENCE {} · HASH {}",
                protocol::PROJECTION_ID,
                protocol::SCHEMA_VERSION,
                cursor.snapshot_id,
                cursor.version,
                cursor.sequence,
                cursor
                    .state_hash
                    .clone()
                    .unwrap_or_else(|| "not restated since the latest delta".to_string())
            ),
            _ => format!(
                "PROJECTION {} · SCHEMA {} · NO VERSION APPLIED",
                protocol::PROJECTION_ID,
                protocol::SCHEMA_VERSION
            ),
        };

        let binding = match self.host_binding.as_ref() {
            Some(binding) => format!(
                "HOST BINDING · HOUSE {} · ROOM {} · SPIRIT {} · SESSION {} · SCOPE {} · VISIBILITY {} · AUTHORITY {}",
                binding.house_id,
                binding.room,
                binding.spirit,
                binding.session,
                binding.scope,
                binding.visibility,
                binding.authority_class
            ),
            None => "HOST BINDING NOT RECEIVED · THE CLIENT DOES NOT DECLARE HOUSE, ROOM, SPIRIT, OR SESSION".to_string(),
        };

        let (requested_value, resolved_value, active_project_value, working_set_value) =
            match self.projection.as_ref() {
                Some(projection) => (
                    format!(
                        "{} (wire {})",
                        projection.requested_mode.display(),
                        projection.requested_mode.wire()
                    ),
                    format!(
                        "{} (wire {})",
                        projection.resolved_mode.display(),
                        projection.resolved_mode.wire()
                    ),
                    projection
                        .active_project
                        .clone()
                        .unwrap_or_else(|| ABSENT.to_string()),
                    format!("{} entries", projection.working_set_entries),
                ),
                None => (
                    ABSENT.to_string(),
                    ABSENT.to_string(),
                    ABSENT.to_string(),
                    ABSENT.to_string(),
                ),
            };

        let resolution = match self.projection.as_ref() {
            Some(projection) => format!("RESOLUTION REASON: {}", projection.resolution_reason),
            None => "RESOLUTION REASON: no Host snapshot".to_string(),
        };

        let refresh = match self.projection.as_ref() {
            Some(projection) => {
                match (&projection.last_refresh_at, &projection.last_refresh_reason) {
                    (Some(at), Some(reason)) => format!("LATEST REFRESH {at} · {reason}"),
                    (Some(at), None) => format!("LATEST REFRESH {at} · no reason supplied"),
                    (None, _) => "NO REFRESH RECORDED".to_string(),
                }
            }
            None => format!("LATEST REFRESH {ABSENT}"),
        };

        let recovery = match self.projection.as_ref() {
            Some(projection) => match &projection.recovery_state {
                RecoveryState::Pending { terms } if terms.is_empty() => {
                    "RECOVERY PENDING · NO TERMS SUPPLIED".to_string()
                }
                RecoveryState::Pending { terms } => {
                    format!("RECOVERY PENDING · TERMS: {}", terms.join(", "))
                }
                RecoveryState::Idle => "NO RECOVERY PENDING".to_string(),
            },
            None => format!("RECOVERY {ABSENT}"),
        };

        // Subsystem health stays on its own channel, never merged with the
        // transport state above.
        let health = match self.projection.as_ref() {
            Some(projection) => match &projection.degraded {
                Some(degraded) => format!("SUBSYSTEM DEGRADED: {degraded}"),
                None => "NO DEGRADATION REPORTED".to_string(),
            },
            None => format!("DEGRADATION {ABSENT}"),
        };

        let selection = match self.selection {
            Some(mode) => format!(
                "PROPOSED SELECTION: {} (wire {})",
                mode.display(),
                mode.wire()
            ),
            None => "PROPOSED SELECTION: NONE".to_string(),
        };

        let command_state = match &self.command {
            CommandPhase::Idle => "NO COMMAND SENT".to_string(),
            CommandPhase::Pending { mode, elapsed, .. } => format!(
                "⧗ PENDING · {} · {:.0} s without acknowledgement",
                mode.display(),
                elapsed.floor()
            ),
            CommandPhase::Acknowledged { mode } => {
                format!("◆ ACKNOWLEDGED BY HOST · {}", mode.display())
            }
            CommandPhase::Refused { mode, reason } => {
                format!("✕ REFUSED · {} · {reason}", mode.display())
            }
            CommandPhase::Failed { mode, reason, .. } => {
                format!("⚠ FAILED · {} · {reason}", mode.display())
            }
        };

        let (selected_mode, modes, apply) = mode_controls(
            self.selection,
            self.selection_unavailable(),
            self.write_unavailable(),
        );
        let unavailable = match (&apply, selected_mode) {
            (ControlAvailability::Disabled { reason }, _) => {
                format!(
                    "ACTION UNAVAILABLE: {}",
                    reason.strip_prefix("UNAVAILABLE: ").unwrap_or(reason)
                )
            }
            (ControlAvailability::Enabled { .. }, Some(mode)) => format!(
                "ACTION AVAILABLE: APPLY REQUESTED MODE {} (wire {})",
                mode.display(),
                mode.wire()
            ),
            (ControlAvailability::Enabled { .. }, None) => {
                "ACTION UNAVAILABLE: SELECT A REQUESTED MODE".to_string()
            }
        };

        View {
            link_state: self.link_state.display().to_string(),
            link_detail: self.link_detail.clone(),
            projection_meta,
            binding,
            requested_value,
            resolved_value,
            active_project_value,
            working_set_value,
            resolution,
            refresh,
            recovery,
            health,
            selection,
            command_state,
            unavailable,
            selected_mode,
            url: if closed {
                ControlAvailability::enabled("LOCAL HOST WEBSOCKET ADDRESS")
            } else {
                ControlAvailability::disabled("DISCONNECT TO CHANGE THE HOST ADDRESS")
            },
            connect: match (&url_valid, closed) {
                (Err(reason), _) => ControlAvailability::disabled(format!("UNAVAILABLE: {reason}")),
                (Ok(_), false) => ControlAvailability::disabled(
                    "UNAVAILABLE: A CONNECTION ALREADY EXISTS · DISCONNECT FIRST",
                ),
                (Ok(_), true) => ControlAvailability::enabled("OPEN THE HOST CONNECTION"),
            },
            disconnect: if closed {
                ControlAvailability::disabled("UNAVAILABLE: NO CONNECTION TO CLOSE")
            } else {
                ControlAvailability::enabled("CLOSE THE HOST CONNECTION")
            },
            snapshot: if phase == LinkPhase::Open {
                ControlAvailability::enabled("REQUEST A FRESH SNAPSHOT AND RESYNCHRONIZE")
            } else {
                ControlAvailability::disabled("UNAVAILABLE: THE HOST CONNECTION IS NOT OPEN")
            },
            modes,
            apply,
        }
    }

    fn render(&mut self) {
        let view = self.build_view();
        let Some(bound) = self.bound.as_mut() else {
            return;
        };

        // Fixed copy is re-asserted before any Host content is written.
        bound.disclosure.set_text(RECALL_POLICY_DISCLOSURE);
        bound.disclosure.set_visible(true);

        bound.link_state.set_text(view.link_state.as_str());
        bound.link_detail.set_text(view.link_detail.as_str());
        bound
            .projection_meta
            .set_text(view.projection_meta.as_str());
        bound.binding.set_text(view.binding.as_str());
        bound
            .requested_value
            .set_text(view.requested_value.as_str());
        bound.resolved_value.set_text(view.resolved_value.as_str());
        bound
            .active_project_value
            .set_text(view.active_project_value.as_str());
        bound
            .working_set_value
            .set_text(view.working_set_value.as_str());
        bound.resolution.set_text(view.resolution.as_str());
        bound.refresh.set_text(view.refresh.as_str());
        bound.recovery.set_text(view.recovery.as_str());
        bound.health.set_text(view.health.as_str());
        bound.selection.set_text(view.selection.as_str());
        bound.command_state.set_text(view.command_state.as_str());
        bound.unavailable.set_text(view.unavailable.as_str());

        bound.url_field.set_editable(view.url.is_enabled());
        bound.url_field.set_tooltip_text(view.url.tooltip());

        bound
            .connect_button
            .set_disabled(!view.connect.is_enabled());
        bound
            .connect_button
            .set_tooltip_text(view.connect.tooltip());
        bound
            .disconnect_button
            .set_disabled(!view.disconnect.is_enabled());
        bound
            .disconnect_button
            .set_tooltip_text(view.disconnect.tooltip());
        bound
            .snapshot_button
            .set_disabled(!view.snapshot.is_enabled());
        bound
            .snapshot_button
            .set_tooltip_text(view.snapshot.tooltip());

        for (index, button) in bound.mode_buttons.iter_mut().enumerate() {
            let mode = RequestedMode::ALL[index];
            let selected = view.selected_mode == Some(mode);
            // Selection is carried by a mark and by the variation, never by hue
            // alone.
            let label = if selected {
                format!("◆ {}", mode.display())
            } else {
                format!("◇ {}", mode.display())
            };
            button.set_text(label.as_str());
            button.set_theme_type_variation(if selected {
                "AthanorTabActive"
            } else {
                "AthanorTab"
            });
            button.set_disabled(!view.modes.is_enabled());
            button.set_tooltip_text(view.modes.tooltip());
        }

        bound.apply_button.set_disabled(!view.apply.is_enabled());
        bound.apply_button.set_tooltip_text(view.apply.tooltip());

        self.dirty = false;
    }
}

// ---------------------------------------------------------------------------
// Operator actions
// ---------------------------------------------------------------------------

#[godot_api]
impl AthanorRecallPolicy {
    #[func]
    fn on_url_submitted(&mut self, _text: GString) {
        self.on_connect_pressed();
    }

    #[func]
    fn on_connect_pressed(&mut self) {
        if self.session_phase() != LinkPhase::Closed {
            self.render();
            return;
        }
        let url = self.current_url();
        let result = self
            .session
            .as_mut()
            .ok_or_else(|| "shared Host session not found".to_string())
            .and_then(|session| session.bind_mut().open(&url));
        match result {
            Ok(()) => {
                self.link_state = LinkState::Connecting;
                self.link_detail = format!("opening shared connection to {url}");
                self.command = CommandPhase::Idle;
            }
            Err(reason) => {
                self.link_state = LinkState::Idle;
                self.link_detail = format!("connection not started: {reason}");
            }
        }
        self.render();
    }

    #[func]
    fn on_disconnect_pressed(&mut self) {
        if self.session_phase() == LinkPhase::Closed {
            self.render();
            return;
        }
        if let Some(session) = self.session.as_mut() {
            session.bind_mut().close();
        }
        self.fail_pending("the operator closed the connection before Host acknowledgement");
        self.drop_projection();
        self.link_state = LinkState::Disconnected;
        self.link_detail = "shared connection closed by the operator".to_string();
        self.render();
    }

    #[func]
    fn on_snapshot_pressed(&mut self) {
        if self.session_phase() != LinkPhase::Open {
            self.render();
            return;
        }
        self.request_snapshot();
        self.render();
    }

    #[func]
    fn on_host_opened(&mut self) {
        self.link_state = LinkState::Connected;
        self.link_detail = format!(
            "shared connection opened to {}; waiting for Host bootstrap",
            self.session_url()
        );
        self.render();
    }

    #[func]
    fn on_host_closed(&mut self, detail: GString) {
        self.link_state = LinkState::Disconnected;
        self.link_detail = detail.to_string();
        self.fail_pending("the connection closed before Host acknowledgement");
        self.drop_projection();
        self.render();
    }

    #[func]
    fn on_host_malformed(&mut self, detail: GString) {
        self.link_detail = format!("packet discarded: {detail}");
        self.render();
    }

    #[func]
    fn on_host_unavailable(&mut self, detail: GString) {
        self.link_state = LinkState::Idle;
        self.link_detail = detail.to_string();
        self.fail_pending("the shared Host session became unavailable");
        self.drop_projection();
        self.render();
    }

    #[func]
    fn on_host_message(&mut self, envelope: VarDictionary) {
        if protocol::event_projection_id(&envelope).ok().as_deref()
            != Some(::protocol::RECALL_POLICY_PROJECTION_ID)
        {
            return;
        }
        match protocol::parse_inbound(&envelope) {
            Ok(inbound) => self.handle_inbound(inbound),
            Err(reason) => {
                self.link_state = LinkState::ProtocolRefused;
                self.link_detail = format!("envelope refused: {reason}");
                self.fail_pending("the Host envelope was refused before acknowledgement");
                self.drop_projection();
            }
        }
        self.dirty = true;
        self.render();
    }

    #[func]
    fn on_mode_auto_pressed(&mut self) {
        self.select_mode(RequestedMode::Auto);
    }

    #[func]
    fn on_mode_conversation_pressed(&mut self) {
        self.select_mode(RequestedMode::Conversation);
    }

    #[func]
    fn on_mode_work_pressed(&mut self) {
        self.select_mode(RequestedMode::Work);
    }

    #[func]
    fn on_mode_quiet_pressed(&mut self) {
        self.select_mode(RequestedMode::Quiet);
    }

    /// Authorises the one mutation. Proposing a mode and applying it stay
    /// separate operator acts.
    #[func]
    fn on_apply_pressed(&mut self) {
        if self.write_unavailable().is_some() {
            self.render();
            return;
        }
        let (Some(mode), Some(binding), Some(cursor)) = (
            self.selection,
            self.host_binding.clone(),
            self.cursor.clone(),
        ) else {
            self.render();
            return;
        };

        // A retry after a transport failure replays the same idempotency key so
        // the Host returns the existing result or a stable conflict.
        let reuse = match &self.command {
            CommandPhase::Failed {
                mode: failed_mode,
                idempotency_key,
                ..
            } if *failed_mode == mode => Some(idempotency_key.clone()),
            _ => None,
        };
        let idempotency_key = match reuse {
            Some(key) => key,
            None => self.new_session_identifier(),
        };

        let identity = self.new_identity(idempotency_key.clone());
        let correlation_id = identity.message_id.clone();
        let envelope =
            protocol::set_requested_mode_command(&binding, &identity, mode, cursor.version);

        self.command = match self.send_session(&envelope) {
            Ok(()) => CommandPhase::Pending {
                correlation_id,
                idempotency_key,
                mode,
                elapsed: 0.0,
            },
            Err(reason) => CommandPhase::Failed {
                mode,
                idempotency_key,
                reason,
            },
        };
        self.base_mut().set_process(true);
        self.render();
    }
}

impl AthanorRecallPolicy {
    fn select_mode(&mut self, mode: RequestedMode) {
        if self.selection_unavailable().is_some() {
            self.render();
            return;
        }
        self.selection = if self.selection == Some(mode) {
            None
        } else {
            Some(mode)
        };
        // A new proposal clears a stale outcome so pending, refused, failed and
        // acknowledged never blur together.
        let stale_outcome = matches!(
            self.command,
            CommandPhase::Refused { .. } | CommandPhase::Acknowledged { .. }
        );
        if stale_outcome {
            self.command = CommandPhase::Idle;
        }
        self.render();
    }
}

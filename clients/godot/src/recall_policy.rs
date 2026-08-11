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
use godot::classes::{Button, IPanelContainer, Label, LineEdit, Node, Os, PanelContainer};
use godot::obj::Inherits;
use godot::prelude::*;

use crate::host_link::{HostLink, LinkEvent, LinkPhase};
use crate::protocol::{
    self, CommandIdentity, HostBinding, Inbound, ProjectionCursor, RecallPolicyProjection,
    RequestedMode,
};

/// Fixed copy. Rendered before any Host content and never softened.
const DISCLOSURE: &str = "SEM AUTORIDADE · ESTE CLIENTE NÃO É AUTORIDADE DE MEMÓRIA, IDENTIDADE OU POLÍTICA · NENHUM ESTADO APARECE AQUI SEM SNAPSHOT AUTENTICADO DO ATHANOR HOST · HOUSE, ROOM, SPIRIT E SESSION VÊM SÓ DO SNAPSHOT DO HOST, NUNCA DO CONTEXTO SINTÉTICO DA CASCA NEM DA PASTA DE TRABALHO";

/// Placeholder for every value the Host has not stated.
const ABSENT: &str = "—";

const DEFAULT_HOST_URL: &str = protocol::DEFAULT_HOST_URL;

/// Bound wait for one command outcome before the operator regains the action.
const PENDING_TIMEOUT_SECONDS: f64 = 15.0;

// ---------------------------------------------------------------------------
// Local presentation state
// ---------------------------------------------------------------------------

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
            LinkState::Idle => "◇ SEM CONEXÃO",
            LinkState::Connecting => "◈ CONECTANDO",
            LinkState::Connected => "◆ CONECTADO · SEM SNAPSHOT",
            LinkState::Ready => "◆ SNAPSHOT APLICADO",
            LinkState::Disconnected => "◇ DESCONECTADO",
            LinkState::ProtocolRefused => "✕ PROTOCOLO RECUSADO",
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
    selected_modes: [bool; 4],
    url_editable: bool,
    url_reason: String,
    connect_enabled: bool,
    connect_reason: String,
    disconnect_enabled: bool,
    disconnect_reason: String,
    snapshot_enabled: bool,
    snapshot_reason: String,
    modes_enabled: bool,
    modes_reason: String,
    apply_enabled: bool,
    apply_reason: String,
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

    link: HostLink,
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
            host_url: GString::from(DEFAULT_HOST_URL),
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
            link: HostLink::new(),
            link_state: LinkState::Idle,
            link_detail: String::from("nenhuma conexão iniciada nesta sessão"),
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
        let token = Os::singleton()
            .get_environment("ATHANOR_HOST_TOKEN")
            .to_string();
        if token.trim().is_empty() {
            // Nothing to poll until the operator supplies a token and opens a link.
            self.base_mut().set_process(false);
            self.render();
        } else {
            self.on_connect_pressed();
        }
    }

    fn process(&mut self, delta: f64) {
        let events = self.link.poll();
        for event in events {
            self.handle_link_event(event);
        }

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
                "o Host não respondeu em {PENDING_TIMEOUT_SECONDS:.0} s"
            ));
        } else if ticked {
            self.dirty = true;
        }

        if self.dirty {
            self.render();
        }

        let idle = self.link.phase() == LinkPhase::Closed
            && !matches!(self.command, CommandPhase::Pending { .. });
        if idle {
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
                "AthanorRecallPolicy: vínculos de cena ausentes: {}",
                missing.join(", ")
            );
            self.bound = None;
            return;
        }

        self.bound = Some(Bound {
            disclosure: disclosure.expect("resolvido"),
            url_field: url_field.expect("resolvido"),
            connect_button: connect_button.expect("resolvido"),
            disconnect_button: disconnect_button.expect("resolvido"),
            snapshot_button: snapshot_button.expect("resolvido"),
            link_state: link_state.expect("resolvido"),
            link_detail: link_detail.expect("resolvido"),
            projection_meta: projection_meta.expect("resolvido"),
            binding: binding.expect("resolvido"),
            requested_value: requested_value.expect("resolvido"),
            resolved_value: resolved_value.expect("resolvido"),
            active_project_value: active_project_value.expect("resolvido"),
            working_set_value: working_set_value.expect("resolvido"),
            resolution: resolution.expect("resolvido"),
            refresh: refresh.expect("resolvido"),
            recovery: recovery.expect("resolvido"),
            health: health.expect("resolvido"),
            mode_buttons: vec![
                mode_auto.expect("resolvido"),
                mode_conversation.expect("resolvido"),
                mode_work.expect("resolvido"),
                mode_quiet.expect("resolvido"),
            ],
            selection: selection.expect("resolvido"),
            apply_button: apply_button.expect("resolvido"),
            command_state: command_state.expect("resolvido"),
            unavailable: unavailable.expect("resolvido"),
        });
    }

    fn wire_controls(&mut self) {
        let this = self.to_gd();
        let host_url = self.host_url.clone();
        let Some(bound) = self.bound.as_mut() else {
            return;
        };

        bound.disclosure.set_text(DISCLOSURE);

        if bound.url_field.get_text().to_string().trim().is_empty() {
            bound.url_field.set_text(&host_url);
        }
        bound.url_field.set_placeholder(DEFAULT_HOST_URL);
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
        let message_id = self.link.new_identifier();
        let idempotency_key = if idempotency_key.trim().is_empty() {
            self.link.new_identifier()
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
        match self.link.send(&envelope) {
            Ok(()) => {
                self.link_detail = format!("snapshot pedido a {}", self.link.url());
            }
            Err(reason) => {
                self.link_detail = format!("pedido de snapshot não enviado: {reason}");
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
        if let Err(reason) = self.link.send(&envelope) {
            self.link_detail = format!("confirmação de versão não enviada: {reason}");
        }
    }

    fn handle_link_event(&mut self, event: LinkEvent) {
        self.dirty = true;
        match event {
            LinkEvent::Opened => {
                self.link_state = LinkState::Connected;
                self.link_detail = format!("conexão aberta com {}", self.link.url());
                self.request_snapshot();
            }
            LinkEvent::Closed { detail } => {
                self.link_state = LinkState::Disconnected;
                self.link_detail = detail;
                self.fail_pending("a conexão foi encerrada antes da confirmação do Host");
                self.drop_projection();
            }
            LinkEvent::Malformed { detail } => {
                self.link_detail = format!("pacote descartado: {detail}");
            }
            LinkEvent::Message(envelope) => match protocol::parse_inbound(&envelope) {
                Ok(inbound) => self.handle_inbound(inbound),
                Err(reason) => {
                    self.link_state = LinkState::ProtocolRefused;
                    self.link_detail = format!("envelope recusado: {reason}");
                    self.fail_pending("o envelope do Host foi recusado antes da confirmação");
                    self.drop_projection();
                }
            },
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
                self.link_detail = format!("snapshot recebido de {}", self.link.url());
                self.send_acknowledge();
            }
            Inbound::Delta(delta) => {
                let applied = match (self.cursor.as_mut(), self.projection.as_mut()) {
                    (Some(cursor), Some(projection)) => {
                        protocol::apply_delta(cursor, projection, &delta)
                    }
                    _ => Err("delta recebida sem snapshot aplicado".to_string()),
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
                                .unwrap_or_else(|| "o Host não informou motivo".to_string()),
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
                                .unwrap_or_else(|| "o Host não informou motivo".to_string()),
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
            return Some(format!("ENDEREÇO DO HOST INVÁLIDO: {reason}"));
        }
        match self.link_state {
            LinkState::Idle => return Some("SEM CONEXÃO COM O HOST".to_string()),
            LinkState::Connecting => {
                return Some("AGUARDANDO ABERTURA DA CONEXÃO COM O HOST".to_string());
            }
            LinkState::Disconnected => {
                return Some("CONEXÃO ENCERRADA · RECONECTE PARA OBTER UM SNAPSHOT".to_string());
            }
            LinkState::ProtocolRefused => {
                return Some("PROTOCOLO RECUSADO · PEÇA UM SNAPSHOT VÁLIDO".to_string());
            }
            LinkState::Connected | LinkState::Ready => {}
        }
        if self.projection.is_none() {
            return Some("AGUARDANDO SNAPSHOT REAL DO HOST".to_string());
        }
        if matches!(self.command, CommandPhase::Pending { .. }) {
            return Some("COMANDO PENDENTE · AGUARDANDO CONFIRMAÇÃO DO HOST".to_string());
        }
        None
    }

    /// Reason the authorising control cannot be used, if any.
    fn write_unavailable(&self) -> Option<String> {
        if let Some(reason) = self.selection_unavailable() {
            return Some(reason);
        }
        let Some(selection) = self.selection else {
            return Some("SELECIONE UM MODO SOLICITADO ANTES DE APLICAR".to_string());
        };
        if let CommandPhase::Refused { mode, reason } = &self.command {
            if *mode == selection {
                return Some(format!("O HOST RECUSOU ESTE MODO: {reason}"));
            }
        }
        let projection = self.projection.as_ref()?;
        if projection.requested_mode == selection {
            return Some(format!(
                "O MODO SOLICITADO JÁ É {} (wire {})",
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
        let phase = self.link.phase();
        let closed = phase == LinkPhase::Closed;

        let projection_meta = match (self.cursor.as_ref(), self.projection.as_ref()) {
            (Some(cursor), Some(_)) => format!(
                "PROJEÇÃO {} · SCHEMA {} · SNAPSHOT {} · VERSÃO {} · SEQUÊNCIA {} · HASH {}",
                protocol::PROJECTION_ID,
                protocol::SCHEMA_VERSION,
                cursor.snapshot_id,
                cursor.version,
                cursor.sequence,
                cursor
                    .state_hash
                    .clone()
                    .unwrap_or_else(|| "não reafirmado desde a última delta".to_string())
            ),
            _ => format!(
                "PROJEÇÃO {} · SCHEMA {} · NENHUMA VERSÃO APLICADA",
                protocol::PROJECTION_ID,
                protocol::SCHEMA_VERSION
            ),
        };

        let binding = match self.host_binding.as_ref() {
            Some(binding) => format!(
                "VÍNCULO DO HOST · HOUSE {} · ROOM {} · SPIRIT {} · SESSION {} · SCOPE {} · VISIBILITY {} · AUTHORITY {}",
                binding.house_id,
                binding.room,
                binding.spirit,
                binding.session,
                binding.scope,
                binding.visibility,
                binding.authority_class
            ),
            None => "VÍNCULO DO HOST NÃO RECEBIDO · O CLIENTE NÃO DECLARA HOUSE, ROOM, SPIRIT NEM SESSION".to_string(),
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
                    format!("{} entradas", projection.working_set_entries),
                ),
                None => (
                    ABSENT.to_string(),
                    ABSENT.to_string(),
                    ABSENT.to_string(),
                    ABSENT.to_string(),
                ),
            };

        let resolution = match self.projection.as_ref() {
            Some(projection) => format!("MOTIVO DA RESOLUÇÃO: {}", projection.resolution_reason),
            None => "MOTIVO DA RESOLUÇÃO: sem snapshot do Host".to_string(),
        };

        let refresh = match self.projection.as_ref() {
            Some(projection) => {
                match (&projection.last_refresh_at, &projection.last_refresh_reason) {
                    (Some(at), Some(reason)) => format!("ÚLTIMO REFRESH {at} · {reason}"),
                    (Some(at), None) => format!("ÚLTIMO REFRESH {at} · sem motivo informado"),
                    (None, _) => "NENHUM REFRESH REGISTRADO".to_string(),
                }
            }
            None => format!("ÚLTIMO REFRESH {ABSENT}"),
        };

        let recovery = match self.projection.as_ref() {
            Some(projection) => {
                if projection.recovery_pending {
                    if projection.recovery_terms.is_empty() {
                        "RECUPERAÇÃO PENDENTE · SEM TERMOS INFORMADOS".to_string()
                    } else {
                        format!(
                            "RECUPERAÇÃO PENDENTE · TERMOS: {}",
                            projection.recovery_terms.join(", ")
                        )
                    }
                } else {
                    "SEM RECUPERAÇÃO PENDENTE".to_string()
                }
            }
            None => format!("RECUPERAÇÃO {ABSENT}"),
        };

        // Subsystem health stays on its own channel, never merged with the
        // transport state above.
        let health = match self.projection.as_ref() {
            Some(projection) => match &projection.degraded {
                Some(degraded) => format!("SUBSISTEMA DEGRADADO: {degraded}"),
                None => "NENHUMA DEGRADAÇÃO INFORMADA".to_string(),
            },
            None => format!("DEGRADAÇÃO {ABSENT}"),
        };

        let selection = match self.selection {
            Some(mode) => format!(
                "SELEÇÃO PROPOSTA: {} (wire {})",
                mode.display(),
                mode.wire()
            ),
            None => "SELEÇÃO PROPOSTA: NENHUMA".to_string(),
        };

        let command_state = match &self.command {
            CommandPhase::Idle => "SEM COMANDO ENVIADO".to_string(),
            CommandPhase::Pending { mode, elapsed, .. } => format!(
                "⧗ PENDENTE · {} · {:.0} s sem confirmação",
                mode.display(),
                elapsed.floor()
            ),
            CommandPhase::Acknowledged { mode } => {
                format!("◆ CONFIRMADO PELO HOST · {}", mode.display())
            }
            CommandPhase::Refused { mode, reason } => {
                format!("✕ RECUSADO · {} · {reason}", mode.display())
            }
            CommandPhase::Failed { mode, reason, .. } => {
                format!("⚠ FALHOU · {} · {reason}", mode.display())
            }
        };

        let apply_reason_text = self.write_unavailable();
        let modes_reason_text = self.selection_unavailable();

        let mut selected_modes = [false; 4];
        if let Some(selected) = self.selection {
            for (index, mode) in RequestedMode::ALL.into_iter().enumerate() {
                selected_modes[index] = mode == selected;
            }
        }

        let unavailable = match (&apply_reason_text, self.selection) {
            (Some(reason), _) => format!("AÇÃO INDISPONÍVEL: {reason}"),
            (None, Some(mode)) => format!(
                "AÇÃO DISPONÍVEL: APLICAR MODO SOLICITADO {} (wire {})",
                mode.display(),
                mode.wire()
            ),
            (None, None) => "AÇÃO INDISPONÍVEL: SELECIONE UM MODO SOLICITADO".to_string(),
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
            selected_modes,
            url_editable: closed,
            url_reason: if closed {
                "ENDEREÇO WEBSOCKET DO HOST LOCAL".to_string()
            } else {
                "DESCONECTE PARA ALTERAR O ENDEREÇO DO HOST".to_string()
            },
            connect_enabled: closed && url_valid.is_ok(),
            connect_reason: match (&url_valid, closed) {
                (Err(reason), _) => format!("INDISPONÍVEL: {reason}"),
                (Ok(_), false) => {
                    "INDISPONÍVEL: JÁ EXISTE UMA CONEXÃO · DESCONECTE PRIMEIRO".to_string()
                }
                (Ok(_), true) => "ABRIR A CONEXÃO COM O HOST".to_string(),
            },
            disconnect_enabled: !closed,
            disconnect_reason: if closed {
                "INDISPONÍVEL: NÃO HÁ CONEXÃO PARA ENCERRAR".to_string()
            } else {
                "ENCERRAR A CONEXÃO COM O HOST".to_string()
            },
            snapshot_enabled: phase == LinkPhase::Open,
            snapshot_reason: if phase == LinkPhase::Open {
                "PEDIR UM SNAPSHOT NOVO E RESSINCRONIZAR".to_string()
            } else {
                "INDISPONÍVEL: A CONEXÃO COM O HOST NÃO ESTÁ ABERTA".to_string()
            },
            modes_enabled: modes_reason_text.is_none(),
            modes_reason: match &modes_reason_text {
                Some(reason) => format!("INDISPONÍVEL: {reason}"),
                None => "PROPOR UM MODO SOLICITADO · A APLICAÇÃO É UM PASSO SEPARADO".to_string(),
            },
            apply_enabled: apply_reason_text.is_none(),
            apply_reason: match &apply_reason_text {
                Some(reason) => format!("INDISPONÍVEL: {reason}"),
                None => "ENVIAR O COMANDO DE MODO SOLICITADO AO HOST".to_string(),
            },
        }
    }

    fn render(&mut self) {
        let view = self.build_view();
        let Some(bound) = self.bound.as_mut() else {
            return;
        };

        // Fixed copy is re-asserted before any Host content is written.
        bound.disclosure.set_text(DISCLOSURE);
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

        bound.url_field.set_editable(view.url_editable);
        bound.url_field.set_tooltip_text(view.url_reason.as_str());

        bound.connect_button.set_disabled(!view.connect_enabled);
        bound
            .connect_button
            .set_tooltip_text(view.connect_reason.as_str());
        bound
            .disconnect_button
            .set_disabled(!view.disconnect_enabled);
        bound
            .disconnect_button
            .set_tooltip_text(view.disconnect_reason.as_str());
        bound.snapshot_button.set_disabled(!view.snapshot_enabled);
        bound
            .snapshot_button
            .set_tooltip_text(view.snapshot_reason.as_str());

        for (index, button) in bound.mode_buttons.iter_mut().enumerate() {
            let mode = RequestedMode::ALL[index];
            let selected = view.selected_modes[index];
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
            button.set_disabled(!view.modes_enabled);
            button.set_tooltip_text(view.modes_reason.as_str());
        }

        bound.apply_button.set_disabled(!view.apply_enabled);
        bound
            .apply_button
            .set_tooltip_text(view.apply_reason.as_str());

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
        if self.link.phase() != LinkPhase::Closed {
            self.render();
            return;
        }
        let url = self.current_url();
        let bearer_token = Os::singleton()
            .get_environment("ATHANOR_HOST_TOKEN")
            .to_string();
        match self.link.open(&url, &bearer_token) {
            Ok(()) => {
                self.link_state = LinkState::Connecting;
                self.link_detail = format!("abrindo conexão com {url}");
                self.command = CommandPhase::Idle;
                self.base_mut().set_process(true);
            }
            Err(reason) => {
                self.link_state = LinkState::Idle;
                self.link_detail = format!("conexão não iniciada: {reason}");
            }
        }
        self.render();
    }

    #[func]
    fn on_disconnect_pressed(&mut self) {
        if self.link.phase() == LinkPhase::Closed {
            self.render();
            return;
        }
        self.link.close();
        self.fail_pending("o operador encerrou a conexão antes da confirmação do Host");
        self.drop_projection();
        self.link_state = LinkState::Disconnected;
        self.link_detail = "conexão encerrada pelo operador".to_string();
        self.render();
    }

    #[func]
    fn on_snapshot_pressed(&mut self) {
        if self.link.phase() != LinkPhase::Open {
            self.render();
            return;
        }
        self.request_snapshot();
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
            None => self.link.new_identifier(),
        };

        let identity = self.new_identity(idempotency_key.clone());
        let correlation_id = identity.message_id.clone();
        let envelope =
            protocol::set_requested_mode_command(&binding, &identity, mode, cursor.version);

        self.command = match self.link.send(&envelope) {
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

// [gui/host/session] [transport/websocket] [security/auth]

use godot::classes::{INode, Node, Os};
use godot::prelude::*;

use crate::host_link::{HostLink, LinkEvent, LinkPhase};
use crate::protocol::{self, HostBinding};

#[derive(GodotClass)]
#[class(base = Node)]
pub struct AthanorHostSession {
    #[export]
    host_url: GString,

    link: HostLink,
    binding: Option<HostBinding>,
    base: Base<Node>,
}

#[godot_api]
impl INode for AthanorHostSession {
    fn init(base: Base<Node>) -> Self {
        Self {
            host_url: GString::new(),
            link: HostLink::new(),
            binding: None,
            base,
        }
    }

    fn ready(&mut self) {
        let token = Os::singleton()
            .get_environment("ATHANOR_HOST_TOKEN")
            .to_string();
        if token.trim().is_empty() {
            self.base_mut().set_process(false);
            self.base_mut().emit_signal(
                "unavailable",
                &["ATHANOR_HOST_TOKEN is missing; no connection was invented".to_variant()],
            );
            return;
        }
        let configured = Os::singleton()
            .get_environment("ATHANOR_HOST_WS_URL")
            .to_string();
        let url = if configured.trim().is_empty() {
            self.host_url.to_string()
        } else {
            configured
        };
        if let Err(reason) = self.open(&url) {
            self.base_mut()
                .emit_signal("unavailable", &[reason.to_variant()]);
        }
    }

    fn process(&mut self, _delta: f64) {
        for event in self.link.poll() {
            match event {
                LinkEvent::Opened => {
                    let message_id = self.link.new_identifier();
                    let bootstrap = protocol::subscribe_command(
                        &protocol::CommandIdentity {
                            message_id: message_id.clone(),
                            idempotency_key: message_id.clone(),
                            causation_id: String::new(),
                        },
                        None,
                    );
                    if let Err(reason) = self.link.send(&bootstrap) {
                        self.base_mut()
                            .emit_signal("unavailable", &[reason.to_variant()]);
                    } else {
                        self.base_mut().emit_signal("opened", &[]);
                    }
                }
                LinkEvent::Closed { detail } => {
                    self.binding = None;
                    self.base_mut()
                        .emit_signal("closed", &[detail.to_variant()]);
                }
                LinkEvent::Malformed { detail } => {
                    self.base_mut()
                        .emit_signal("malformed", &[detail.to_variant()]);
                }
                LinkEvent::Message(envelope) => {
                    if let Ok(binding) = HostBinding::parse(&envelope) {
                        self.binding = Some(binding);
                    }
                    self.base_mut()
                        .emit_signal("message", &[envelope.to_variant()]);
                }
            }
        }
        if self.link.phase() == LinkPhase::Closed {
            self.base_mut().set_process(false);
        }
    }
}

#[godot_api]
impl AthanorHostSession {
    #[signal]
    fn opened();

    #[signal]
    fn closed(detail: GString);

    #[signal]
    fn malformed(detail: GString);

    #[signal]
    fn unavailable(detail: GString);

    #[signal]
    fn message(envelope: VarDictionary);

    #[func]
    fn reconnect(&mut self, url: GString) -> bool {
        match self.open(&url.to_string()) {
            Ok(()) => true,
            Err(reason) => {
                self.base_mut()
                    .emit_signal("unavailable", &[reason.to_variant()]);
                false
            }
        }
    }

    #[func]
    fn disconnect_host(&mut self) {
        self.close();
    }
}

impl AthanorHostSession {
    pub fn phase(&self) -> LinkPhase {
        self.link.phase()
    }

    pub fn url(&self) -> &str {
        self.link.url()
    }

    pub fn binding(&self) -> Option<HostBinding> {
        self.binding.clone()
    }

    pub fn new_identifier(&mut self) -> String {
        self.link.new_identifier()
    }

    pub fn send(&mut self, envelope: &VarDictionary) -> Result<(), String> {
        self.link.send(envelope)
    }

    pub fn open(&mut self, url: &str) -> Result<(), String> {
        if self.link.phase() != LinkPhase::Closed {
            self.link.close();
        }
        self.binding = None;
        let token = Os::singleton()
            .get_environment("ATHANOR_HOST_TOKEN")
            .to_string();
        self.link.open(url, &token)?;
        self.base_mut().set_process(true);
        Ok(())
    }

    pub fn close(&mut self) {
        self.link.close();
        self.binding = None;
        self.base_mut().set_process(false);
    }
}

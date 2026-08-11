//! Configurable WebSocket link to a local Athanor Host.
//!
//! This is the only outbound network surface in the client. It refuses any
//! scheme other than `ws://` and `wss://`, so no direct PostgreSQL, NATS,
//! provider, or harness path can be configured through it.

use godot::classes::web_socket_peer::State;
use godot::classes::{Json, RandomNumberGenerator, WebSocketPeer};
use godot::global::Error as GodotError;
use godot::prelude::*;

/// Transport phase, independent from projection readiness and from subsystem
/// health.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum LinkPhase {
    Closed,
    Connecting,
    Open,
}

/// One observable transport fact produced by a poll.
pub enum LinkEvent {
    Opened,
    Closed { detail: String },
    Message(VarDictionary),
    Malformed { detail: String },
}

pub struct HostLink {
    peer: Gd<WebSocketPeer>,
    rng: Gd<RandomNumberGenerator>,
    phase: LinkPhase,
    url: String,
}

impl HostLink {
    pub fn new() -> Self {
        let mut rng = RandomNumberGenerator::new_gd();
        rng.randomize();
        Self {
            peer: WebSocketPeer::new_gd(),
            rng,
            phase: LinkPhase::Closed,
            url: String::new(),
        }
    }

    pub fn phase(&self) -> LinkPhase {
        self.phase
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Rejects anything that is not a WebSocket address before touching the
    /// peer, so a misconfigured field cannot become a silent parallel path.
    pub fn validate_url(url: &str) -> Result<(), String> {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err("endereço do Host vazio".to_string());
        }
        if !trimmed.starts_with("ws://") && !trimmed.starts_with("wss://") {
            return Err("endereço do Host precisa começar com ws:// ou wss://".to_string());
        }
        if trimmed.len() <= "wss://".len() {
            return Err("endereço do Host não tem host nem porta".to_string());
        }
        Ok(())
    }

    pub fn open(&mut self, url: &str, bearer_token: &str) -> Result<(), String> {
        Self::validate_url(url)?;
        let bearer_token = bearer_token.trim();
        if bearer_token.is_empty() {
            return Err(
                "ATHANOR_HOST_TOKEN vazio; o cliente não abre conexão sem autenticação".to_string(),
            );
        }
        let mut headers = PackedStringArray::new();
        let authorization = format!("Authorization: Bearer {bearer_token}");
        headers.push(&GString::from(authorization.as_str()));
        self.peer.set_handshake_headers(&headers);
        let trimmed = url.trim().to_string();
        let error = self.peer.connect_to_url(&trimmed);
        if error != GodotError::OK {
            return Err(format!("Godot recusou a conexão: {error:?}"));
        }
        self.url = trimmed;
        self.phase = LinkPhase::Connecting;
        Ok(())
    }

    pub fn close(&mut self) {
        self.peer.close();
        self.phase = LinkPhase::Closed;
    }

    pub fn send(&mut self, envelope: &VarDictionary) -> Result<(), String> {
        if self.phase != LinkPhase::Open {
            return Err("a conexão com o Host não está aberta".to_string());
        }
        let text = Json::stringify(&envelope.to_variant());
        let error = self.peer.send_text(&text);
        if error != GodotError::OK {
            return Err(format!("Godot recusou o envio: {error:?}"));
        }
        Ok(())
    }

    /// Drains transport state and every queued packet. Never interprets an
    /// envelope; that stays in `protocol`.
    pub fn poll(&mut self) -> Vec<LinkEvent> {
        let mut events = Vec::new();
        self.peer.poll();

        // Godot's engine enums are generated as constant-carrying structs, so
        // compare them by value instead of pattern-matching them.
        let state = self.peer.get_ready_state();
        if state == State::CONNECTING {
            self.phase = LinkPhase::Connecting;
        } else if state == State::OPEN {
            if self.phase != LinkPhase::Open {
                self.phase = LinkPhase::Open;
                events.push(LinkEvent::Opened);
            }
        } else if state != State::CLOSING {
            if self.phase != LinkPhase::Closed {
                self.phase = LinkPhase::Closed;
                let code = self.peer.get_close_code();
                let reason = self.peer.get_close_reason().to_string();
                let detail = if reason.is_empty() {
                    format!("conexão encerrada (código {code})")
                } else {
                    format!("conexão encerrada (código {code}): {reason}")
                };
                events.push(LinkEvent::Closed { detail });
            }
            return events;
        }

        while self.peer.get_available_packet_count() > 0 {
            let packet = self.peer.get_packet();
            let text = packet.get_string_from_utf8();
            if text.to_string().is_empty() {
                events.push(LinkEvent::Malformed {
                    detail: "pacote vazio ou não UTF-8".to_string(),
                });
                continue;
            }
            let parsed = Json::parse_string(&text);
            match parsed.try_to::<VarDictionary>() {
                Ok(dictionary) => events.push(LinkEvent::Message(dictionary)),
                Err(_) => events.push(LinkEvent::Malformed {
                    detail: "pacote não é um envelope JSON".to_string(),
                }),
            }
        }

        events
    }

    /// 128-bit hex identifier for message and idempotency identity.
    pub fn new_identifier(&mut self) -> String {
        let mut hex = String::with_capacity(32);
        for _ in 0..4 {
            hex.push_str(&format!("{:08x}", self.rng.randi() as u32));
        }
        hex
    }
}

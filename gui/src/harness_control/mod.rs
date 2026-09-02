//! Authenticated loopback control client for the managed harness registry.

use std::io::{BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::time::Duration;

use ::protocol::harness::{
    HARNESS_CONTROL_FORMAT, HarnessCommand, HarnessControlRequest, HarnessControlResponse,
};
use godot::prelude::*;

const IO_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(GodotClass)]
#[class(base = Node)]
pub struct AthanorHarnessControl {
    address: String,
    token: String,
    last_error_kind: String,
    last_error_detail: String,
    base: Base<Node>,
}

#[godot_api]
impl INode for AthanorHarnessControl {
    fn init(base: Base<Node>) -> Self {
        Self {
            address: std::env::var("ATHANOR_CONTROL_ADDR").unwrap_or_default(),
            token: std::env::var("ATHANOR_CONTROL_TOKEN").unwrap_or_default(),
            last_error_kind: "unavailable".into(),
            last_error_detail: "managed control is not configured".into(),
            base,
        }
    }
}

#[godot_api]
impl AthanorHarnessControl {
    #[func]
    fn last_error_kind(&self) -> GString {
        GString::from(self.last_error_kind.as_str())
    }

    #[func]
    fn last_error_detail(&self) -> GString {
        GString::from(self.last_error_detail.as_str())
    }

    #[func]
    fn configured(&self) -> bool {
        !self.address.trim().is_empty() && !self.token.trim().is_empty()
    }

    #[func]
    fn list(&mut self) -> GString {
        self.request(HarnessCommand::List {})
    }

    #[func]
    fn start(&mut self, harness_id: GString) -> GString {
        self.request(HarnessCommand::Start {
            harness_id: harness_id.to_string(),
        })
    }

    #[func]
    fn stop(&mut self, harness_id: GString) -> GString {
        self.request(HarnessCommand::Stop {
            harness_id: harness_id.to_string(),
        })
    }

    #[func]
    fn restart(&mut self, harness_id: GString) -> GString {
        self.request(HarnessCommand::Restart {
            harness_id: harness_id.to_string(),
        })
    }

    fn request(&mut self, command: HarnessCommand) -> GString {
        self.last_error_kind.clear();
        self.last_error_detail.clear();

        if self.address.trim().is_empty() {
            return self.fail("unavailable", "ATHANOR_CONTROL_ADDR is missing");
        }
        if self.token.trim().is_empty() {
            return self.fail("unavailable", "ATHANOR_CONTROL_TOKEN is missing");
        }
        let address = match self.address.parse::<SocketAddr>() {
            Ok(address) if address.ip().is_loopback() => address,
            Ok(_) => return self.fail("unavailable", "control address must be loopback"),
            Err(error) => {
                return self.fail("unavailable", &format!("invalid control address: {error}"));
            }
        };

        let request = HarnessControlRequest {
            format: HARNESS_CONTROL_FORMAT,
            request_id: uuid::Uuid::new_v4().to_string(),
            token: self.token.clone(),
            command,
        };
        if let Err(reason) = request.validate() {
            return self.fail("malformed", &format!("request refused locally: {reason}"));
        }
        let mut stream = match std::net::TcpStream::connect_timeout(&address, IO_TIMEOUT) {
            Ok(stream) => stream,
            Err(error) => {
                return self.fail(
                    "unavailable",
                    &format!("control connection refused: {error}"),
                );
            }
        };
        let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
        let encoded = match serde_json::to_vec(&request) {
            Ok(encoded) => encoded,
            Err(error) => {
                return self.fail("malformed", &format!("request encoding failed: {error}"));
            }
        };
        if let Err(error) = stream
            .write_all(&encoded)
            .and_then(|_| stream.write_all(b"\n"))
        {
            return self.fail("unavailable", &format!("control write failed: {error}"));
        }

        let mut line = Vec::new();
        let mut reader = BufReader::new(stream);
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => return self.fail("unavailable", "control owner closed without a response"),
            Ok(size) if size > MAX_RESPONSE_BYTES => {
                return self.fail("malformed", "control response exceeded the size limit");
            }
            Err(error) => {
                return self.fail("unavailable", &format!("control read failed: {error}"));
            }
            Ok(_) => {}
        }
        let response: HarnessControlResponse = match serde_json::from_slice(&line) {
            Ok(response) => response,
            Err(error) => {
                return self.fail("malformed", &format!("malformed control response: {error}"));
            }
        };
        if let Err(error) = response.validate() {
            return self.fail("malformed", &format!("invalid control response: {error}"));
        }
        let payload = match serde_json::to_string(&response) {
            Ok(payload) => payload,
            Err(error) => {
                return self.fail("malformed", &format!("response encoding failed: {error}"));
            }
        };
        if !response.ok {
            self.last_error_kind = "refused".into();
            self.last_error_detail = response
                .error
                .unwrap_or_else(|| "control owner refused the request".into());
        } else {
            self.last_error_detail = "managed control response received".into();
        }
        GString::from(payload.as_str())
    }

    fn fail(&mut self, kind: &str, detail: &str) -> GString {
        self.last_error_kind = kind.into();
        self.last_error_detail = detail.into();
        GString::new()
    }
}

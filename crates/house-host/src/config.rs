use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

pub use house_protocol::DEFAULT_HOST_WS_PATH as DEFAULT_WS_PATH;
pub const DEFAULT_BIND: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8787);

#[derive(Clone, Debug)]
pub struct HostConfig {
    pub bind: SocketAddr,
    pub ws_path: String,
    pub bearer_token: String,
    pub room_dir: PathBuf,
    pub state_dir: PathBuf,
    pub house_id: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub recipient: String,
    pub akasha_enabled: bool,
    pub nats_url: Option<String>,
}

impl HostConfig {
    pub fn from_env() -> Result<Self, String> {
        let bind = optional("ATHANOR_HOST_BIND")
            .map(|value| value.parse::<SocketAddr>())
            .transpose()
            .map_err(|error| format!("ATHANOR_HOST_BIND is invalid: {error}"))?
            .unwrap_or(DEFAULT_BIND);
        if !bind.ip().is_loopback() {
            return Err("ATHANOR_HOST_BIND must be loopback for this Host slice".into());
        }
        let ws_path =
            optional("ATHANOR_HOST_WS_PATH").unwrap_or_else(|| DEFAULT_WS_PATH.to_owned());
        validate_ws_path(&ws_path)?;
        let bearer_token = required("ATHANOR_HOST_TOKEN")?;
        let room_dir = PathBuf::from(required("ATHANOR_HOST_ROOM_DIR")?);
        let state_dir = PathBuf::from(required("ATHANOR_HOST_STATE_DIR")?);
        let house_id = required("ATHANOR_HOST_HOUSE_ID")?;
        let room = required("ATHANOR_HOST_ROOM")?;
        if !is_safe_room(&room) {
            return Err("ATHANOR_HOST_ROOM must be a safe non-reserved room key".into());
        }
        let spirit = required("ATHANOR_HOST_SPIRIT")?;
        let session = required("ATHANOR_HOST_SESSION")?;
        let recipient =
            optional("ATHANOR_HOST_RECIPIENT").unwrap_or_else(|| "house-host".to_owned());
        let nats_url = optional("SOLARISAEL_NATS_URL");
        let akasha_enabled = optional("DATABASE_URL").is_some() || nats_url.is_some();
        Ok(Self {
            bind,
            ws_path,
            bearer_token,
            room_dir,
            state_dir,
            house_id,
            room,
            spirit,
            session,
            recipient,
            akasha_enabled,
            nats_url,
        })
    }

    pub fn room_state_path(&self) -> PathBuf {
        self.room_dir
            .join(".omp")
            .join("runtime")
            .join("solarisael-house-state.json")
    }

    pub fn scope(&self) -> String {
        format!("room:{}:recall_policy", self.room)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.bind.ip().is_loopback() {
            return Err("Host bind address must be loopback".into());
        }
        validate_ws_path(&self.ws_path)?;
        if self.bearer_token.trim().is_empty() {
            return Err("Host bearer token must not be blank".into());
        }
        if !is_safe_room(&self.room) {
            return Err("Host room must be a safe non-reserved room key".into());
        }
        if let Some(url) = &self.nats_url {
            if url.len() > 2048
                || !url.starts_with("nats://")
                || url.bytes().any(|byte| byte.is_ascii_whitespace())
            {
                return Err("Host SOLARISAEL_NATS_URL must be one bounded nats:// URL".into());
            }
        }
        for (name, value) in [
            ("house_id", &self.house_id),
            ("spirit", &self.spirit),
            ("session", &self.session),
            ("recipient", &self.recipient),
        ] {
            if value.trim().is_empty() {
                return Err(format!("Host {name} must not be blank"));
            }
        }
        Ok(())
    }
}

fn optional(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn required(name: &str) -> Result<String, String> {
    optional(name).ok_or_else(|| format!("{name} must be set and nonblank"))
}

fn validate_ws_path(path: &str) -> Result<(), String> {
    if !path.starts_with('/') || path.contains('?') || path.contains('#') || path.contains("//") {
        return Err(
            "Host WebSocket path must be one absolute path without query or fragment".into(),
        );
    }
    Ok(())
}

fn is_safe_room(value: &str) -> bool {
    value != "house"
        && !value.is_empty()
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' | b'0'..=b'9' => true,
            b'-' => index > 0 && index + 1 < value.len(),
            _ => false,
        })
        && !value.contains("--")
}

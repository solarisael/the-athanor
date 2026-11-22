use std::net::SocketAddr;
use std::path::PathBuf;

pub const KNOCK_AUTONOMY_ENV: &str = "ATHANOR_HOST_KNOCK_AUTONOMY";
// [host/wire/recipient] [protocol/command/binding]
pub(crate) const HOST_RECIPIENT: &str = "house-host";

/// Host-owned autonomy for Hallway Knock coordination.
///
/// Autonomy is a Host property, never a caller property: a bearer token grants
/// access to this Host, not permission to act as another room. Absent or unset
/// configuration means [`KnockAutonomy::Claim`] for every room; an operator may
/// explicitly set `ATHANOR_HOST_KNOCK_AUTONOMY=off` to disable autonomous claims.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum KnockAutonomy {
    Off,
    #[default]
    Claim,
}

impl KnockAutonomy {
    /// Exact, case-sensitive parsing. Anything else is a startup error rather
    /// than a silent downgrade, so a typo can never be read as consent.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "off" => Ok(Self::Off),
            "claim" => Ok(Self::Claim),
            other => Err(format!(
                "{KNOCK_AUTONOMY_ENV} must be exactly \"off\" or \"claim\"; got {other:?}"
            )),
        }
    }

    /// Absent configuration enables bounded claims for every room; a present
    /// value must still be exact so an attempted opt-out cannot fail silently.
    pub fn from_optional(value: Option<&str>) -> Result<Self, String> {
        match value {
            None => Ok(Self::Claim),
            Some(value) => Self::parse(value),
        }
    }

    pub fn claims_enabled(self) -> bool {
        matches!(self, Self::Claim)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Claim => "claim",
        }
    }
}

/// The settings that every room of one Host shares. A Host can start with
/// zero rooms, so these settings do not come from a room.
#[derive(Clone, Debug)]
pub struct HouseConfig {
    pub bind: SocketAddr,
    pub bearer_token: String,
    pub house_id: String,
    pub database_url: Option<String>,
    pub nats_url: Option<String>,
    pub nats_auth: Option<origami::cranes::broker::NatsAuth>,
}

impl HouseConfig {
    // [host/config/validation] [security/loopback]
    pub fn validate(&self) -> Result<(), String> {
        validate_house(
            self.bind,
            &self.bearer_token,
            &self.house_id,
            self.nats_url.as_deref(),
        )
    }

    /// The first House setting that the room does not share, if any.
    pub(crate) fn disagreement(&self, room: &HostConfig) -> Option<&'static str> {
        [
            ("bind address", self.bind == room.bind),
            ("bearer token", self.bearer_token == room.bearer_token),
            ("house identifier", self.house_id == room.house_id),
            ("DATABASE_URL", self.database_url == room.database_url),
            ("ATHANOR_NATS_URL", self.nats_url == room.nats_url),
            ("NATS credentials", self.nats_auth == room.nats_auth),
        ]
        .into_iter()
        .find_map(|(setting, agrees)| (!agrees).then_some(setting))
    }
}

fn validate_house(
    bind: SocketAddr,
    bearer_token: &str,
    house_id: &str,
    nats_url: Option<&str>,
) -> Result<(), String> {
    if !bind.ip().is_loopback() {
        return Err("Host bind address must be loopback".into());
    }
    if bearer_token.trim().is_empty() {
        return Err("Host bearer token must not be blank".into());
    }
    if house_id.trim().is_empty() {
        return Err("Host house_id must not be blank".into());
    }
    if let Some(url) = nats_url
        && (url.len() > 2048
            || !url.starts_with("nats://")
            || url.bytes().any(|byte| byte.is_ascii_whitespace()))
    {
        return Err("Host ATHANOR_NATS_URL must be one bounded nats:// URL".into());
    }

    Ok(())
}

#[derive(Clone, Debug)]
pub struct HostConfig {
    pub bind: SocketAddr,
    pub bearer_token: String,
    pub room_dir: PathBuf,
    pub state_dir: PathBuf,
    pub house_id: String,
    pub room: String,
    pub spirit: String,
    pub session: String,
    pub database_url: Option<String>,
    pub nats_url: Option<String>,
    pub nats_auth: Option<origami::cranes::broker::NatsAuth>,
    pub knock_autonomy: KnockAutonomy,
    /// The person this Host process speaks for. The Host stamps it on every
    /// memory write and recall; no caller can name another one.
    pub operator: Option<String>,
}

impl HostConfig {
    pub fn room_state_path(&self) -> PathBuf {
        self.room_dir
            .join(".omp")
            .join("runtime")
            .join("athanor-house-state.json")
    }

    pub fn scope(&self) -> String {
        format!("room:{}:recall_policy", self.room)
    }

    pub fn akasha_enabled(&self) -> bool {
        self.database_url.is_some() || self.nats_url.is_some()
    }

    // [host/routing] [protocol/room/key]
    pub(crate) fn room_path(&self) -> String {
        format!("{}{}", protocol::HOST_ROOM_PATH_PREFIX, self.room)
    }

    // [host/config/validation] [security/loopback] [protocol/room/key]
    pub fn validate(&self) -> Result<(), String> {
        validate_house(
            self.bind,
            &self.bearer_token,
            &self.house_id,
            self.nats_url.as_deref(),
        )?;
        if !protocol::is_safe_room_key(&self.room) {
            return Err("Host room must be a safe non-reserved room key".into());
        }
        if let Some(operator) = &self.operator
            && hearth::OperatorKey::new(operator.as_str()).is_err()
        {
            return Err("Host operator must be a safe operator key".into());
        }
        for (name, value) in [("spirit", &self.spirit), ("session", &self.session)] {
            if value.trim().is_empty() {
                return Err(format!("Host {name} must not be blank"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{KNOCK_AUTONOMY_ENV, KnockAutonomy};

    #[test]
    fn absent_knock_autonomy_claims_for_every_room() {
        let autonomy = KnockAutonomy::from_optional(None).expect("absent autonomy is valid");
        assert_eq!(autonomy, KnockAutonomy::Claim);
        assert_eq!(autonomy, KnockAutonomy::default());
        assert!(autonomy.claims_enabled());
        assert_eq!(autonomy.as_str(), "claim");
    }

    #[test]
    fn only_the_exact_claim_value_enables_autonomy() {
        let autonomy = KnockAutonomy::from_optional(Some("claim")).expect("claim is valid");
        assert_eq!(autonomy, KnockAutonomy::Claim);
        assert!(autonomy.claims_enabled());
        assert_eq!(autonomy.as_str(), "claim");

        let off = KnockAutonomy::from_optional(Some("off")).expect("off is valid");
        assert_eq!(off, KnockAutonomy::Off);
        assert!(!off.claims_enabled());
    }

    #[test]
    fn near_miss_autonomy_values_are_startup_errors_never_silent_consent() {
        for value in [
            "Claim", "CLAIM", " claim", "claim ", "on", "true", "1", "yes", "",
        ] {
            let error = KnockAutonomy::from_optional(Some(value))
                .expect_err("near-miss autonomy value must be rejected");
            assert!(
                error.contains(KNOCK_AUTONOMY_ENV),
                "refusal must name the variable: {error}"
            );
        }
    }
}

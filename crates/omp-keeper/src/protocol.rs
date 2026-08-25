// mirror: house-protocol restart door lands in the integrator's merge
#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::fmt;

pub const PROTOCOL_VERSION: u8 = 1;

pub const METHOD_RESTART_REQUEST: &str = "restart_request";
pub const METHOD_RESTART_CLAIM: &str = "restart_claim";
pub const METHOD_RESTART_TRANSITION: &str = "restart_transition";
pub const METHOD_RESTART_VERIFY: &str = "restart_verify";
pub const METHOD_RESTART_STATUS: &str = "restart_status";

pub const REQUESTED_TTL_SECS: u64 = 300;
pub const EXITING_DEADLINE_SECS: u64 = 60;
pub const RELAUNCHING_DEADLINE_SECS: u64 = 120;
pub const RELAUNCH_ATTEMPTS: u32 = 2;
pub const STORM_GUARD_EXITING_PER_HOUR: u32 = 3;
pub const STORM_REFUSAL_CODE: &str = "restart_storm";
pub const ARMED_EXIT_CODE: i32 = 87;

pub const STATE_REQUESTED: &str = "requested";
pub const STATE_CLAIMED: &str = "claimed";
pub const STATE_EXITING: &str = "exiting";
pub const STATE_RELAUNCHING: &str = "relaunching";
pub const STATE_VERIFIED: &str = "verified";
pub const STATE_EXPIRED: &str = "expired";
pub const STATE_FAILED_PREFIX: &str = "failed:";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub enum RestartState {
    Requested,
    Claimed,
    Exiting,
    Relaunching,
    Verified,
    Expired,
    Failed(String),
}

impl RestartState {
    pub fn wire(&self) -> String {
        match self {
            Self::Requested => STATE_REQUESTED.to_string(),
            Self::Claimed => STATE_CLAIMED.to_string(),
            Self::Exiting => STATE_EXITING.to_string(),
            Self::Relaunching => STATE_RELAUNCHING.to_string(),
            Self::Verified => STATE_VERIFIED.to_string(),
            Self::Expired => STATE_EXPIRED.to_string(),
            Self::Failed(stage) => format!("{STATE_FAILED_PREFIX}{stage}"),
        }
    }
}

impl fmt::Display for RestartState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.wire())
    }
}

impl TryFrom<String> for RestartState {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            STATE_REQUESTED => Ok(Self::Requested),
            STATE_CLAIMED => Ok(Self::Claimed),
            STATE_EXITING => Ok(Self::Exiting),
            STATE_RELAUNCHING => Ok(Self::Relaunching),
            STATE_VERIFIED => Ok(Self::Verified),
            STATE_EXPIRED => Ok(Self::Expired),
            other => match other.strip_prefix(STATE_FAILED_PREFIX) {
                Some(stage) if !stage.is_empty() => Ok(Self::Failed(stage.to_string())),
                _ => Err(format!("unknown restart state {other}")),
            },
        }
    }
}

impl From<RestartState> for String {
    fn from(value: RestartState) -> Self {
        value.wire()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RestartMode {
    Resume,
    Fresh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TransitionTarget {
    Exiting,
    Relaunching,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RestartStatusParams {
    pub workspace: String,
}

#[derive(Clone, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RestartClaimParams {
    pub intent_id: String,
    pub claimant: String,
    pub capability: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RestartTransitionParams {
    pub intent_id: String,
    pub claim_token: String,
    pub to: TransitionTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageDeadlines {
    #[serde(default = "default_requested_ttl_secs")]
    pub requested_ttl_secs: u64,
    #[serde(default = "default_exiting_secs")]
    pub exiting_secs: u64,
    #[serde(default = "default_relaunching_secs")]
    pub relaunching_secs: u64,
}

impl Default for StageDeadlines {
    fn default() -> Self {
        Self {
            requested_ttl_secs: REQUESTED_TTL_SECS,
            exiting_secs: EXITING_DEADLINE_SECS,
            relaunching_secs: RELAUNCHING_DEADLINE_SECS,
        }
    }
}

fn default_requested_ttl_secs() -> u64 {
    REQUESTED_TTL_SECS
}

fn default_exiting_secs() -> u64 {
    EXITING_DEADLINE_SECS
}

fn default_relaunching_secs() -> u64 {
    RELAUNCHING_DEADLINE_SECS
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingIntent {
    pub intent_id: String,
    pub state: RestartState,
    pub mode: RestartMode,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub deadlines: StageDeadlines,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartStatusResult {
    #[serde(default)]
    pub pending: Option<PendingIntent>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartClaimResult {
    pub claim_token: String,
    pub claim_epoch: i64,
    #[serde(default)]
    pub stage_deadlines: StageDeadlines,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartTransitionResult {
    pub state: RestartState,
}

#[derive(Clone, Debug, Serialize)]
pub struct RequestEnvelope<'a, P> {
    pub protocol: u8,
    pub id: &'a str,
    pub method: &'a str,
    pub params: P,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ResponseEnvelope {
    #[serde(default)]
    pub protocol: Option<u8>,
    pub id: String,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<ProtocolErrorBody>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProtocolErrorBody {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
    #[serde(default)]
    pub details: Option<serde_json::Value>,
}

impl ProtocolErrorBody {
    pub fn is_storm_refusal(&self) -> bool {
        self.code == STORM_REFUSAL_CODE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip_through_their_wire_text() {
        for (text, state) in [
            (STATE_REQUESTED, RestartState::Requested),
            (STATE_CLAIMED, RestartState::Claimed),
            (STATE_EXITING, RestartState::Exiting),
            (STATE_RELAUNCHING, RestartState::Relaunching),
            (STATE_VERIFIED, RestartState::Verified),
            (STATE_EXPIRED, RestartState::Expired),
        ] {
            assert_eq!(state.wire(), text);
            assert_eq!(RestartState::try_from(text.to_string()), Ok(state));
        }
        assert_eq!(
            RestartState::try_from("failed:relaunching".to_string()),
            Ok(RestartState::Failed("relaunching".to_string()))
        );
        assert_eq!(
            RestartState::Failed("relaunching".to_string()).wire(),
            "failed:relaunching"
        );
    }

    #[test]
    fn refuses_states_the_contract_does_not_name() {
        for text in ["", "failed", "failed:", "Exiting", "restarting"] {
            assert!(
                RestartState::try_from(text.to_string()).is_err(),
                "state {text} must refuse"
            );
        }
    }

    #[test]
    fn a_status_answer_without_deadlines_keeps_the_contract_defaults() {
        let result: RestartStatusResult = serde_json::from_str(
            r#"{"pending":{"intentId":"i1","state":"exiting","mode":"resume","sessionId":null}}"#,
        )
        .expect("status parses");
        let pending = result.pending.expect("pending intent");
        assert_eq!(pending.state, RestartState::Exiting);
        assert_eq!(pending.mode, RestartMode::Resume);
        assert_eq!(pending.deadlines.exiting_secs, EXITING_DEADLINE_SECS);
        assert_eq!(pending.deadlines.relaunching_secs, RELAUNCHING_DEADLINE_SECS);
        assert_eq!(pending.deadlines.requested_ttl_secs, REQUESTED_TTL_SECS);

        let empty: RestartStatusResult = serde_json::from_str("{}").expect("empty status parses");
        assert!(empty.pending.is_none());
    }

    #[test]
    fn transition_params_serialize_as_the_contract_names_them() {
        let params = RestartTransitionParams {
            intent_id: "i1".to_string(),
            claim_token: "t1".to_string(),
            to: TransitionTarget::Relaunching,
            detail: None,
        };
        let line = serde_json::to_string(&RequestEnvelope {
            protocol: PROTOCOL_VERSION,
            id: "1",
            method: METHOD_RESTART_TRANSITION,
            params: &params,
        })
        .expect("envelope serializes");
        assert_eq!(
            line,
            r#"{"protocol":1,"id":"1","method":"restart_transition","params":{"intentId":"i1","claimToken":"t1","to":"relaunching"}}"#
        );
    }

    #[test]
    fn storm_refusals_are_recognized_by_their_code_alone() {
        let storm = ProtocolErrorBody {
            code: STORM_REFUSAL_CODE.to_string(),
            message: "too many restarts".to_string(),
            retryable: false,
            details: None,
        };
        assert!(storm.is_storm_refusal());
        let other = ProtocolErrorBody {
            code: "stale_lease".to_string(),
            message: "superseded".to_string(),
            retryable: false,
            details: None,
        };
        assert!(!other.is_storm_refusal());
    }
}

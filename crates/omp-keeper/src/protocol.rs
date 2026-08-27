//! The keeper's side of the restart wire.
//!
//! The vocabulary — states, params, receipts — belongs to
//! `protocol::restart` and is re-exported here so the keeper's modules
//! keep one import path. What this module declares itself is only what that
//! door does not own: the JSONL client envelope the keeper speaks to a
//! substrate child over stdio, the three method names the substrate dispatches
//! on, and the local facts named at their declarations below.

use serde::{Deserialize, Serialize};

pub use ::protocol::PROTOCOL_VERSION;
pub use ::protocol::restart::{
    RestartClaimParams, RestartClaimReceipt, RestartMode, RestartState, RestartStatusDeadlines,
    RestartStatusIntent, RestartStatusParams, RestartStatusReceipt, RestartTransitionParams,
    RestartTransitionReceipt, RestartTransitionTarget,
};

/// The three methods the keeper calls. `restart_request` and `restart_verify`
/// belong to the adapter and to the successor, so the keeper never names them.
pub const METHOD_RESTART_CLAIM: &str = "restart_claim";
pub const METHOD_RESTART_TRANSITION: &str = "restart_transition";
pub const METHOD_RESTART_STATUS: &str = "restart_status";

/// The refusal the keeper answers with one plain operator sentence instead of a
/// retry. The substrate raises it by this code; the wire door carries no
/// refusal vocabulary.
pub const STORM_REFUSAL_CODE: &str = "restart_storm";

/// The exit code an armed adapter leaves behind. A hint only: the keeper asks
/// `restart_status` for every exit code, armed or not.
pub const ARMED_EXIT_CODE: i32 = 87;

/// The net under the `exiting` stage, used only when a status answer carries no
/// `exitingDeadlineAt` at all. The House normally publishes that instant — the
/// substrate sets it on the exiting transition — and the keeper obeys the
/// instant, not this number. It exists so a missing field cannot mean "wait
/// forever", and it matches the contract's stage seconds.
pub const EXITING_DEADLINE_SECS: i64 = 60;

#[derive(Clone, Debug, Serialize)]
pub struct RequestEnvelope<'a, P> {
    pub protocol: u8,
    pub id: &'a str,
    pub method: &'a str,
    pub params: P,
}

/// The keeper reads the two fields it acts on. A response's own `protocol`, and
/// a refusal's `retryable`/`details`, are ignored on purpose: the keeper answers
/// one refusal one way and never retries a decision the House already made.
#[derive(Clone, Debug, Deserialize)]
pub struct ResponseEnvelope {
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
}

impl ProtocolErrorBody {
    pub fn is_storm_refusal(&self) -> bool {
        self.code == STORM_REFUSAL_CODE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::protocol::restart::RestartMode;

    #[test]
    fn a_status_answer_carries_its_intent_or_says_there_is_none() {
        let receipt: RestartStatusReceipt = serde_json::from_str(
            r#"{"workspace":"D:/w","intent":{"intentId":"i1","state":"exiting","mode":"resume","sessionId":null,"deadlines":{"expiresAt":"2026-08-25T00:00:00Z","exitingDeadlineAt":"2026-08-25T00:01:00Z"}}}"#,
        )
        .expect("status parses");
        let intent = receipt.intent.expect("pending intent");
        assert_eq!(intent.state, RestartState::Exiting);
        assert_eq!(intent.mode, RestartMode::Resume);
        assert_eq!(
            intent.deadlines.exiting_deadline_at.as_deref(),
            Some("2026-08-25T00:01:00Z")
        );

        let empty: RestartStatusReceipt =
            serde_json::from_str(r#"{"workspace":"D:/w","intent":null}"#)
                .expect("empty status parses");
        assert!(empty.intent.is_none());
    }

    #[test]
    fn transition_params_serialize_as_the_contract_names_them() {
        let params = RestartTransitionParams {
            intent_id: "i1".to_string(),
            claim_token: Some("t1".to_string()),
            requester_session: None,
            capability: None,
            to: RestartTransitionTarget::Relaunching,
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
        // straight off the wire, extra refusal fields and all
        let storm: ProtocolErrorBody = serde_json::from_str(
            r#"{"code":"restart_storm","message":"too many restarts","retryable":false,"details":null}"#,
        )
        .expect("refusal parses");
        assert!(storm.is_storm_refusal());
        let other: ProtocolErrorBody =
            serde_json::from_str(r#"{"code":"stale_lease","message":"superseded"}"#)
                .expect("refusal parses");
        assert!(!other.is_storm_refusal());
    }
}

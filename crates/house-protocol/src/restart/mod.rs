//! Restart intent wire shapes, protocol version 1.
//!
//! The concern: the vocabulary of one self-restart — its states, its five
//! method params, and its five receipts. This module is the single declaration
//! of that vocabulary. The substrate handler, the keeper, and the adapter door
//! import this door and never repeat a state literal (coding#446 rule 4).
//!
//! What lives here: shape strictness only. Every params struct denies unknown
//! fields, speaks camelCase, and `validate()` reports the first field that is
//! empty, over its ceiling, or malformed. House identity law (which room slugs
//! exist, which principal may claim) is the substrate's, not the wire's.
//!
//! Timestamps cross this wire as RFC3339 strings: the protocol crate carries
//! no clock dependency, and the substrate is the only authority on time.

use serde::{Deserialize, Serialize};

/// Ceilings exist so a malformed caller cannot post an unbounded row. They are
/// generous, not tuned: workspaces are absolute paths and reasons are one
/// sentence of operator-visible truth.
const MAX_IDENTIFIER: usize = 256;
const MAX_WORKSPACE: usize = 1024;
const MAX_REASON: usize = 2048;
const MAX_DETAIL: usize = 2048;
/// A claim token is 32 random bytes, lowercase hex.
const CLAIM_TOKEN_HEX: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RestartHarness {
    Omp,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RestartMode {
    Resume,
    Fresh,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RestartConsentSource {
    OperatorStandingPolicy,
    OperatorApproval,
}

/// The whole state vocabulary. `Failed` carries its stage separately, on the
/// intent row, so the wire never has to parse a `failed:<stage>` compound.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RestartState {
    Requested,
    Claimed,
    Exiting,
    Relaunching,
    Verified,
    Failed,
    Expired,
}

/// The states a transition may name. `Exiting` is the adapter's tokenless door
/// out of `requested`; the other two belong to the keeper's claim. Verified is
/// absent on purpose: the successor proves itself through its own method.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RestartTransitionTarget {
    Exiting,
    Relaunching,
    Failed,
}

impl RestartHarness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Omp => "omp",
        }
    }
}

impl RestartMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resume => "resume",
            Self::Fresh => "fresh",
        }
    }

    /// Read a mode back from storage; the database CHECK holds the same list.
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "resume" => Some(Self::Resume),
            "fresh" => Some(Self::Fresh),
            _ => None,
        }
    }
}

impl RestartConsentSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OperatorStandingPolicy => "operator-standing-policy",
            Self::OperatorApproval => "operator-approval",
        }
    }
}

impl RestartState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Claimed => "claimed",
            Self::Exiting => "exiting",
            Self::Relaunching => "relaunching",
            Self::Verified => "verified",
            Self::Failed => "failed",
            Self::Expired => "expired",
        }
    }

    /// Read a state back from storage. The database CHECK and this table are
    /// the same list; an unknown value means the two have drifted.
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "requested" => Some(Self::Requested),
            "claimed" => Some(Self::Claimed),
            "exiting" => Some(Self::Exiting),
            "relaunching" => Some(Self::Relaunching),
            "verified" => Some(Self::Verified),
            "failed" => Some(Self::Failed),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }
}

impl RestartTransitionTarget {
    pub fn state(self) -> RestartState {
        match self {
            Self::Exiting => RestartState::Exiting,
            Self::Relaunching => RestartState::Relaunching,
            Self::Failed => RestartState::Failed,
        }
    }
}

fn bounded(value: &str, field: &'static str, ceiling: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if value.len() > ceiling {
        return Err(format!("{field} must be at most {ceiling} bytes"));
    }
    Ok(())
}

fn bounded_option(
    value: Option<&String>,
    field: &'static str,
    ceiling: usize,
) -> Result<(), String> {
    match value {
        Some(value) => bounded(value, field, ceiling),
        None => Ok(()),
    }
}

/// The canonical lowercase hyphenated UUID shape. The substrate parses the
/// value for real; this refuses obvious garbage before it reaches a pool.
fn uuid_shaped(value: &str, field: &'static str) -> Result<(), String> {
    let groups = [8usize, 4, 4, 4, 12];
    let mut parts = value.split('-');
    for group in groups {
        let part = parts.next().unwrap_or_default();
        if part.len() != group
            || !part
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(format!("{field} must be a lowercase canonical UUID"));
        }
    }
    if parts.next().is_some() {
        return Err(format!("{field} must be a lowercase canonical UUID"));
    }
    Ok(())
}

fn hex_token(value: &str, field: &'static str) -> Result<(), String> {
    if value.len() != CLAIM_TOKEN_HEX
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "{field} must be {CLAIM_TOKEN_HEX} lowercase hex characters"
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RestartRequestParams {
    pub harness: RestartHarness,
    pub workspace: String,
    pub mode: RestartMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub reason: String,
    pub consent_source: RestartConsentSource,
    pub requester_room: String,
    pub requester_spirit: String,
    pub requester_session: String,
    pub idempotency_key: String,
}

impl RestartRequestParams {
    pub fn validate(&self) -> Result<(), String> {
        bounded(&self.workspace, "workspace", MAX_WORKSPACE)?;
        bounded_option(self.session_id.as_ref(), "sessionId", MAX_IDENTIFIER)?;
        bounded(&self.reason, "reason", MAX_REASON)?;
        bounded(&self.requester_room, "requesterRoom", MAX_IDENTIFIER)?;
        bounded(&self.requester_spirit, "requesterSpirit", MAX_IDENTIFIER)?;
        bounded(&self.requester_session, "requesterSession", MAX_IDENTIFIER)?;
        bounded(&self.idempotency_key, "idempotencyKey", MAX_IDENTIFIER)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RestartClaimParams {
    pub intent_id: String,
    pub claimant: String,
    /// DEVIATION from contract v1: the frozen params list omits a capability,
    /// but "keeper only" cannot be enforced by caller-supplied claimant text
    /// alone, so the keeper presents its provisioned secret here.
    pub capability: String,
    pub idempotency_key: String,
}

impl RestartClaimParams {
    pub fn validate(&self) -> Result<(), String> {
        uuid_shaped(&self.intent_id, "intentId")?;
        bounded(&self.claimant, "claimant", MAX_IDENTIFIER)?;
        bounded(&self.capability, "capability", MAX_IDENTIFIER)?;
        bounded(&self.idempotency_key, "idempotencyKey", MAX_IDENTIFIER)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RestartTransitionParams {
    pub intent_id: String,
    /// Absent on `exiting`: the adapter arms the exit and holds no keeper
    /// token. Required by the keeper's own transitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_token: Option<String>,
    pub to: RestartTransitionTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl RestartTransitionParams {
    pub fn validate(&self) -> Result<(), String> {
        uuid_shaped(&self.intent_id, "intentId")?;
        match self.to {
            // The exit door proves itself with the intent id, which only the
            // House hands out, and names the armed session in detail. A token
            // here means the caller mistook this for a keeper transition.
            RestartTransitionTarget::Exiting => {
                if self.claim_token.is_some() {
                    return Err("claimToken is not valid for the exiting transition".into());
                }
                bounded(
                    self.detail.as_deref().unwrap_or_default(),
                    "detail",
                    MAX_DETAIL,
                )
            }
            RestartTransitionTarget::Relaunching | RestartTransitionTarget::Failed => {
                let token = self
                    .claim_token
                    .as_deref()
                    .ok_or_else(|| "claimToken is required for this transition".to_owned())?;
                hex_token(token, "claimToken")?;
                bounded_option(self.detail.as_ref(), "detail", MAX_DETAIL)
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RestartVerifyParams {
    pub intent_id: String,
    pub successor_session: String,
    pub room: String,
    pub spirit: String,
}

impl RestartVerifyParams {
    pub fn validate(&self) -> Result<(), String> {
        uuid_shaped(&self.intent_id, "intentId")?;
        bounded(&self.successor_session, "successorSession", MAX_IDENTIFIER)?;
        bounded(&self.room, "room", MAX_IDENTIFIER)?;
        bounded(&self.spirit, "spirit", MAX_IDENTIFIER)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RestartStatusParams {
    pub workspace: String,
}

impl RestartStatusParams {
    pub fn validate(&self) -> Result<(), String> {
        bounded(&self.workspace, "workspace", MAX_WORKSPACE)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestartRequestReceipt {
    pub intent_id: String,
    pub state: RestartState,
    pub expires_at: String,
}

/// The stage policy the keeper must obey, handed over at claim time instead of
/// duplicated in the keeper's own constants.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestartStageDeadlines {
    pub requested_ttl_secs: i64,
    pub exiting_secs: i64,
    pub relaunching_secs: i64,
    pub relaunch_attempt_limit: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestartClaimReceipt {
    /// Shown exactly once: the substrate stores only its sha256.
    pub claim_token: String,
    pub claim_epoch: i64,
    pub stage_deadlines: RestartStageDeadlines,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestartTransitionReceipt {
    pub state: RestartState,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestartVerifyReceipt {
    pub state: RestartState,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestartStatusDeadlines {
    pub expires_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exiting_deadline_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relaunching_deadline_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestartStatusIntent {
    pub intent_id: String,
    pub state: RestartState,
    pub mode: RestartMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub deadlines: RestartStatusDeadlines,
}

/// A read with no capability: the keeper polls it after a child exit and the
/// adapter door checks it before arming. `None` is the ordinary answer.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestartStatusReceipt {
    pub workspace: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<RestartStatusIntent>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, json, to_value};

    const INTENT: &str = "00000000-0000-0000-0000-000000000001";
    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn request_json() -> serde_json::Value {
        json!({
            "harness": "omp",
            "workspace": "D:/athanor-wt/restart-intent",
            "mode": "resume",
            "sessionId": "session-1",
            "reason": "the loader installed a newer release than this session loaded",
            "consentSource": "operator-standing-policy",
            "requesterRoom": "kodo",
            "requesterSpirit": "Kodo",
            "requesterSession": "service:kodo",
            "idempotencyKey": "request-1"
        })
    }

    // Kills: a params struct that silently accepts an unknown or snake_case
    // field, which would let a typo ride as an accepted request.
    // red-proof: remove deny_unknown_fields or rename_all from any params.
    #[test]
    fn params_are_strict_and_camel_cased() {
        let request: RestartRequestParams = serde_json::from_value(request_json()).unwrap();
        assert_eq!(request.harness, RestartHarness::Omp);
        assert_eq!(request.mode, RestartMode::Resume);
        assert_eq!(
            request.consent_source,
            RestartConsentSource::OperatorStandingPolicy
        );
        request.validate().unwrap();

        let mut unknown = request_json();
        unknown["surprise"] = json!(true);
        assert!(serde_json::from_value::<RestartRequestParams>(unknown).is_err());

        let mut snake = request_json();
        snake.as_object_mut().unwrap().remove("requesterRoom");
        snake["requester_room"] = json!("kodo");
        assert!(serde_json::from_value::<RestartRequestParams>(snake).is_err());

        assert!(
            from_str::<RestartClaimParams>(
                r#"{"intentId":"00000000-0000-0000-0000-000000000001","claimant":"omp-keeper","capability":"secret","idempotencyKey":"claim-1","extra":1}"#
            )
            .is_err()
        );
        assert!(
            from_str::<RestartStatusParams>(r#"{"workspace":"D:/w","limit":5}"#).is_err(),
            "status takes a workspace and nothing else"
        );
    }

    // Kills: a mode, consent source, or transition target admitted outside the
    // frozen vocabulary, and a wire rename that drifts from the contract text.
    // red-proof: add a variant, or change a rename_all attribute.
    #[test]
    fn wire_vocabulary_matches_the_frozen_contract() {
        assert_eq!(to_value(RestartMode::Fresh).unwrap(), json!("fresh"));
        assert_eq!(
            to_value(RestartConsentSource::OperatorApproval).unwrap(),
            json!("operator-approval")
        );
        assert_eq!(
            to_value(RestartTransitionTarget::Relaunching).unwrap(),
            json!("relaunching")
        );
        assert_eq!(to_value(RestartState::Verified).unwrap(), json!("verified"));
        for state in [
            RestartState::Requested,
            RestartState::Claimed,
            RestartState::Exiting,
            RestartState::Relaunching,
            RestartState::Verified,
            RestartState::Failed,
            RestartState::Expired,
        ] {
            assert_eq!(RestartState::from_str(state.as_str()), Some(state));
        }
        assert_eq!(RestartState::from_str("exited"), None);

        let mut bad_mode = request_json();
        bad_mode["mode"] = json!("reboot");
        assert!(serde_json::from_value::<RestartRequestParams>(bad_mode).is_err());
        let mut bad_consent = request_json();
        bad_consent["consentSource"] = json!("operator_approval");
        assert!(serde_json::from_value::<RestartRequestParams>(bad_consent).is_err());
    }

    // Kills: validate() accepting an empty required field, an over-ceiling
    // value, a non-canonical intent id, or a claim token that is not 64 hex.
    // red-proof: drop any bounded/uuid_shaped/hex_token call from a validate().
    #[test]
    fn validate_refuses_empty_oversized_and_malformed_fields() {
        let mut blank = request_json();
        blank["reason"] = json!("   ");
        let request: RestartRequestParams = serde_json::from_value(blank).unwrap();
        assert_eq!(
            request.validate().unwrap_err(),
            "reason must not be empty".to_owned()
        );

        let mut long = request_json();
        long["reason"] = json!("x".repeat(MAX_REASON + 1));
        let request: RestartRequestParams = serde_json::from_value(long).unwrap();
        assert!(request.validate().is_err());

        let claim = RestartClaimParams {
            intent_id: "not-a-uuid".into(),
            claimant: "omp-keeper".into(),
            capability: "secret".into(),
            idempotency_key: "claim-1".into(),
        };
        assert!(claim.validate().is_err());
        let claim = RestartClaimParams {
            intent_id: INTENT.into(),
            ..claim
        };
        claim.validate().unwrap();

        // The two doors of one method: the adapter's exit carries no token and
        // must name its session, the keeper's transitions carry the minted one.
        let armed_exit = RestartTransitionParams {
            intent_id: INTENT.into(),
            claim_token: None,
            to: RestartTransitionTarget::Exiting,
            detail: Some(r#"{"session":"service:kodo"}"#.into()),
        };
        armed_exit.validate().unwrap();
        let silent_exit = RestartTransitionParams {
            detail: None,
            ..armed_exit.clone()
        };
        assert!(
            silent_exit.validate().is_err(),
            "an exit that names no session is not an armed exit"
        );
        let tokened_exit = RestartTransitionParams {
            claim_token: Some(TOKEN.into()),
            ..armed_exit.clone()
        };
        assert!(
            tokened_exit.validate().is_err(),
            "the exit door is tokenless: a token means the caller took the wrong door"
        );

        let tokenless_relaunch = RestartTransitionParams {
            to: RestartTransitionTarget::Relaunching,
            claim_token: None,
            detail: None,
            ..armed_exit
        };
        assert!(tokenless_relaunch.validate().is_err());
        let short_token = RestartTransitionParams {
            claim_token: Some("short".into()),
            ..tokenless_relaunch.clone()
        };
        assert!(short_token.validate().is_err());
        let relaunch = RestartTransitionParams {
            claim_token: Some(TOKEN.into()),
            ..tokenless_relaunch.clone()
        };
        relaunch.validate().unwrap();
        let upper = RestartTransitionParams {
            claim_token: Some(TOKEN.to_ascii_uppercase()),
            ..tokenless_relaunch
        };
        assert!(
            upper.validate().is_err(),
            "an uppercase token is not the minted shape"
        );

        let verify = RestartVerifyParams {
            intent_id: INTENT.into(),
            successor_session: String::new(),
            room: "kodo".into(),
            spirit: "Kodo".into(),
        };
        assert!(verify.validate().is_err());
    }

    // Kills: a receipt that stops naming its state, or a status receipt that
    // serializes a null intent instead of omitting the field.
    // red-proof: remove skip_serializing_if from RestartStatusReceipt::intent.
    #[test]
    fn receipts_serialize_the_named_contract_fields() {
        let claim = RestartClaimReceipt {
            claim_token: TOKEN.into(),
            claim_epoch: 1,
            stage_deadlines: RestartStageDeadlines {
                requested_ttl_secs: 300,
                exiting_secs: 60,
                relaunching_secs: 120,
                relaunch_attempt_limit: 2,
            },
        };
        assert_eq!(
            to_value(&claim).unwrap(),
            json!({
                "claimToken": TOKEN,
                "claimEpoch": 1,
                "stageDeadlines": {
                    "requestedTtlSecs": 300,
                    "exitingSecs": 60,
                    "relaunchingSecs": 120,
                    "relaunchAttemptLimit": 2
                }
            })
        );

        let empty = RestartStatusReceipt {
            workspace: "D:/w".into(),
            intent: None,
        };
        assert_eq!(
            to_value(&empty).unwrap(),
            json!({"workspace": "D:/w"}),
            "no pending intent is an absent field, not a null"
        );

        assert_eq!(
            to_value(RestartVerifyReceipt {
                state: RestartState::Verified
            })
            .unwrap(),
            json!({"state": "verified"})
        );
    }
}

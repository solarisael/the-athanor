//! Who may move a restart intent.
//!
//! The concern: this plane hands its intent id to anyone — `restart_status`
//! takes no capability — so the id proves nothing and every write door needs a
//! real fence. Two fences live here and nowhere else: a provisioned secret in
//! `restart.principal_capabilities`, and the requester identity the intent row
//! already carries. Kintsu's 2026-08-25 review named the earlier shape
//! unauthorized (caller-asserted consent, a readable id treated as proof), and
//! the contract lines that called the exit "tokenless" and the id the
//! successor's proof fell with it.
//!
//! The secret table is the Docket's room-capability pattern
//! (`docket.rs::require_docket_capability`) over this plane's own table: one
//! slug law for every principal, the sha256 only, constant-time comparison, and
//! provisioning offline through `substrate/provision-restart-capability.ps1`.

use super::{constant_time_equal, refusal, sha256_hex};
use crate::config::AppError;
use sqlx::{Executor, Postgres};

/// The keeper's own principal row: it owns the terminal and impersonates no
/// room.
pub(super) const KEEPER_CLAIM_CLASS: &str = "restart_claim";
/// The three classes the requester room holds. Splitting them means the room
/// that may ask for a restart is not automatically the room that may arm an
/// exit, and a leaked exit secret verifies no successor.
pub(super) const REQUEST_CLASS: &str = "restart_request";
pub(super) const EXIT_CLASS: &str = "restart_exit";
pub(super) const VERIFY_CLASS: &str = "restart_verify";

/// Gate one door on a provisioned secret. Takes an executor and not a pool
/// because the exit door can only name its principal after it holds the intent
/// row: the room comes off the locked row, never off the caller.
pub(super) async fn require_capability<'e, E>(
    executor: E,
    principal: &str,
    operation_class: &str,
    capability: &str,
) -> Result<(), AppError>
where
    E: Executor<'e, Database = Postgres>,
{
    let expected: Option<String> = sqlx::query_scalar(
        "SELECT capability_hash FROM restart.principal_capabilities WHERE principal=$1 AND operation_class=$2",
    )
    .bind(principal)
    .bind(operation_class)
    .fetch_optional(executor)
    .await?;
    let supplied = sha256_hex(capability.as_bytes());
    if expected
        .as_deref()
        .is_none_or(|hash| !constant_time_equal(supplied.as_bytes(), hash.as_bytes()))
    {
        return Err(refusal(
            "restart_capability",
            "the presented capability does not authorize this restart operation",
        ));
    }
    Ok(())
}

/// The exit door's second fence: the caller is the session that asked. The
/// adapter sends this from its own session binding, so a room secret alone
/// cannot arm a restart somebody else requested.
pub(super) fn require_requester_session(
    stored_session: &str,
    presented_session: &str,
) -> Result<(), AppError> {
    if constant_time_equal(stored_session.as_bytes(), presented_session.as_bytes()) {
        Ok(())
    } else {
        Err(refusal(
            "exit_not_authorized",
            "the exit must come from the session that requested the restart",
        ))
    }
}

/// The verify door's second fence: the successor belongs to the room that asked
/// and is not the session that left. A restart nobody came back from must stay
/// visible to Insula, so the predecessor cannot sign its own return.
pub(super) fn require_successor(
    requester_room: &str,
    requester_session: &str,
    request_room: &str,
    successor_session: &str,
) -> Result<(), AppError> {
    if requester_room != request_room || requester_session == successor_session {
        return Err(refusal(
            "verify_not_authorized",
            "only a new session of the requesting room can verify this intent",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(error: &AppError) -> &str {
        match error {
            AppError::Refusal { code, .. } => code,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    // Kills: an exit fence that accepts a near-miss session string, or one that
    // reports the mismatch under some other name than exit_not_authorized.
    // red-proof: compare only the first bytes, or return Ok on mismatch.
    #[test]
    fn the_exit_fence_admits_only_the_requesting_session() {
        require_requester_session("service:kodo", "service:kodo").unwrap();
        let wrong = require_requester_session("service:kodo", "service:kodo-2").unwrap_err();
        assert_eq!(code(&wrong), "exit_not_authorized");
        assert_eq!(
            code(&require_requester_session("service:kodo", "").unwrap_err()),
            "exit_not_authorized"
        );
    }

    // Kills: a verify door that lets a foreign room, or the very session that
    // exited, sign the return — which would hide a real divergence from Insula.
    // red-proof: drop either clause of the require_successor condition.
    #[test]
    fn the_verify_fence_demands_a_new_session_of_the_asking_room() {
        require_successor("kodo", "service:kodo", "kodo", "service:kodo-2").unwrap();
        assert_eq!(
            code(&require_successor("kodo", "service:kodo", "tuner", "service:tuner").unwrap_err()),
            "verify_not_authorized"
        );
        assert_eq!(
            code(&require_successor("kodo", "service:kodo", "kodo", "service:kodo").unwrap_err()),
            "verify_not_authorized"
        );
    }
}

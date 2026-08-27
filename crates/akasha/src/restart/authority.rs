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
use protocol::restart::RestartMode;
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

/// The successor's logical session identity depends on launch mode. Resume is
/// the same harness session in a new process incarnation, so the requested,
/// requesting, and returning session ids must agree. Fresh creates a different
/// logical session. The attempt-scoped proof is checked separately after the
/// intent row is locked.
pub(super) fn require_successor_identity(
    mode: RestartMode,
    requester_room: &str,
    requester_session: &str,
    recorded_session: Option<&str>,
    request_room: &str,
    successor_session: &str,
) -> Result<(), AppError> {
    let session_matches_mode = match mode {
        RestartMode::Resume => recorded_session.is_some_and(|recorded| {
            constant_time_equal(recorded.as_bytes(), requester_session.as_bytes())
                && constant_time_equal(recorded.as_bytes(), successor_session.as_bytes())
        }),
        RestartMode::Fresh => {
            !constant_time_equal(requester_session.as_bytes(), successor_session.as_bytes())
        }
    };
    if requester_room != request_room || !session_matches_mode {
        return Err(refusal(
            "verify_not_authorized",
            "the successor identity does not match this restart intent",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::restart::RestartMode;
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

    // Resume keeps the logical session identity, so the recorded session is
    // required and must match exactly. Fresh mode must instead prove a new
    // session; the room fence remains independent of that mode choice.
    #[test]
    fn the_verify_fence_matches_mode_specific_session_identity() {
        require_successor_identity(
            RestartMode::Resume,
            "kodo",
            "service:kodo",
            Some("service:kodo"),
            "kodo",
            "service:kodo",
        )
        .unwrap();
        assert_eq!(
            code(
                &require_successor_identity(
                    RestartMode::Resume,
                    "kodo",
                    "service:kodo",
                    Some("service:kodo"),
                    "kodo",
                    "service:kodo-2",
                )
                .unwrap_err()
            ),
            "verify_not_authorized"
        );
        assert_eq!(
            code(
                &require_successor_identity(
                    RestartMode::Resume,
                    "kodo",
                    "service:kodo",
                    None,
                    "kodo",
                    "service:kodo",
                )
                .unwrap_err()
            ),
            "verify_not_authorized"
        );
        require_successor_identity(
            RestartMode::Fresh,
            "kodo",
            "service:kodo",
            None,
            "kodo",
            "service:kodo-2",
        )
        .unwrap();
        assert_eq!(
            code(
                &require_successor_identity(
                    RestartMode::Fresh,
                    "kodo",
                    "service:kodo",
                    None,
                    "kodo",
                    "service:kodo",
                )
                .unwrap_err()
            ),
            "verify_not_authorized"
        );
        assert_eq!(
            code(
                &require_successor_identity(
                    RestartMode::Fresh,
                    "kodo",
                    "service:kodo",
                    None,
                    "tuner",
                    "service:tuner",
                )
                .unwrap_err()
            ),
            "verify_not_authorized"
        );
    }
}

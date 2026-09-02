use crate::clock::Deadline;
use crate::protocol::{ARMED_EXIT_CODE, RestartState, RestartStatusIntent};
use chrono::{DateTime, Utc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusStep {
    Stop,
    Claim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitingAction {
    Wait,
    Kill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelaunchAction {
    Retry,
    Fail,
}

/// What one look at the House says about our own intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyWatch {
    /// Ours, still on its way.
    Waiting,
    /// Ours, verified. The only positive proof there is.
    Verified,
    /// Ours is finished but not as verified, or the answer was not about ours at
    /// all. Which terminal state it reached is not the keeper's to guess.
    Terminal,
}

/// The 87 exit is the fast-path hint only; the keeper asks restart_status for every exit code.
pub fn armed_exit_hint(exit_code: Option<i32>) -> bool {
    exit_code == Some(ARMED_EXIT_CODE)
}

/// Claiming is legal from requested (a crash the adapter never armed) or exiting (an armed exit).
pub fn status_step(pending_state: Option<RestartState>) -> StatusStep {
    match pending_state {
        Some(RestartState::Requested) | Some(RestartState::Exiting) => StatusStep::Claim,
        _ => StatusStep::Stop,
    }
}

/// Kill an `exiting` child once the deadline carried on its intent has passed.
///
/// The deadline is an instant, never a stopwatch. A keeper that starts looking
/// after the adapter already armed is late on its first look and must act on it,
/// which is exactly what a fresh local clock hides.
pub fn exiting_action(
    pending_state: Option<RestartState>,
    deadline: Option<Deadline>,
    now: DateTime<Utc>,
) -> ExitingAction {
    let Some(deadline) = deadline else {
        return ExitingAction::Wait;
    };
    if pending_state == Some(RestartState::Exiting) && deadline.has_passed(now) {
        ExitingAction::Kill
    } else {
        ExitingAction::Wait
    }
}

/// The attempt budget is the substrate's, handed over in the claim receipt, so
/// the keeper never carries a second copy of it.
pub fn relaunch_action(failed_attempts: i32, attempt_limit: i32) -> RelaunchAction {
    if failed_attempts < attempt_limit {
        RelaunchAction::Retry
    } else {
        RelaunchAction::Fail
    }
}

/// Has our own successor verified?
///
/// Answer the exact-id read, never the workspace read. The workspace read
/// reports live states only, so it can never say `verified` and its silence
/// means nothing about which end our intent reached; asking by id is what makes
/// a positive sighting possible at all (protocol restart: intentId
/// present returns that intent in whatever state, terminal included).
///
/// Only `verified` for our own id is proof. Absent, or some other intent, is
/// terminal-but-unproven and takes the retry path — a stranger's row says
/// nothing about ours, and reading it as success is how a keeper declares
/// victory over a restart that never happened.
pub fn verify_watch(intent_id: &str, observed: Option<&RestartStatusIntent>) -> VerifyWatch {
    let Some(observed) = observed else {
        return VerifyWatch::Terminal;
    };
    if observed.intent_id != intent_id {
        return VerifyWatch::Terminal;
    }
    match observed.state {
        RestartState::Verified => VerifyWatch::Verified,
        RestartState::Failed | RestartState::Expired => VerifyWatch::Terminal,
        RestartState::Requested
        | RestartState::Exiting
        | RestartState::Claimed
        | RestartState::Relaunching => VerifyWatch::Waiting,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::house_instant;
    use crate::protocol::{RestartStatusDeadlines, RestartStatusIntent};
    use ::protocol::restart::RestartMode;

    fn at(text: &str) -> DateTime<Utc> {
        house_instant(text).expect("a readable instant")
    }

    fn intent(intent_id: &str, state: RestartState) -> RestartStatusIntent {
        RestartStatusIntent {
            intent_id: intent_id.to_string(),
            state,
            mode: RestartMode::Resume,
            session_id: None,
            deadlines: RestartStatusDeadlines {
                expires_at: "2026-08-25T18:05:00+00:00".to_string(),
                exiting_deadline_at: None,
                relaunching_deadline_at: None,
            },
        }
    }

    #[test]
    fn the_armed_exit_code_is_the_only_hint() {
        assert!(armed_exit_hint(Some(87)));
        assert!(!armed_exit_hint(Some(0)));
        assert!(!armed_exit_hint(Some(1)));
        assert!(!armed_exit_hint(Some(-87)));
        assert!(!armed_exit_hint(None));
    }

    #[test]
    fn only_requested_or_exiting_may_be_claimed() {
        assert_eq!(
            status_step(Some(RestartState::Requested)),
            StatusStep::Claim
        );
        assert_eq!(status_step(Some(RestartState::Exiting)), StatusStep::Claim);
        for state in [
            RestartState::Claimed,
            RestartState::Relaunching,
            RestartState::Verified,
            RestartState::Expired,
            RestartState::Failed,
        ] {
            assert_eq!(
                status_step(Some(state)),
                StatusStep::Stop,
                "claiming from {} is illegal, so the keeper stops",
                state.as_str()
            );
        }
        assert_eq!(status_step(None), StatusStep::Stop);
    }

    /// The mutation this kills: measuring the stage from the keeper's first sight
    /// of `exiting` instead of reading the instant on the intent. Under a
    /// stopwatch every one of these deadlines is "just started".
    #[test]
    fn an_exiting_intent_is_killed_by_the_instant_on_the_intent_not_by_elapsed_time() {
        let deadline = Deadline::House(at("2026-08-25T18:01:00Z"));
        assert_eq!(
            exiting_action(
                Some(RestartState::Exiting),
                Some(deadline),
                at("2026-08-25T18:01:01Z")
            ),
            ExitingAction::Kill
        );
        // an hour late is still late, however long this keeper has been looking
        assert_eq!(
            exiting_action(
                Some(RestartState::Exiting),
                Some(deadline),
                at("2026-08-25T19:00:00Z")
            ),
            ExitingAction::Kill
        );
        // standing exactly on the instant is not past it
        assert_eq!(
            exiting_action(
                Some(RestartState::Exiting),
                Some(deadline),
                at("2026-08-25T18:01:00Z")
            ),
            ExitingAction::Wait
        );
        assert_eq!(
            exiting_action(
                Some(RestartState::Exiting),
                Some(deadline),
                at("2026-08-25T18:00:59Z")
            ),
            ExitingAction::Wait
        );
    }

    #[test]
    fn no_state_but_exiting_ever_escalates_to_kill() {
        let long_past = Deadline::House(at("2020-01-01T00:00:00Z"));
        for state in [
            RestartState::Requested,
            RestartState::Claimed,
            RestartState::Relaunching,
            RestartState::Verified,
            RestartState::Expired,
            RestartState::Failed,
        ] {
            assert_eq!(
                exiting_action(Some(state), Some(long_past), Utc::now()),
                ExitingAction::Wait,
                "state {} must never escalate to kill",
                state.as_str()
            );
        }
        assert_eq!(
            exiting_action(None, Some(long_past), Utc::now()),
            ExitingAction::Wait,
            "no pending intent is no licence to kill Sol's session"
        );
        assert_eq!(
            exiting_action(Some(RestartState::Exiting), None, Utc::now()),
            ExitingAction::Wait,
            "no deadline in hand is no licence either"
        );
    }

    #[test]
    fn the_relaunch_obeys_the_limit_the_claim_handed_over() {
        // The contract's budget today: one launch and one retry.
        assert_eq!(relaunch_action(0, 2), RelaunchAction::Retry);
        assert_eq!(relaunch_action(1, 2), RelaunchAction::Retry);
        assert_eq!(relaunch_action(2, 2), RelaunchAction::Fail);
        assert_eq!(relaunch_action(97, 2), RelaunchAction::Fail);
        // A substrate that tightens or loosens the budget is obeyed, not second-guessed.
        assert_eq!(relaunch_action(0, 1), RelaunchAction::Retry);
        assert_eq!(relaunch_action(1, 1), RelaunchAction::Fail);
        assert_eq!(relaunch_action(0, 0), RelaunchAction::Fail);
        assert_eq!(relaunch_action(0, -1), RelaunchAction::Fail);
    }

    /// The mutation this kills: reading anything-but-ours as success. Absence and
    /// a stranger's row both used to mean "verified", so a keeper could declare
    /// victory over a restart that never happened.
    #[test]
    fn only_our_own_id_reported_verified_is_proof_of_a_verify() {
        let ours = "3f6b9c2a-7d41-4e58-9a0b-1c8e5d2f4a67";
        let stranger = "8c1d0e4f-2a3b-4c5d-9e6f-7a8b9c0d1e2f";
        assert_eq!(
            verify_watch(ours, Some(&intent(ours, RestartState::Verified))),
            VerifyWatch::Verified,
            "ours, said verified by the House, is the only proof there is"
        );
        assert_eq!(
            verify_watch(ours, None),
            VerifyWatch::Terminal,
            "absent says our intent is finished, never which end it reached"
        );
        // every state a stranger's row could carry, including verified: a verify
        // that belongs to someone else is not ours
        for state in [
            RestartState::Requested,
            RestartState::Exiting,
            RestartState::Claimed,
            RestartState::Relaunching,
            RestartState::Verified,
            RestartState::Failed,
            RestartState::Expired,
        ] {
            assert_eq!(
                verify_watch(ours, Some(&intent(stranger, state))),
                VerifyWatch::Terminal,
                "a stranger's intent as {} proves nothing about ours",
                state.as_str()
            );
        }
        // ours, still on its way
        for state in [
            RestartState::Requested,
            RestartState::Exiting,
            RestartState::Claimed,
            RestartState::Relaunching,
        ] {
            assert_eq!(
                verify_watch(ours, Some(&intent(ours, state))),
                VerifyWatch::Waiting,
                "our intent live as {} is not yet a verify",
                state.as_str()
            );
        }
        // ours, finished the wrong way: never a verify, and never a wait either
        for state in [RestartState::Failed, RestartState::Expired] {
            assert_eq!(
                verify_watch(ours, Some(&intent(ours, state))),
                VerifyWatch::Terminal,
                "our intent as {} is over, not pending",
                state.as_str()
            );
        }
    }
}

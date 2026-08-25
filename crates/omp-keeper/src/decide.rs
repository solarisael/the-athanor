use crate::protocol::{ARMED_EXIT_CODE, RestartState};

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

pub fn exiting_action(
    pending_state: Option<RestartState>,
    elapsed_secs: u64,
    deadline_secs: u64,
) -> ExitingAction {
    if pending_state == Some(RestartState::Exiting) && elapsed_secs > deadline_secs {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::EXITING_DEADLINE_SECS;

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

    #[test]
    fn only_an_exiting_intent_past_its_deadline_escalates_to_kill() {
        let deadline = EXITING_DEADLINE_SECS;
        assert_eq!(
            exiting_action(Some(RestartState::Exiting), deadline + 1, deadline),
            ExitingAction::Kill
        );
        assert_eq!(
            exiting_action(Some(RestartState::Exiting), deadline, deadline),
            ExitingAction::Wait
        );
        assert_eq!(
            exiting_action(Some(RestartState::Exiting), 0, deadline),
            ExitingAction::Wait
        );
        for state in [
            RestartState::Requested,
            RestartState::Claimed,
            RestartState::Relaunching,
            RestartState::Verified,
            RestartState::Expired,
            RestartState::Failed,
        ] {
            assert_eq!(
                exiting_action(Some(state), deadline + 600, deadline),
                ExitingAction::Wait,
                "state {} must never escalate to kill",
                state.as_str()
            );
        }
        assert_eq!(
            exiting_action(None, deadline + 600, deadline),
            ExitingAction::Wait
        );
    }

    #[test]
    fn a_zero_deadline_still_needs_a_full_second_of_overrun() {
        assert_eq!(
            exiting_action(Some(RestartState::Exiting), 0, 0),
            ExitingAction::Wait
        );
        assert_eq!(
            exiting_action(Some(RestartState::Exiting), 1, 0),
            ExitingAction::Kill
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
}

use crate::protocol::{ARMED_EXIT_CODE, RELAUNCH_ATTEMPTS, STATE_EXITING, STATE_REQUESTED};

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
pub fn status_step(pending_state: Option<&str>) -> StatusStep {
    match pending_state {
        Some(state) if state == STATE_REQUESTED || state == STATE_EXITING => StatusStep::Claim,
        _ => StatusStep::Stop,
    }
}

pub fn exiting_action(pending_state: Option<&str>, elapsed_secs: u64, deadline_secs: u64) -> ExitingAction {
    if pending_state == Some(STATE_EXITING) && elapsed_secs > deadline_secs {
        ExitingAction::Kill
    } else {
        ExitingAction::Wait
    }
}

pub fn relaunch_action(failed_attempts: u32) -> RelaunchAction {
    if failed_attempts < RELAUNCH_ATTEMPTS {
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
        assert_eq!(status_step(Some("requested")), StatusStep::Claim);
        assert_eq!(status_step(Some("exiting")), StatusStep::Claim);
        for state in [
            "claimed",
            "relaunching",
            "verified",
            "expired",
            "failed:relaunching",
        ] {
            assert_eq!(
                status_step(Some(state)),
                StatusStep::Stop,
                "claiming from {state} is illegal, so the keeper stops"
            );
        }
        assert_eq!(status_step(None), StatusStep::Stop);
    }

    #[test]
    fn only_an_exiting_intent_past_its_deadline_escalates_to_kill() {
        let deadline = EXITING_DEADLINE_SECS;
        assert_eq!(
            exiting_action(Some("exiting"), deadline + 1, deadline),
            ExitingAction::Kill
        );
        assert_eq!(
            exiting_action(Some("exiting"), deadline, deadline),
            ExitingAction::Wait
        );
        assert_eq!(
            exiting_action(Some("exiting"), 0, deadline),
            ExitingAction::Wait
        );
        for state in [
            "requested",
            "claimed",
            "relaunching",
            "verified",
            "expired",
            "failed:exiting",
        ] {
            assert_eq!(
                exiting_action(Some(state), deadline + 600, deadline),
                ExitingAction::Wait,
                "state {state} must never escalate to kill"
            );
        }
        assert_eq!(
            exiting_action(None, deadline + 600, deadline),
            ExitingAction::Wait
        );
    }

    #[test]
    fn a_zero_deadline_still_needs_a_full_second_of_overrun() {
        assert_eq!(exiting_action(Some("exiting"), 0, 0), ExitingAction::Wait);
        assert_eq!(exiting_action(Some("exiting"), 1, 0), ExitingAction::Kill);
    }

    #[test]
    fn the_relaunch_retries_once_and_then_fails() {
        assert_eq!(relaunch_action(0), RelaunchAction::Retry);
        assert_eq!(relaunch_action(1), RelaunchAction::Retry);
        assert_eq!(relaunch_action(2), RelaunchAction::Fail);
        assert_eq!(relaunch_action(97), RelaunchAction::Fail);
    }
}

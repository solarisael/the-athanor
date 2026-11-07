//! The recipient's turn for one Hallway Knock, as the adapter doorman sees it.
//! Contract: `docs/state-machines/hallway-knock.md`. The doorman reports what it saw and how long ago;
//! the plan says what to settle with the Host next, or why the knock fails.

use serde::Serialize;

/// SQL `started` must follow delivery within this time.
pub const START_SETTLE_LIMIT_MS: u64 = 25_000;
/// The recipient turn must start within this time after delivery.
pub const TURN_START_LIMIT_MS: u64 = 60_000;
/// SQL `completed` must follow the turn end within this time.
pub const COMPLETION_SETTLE_LIMIT_MS: u64 = 25_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KnockTurnFacts {
    pub delivered: bool,
    pub start_settled: bool,
    pub turn_started: bool,
    pub turn_ended: bool,
    pub delivered_elapsed_ms: Option<u64>,
    pub ended_elapsed_ms: Option<u64>,
    /// Time left until the knock's `expires_at`. Zero or less means the knock has expired.
    pub expires_in_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnockSettlement {
    Started,
    Completed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct KnockTurnPlan {
    pub deliver: bool,
    pub settlements: &'static [KnockSettlement],
    pub failure: Option<&'static str>,
}

pub fn plan_knock_turn(facts: &KnockTurnFacts) -> KnockTurnPlan {
    if !facts.delivered {
        return KnockTurnPlan {
            deliver: true,
            settlements: &[],
            failure: None,
        };
    }

    let failure = turn_failure(facts);
    if failure.is_some() {
        return KnockTurnPlan {
            deliver: false,
            settlements: &[],
            failure,
        };
    }

    let turn_done = facts.turn_started && facts.turn_ended;
    let settlements: &[KnockSettlement] = match (facts.start_settled, turn_done) {
        (false, true) => &[KnockSettlement::Started, KnockSettlement::Completed],
        (false, false) => &[KnockSettlement::Started],
        (true, true) => &[KnockSettlement::Completed],
        (true, false) => &[],
    };
    KnockTurnPlan {
        deliver: false,
        settlements,
        failure: None,
    }
}

// The settle timers come first: each names the step that stalled. Expiry is the last exit,
// for a turn that started and never ended (hallway-knock D4).
fn turn_failure(facts: &KnockTurnFacts) -> Option<&'static str> {
    let delivered_for = |limit: u64| {
        facts
            .delivered_elapsed_ms
            .is_some_and(|elapsed| elapsed >= limit)
    };
    let ended_for = |limit: u64| {
        facts
            .ended_elapsed_ms
            .is_some_and(|elapsed| elapsed >= limit)
    };
    let expired = facts.expires_in_ms.is_some_and(|left| left <= 0);

    if !facts.start_settled && delivered_for(START_SETTLE_LIMIT_MS) {
        Some("recipient turn start did not settle within 25000ms")
    } else if !facts.turn_started && delivered_for(TURN_START_LIMIT_MS) {
        Some("recipient turn did not start within 60000ms")
    } else if facts.turn_ended && ended_for(COMPLETION_SETTLE_LIMIT_MS) {
        Some("recipient turn completion did not settle within 25000ms")
    } else if facts.turn_started && !facts.turn_ended && expired {
        Some("recipient turn did not end before the knock expired")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delivered() -> KnockTurnFacts {
        KnockTurnFacts {
            delivered: true,
            delivered_elapsed_ms: Some(0),
            ..Default::default()
        }
    }

    #[test]
    fn deadlines_preserve_acknowledgement_precedence() {
        let ended = KnockTurnFacts {
            turn_started: true,
            turn_ended: true,
            ended_elapsed_ms: Some(0),
            ..delivered()
        };

        let late = plan_knock_turn(&KnockTurnFacts {
            delivered_elapsed_ms: Some(START_SETTLE_LIMIT_MS),
            ..ended
        });
        assert!(late.failure.is_some());
        assert!(late.settlements.is_empty());

        let in_time = plan_knock_turn(&KnockTurnFacts {
            delivered_elapsed_ms: Some(START_SETTLE_LIMIT_MS - 1),
            ..ended
        });
        assert_eq!(
            in_time.settlements,
            &[KnockSettlement::Started, KnockSettlement::Completed]
        );
    }

    #[test]
    fn a_started_turn_that_never_ends_fails_when_the_knock_expires() {
        let running = KnockTurnFacts {
            start_settled: true,
            turn_started: true,
            ..delivered()
        };

        let before = plan_knock_turn(&KnockTurnFacts {
            expires_in_ms: Some(1),
            ..running
        });
        assert_eq!(before.failure, None);
        assert!(before.settlements.is_empty());

        let at_expiry = plan_knock_turn(&KnockTurnFacts {
            expires_in_ms: Some(0),
            ..running
        });
        assert_eq!(
            at_expiry.failure,
            Some("recipient turn did not end before the knock expired")
        );
    }

    #[test]
    fn expiry_never_fails_an_ended_turn_that_is_settling() {
        let ended = KnockTurnFacts {
            start_settled: true,
            turn_started: true,
            turn_ended: true,
            ended_elapsed_ms: Some(0),
            expires_in_ms: Some(-5_000),
            ..delivered()
        };

        let plan = plan_knock_turn(&ended);
        assert_eq!(plan.failure, None);
        assert_eq!(plan.settlements, &[KnockSettlement::Completed]);
    }
}

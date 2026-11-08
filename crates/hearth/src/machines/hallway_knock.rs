//! The SQL status of one Hallway Knock and every legal move between statuses.
//! Contract: `docs/state-machines/hallway-knock.md`. The store asks `knock_step` before each UPDATE.

use crate::hallway::HallwayKnockOutcome;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HallwayKnockStatus {
    Pending,
    Claimed,
    Started,
    Completed,
    Failed,
    /// Nobody claimed the turn before the exchange clock ran out (ruling R1).
    Expired,
}

impl HallwayKnockStatus {
    pub const ALL: [Self; 6] = [
        Self::Pending,
        Self::Claimed,
        Self::Started,
        Self::Completed,
        Self::Failed,
        Self::Expired,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Claimed => "claimed",
            Self::Started => "started",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Expired => "expired",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnockEvent {
    /// A recipient presence takes the turn. On `Claimed` this is a reclaim; the store offers it
    /// only after the holder's lease has lapsed.
    Claim,
    /// The lease holder reports on its turn.
    Settle(HallwayKnockOutcome),
    /// The exchange clock (`expires_at`) ran out.
    Expire,
}

/// The status `event` moves a Knock to, or `None` when the move is illegal.
pub const fn knock_step(
    status: HallwayKnockStatus,
    event: KnockEvent,
) -> Option<HallwayKnockStatus> {
    use HallwayKnockOutcome::{Completed, Failed, Started};
    use HallwayKnockStatus as S;
    use KnockEvent::{Claim, Expire, Settle};

    match (status, event) {
        (S::Pending | S::Claimed, Claim) => Some(S::Claimed),
        (S::Claimed, Settle(Started)) => Some(S::Started),
        (S::Started, Settle(Completed)) => Some(S::Completed),
        (S::Claimed | S::Started, Settle(Failed) | Expire) => Some(S::Failed),
        (S::Pending, Expire) => Some(S::Expired),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use HallwayKnockStatus as S;

    const EVENTS: [KnockEvent; 5] = [
        KnockEvent::Claim,
        KnockEvent::Settle(HallwayKnockOutcome::Started),
        KnockEvent::Settle(HallwayKnockOutcome::Completed),
        KnockEvent::Settle(HallwayKnockOutcome::Failed),
        KnockEvent::Expire,
    ];

    #[test]
    fn every_status_round_trips_through_its_sql_name() {
        for status in S::ALL {
            assert_eq!(S::parse(status.as_str()), Some(status));
        }
        assert_eq!(S::parse("Pending"), None);
    }

    #[test]
    fn terminal_statuses_have_no_exit() {
        for status in [S::Completed, S::Failed, S::Expired] {
            for event in EVENTS {
                assert_eq!(knock_step(status, event), None, "{status:?} on {event:?}");
            }
        }
    }

    // D1 and R1: an unclaimed Knock leaves `pending` at expiry, as `expired`, never `failed`.
    #[test]
    fn expiry_closes_every_live_status() {
        assert_eq!(knock_step(S::Pending, KnockEvent::Expire), Some(S::Expired));
        assert_eq!(knock_step(S::Claimed, KnockEvent::Expire), Some(S::Failed));
        assert_eq!(knock_step(S::Started, KnockEvent::Expire), Some(S::Failed));
    }

    // L4: a status moves only forward. The lease holder cannot skip `started`.
    #[test]
    fn settle_moves_follow_the_turn() {
        let settle = |status, outcome| knock_step(status, KnockEvent::Settle(outcome));

        assert_eq!(settle(S::Claimed, HallwayKnockOutcome::Completed), None);
        assert_eq!(settle(S::Started, HallwayKnockOutcome::Started), None);
        assert_eq!(settle(S::Pending, HallwayKnockOutcome::Failed), None);
        assert_eq!(knock_step(S::Started, KnockEvent::Claim), None);
    }
}

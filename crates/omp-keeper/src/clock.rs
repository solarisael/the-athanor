//! The House clock.
//!
//! The concern: which instant a stage ends at. The keeper owns no stage seconds
//! of its own. `restart_status` publishes absolute RFC3339 instants — the
//! substrate computes them in Postgres and hands them over as text
//! (akasha/src/restart/mod.rs `STATUS_COLUMNS`) — and this module is the only
//! place that turns one into a comparable instant and asks whether it has
//! passed.
//!
//! A number of seconds appears here in exactly one role: the net for a House
//! that published no instant at all, so a missing field can never mean "wait
//! forever". It is never the keeper's preferred clock.

use anyhow::{Context, Result};
use chrono::{DateTime, TimeDelta, Utc};

/// One stage deadline, and which clock named it. The source is not decoration:
/// the console says which clock it obeyed, so a kill can be read back later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deadline {
    /// The absolute instant the House published on the intent.
    House(DateTime<Utc>),
    /// No instant on the wire, so the keeper's own net from a stage's seconds.
    Net(DateTime<Utc>),
}

impl Deadline {
    pub fn at(self) -> DateTime<Utc> {
        match self {
            Self::House(at) | Self::Net(at) => at,
        }
    }

    /// Strictly past: an intent standing exactly on its deadline still holds the
    /// instant it is standing on.
    pub fn has_passed(self, now: DateTime<Utc>) -> bool {
        now > self.at()
    }

    pub fn source(self) -> &'static str {
        match self {
            Self::House(_) => "the House deadline",
            Self::Net(_) => "a keeper fallback deadline",
        }
    }
}

/// Read an instant the House published. The substrate writes
/// `DateTime<Utc>::to_rfc3339`, so an offset is always present.
pub fn house_instant(published: &str) -> Result<DateTime<Utc>> {
    let parsed = DateTime::parse_from_rfc3339(published.trim())
        .with_context(|| format!("the House published an unreadable instant: {published}"))?;
    Ok(parsed.with_timezone(&Utc))
}

/// The deadline for one stage: the instant the House named, or `now + seconds`
/// when it named none.
pub fn deadline(published: Option<&str>, now: DateTime<Utc>, net_secs: i64) -> Result<Deadline> {
    match published {
        Some(published) => Ok(Deadline::House(house_instant(published)?)),
        None => {
            let span = TimeDelta::try_seconds(net_secs.max(0))
                .with_context(|| format!("{net_secs} seconds is not a usable stage length"))?;
            Ok(Deadline::Net(now + span))
        }
    }
}

/// The instant the House published, as a deadline that names the House as its
/// source. The relaunching stage takes only this door: it has no net, because a
/// locally minted window there would give a silent successor more time than the
/// House allowed. See `keeper::relaunching_window`.
pub fn house_deadline(published: &str) -> Result<Deadline> {
    Ok(Deadline::House(house_instant(published)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime<Utc> {
        house_instant(text).expect("a readable instant")
    }

    #[test]
    fn a_published_instant_is_read_in_utc_whatever_offset_it_carries() {
        // to_rfc3339 on a Utc value writes +00:00; a shifted offset is the same instant
        assert_eq!(
            at("2026-08-25T18:00:00+00:00"),
            at("2026-08-25T20:00:00+02:00")
        );
        assert_eq!(at("2026-08-25T18:00:00Z"), at("2026-08-25T18:00:00+00:00"));
        assert!(house_instant("not an instant").is_err());
        assert!(house_instant("").is_err());
    }

    #[test]
    fn the_deadline_is_the_houses_instant_whenever_the_house_named_one() {
        let now = at("2026-08-25T18:00:00Z");
        let published = deadline(Some("2026-08-25T18:00:30Z"), now, 600).expect("a deadline");
        assert_eq!(published, Deadline::House(at("2026-08-25T18:00:30Z")));
        assert!(
            !published.has_passed(now),
            "thirty seconds of House time is not the keeper's 600"
        );
        assert!(published.has_passed(at("2026-08-25T18:00:31Z")));
    }

    #[test]
    fn an_instant_already_in_the_past_has_passed_the_moment_it_is_read() {
        // the keeper starting late is the case that used to restart the clock
        let now = at("2026-08-25T18:05:00Z");
        let published = deadline(Some("2026-08-25T18:00:00Z"), now, 60).expect("a deadline");
        assert!(published.has_passed(now));
    }

    #[test]
    fn the_net_only_catches_a_house_that_published_no_instant() {
        let now = at("2026-08-25T18:00:00Z");
        let net = deadline(None, now, 60).expect("a deadline");
        assert_eq!(net, Deadline::Net(at("2026-08-25T18:01:00Z")));
        assert!(!net.has_passed(at("2026-08-25T18:01:00Z")));
        assert!(net.has_passed(at("2026-08-25T18:01:01Z")));
        assert_eq!(net.source(), "a keeper fallback deadline");
        // a nonsense stage length becomes "now", never a panic and never forever
        assert_eq!(
            deadline(None, now, -30).expect("a deadline"),
            Deadline::Net(now)
        );
    }
}

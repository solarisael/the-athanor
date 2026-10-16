use super::error::{Error, invalid};
use super::prepare::identity;
use chrono::{Datelike, NaiveDate};
use hearth::orihon::{MAX_READ_RECORDS, ReadReceipt, ReadRequest, Record};

pub(super) struct ReadBounds {
    pub request: ReadRequest,
}

pub(super) fn read_bounds(room: &str, request: ReadRequest) -> Result<ReadBounds, Error> {
    identity(room, "room")?;
    if request.limit == 0 || request.limit > MAX_READ_RECORDS {
        return Err(invalid("read limit must be 1 through 200"));
    }
    if request.after_seq.is_some_and(|value| value < 0)
        || request.until_seq.is_some_and(|value| value < 0)
        || request.turn_no.is_some_and(|value| value < 0)
    {
        return Err(invalid("read ordinals must be nonnegative"));
    }
    if matches!((request.after_seq, request.until_seq), (Some(after), Some(until)) if until < after)
    {
        return Err(invalid("untilSeq cannot precede afterSeq"));
    }
    if let Some(day) = request.day.as_deref() {
        let date = NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .map_err(|_| invalid("day must be YYYY-MM-DD"))?;
        if date.format("%Y-%m-%d").to_string() != day || !(1..=9999).contains(&date.year()) {
            return Err(invalid("day must be YYYY-MM-DD"));
        }
    }
    Ok(ReadBounds { request })
}

impl ReadBounds {
    pub fn includes(&self, record: &Record) -> bool {
        self.request
            .after_seq
            .is_none_or(|after| record.seq > after)
            && self
                .request
                .until_seq
                .is_none_or(|until| record.seq <= until)
            && self
                .request
                .turn_no
                .is_none_or(|turn| record.turn_no == turn)
            && self
                .request
                .day
                .as_deref()
                .is_none_or(|day| record.said_at.starts_with(day))
    }

    pub fn receipt(&self, mut rows: Vec<Record>) -> ReadReceipt {
        let has_more = rows.len() > self.request.limit;
        rows.truncate(self.request.limit);
        let next_after = if has_more {
            rows.last().map(|row| row.seq)
        } else {
            None
        };
        ReadReceipt {
            rows,
            has_more,
            next_after,
        }
    }
}

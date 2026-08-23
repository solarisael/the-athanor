use crate::config::{PATH_DATE_RE, STITCHED_PATH_DATE_RE};
use chrono::NaiveDate;

pub(crate) fn derive_dates(path: &str, primary_date: NaiveDate) -> Vec<NaiveDate> {
    let mut out = vec![primary_date];
    for captures in PATH_DATE_RE.captures_iter(path) {
        if let (Ok(year), Ok(month), Ok(day)) = (
            captures[1].parse(),
            captures[2].parse(),
            captures[3].parse(),
        ) && let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
        {
            out.push(date);
        }
    }
    for captures in STITCHED_PATH_DATE_RE.captures_iter(path) {
        if let (Ok(year), Ok(month), Ok(day), Ok(hour)) = (
            captures[1].parse::<i32>(),
            captures[2].parse::<u32>(),
            captures[3].parse::<u32>(),
            captures[4].parse::<u32>(),
        ) && hour < 24
            && let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
        {
            out.push(date + chrono::Duration::days(1));
        }
    }
    out.sort();
    out.dedup();
    out
}

use super::evaluator_timezone::format_offset;
use chrono::{DateTime, FixedOffset, NaiveDateTime, NaiveTime, Timelike};

pub(super) fn format_time_literal(time: NaiveTime, include_seconds: bool) -> String {
    let nanos = time.nanosecond();
    if !include_seconds && nanos == 0 && time.second() == 0 {
        return format!("{:02}:{:02}", time.hour(), time.minute());
    }
    if nanos == 0 {
        format!(
            "{:02}:{:02}:{:02}",
            time.hour(),
            time.minute(),
            time.second()
        )
    } else {
        let mut frac = format!("{nanos:09}");
        while frac.ends_with('0') {
            frac.pop();
        }
        format!(
            "{:02}:{:02}:{:02}.{frac}",
            time.hour(),
            time.minute(),
            time.second()
        )
    }
}

pub(super) fn format_datetime_literal(dt: NaiveDateTime, include_seconds: bool) -> String {
    format!(
        "{}T{}",
        dt.date().format("%Y-%m-%d"),
        format_time_literal(dt.time(), include_seconds)
    )
}

pub(super) fn format_datetime_with_offset_literal(
    dt: DateTime<FixedOffset>,
    include_seconds: bool,
) -> String {
    format!(
        "{}{}",
        format_datetime_literal(dt.naive_local(), include_seconds),
        format_offset(*dt.offset())
    )
}

use chrono::{Datelike, FixedOffset, NaiveDate, NaiveTime, Timelike};
use std::cmp::Ordering;

pub(super) fn compare_time_with_offset(
    lt: NaiveTime,
    lo: FixedOffset,
    rt: NaiveTime,
    ro: FixedOffset,
) -> Ordering {
    let l = time_of_day_nanos(lt) - (lo.local_minus_utc() as i128 * 1_000_000_000);
    let r = time_of_day_nanos(rt) - (ro.local_minus_utc() as i128 * 1_000_000_000);
    l.cmp(&r)
}

pub(super) fn compare_time_of_day(left: NaiveTime, right: NaiveTime) -> Ordering {
    time_of_day_nanos(left).cmp(&time_of_day_nanos(right))
}

pub(super) fn time_of_day_nanos(time: NaiveTime) -> i128 {
    time.num_seconds_from_midnight() as i128 * 1_000_000_000 + time.nanosecond() as i128
}

pub(super) fn shift_time_of_day(time: NaiveTime, delta_nanos: i64) -> Option<NaiveTime> {
    let day_nanos: i128 = 86_400_000_000_000;
    let current = time_of_day_nanos(time);
    let shifted = (current + delta_nanos as i128).rem_euclid(day_nanos);
    let secs = (shifted / 1_000_000_000) as u32;
    let nanos = (shifted % 1_000_000_000) as u32;
    NaiveTime::from_num_seconds_from_midnight_opt(secs, nanos)
}

pub(super) fn add_months(date: NaiveDate, delta_months: i32) -> Option<NaiveDate> {
    if delta_months == 0 {
        return Some(date);
    }

    let total_months = date.year() * 12 + (date.month0() as i32) + delta_months;
    let new_year = total_months.div_euclid(12);
    let new_month = (total_months.rem_euclid(12) + 1) as u32;

    let mut day = date.day();
    loop {
        if let Some(d) = NaiveDate::from_ymd_opt(new_year, new_month, day) {
            return Some(d);
        }
        if day == 1 {
            return None;
        }
        day -= 1;
    }
}

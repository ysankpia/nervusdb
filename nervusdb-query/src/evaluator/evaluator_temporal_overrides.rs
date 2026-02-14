use super::Value;
use super::evaluator_temporal_map::{map_i32, map_u32, weekday_from_cypher};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use std::collections::BTreeMap;

pub(super) fn apply_date_overrides(
    date: NaiveDate,
    map: Option<&BTreeMap<String, Value>>,
) -> Option<NaiveDate> {
    let mut current = date;

    if let Some(overrides) = map {
        if let Some(week) = map_u32(overrides, "week") {
            let year = map_i32(overrides, "year").unwrap_or_else(|| current.iso_week().year());
            let day_of_week = map_u32(overrides, "dayOfWeek").unwrap_or(1);
            let weekday = weekday_from_cypher(day_of_week)?;
            current = NaiveDate::from_isoywd_opt(year, week, weekday)?;
        } else if let Some(day_of_week) = map_u32(overrides, "dayOfWeek") {
            let weekday = weekday_from_cypher(day_of_week)?;
            let week_start = current.checked_sub_signed(Duration::days(i64::from(
                current.weekday().num_days_from_monday(),
            )))?;
            current = week_start
                .checked_add_signed(Duration::days(i64::from(weekday.num_days_from_monday())))?;
        }

        let year = map_i32(overrides, "year").unwrap_or_else(|| current.year());

        if let Some(ordinal_day) = map_u32(overrides, "ordinalDay") {
            return NaiveDate::from_yo_opt(year, ordinal_day);
        }

        if let Some(quarter) = map_u32(overrides, "quarter") {
            if !(1..=4).contains(&quarter) {
                return None;
            }
            let start_month = ((quarter - 1) * 3) + 1;
            let start_date = NaiveDate::from_ymd_opt(year, start_month, 1)?;
            if let Some(day_of_quarter) = map_u32(overrides, "dayOfQuarter") {
                return start_date
                    .checked_add_signed(Duration::days(i64::from(day_of_quarter) - 1));
            }
            let month_in_quarter = current.month0() % 3;
            let month = map_u32(overrides, "month").unwrap_or(start_month + month_in_quarter);
            let day = map_u32(overrides, "day").unwrap_or_else(|| current.day());
            return NaiveDate::from_ymd_opt(year, month, day);
        }

        let month = map_u32(overrides, "month").unwrap_or_else(|| current.month());
        let day = map_u32(overrides, "day").unwrap_or_else(|| current.day());
        current = NaiveDate::from_ymd_opt(year, month, day)?;
    }

    Some(current)
}

pub(super) fn apply_time_overrides(
    time: NaiveTime,
    map: Option<&BTreeMap<String, Value>>,
) -> Option<(NaiveTime, bool)> {
    let mut hour = time.hour();
    let mut minute = time.minute();
    let mut second = time.second();
    let mut nanosecond = time.nanosecond();

    let mut include_seconds = second != 0 || nanosecond != 0;

    if let Some(overrides) = map {
        if let Some(v) = map_u32(overrides, "hour") {
            hour = v;
        }
        if let Some(v) = map_u32(overrides, "minute") {
            minute = v;
        }
        if let Some(v) = map_u32(overrides, "second") {
            second = v;
            include_seconds = true;
        }
        if let Some(v) = map_u32(overrides, "millisecond") {
            if v >= 1_000 {
                return None;
            }
            nanosecond = v.saturating_mul(1_000_000) + (nanosecond % 1_000_000);
            include_seconds = true;
        }
        if let Some(v) = map_u32(overrides, "microsecond") {
            if v >= 1_000_000 {
                return None;
            }
            nanosecond = v.saturating_mul(1_000) + (nanosecond % 1_000);
            include_seconds = true;
        }
        if let Some(v) = map_u32(overrides, "nanosecond") {
            if v >= 1_000_000_000 {
                return None;
            }
            nanosecond = if v < 1_000 {
                (nanosecond / 1_000) * 1_000 + v
            } else {
                v
            };
            include_seconds = true;
        }
    }

    NaiveTime::from_hms_nano_opt(hour, minute, second, nanosecond).map(|t| (t, include_seconds))
}

pub(super) fn apply_datetime_overrides(
    dt: NaiveDateTime,
    map: Option<&BTreeMap<String, Value>>,
) -> Option<(NaiveDate, NaiveTime, bool)> {
    let date = apply_date_overrides(dt.date(), map)?;
    let (time, include_seconds) = apply_time_overrides(dt.time(), map)?;
    Some((date, time, include_seconds))
}

pub(super) fn truncate_date_literal(unit: &str, date: NaiveDate) -> Option<NaiveDate> {
    match unit {
        "day" => Some(date),
        "week" => {
            let delta = i64::from(date.weekday().num_days_from_monday());
            date.checked_sub_signed(Duration::days(delta))
        }
        "weekyear" => NaiveDate::from_isoywd_opt(date.iso_week().year(), 1, chrono::Weekday::Mon),
        "month" => NaiveDate::from_ymd_opt(date.year(), date.month(), 1),
        "quarter" => {
            let month = ((date.month0() / 3) * 3) + 1;
            NaiveDate::from_ymd_opt(date.year(), month, 1)
        }
        "year" => NaiveDate::from_ymd_opt(date.year(), 1, 1),
        "decade" => {
            let year = date.year().div_euclid(10) * 10;
            NaiveDate::from_ymd_opt(year, 1, 1)
        }
        "century" => NaiveDate::from_ymd_opt(date.year().div_euclid(100) * 100, 1, 1),
        "millennium" => NaiveDate::from_ymd_opt(date.year().div_euclid(1000) * 1000, 1, 1),
        _ => None,
    }
}

pub(super) fn truncate_time_literal(unit: &str, time: NaiveTime) -> Option<NaiveTime> {
    let hour = time.hour();
    let minute = time.minute();
    let second = time.second();
    let nanos = time.nanosecond();

    match unit {
        "day" => NaiveTime::from_hms_nano_opt(0, 0, 0, 0),
        "hour" => NaiveTime::from_hms_nano_opt(hour, 0, 0, 0),
        "minute" => NaiveTime::from_hms_nano_opt(hour, minute, 0, 0),
        "second" => NaiveTime::from_hms_nano_opt(hour, minute, second, 0),
        "millisecond" => {
            let truncated = (nanos / 1_000_000) * 1_000_000;
            NaiveTime::from_hms_nano_opt(hour, minute, second, truncated)
        }
        "microsecond" => {
            let truncated = (nanos / 1_000) * 1_000;
            NaiveTime::from_hms_nano_opt(hour, minute, second, truncated)
        }
        _ => None,
    }
}

pub(super) fn truncate_naive_datetime_literal(
    unit: &str,
    dt: NaiveDateTime,
) -> Option<NaiveDateTime> {
    if matches!(
        unit,
        "millennium"
            | "century"
            | "decade"
            | "year"
            | "weekyear"
            | "quarter"
            | "month"
            | "week"
            | "day"
    ) {
        let date = truncate_date_literal(unit, dt.date())?;
        return date.and_hms_nano_opt(0, 0, 0, 0);
    }

    let time = truncate_time_literal(unit, dt.time())?;
    Some(dt.date().and_time(time))
}

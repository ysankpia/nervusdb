use super::TemporalValue;
use super::Value;
use super::evaluator_temporal_parse::parse_temporal_string;
use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Timelike, Weekday};
use std::collections::BTreeMap;

pub(super) fn make_date_from_map(map: &BTreeMap<String, Value>) -> Option<NaiveDate> {
    let base_date = match map.get("date").or_else(|| map.get("datetime")) {
        Some(Value::String(s)) => match parse_temporal_string(s) {
            Some(TemporalValue::Date(date)) => Some(date),
            Some(TemporalValue::LocalDateTime(dt)) => Some(dt.date()),
            Some(TemporalValue::DateTime(dt)) => Some(dt.naive_local().date()),
            _ => None,
        },
        _ => None,
    };

    if let Some(week) = map_u32(map, "week") {
        let year = map_i32(map, "year").or_else(|| base_date.map(|d| d.iso_week().year()))?;
        let day_of_week = map_u32(map, "dayOfWeek")
            .or_else(|| base_date.map(|d| cypher_day_of_week(d.weekday())))
            .unwrap_or(1);
        let weekday = weekday_from_cypher(day_of_week)?;
        return NaiveDate::from_isoywd_opt(year, week, weekday);
    }

    let year = map_i32(map, "year").or_else(|| base_date.map(|d| d.year()))?;

    if let Some(ordinal_day) = map_u32(map, "ordinalDay") {
        return NaiveDate::from_yo_opt(year, ordinal_day);
    }

    if let Some(quarter) = map_u32(map, "quarter") {
        if !(1..=4).contains(&quarter) {
            return None;
        }
        let start_month = ((quarter - 1) * 3) + 1;
        let start_date = NaiveDate::from_ymd_opt(year, start_month, 1)?;
        if let Some(day_of_quarter) = map_u32(map, "dayOfQuarter") {
            return start_date.checked_add_signed(Duration::days(i64::from(day_of_quarter) - 1));
        }
        let month_in_quarter = base_date.map(|d| d.month0() % 3).unwrap_or(0);
        let month = map_u32(map, "month").unwrap_or(start_month + month_in_quarter);
        let day = map_u32(map, "day")
            .or_else(|| base_date.map(|d| d.day()))
            .unwrap_or(1);
        return NaiveDate::from_ymd_opt(year, month, day);
    }

    let month = map_u32(map, "month")
        .or_else(|| base_date.map(|d| d.month()))
        .unwrap_or(1);
    let day = map_u32(map, "day")
        .or_else(|| base_date.map(|d| d.day()))
        .unwrap_or(1);
    NaiveDate::from_ymd_opt(year, month, day)
}

pub(super) fn make_time_from_map(map: &BTreeMap<String, Value>) -> Option<(NaiveTime, bool)> {
    let base_time = match map.get("time") {
        Some(Value::String(s)) => match parse_temporal_string(s) {
            Some(TemporalValue::LocalTime(t)) => Some(t),
            Some(TemporalValue::Time { time, .. }) => Some(time),
            Some(TemporalValue::LocalDateTime(dt)) => Some(dt.time()),
            Some(TemporalValue::DateTime(dt)) => Some(dt.naive_local().time()),
            _ => None,
        },
        _ => None,
    };

    let mut hour = base_time.map(|t| t.hour()).unwrap_or(0);
    let mut minute = base_time.map(|t| t.minute()).unwrap_or(0);
    let mut second = base_time.map(|t| t.second()).unwrap_or(0);
    let mut nanos = base_time.map(|t| t.nanosecond()).unwrap_or(0);

    if let Some(v) = map_u32(map, "hour") {
        hour = v;
    }
    if let Some(v) = map_u32(map, "minute") {
        minute = v;
    }
    if let Some(v) = map_u32(map, "second") {
        second = v;
    }

    let has_subsecond = map.contains_key("millisecond")
        || map.contains_key("microsecond")
        || map.contains_key("nanosecond");

    if has_subsecond {
        nanos = 0;
        if let Some(v) = map_u32(map, "millisecond") {
            nanos = nanos.saturating_add(v.saturating_mul(1_000_000));
        }
        if let Some(v) = map_u32(map, "microsecond") {
            nanos = nanos.saturating_add(v.saturating_mul(1_000));
        }
        if let Some(v) = map_u32(map, "nanosecond") {
            nanos = nanos.saturating_add(v);
        }
    }

    let include_seconds = map.contains_key("second")
        || map.contains_key("millisecond")
        || map.contains_key("microsecond")
        || map.contains_key("nanosecond")
        || second != 0
        || nanos != 0;

    NaiveTime::from_hms_nano_opt(hour, minute, second, nanos).map(|t| (t, include_seconds))
}

pub(super) fn weekday_from_cypher(day_of_week: u32) -> Option<Weekday> {
    match day_of_week {
        1 => Some(Weekday::Mon),
        2 => Some(Weekday::Tue),
        3 => Some(Weekday::Wed),
        4 => Some(Weekday::Thu),
        5 => Some(Weekday::Fri),
        6 => Some(Weekday::Sat),
        7 => Some(Weekday::Sun),
        _ => None,
    }
}

pub(super) fn cypher_day_of_week(day: Weekday) -> u32 {
    day.number_from_monday()
}

pub(super) fn map_i64(map: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    match map.get(key) {
        Some(Value::Int(v)) => Some(*v),
        Some(Value::Float(v)) => Some(*v as i64),
        _ => None,
    }
}

pub(super) fn map_i32(map: &BTreeMap<String, Value>, key: &str) -> Option<i32> {
    map_i64(map, key).map(|v| v as i32)
}

pub(super) fn map_u32(map: &BTreeMap<String, Value>, key: &str) -> Option<u32> {
    map_i64(map, key).and_then(|v| if v >= 0 { Some(v as u32) } else { None })
}

pub(super) fn map_string(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match map.get(key) {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

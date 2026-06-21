use super::TemporalValue;
use super::Value;
use super::evaluator_timezone::parse_fixed_offset;
use chrono::{
    DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone,
    Timelike, Weekday,
};
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

pub(super) fn extract_timezone_name(input: &str) -> Option<String> {
    let s = input.trim();
    let start = s.find('[')?;
    let end = s.rfind(']')?;
    if end <= start + 1 {
        return None;
    }
    Some(s[start + 1..end].to_string())
}

pub(super) fn parse_temporal_string(s: &str) -> Option<TemporalValue> {
    let s = s.trim();
    let s_no_zone = s.split('[').next().unwrap_or(s).trim();
    let s_z_to_offset = if s_no_zone.ends_with('Z') {
        Some(format!(
            "{}+00:00",
            &s_no_zone[..s_no_zone.len().saturating_sub(1)]
        ))
    } else {
        None
    };

    if s_no_zone.contains('T') {
        for fmt in [
            "%Y-%m-%dT%H:%M:%S%.f%:z",
            "%Y-%m-%dT%H:%M:%S%.f%z",
            "%Y-%m-%dT%H:%M%:z",
            "%Y-%m-%dT%H:%M%z",
        ] {
            if let Ok(dt) = DateTime::parse_from_str(s_no_zone, fmt) {
                return Some(TemporalValue::DateTime(dt));
            }
            if let Some(normalized) = &s_z_to_offset
                && let Ok(dt) = DateTime::parse_from_str(normalized, fmt)
            {
                return Some(TemporalValue::DateTime(dt));
            }
        }

        for fmt in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"] {
            if let Ok(dt) = NaiveDateTime::parse_from_str(s_no_zone, fmt) {
                return Some(TemporalValue::LocalDateTime(dt));
            }
            if let Some(normalized) = &s_z_to_offset
                && let Ok(dt) = NaiveDateTime::parse_from_str(normalized, fmt)
            {
                return Some(TemporalValue::LocalDateTime(dt));
            }
        }

        if let Some((date_part, time_part)) = s_no_zone.split_once('T') {
            let date = parse_date_literal(date_part)?;

            if time_part.ends_with('Z') {
                let bare = &time_part[..time_part.len().saturating_sub(1)];
                let time = parse_time_literal(bare)?;
                let offset = FixedOffset::east_opt(0).expect("UTC offset");
                let dt = offset.from_local_datetime(&date.and_time(time)).single()?;
                return Some(TemporalValue::DateTime(dt));
            }

            if let Some(split_idx) = find_offset_split_index(time_part) {
                let (time_part, offset_part) = time_part.split_at(split_idx);
                let time = parse_time_literal(time_part)?;
                let offset = parse_fixed_offset(offset_part)?;
                let dt = offset.from_local_datetime(&date.and_time(time)).single()?;
                return Some(TemporalValue::DateTime(dt));
            }

            let time = parse_time_literal(time_part)?;
            return Some(TemporalValue::LocalDateTime(date.and_time(time)));
        }
    }

    if let Some(date) = parse_date_literal(s_no_zone) {
        return Some(TemporalValue::Date(date));
    }

    if s_no_zone.ends_with('Z') {
        let time_part = &s_no_zone[..s_no_zone.len().saturating_sub(1)];
        if let Some(time) = parse_time_literal(time_part) {
            let offset = FixedOffset::east_opt(0).expect("UTC offset");
            return Some(TemporalValue::Time { time, offset });
        }
    }

    if let Some(split_idx) = find_offset_split_index(s_no_zone) {
        let (time_part, offset_part) = s_no_zone.split_at(split_idx);
        let time = parse_time_literal(time_part)?;
        let offset = parse_fixed_offset(offset_part)?;
        return Some(TemporalValue::Time { time, offset });
    }

    parse_time_literal(s_no_zone).map(TemporalValue::LocalTime)
}

pub(super) fn find_offset_split_index(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    (1..bytes.len())
        .rev()
        .find(|&idx| bytes[idx] == b'+' || bytes[idx] == b'-')
}

pub(super) fn parse_time_literal(s: &str) -> Option<NaiveTime> {
    let s = s.trim();

    if let Ok(parsed) = NaiveTime::parse_from_str(s, "%H:%M:%S%.f") {
        return Some(parsed);
    }
    if let Ok(parsed) = NaiveTime::parse_from_str(s, "%H:%M") {
        return Some(parsed);
    }

    let (digits, frac) = if let Some((base, fraction)) = s.split_once('.') {
        (base, Some(fraction))
    } else {
        (s, None)
    };

    if !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let nanos = match frac {
        Some(f) => {
            if !f.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            let mut frac_digits = f.chars().take(9).collect::<String>();
            while frac_digits.len() < 9 {
                frac_digits.push('0');
            }
            frac_digits.parse::<u32>().ok()?
        }
        None => 0,
    };

    match digits.len() {
        2 => {
            let hour: u32 = digits[0..2].parse().ok()?;
            NaiveTime::from_hms_nano_opt(hour, 0, 0, nanos)
        }
        4 => {
            let hour: u32 = digits[0..2].parse().ok()?;
            let minute: u32 = digits[2..4].parse().ok()?;
            NaiveTime::from_hms_nano_opt(hour, minute, 0, nanos)
        }
        6 => {
            let hour: u32 = digits[0..2].parse().ok()?;
            let minute: u32 = digits[2..4].parse().ok()?;
            let second: u32 = digits[4..6].parse().ok()?;
            NaiveTime::from_hms_nano_opt(hour, minute, second, nanos)
        }
        _ => None,
    }
}

pub(super) fn parse_date_literal(input: &str) -> Option<NaiveDate> {
    let s = input.trim();

    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(date);
    }

    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y%m%d") {
        return Some(date);
    }

    if let Some((year, week, day_of_week)) = parse_week_date_components(s) {
        let weekday = weekday_from_cypher(day_of_week)?;
        if let Some(date) = NaiveDate::from_isoywd_opt(year, week, weekday) {
            return Some(date);
        }
    }

    if let Some((year, ordinal)) = parse_ordinal_date_components(s)
        && let Some(date) = NaiveDate::from_yo_opt(year, ordinal)
    {
        return Some(date);
    }

    if let Some((year, month)) = parse_year_month_components(s)
        && let Some(date) = NaiveDate::from_ymd_opt(year, month, 1)
    {
        return Some(date);
    }

    if s.len() == 4 && s.chars().all(|ch| ch.is_ascii_digit()) {
        let year: i32 = s.parse().ok()?;
        return NaiveDate::from_ymd_opt(year, 1, 1);
    }

    None
}

fn parse_week_date_components(s: &str) -> Option<(i32, u32, u32)> {
    if let Some((year_part, rest)) = s.split_once("-W") {
        let year: i32 = year_part.parse().ok()?;
        if let Some((week_part, day_part)) = rest.split_once('-') {
            let week: u32 = week_part.parse().ok()?;
            let day: u32 = day_part.parse().ok()?;
            return Some((year, week, day));
        }
        let week: u32 = rest.parse().ok()?;
        return Some((year, week, 1));
    }

    if s.len() == 8 && s.chars().nth(4) == Some('W') {
        let year: i32 = s[0..4].parse().ok()?;
        let week: u32 = s[5..7].parse().ok()?;
        let day: u32 = s[7..8].parse().ok()?;
        return Some((year, week, day));
    }

    if s.len() == 7 && s.chars().nth(4) == Some('W') {
        let year: i32 = s[0..4].parse().ok()?;
        let week: u32 = s[5..7].parse().ok()?;
        return Some((year, week, 1));
    }

    None
}

fn parse_ordinal_date_components(s: &str) -> Option<(i32, u32)> {
    if let Some((year_part, ordinal_part)) = s.split_once('-')
        && year_part.len() == 4
        && ordinal_part.len() == 3
        && year_part.chars().all(|ch| ch.is_ascii_digit())
        && ordinal_part.chars().all(|ch| ch.is_ascii_digit())
    {
        let year: i32 = year_part.parse().ok()?;
        let ordinal: u32 = ordinal_part.parse().ok()?;
        return Some((year, ordinal));
    }

    if s.len() == 7 && s.chars().all(|ch| ch.is_ascii_digit()) {
        let year: i32 = s[0..4].parse().ok()?;
        let tail: u32 = s[4..7].parse().ok()?;
        if (1..=366).contains(&tail) {
            return Some((year, tail));
        }
    }

    None
}

fn parse_year_month_components(s: &str) -> Option<(i32, u32)> {
    if let Some((year_part, month_part)) = s.split_once('-')
        && year_part.len() == 4
        && month_part.len() == 2
        && year_part.chars().all(|ch| ch.is_ascii_digit())
        && month_part.chars().all(|ch| ch.is_ascii_digit())
    {
        let year: i32 = year_part.parse().ok()?;
        let month: u32 = month_part.parse().ok()?;
        return Some((year, month));
    }

    if s.len() == 6 && s.chars().all(|ch| ch.is_ascii_digit()) {
        let year: i32 = s[0..4].parse().ok()?;
        let month: u32 = s[4..6].parse().ok()?;
        return Some((year, month));
    }

    None
}

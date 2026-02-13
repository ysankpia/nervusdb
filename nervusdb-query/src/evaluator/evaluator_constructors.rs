use super::evaluator_duration::{duration_from_map, duration_value, parse_duration_literal};
use super::evaluator_large_temporal::{
    format_large_date_literal, format_large_localdatetime_literal, parse_large_date_literal,
    parse_large_localdatetime_literal,
};
use super::evaluator_numeric::value_as_i64;
use super::evaluator_temporal_format::{
    format_datetime_literal, format_datetime_with_offset_literal, format_time_literal,
};
use super::evaluator_temporal_map::{make_date_from_map, make_time_from_map, map_string};
use super::evaluator_temporal_math::shift_time_of_day;
use super::evaluator_temporal_overrides::{apply_date_overrides, apply_time_overrides};
use super::evaluator_temporal_parse::{
    extract_timezone_name, find_offset_split_index, parse_date_literal, parse_temporal_string,
    parse_time_literal,
};
use super::evaluator_timezone::{
    format_offset, parse_fixed_offset, timezone_named_offset, timezone_named_offset_standard,
};
use super::{TemporalValue, Value};
use chrono::{FixedOffset, NaiveTime, TimeZone, Timelike};

pub(super) fn construct_datetime_from_epoch(args: &[Value]) -> Value {
    let Some(seconds) = args.first().and_then(value_as_i64) else {
        return Value::Null;
    };
    let nanos = args.get(1).and_then(value_as_i64).unwrap_or(0);

    let extra_seconds = nanos.div_euclid(1_000_000_000);
    let nanos_part = nanos.rem_euclid(1_000_000_000) as u32;
    let total_seconds = seconds.saturating_add(extra_seconds);

    let offset = FixedOffset::east_opt(0).expect("UTC offset");
    let Some(dt) = offset.timestamp_opt(total_seconds, nanos_part).single() else {
        return Value::Null;
    };

    Value::String(format_datetime_with_offset_literal(dt, true))
}

pub(super) fn construct_datetime_from_epoch_millis(args: &[Value]) -> Value {
    let Some(millis) = args.first().and_then(value_as_i64) else {
        return Value::Null;
    };

    let seconds = millis.div_euclid(1_000);
    let millis_part = millis.rem_euclid(1_000);
    let nanos = (millis_part as u32) * 1_000_000;

    let offset = FixedOffset::east_opt(0).expect("UTC offset");
    let Some(dt) = offset.timestamp_opt(seconds, nanos).single() else {
        return Value::Null;
    };

    Value::String(format_datetime_with_offset_literal(dt, true))
}

pub(super) fn construct_duration(arg: Option<&Value>) -> Value {
    match arg {
        Some(Value::Map(map)) => duration_value(duration_from_map(map)),
        Some(Value::String(s)) => parse_duration_literal(s)
            .map(duration_value)
            .unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

pub(super) fn construct_date(arg: Option<&Value>) -> Value {
    match arg {
        None => Value::String("1970-01-01".to_string()),
        Some(Value::Map(map)) => make_date_from_map(map)
            .map(|d| Value::String(d.format("%Y-%m-%d").to_string()))
            .unwrap_or(Value::Null),
        Some(Value::String(s)) => {
            if let Some(parsed) = parse_temporal_string(s) {
                let date = match parsed {
                    TemporalValue::Date(date) => Some(date),
                    TemporalValue::LocalDateTime(dt) => Some(dt.date()),
                    TemporalValue::DateTime(dt) => Some(dt.naive_local().date()),
                    _ => None,
                };
                date.map(|d| Value::String(d.format("%Y-%m-%d").to_string()))
                    .unwrap_or(Value::Null)
            } else {
                parse_date_literal(s)
                    .map(|d| Value::String(d.format("%Y-%m-%d").to_string()))
                    .or_else(|| {
                        parse_large_date_literal(s)
                            .map(|d| Value::String(format_large_date_literal(d)))
                    })
                    .unwrap_or(Value::Null)
            }
        }
        _ => Value::Null,
    }
}

pub(super) fn construct_local_time(arg: Option<&Value>) -> Value {
    match arg {
        None => Value::String("00:00".to_string()),
        Some(Value::Map(map)) => make_time_from_map(map)
            .map(|(t, include_seconds)| Value::String(format_time_literal(t, include_seconds)))
            .unwrap_or(Value::Null),
        Some(Value::String(s)) => {
            // Parse time-like strings (e.g. `2140`, `2140-02`) as time first.
            // `parse_temporal_string()` prefers dates for inputs like `2140`, which breaks localtime().
            if let Some((time, _offset)) = parse_time_literal_with_optional_offset(s) {
                let include_seconds = time.second() != 0 || time.nanosecond() != 0;
                return Value::String(format_time_literal(time, include_seconds));
            }

            match parse_temporal_string(s) {
                Some(TemporalValue::LocalTime(t)) => {
                    let include_seconds = t.second() != 0 || t.nanosecond() != 0;
                    Value::String(format_time_literal(t, include_seconds))
                }
                Some(TemporalValue::Time { time, .. }) => {
                    let include_seconds = time.second() != 0 || time.nanosecond() != 0;
                    Value::String(format_time_literal(time, include_seconds))
                }
                Some(TemporalValue::LocalDateTime(dt)) => {
                    let time = dt.time();
                    let include_seconds = time.second() != 0 || time.nanosecond() != 0;
                    Value::String(format_time_literal(time, include_seconds))
                }
                Some(TemporalValue::DateTime(dt)) => {
                    let time = dt.naive_local().time();
                    let include_seconds = time.second() != 0 || time.nanosecond() != 0;
                    Value::String(format_time_literal(time, include_seconds))
                }
                _ => Value::Null,
            }
        }
        _ => Value::Null,
    }
}

pub(super) fn construct_time(arg: Option<&Value>) -> Value {
    match arg {
        None => Value::String("00:00Z".to_string()),
        Some(Value::Map(map)) => {
            let Some((mut time, include_seconds)) = make_time_from_map(map) else {
                return Value::Null;
            };

            let base_offset = map
                .get("time")
                .and_then(|v| match v {
                    Value::String(raw) => parse_temporal_string(raw),
                    _ => None,
                })
                .and_then(|parsed| match parsed {
                    TemporalValue::Time { offset, .. } => Some(offset),
                    TemporalValue::DateTime(dt) => Some(*dt.offset()),
                    _ => None,
                });

            let mut zone_suffix: Option<String> = None;
            let offset = if let Some(tz) = map_string(map, "timezone") {
                if let Some(parsed) = parse_fixed_offset(&tz) {
                    if let Some(base) = base_offset {
                        let delta = parsed.local_minus_utc() - base.local_minus_utc();
                        if let Some(shifted) =
                            shift_time_of_day(time, i64::from(delta) * 1_000_000_000)
                        {
                            time = shifted;
                        }
                    }
                    parsed
                } else if let Some(named) = timezone_named_offset_standard(&tz) {
                    if let Some(base) = base_offset {
                        let delta = named.local_minus_utc() - base.local_minus_utc();
                        if let Some(shifted) =
                            shift_time_of_day(time, i64::from(delta) * 1_000_000_000)
                        {
                            time = shifted;
                        }
                    }
                    zone_suffix = Some(tz);
                    named
                } else {
                    return Value::Null;
                }
            } else {
                base_offset.unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC offset"))
            };

            let mut out = format!(
                "{}{}",
                format_time_literal(time, include_seconds),
                format_offset(offset)
            );
            if let Some(zone) = zone_suffix {
                out.push('[');
                out.push_str(&zone);
                out.push(']');
            }
            Value::String(out)
        }
        Some(Value::String(s)) => {
            // Parse time-like strings (e.g. `2140-02`) as time first.
            // `parse_temporal_string()` prefers dates for inputs like `2140-02`, which breaks time().
            if let Some((time, offset)) = parse_time_literal_with_optional_offset(s) {
                let include_seconds = time.second() != 0 || time.nanosecond() != 0;
                let offset = offset.unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC"));
                return Value::String(format!(
                    "{}{}",
                    format_time_literal(time, include_seconds),
                    format_offset(offset)
                ));
            }

            match parse_temporal_string(s) {
                Some(TemporalValue::Time { time, offset }) => {
                    let include_seconds = time.second() != 0 || time.nanosecond() != 0;
                    Value::String(format!(
                        "{}{}",
                        format_time_literal(time, include_seconds),
                        format_offset(offset)
                    ))
                }
                Some(TemporalValue::LocalTime(time)) => {
                    let include_seconds = time.second() != 0 || time.nanosecond() != 0;
                    let offset = FixedOffset::east_opt(0).expect("UTC offset");
                    Value::String(format!(
                        "{}{}",
                        format_time_literal(time, include_seconds),
                        format_offset(offset)
                    ))
                }
                Some(TemporalValue::LocalDateTime(dt)) => {
                    let time = dt.time();
                    let include_seconds = time.second() != 0 || time.nanosecond() != 0;
                    Value::String(format!("{}Z", format_time_literal(time, include_seconds)))
                }
                Some(TemporalValue::DateTime(dt)) => {
                    let time = dt.naive_local().time();
                    let include_seconds = time.second() != 0 || time.nanosecond() != 0;
                    Value::String(format!(
                        "{}{}",
                        format_time_literal(time, include_seconds),
                        format_offset(*dt.offset())
                    ))
                }
                _ => Value::Null,
            }
        }
        _ => Value::Null,
    }
}

fn parse_time_literal_with_optional_offset(raw: &str) -> Option<(NaiveTime, Option<FixedOffset>)> {
    let s = raw.trim();
    let s_no_zone = s.split('[').next().unwrap_or(s).trim();

    if s_no_zone.is_empty() {
        return None;
    }

    if s_no_zone.ends_with('Z') {
        let bare = &s_no_zone[..s_no_zone.len().saturating_sub(1)];
        let time = parse_time_literal(bare)?;
        let offset = FixedOffset::east_opt(0).expect("UTC offset");
        return Some((time, Some(offset)));
    }

    if let Some(split_idx) = find_offset_split_index(s_no_zone) {
        let (time_part, offset_part) = s_no_zone.split_at(split_idx);
        let time = parse_time_literal(time_part)?;
        let offset = parse_fixed_offset(offset_part)?;
        return Some((time, Some(offset)));
    }

    let time = parse_time_literal(s_no_zone)?;
    Some((time, None))
}

pub(super) fn construct_local_datetime(arg: Option<&Value>) -> Value {
    match arg {
        None => Value::String("1970-01-01T00:00".to_string()),
        Some(Value::Map(map)) => {
            if let Some(Value::String(raw)) = map.get("datetime")
                && let Some(parsed) = parse_temporal_string(raw)
            {
                let (base_date, base_time) = match parsed {
                    TemporalValue::DateTime(dt) => {
                        (dt.naive_local().date(), dt.naive_local().time())
                    }
                    TemporalValue::LocalDateTime(dt) => (dt.date(), dt.time()),
                    TemporalValue::Date(date) => (
                        date,
                        NaiveTime::from_hms_opt(0, 0, 0).expect("valid midnight"),
                    ),
                    _ => return Value::Null,
                };
                let Some(date) = apply_date_overrides(base_date, Some(map)) else {
                    return Value::Null;
                };
                let Some((time, include_seconds)) = apply_time_overrides(base_time, Some(map))
                else {
                    return Value::Null;
                };
                return Value::String(format_datetime_literal(
                    date.and_time(time),
                    include_seconds,
                ));
            }

            let Some(date) = make_date_from_map(map) else {
                return Value::Null;
            };
            let Some((time, include_seconds)) = make_time_from_map(map) else {
                return Value::Null;
            };
            let dt = date.and_time(time);
            Value::String(format_datetime_literal(dt, include_seconds))
        }
        Some(Value::String(s)) => match parse_temporal_string(s) {
            Some(TemporalValue::LocalDateTime(dt)) => {
                let include_seconds = dt.time().second() != 0 || dt.time().nanosecond() != 0;
                Value::String(format_datetime_literal(dt, include_seconds))
            }
            Some(TemporalValue::DateTime(dt)) => {
                let local = dt.naive_local();
                let include_seconds = local.time().second() != 0 || local.time().nanosecond() != 0;
                Value::String(format_datetime_literal(local, include_seconds))
            }
            _ => parse_large_localdatetime_literal(s)
                .map(|dt| Value::String(format_large_localdatetime_literal(dt)))
                .unwrap_or(Value::Null),
        },
        _ => Value::Null,
    }
}

pub(super) fn construct_datetime(arg: Option<&Value>) -> Value {
    match arg {
        None => Value::String("1970-01-01T00:00Z".to_string()),
        Some(Value::Map(map)) => {
            let mut source_zone: Option<String> = None;
            let (mut date, mut time, mut include_seconds, base_offset) =
                if let Some(Value::String(raw)) = map.get("datetime") {
                    source_zone = extract_timezone_name(raw);
                    let Some(parsed) = parse_temporal_string(raw) else {
                        return Value::Null;
                    };
                    let (base_date, base_time, base_offset) = match parsed {
                        TemporalValue::DateTime(dt) => {
                            let local = dt.naive_local();
                            (local.date(), local.time(), Some(*dt.offset()))
                        }
                        TemporalValue::LocalDateTime(dt) => (dt.date(), dt.time(), None),
                        TemporalValue::Date(date) => (
                            date,
                            NaiveTime::from_hms_opt(0, 0, 0).expect("valid midnight"),
                            None,
                        ),
                        _ => return Value::Null,
                    };

                    let Some(date) = apply_date_overrides(base_date, Some(map)) else {
                        return Value::Null;
                    };
                    let Some((time, include_seconds)) = apply_time_overrides(base_time, Some(map))
                    else {
                        return Value::Null;
                    };
                    (date, time, include_seconds, base_offset)
                } else {
                    let Some(date) = make_date_from_map(map) else {
                        return Value::Null;
                    };
                    let Some((time, include_seconds)) = make_time_from_map(map) else {
                        return Value::Null;
                    };
                    if source_zone.is_none() {
                        source_zone = map.get("time").and_then(|v| match v {
                            Value::String(raw) => extract_timezone_name(raw),
                            _ => None,
                        });
                    }
                    let base_offset = map
                        .get("time")
                        .and_then(|v| match v {
                            Value::String(raw) => parse_temporal_string(raw),
                            _ => None,
                        })
                        .and_then(|parsed| match parsed {
                            TemporalValue::Time { offset, .. } => Some(offset),
                            TemporalValue::DateTime(dt) => Some(*dt.offset()),
                            _ => None,
                        });
                    (date, time, include_seconds, base_offset)
                };

            let mut zone_suffix: Option<String> = None;
            let mut offset = base_offset.unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC"));

            if let Some(tz) = map_string(map, "timezone") {
                if let Some(parsed) = parse_fixed_offset(&tz) {
                    offset = parsed;
                    zone_suffix = None;
                } else if let Some(named) =
                    timezone_named_offset(&tz, date).or_else(|| timezone_named_offset_standard(&tz))
                {
                    offset = named;
                    zone_suffix = Some(tz);
                } else {
                    return Value::Null;
                }

                if let Some(base) = base_offset {
                    let conversion_base = source_zone
                        .as_ref()
                        .and_then(|zone| {
                            timezone_named_offset(zone, date)
                                .or_else(|| timezone_named_offset_standard(zone))
                        })
                        .unwrap_or(base);
                    let Some(base_dt) = conversion_base
                        .from_local_datetime(&date.and_time(time))
                        .single()
                    else {
                        return Value::Null;
                    };
                    let shifted = base_dt.with_timezone(&offset).naive_local();
                    date = shifted.date();
                    time = shifted.time();
                    include_seconds = include_seconds
                        || shifted.time().second() != 0
                        || shifted.time().nanosecond() != 0;
                }

                source_zone = None;
            } else if let Some(zone) = source_zone.as_ref()
                && let Some(named) = timezone_named_offset(zone, date)
                    .or_else(|| timezone_named_offset_standard(zone))
            {
                offset = named;
            }

            let Some(dt) = offset.from_local_datetime(&date.and_time(time)).single() else {
                return Value::Null;
            };
            let mut out = format_datetime_with_offset_literal(dt, include_seconds);
            if let Some(zone) = zone_suffix.or(source_zone) {
                out.push('[');
                out.push_str(&zone);
                out.push(']');
            }
            Value::String(out)
        }
        Some(Value::String(s)) => {
            let zone_name = extract_timezone_name(s);
            match parse_temporal_string(s) {
                Some(TemporalValue::DateTime(dt)) => {
                    let include_seconds = dt.time().second() != 0 || dt.time().nanosecond() != 0;
                    let mut out = format_datetime_with_offset_literal(dt, include_seconds);
                    if let Some(zone) = zone_name {
                        out.push('[');
                        out.push_str(&zone);
                        out.push(']');
                    }
                    Value::String(out)
                }
                Some(TemporalValue::LocalDateTime(dt)) => {
                    let offset = if let Some(zone) = zone_name.as_ref() {
                        timezone_named_offset(zone, dt.date())
                            .or_else(|| timezone_named_offset_standard(zone))
                            .or_else(|| FixedOffset::east_opt(0))
                            .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC offset"))
                    } else {
                        FixedOffset::east_opt(0).expect("UTC offset")
                    };
                    let Some(with_offset) = offset.from_local_datetime(&dt).single() else {
                        return Value::Null;
                    };
                    let include_seconds = dt.time().second() != 0 || dt.time().nanosecond() != 0;
                    let mut out = format_datetime_with_offset_literal(with_offset, include_seconds);
                    if let Some(zone) = zone_name {
                        out.push('[');
                        out.push_str(&zone);
                        out.push(']');
                    }
                    Value::String(out)
                }
                Some(TemporalValue::Date(date)) => {
                    let offset = if let Some(zone) = zone_name.as_ref() {
                        timezone_named_offset(zone, date)
                            .or_else(|| timezone_named_offset_standard(zone))
                            .or_else(|| FixedOffset::east_opt(0))
                            .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC offset"))
                    } else {
                        FixedOffset::east_opt(0).expect("UTC offset")
                    };
                    let Some(with_offset) = offset
                        .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight"))
                        .single()
                    else {
                        return Value::Null;
                    };
                    let mut out = format_datetime_with_offset_literal(with_offset, false);
                    if let Some(zone) = zone_name {
                        out.push('[');
                        out.push_str(&zone);
                        out.push(']');
                    }
                    Value::String(out)
                }
                _ => Value::Null,
            }
        }
        _ => Value::Null,
    }
}

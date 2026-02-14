use super::evaluator_duration_between::parse_temporal_arg;
use super::evaluator_temporal_format::{
    format_datetime_literal, format_datetime_with_offset_literal, format_time_literal,
};
use super::evaluator_temporal_map::map_string;
use super::evaluator_temporal_overrides::{
    apply_date_overrides, apply_datetime_overrides, apply_time_overrides, truncate_date_literal,
    truncate_naive_datetime_literal, truncate_time_literal,
};
use super::evaluator_timezone::{
    format_offset, parse_fixed_offset, timezone_named_offset_standard,
};
use super::{TemporalValue, Value};
use chrono::{FixedOffset, NaiveDate, TimeZone};

pub(super) fn evaluate_temporal_truncate(function_name: &str, args: &[Value]) -> Value {
    if args.len() < 2 {
        return Value::Null;
    }

    let Value::String(unit_raw) = &args[0] else {
        return Value::Null;
    };
    let unit = unit_raw.to_lowercase();
    let Some(temporal) = parse_temporal_arg(&args[1]) else {
        return Value::Null;
    };
    let overrides = args.get(2).and_then(|v| match v {
        Value::Map(map) => Some(map),
        _ => None,
    });

    match function_name {
        "date.truncate" => {
            let base_date = match temporal {
                TemporalValue::Date(date) => date,
                TemporalValue::LocalDateTime(dt) => dt.date(),
                TemporalValue::DateTime(dt) => dt.naive_local().date(),
                _ => return Value::Null,
            };
            let Some(truncated) = truncate_date_literal(&unit, base_date) else {
                return Value::Null;
            };
            let Some(final_date) = apply_date_overrides(truncated, overrides) else {
                return Value::Null;
            };
            Value::String(final_date.format("%Y-%m-%d").to_string())
        }
        "localtime.truncate" => {
            let base_time = match temporal {
                TemporalValue::LocalTime(time) => time,
                TemporalValue::Time { time, .. } => time,
                TemporalValue::LocalDateTime(dt) => dt.time(),
                TemporalValue::DateTime(dt) => dt.naive_local().time(),
                _ => return Value::Null,
            };
            let Some(truncated) = truncate_time_literal(&unit, base_time) else {
                return Value::Null;
            };
            let Some((final_time, include_seconds)) = apply_time_overrides(truncated, overrides)
            else {
                return Value::Null;
            };
            Value::String(format_time_literal(final_time, include_seconds))
        }
        "time.truncate" => {
            let (base_time, base_offset) = match temporal {
                TemporalValue::Time { time, offset } => (time, Some(offset)),
                TemporalValue::LocalTime(time) => (time, None),
                TemporalValue::LocalDateTime(dt) => (dt.time(), None),
                TemporalValue::DateTime(dt) => (dt.naive_local().time(), Some(*dt.offset())),
                _ => return Value::Null,
            };
            let Some(truncated) = truncate_time_literal(&unit, base_time) else {
                return Value::Null;
            };
            let Some((final_time, include_seconds)) = apply_time_overrides(truncated, overrides)
            else {
                return Value::Null;
            };

            let mut zone_suffix = None;
            let offset = if let Some(map) = overrides {
                if let Some(tz) = map_string(map, "timezone") {
                    if let Some(parsed) = parse_fixed_offset(&tz) {
                        parsed
                    } else if let Some(named) = timezone_named_offset_standard(&tz) {
                        zone_suffix = Some(tz);
                        named
                    } else {
                        zone_suffix = Some(tz);
                        base_offset
                            .or_else(|| FixedOffset::east_opt(0))
                            .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC offset"))
                    }
                } else {
                    base_offset
                        .or_else(|| FixedOffset::east_opt(0))
                        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC offset"))
                }
            } else {
                base_offset
                    .or_else(|| FixedOffset::east_opt(0))
                    .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC offset"))
            };

            let mut out = format!(
                "{}{}",
                format_time_literal(final_time, include_seconds),
                format_offset(offset)
            );
            if let Some(zone) = zone_suffix {
                out.push('[');
                out.push_str(&zone);
                out.push(']');
            }
            Value::String(out)
        }
        "localdatetime.truncate" => {
            let base_dt = match temporal {
                TemporalValue::LocalDateTime(dt) => dt,
                TemporalValue::Date(date) => date.and_hms_opt(0, 0, 0).unwrap_or_else(|| {
                    NaiveDate::from_ymd_opt(1970, 1, 1)
                        .expect("valid fallback date")
                        .and_hms_opt(0, 0, 0)
                        .expect("valid fallback time")
                }),
                TemporalValue::DateTime(dt) => dt.naive_local(),
                _ => return Value::Null,
            };
            let Some(truncated) = truncate_naive_datetime_literal(&unit, base_dt) else {
                return Value::Null;
            };
            let Some((final_date, final_time, include_seconds)) =
                apply_datetime_overrides(truncated, overrides)
            else {
                return Value::Null;
            };
            let final_dt = final_date.and_time(final_time);
            Value::String(format_datetime_literal(final_dt, include_seconds))
        }
        "datetime.truncate" => {
            let (base_dt, base_offset) = match temporal {
                TemporalValue::DateTime(dt) => (dt.naive_local(), Some(*dt.offset())),
                TemporalValue::LocalDateTime(dt) => (dt, None),
                TemporalValue::Date(date) => (
                    date.and_hms_opt(0, 0, 0).unwrap_or_else(|| {
                        NaiveDate::from_ymd_opt(1970, 1, 1)
                            .expect("valid fallback date")
                            .and_hms_opt(0, 0, 0)
                            .expect("valid fallback time")
                    }),
                    None,
                ),
                _ => return Value::Null,
            };
            let Some(truncated) = truncate_naive_datetime_literal(&unit, base_dt) else {
                return Value::Null;
            };
            let Some((final_date, final_time, include_seconds)) =
                apply_datetime_overrides(truncated, overrides)
            else {
                return Value::Null;
            };
            let local_dt = final_date.and_time(final_time);

            let mut zone_suffix = None;
            let offset = if let Some(map) = overrides {
                if let Some(tz) = map_string(map, "timezone") {
                    if let Some(parsed) = parse_fixed_offset(&tz) {
                        parsed
                    } else if let Some(named) = timezone_named_offset_standard(&tz) {
                        zone_suffix = Some(tz);
                        named
                    } else {
                        zone_suffix = Some(tz);
                        base_offset
                            .or_else(|| FixedOffset::east_opt(0))
                            .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC offset"))
                    }
                } else {
                    base_offset
                        .or_else(|| FixedOffset::east_opt(0))
                        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC offset"))
                }
            } else {
                base_offset
                    .or_else(|| FixedOffset::east_opt(0))
                    .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC offset"))
            };

            let Some(dt) = offset.from_local_datetime(&local_dt).single() else {
                return Value::Null;
            };
            let mut out = format_datetime_with_offset_literal(dt, include_seconds);
            if let Some(zone) = zone_suffix {
                out.push('[');
                out.push_str(&zone);
                out.push(']');
            }
            Value::String(out)
        }
        _ => Value::Null,
    }
}

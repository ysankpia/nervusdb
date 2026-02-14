use super::evaluator_temporal_format::{
    format_datetime_literal, format_datetime_with_offset_literal, format_time_literal,
};
use super::evaluator_temporal_math::{add_months, shift_time_of_day};
use super::evaluator_temporal_parse::parse_temporal_string;
use super::evaluator_timezone::format_offset;
use super::{Duration, DurationParts, TemporalValue, Value, duration_from_value};
use chrono::TimeZone;

pub(super) fn add_temporal_string_with_duration(base: &str, duration: &Value) -> Option<String> {
    let parts = duration_from_value(duration)?;
    shift_temporal_string_with_duration(base, &parts)
}

pub(super) fn subtract_temporal_string_with_duration(
    base: &str,
    duration: &Value,
) -> Option<String> {
    let parts = duration_from_value(duration)?;
    let negated = DurationParts {
        months: parts.months.saturating_neg(),
        days: parts.days.saturating_neg(),
        nanos: parts.nanos.saturating_neg(),
    };
    shift_temporal_string_with_duration(base, &negated)
}

fn shift_temporal_string_with_duration(base: &str, parts: &DurationParts) -> Option<String> {
    let temporal = parse_temporal_string(base)?;

    match temporal {
        TemporalValue::Date(date) => {
            let day_carry_from_nanos = parts.nanos / 86_400_000_000_000;
            let shifted = add_months(date, parts.months)?.checked_add_signed(Duration::days(
                parts.days.saturating_add(day_carry_from_nanos),
            ))?;
            Some(shifted.format("%Y-%m-%d").to_string())
        }
        TemporalValue::LocalTime(time) => {
            let total_nanos = parts.days.saturating_mul(86_400_000_000_000) + parts.nanos;
            let shifted = shift_time_of_day(time, total_nanos)?;
            Some(format_time_literal(shifted, true))
        }
        TemporalValue::Time { time, offset } => {
            let total_nanos = parts.days.saturating_mul(86_400_000_000_000) + parts.nanos;
            let shifted = shift_time_of_day(time, total_nanos)?;
            Some(format!(
                "{}{}",
                format_time_literal(shifted, true),
                format_offset(offset)
            ))
        }
        TemporalValue::LocalDateTime(dt) => {
            let shifted_date = add_months(dt.date(), parts.months)?;
            let shifted = shifted_date
                .and_time(dt.time())
                .checked_add_signed(Duration::days(parts.days))?
                .checked_add_signed(Duration::nanoseconds(parts.nanos))?;
            Some(format_datetime_literal(shifted, true))
        }
        TemporalValue::DateTime(dt) => {
            let shifted_date = add_months(dt.naive_local().date(), parts.months)?;
            let shifted_local = shifted_date
                .and_time(dt.naive_local().time())
                .checked_add_signed(Duration::days(parts.days))?
                .checked_add_signed(Duration::nanoseconds(parts.nanos))?;
            let shifted = dt.offset().from_local_datetime(&shifted_local).single()?;
            Some(format_datetime_with_offset_literal(shifted, true))
        }
    }
}

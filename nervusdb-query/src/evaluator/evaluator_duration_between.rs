use super::evaluator_duration::{
    duration_iso_from_nanos_i128, duration_value, duration_value_wide,
};
use super::evaluator_large_temporal::{
    large_localdatetime_epoch_nanos, large_months_and_days_between, parse_large_date_literal,
    parse_large_localdatetime_literal,
};
use super::evaluator_temporal_parse::{extract_timezone_name, parse_temporal_string};
use super::{
    DurationMode, LargeTemporal, TemporalOperand, TemporalValue, Value, build_duration_parts,
};

pub(super) fn evaluate_duration_between(function_name: &str, args: &[Value]) -> Value {
    if args.len() != 2 {
        return Value::Null;
    }

    let Some(mode) = duration_mode_from_name(function_name) else {
        return Value::Null;
    };

    if let (Some(lhs_large), Some(rhs_large)) = (
        parse_large_temporal_arg(&args[0]),
        parse_large_temporal_arg(&args[1]),
    ) {
        return evaluate_large_duration_between(mode, lhs_large, rhs_large).unwrap_or(Value::Null);
    }

    let Some(lhs) = parse_temporal_operand(&args[0]) else {
        return Value::Null;
    };
    let Some(rhs) = parse_temporal_operand(&args[1]) else {
        return Value::Null;
    };

    build_duration_parts(mode, &lhs, &rhs)
        .map(duration_value)
        .unwrap_or(Value::Null)
}

fn duration_mode_from_name(function_name: &str) -> Option<DurationMode> {
    match function_name {
        "duration.between" => Some(DurationMode::Between),
        "duration.inmonths" => Some(DurationMode::InMonths),
        "duration.indays" => Some(DurationMode::InDays),
        "duration.inseconds" => Some(DurationMode::InSeconds),
        _ => None,
    }
}

pub(super) fn parse_temporal_arg(value: &Value) -> Option<TemporalValue> {
    match value {
        Value::String(s) => parse_temporal_string(s),
        _ => None,
    }
}

fn parse_temporal_operand(value: &Value) -> Option<TemporalOperand> {
    match value {
        Value::String(s) => parse_temporal_string(s).map(|temporal| TemporalOperand {
            value: temporal,
            zone_name: extract_timezone_name(s),
        }),
        _ => None,
    }
}

fn parse_large_temporal_arg(value: &Value) -> Option<LargeTemporal> {
    let Value::String(raw) = value else {
        return None;
    };

    if raw.contains('T') {
        return parse_large_localdatetime_literal(raw).map(LargeTemporal::LocalDateTime);
    }
    parse_large_date_literal(raw).map(LargeTemporal::Date)
}

fn evaluate_large_duration_between(
    mode: DurationMode,
    lhs: LargeTemporal,
    rhs: LargeTemporal,
) -> Option<Value> {
    match (mode, lhs, rhs) {
        (DurationMode::Between, LargeTemporal::Date(lhs), LargeTemporal::Date(rhs)) => {
            let (months, days) = large_months_and_days_between(lhs, rhs)?;
            Some(duration_value_wide(months, days, 0))
        }
        (
            DurationMode::InSeconds,
            LargeTemporal::LocalDateTime(lhs),
            LargeTemporal::LocalDateTime(rhs),
        ) => {
            let lhs_nanos = large_localdatetime_epoch_nanos(lhs)?;
            let rhs_nanos = large_localdatetime_epoch_nanos(rhs)?;
            let diff = rhs_nanos - lhs_nanos;
            Some(Value::String(duration_iso_from_nanos_i128(diff)))
        }
        _ => None,
    }
}

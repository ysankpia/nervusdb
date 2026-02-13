use crate::ast::{BinaryOperator, Expression, Literal, UnaryOperator};
use crate::executor::{Row, Value, convert_api_property_to_value};
use crate::query_api::Params;
use chrono::{
    DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Timelike,
};
mod evaluator_arithmetic;
mod evaluator_collections;
mod evaluator_compare;
mod evaluator_comprehension;
mod evaluator_constructors;
mod evaluator_duration;
mod evaluator_duration_between;
mod evaluator_duration_core;
mod evaluator_equality;
mod evaluator_graph_functions;
mod evaluator_large_temporal;
mod evaluator_materialize;
mod evaluator_membership;
mod evaluator_numeric;
mod evaluator_pattern;
mod evaluator_scalars;
mod evaluator_temporal_format;
mod evaluator_temporal_functions;
mod evaluator_temporal_map;
mod evaluator_temporal_math;
mod evaluator_temporal_overrides;
mod evaluator_temporal_parse;
mod evaluator_temporal_shift;
mod evaluator_temporal_truncate;
mod evaluator_timezone;
use evaluator_arithmetic::{add_values, divide_values, multiply_values, subtract_values};
use evaluator_collections::evaluate_collection_function;
use evaluator_compare::{compare_values, order_compare_non_null};
use evaluator_comprehension::{evaluate_list_comprehension, evaluate_quantifier};
use evaluator_duration::duration_from_value;
use evaluator_duration_core::build_duration_parts;
use evaluator_equality::cypher_equals;
use evaluator_graph_functions::evaluate_graph_function;
use evaluator_membership::{in_list, string_predicate};
use evaluator_numeric::{
    cast_to_boolean, cast_to_float, cast_to_integer, numeric_mod, numeric_pow,
};
use evaluator_pattern::{
    evaluate_has_label, evaluate_pattern_comprehension, evaluate_pattern_exists,
};
use evaluator_scalars::evaluate_scalar_function;
use evaluator_temporal_functions::evaluate_temporal_function;
use evaluator_temporal_shift::{
    add_temporal_string_with_duration, subtract_temporal_string_with_duration,
};
use nervusdb_api::GraphSnapshot;
use std::cmp::Ordering;

/// Evaluate an expression to a boolean value (for WHERE clauses).
pub fn evaluate_expression_bool<S: GraphSnapshot>(
    expr: &Expression,
    row: &Row,
    snapshot: &S,
    params: &Params,
) -> bool {
    match evaluate_expression_value(expr, row, snapshot, params) {
        Value::Bool(b) => b,
        _ => false,
    }
}

/// Evaluate an expression to a Value.
pub fn evaluate_expression_value<S: GraphSnapshot>(
    expr: &Expression,
    row: &Row,
    snapshot: &S,
    params: &Params,
) -> Value {
    match expr {
        Expression::Literal(l) => match l {
            Literal::String(s) => Value::String(s.clone()),
            Literal::Integer(n) => Value::Int(*n),
            Literal::Float(n) => Value::Float(*n),
            Literal::Boolean(b) => Value::Bool(*b),
            Literal::Null => Value::Null,
        },
        Expression::Variable(name) => {
            // Get value from row, fallback to params (for Subquery correlation)
            row.columns()
                .iter()
                .find_map(|(k, v)| if k == name { Some(v.clone()) } else { None })
                .or_else(|| params.get(name).cloned())
                .unwrap_or(Value::Null)
        }
        Expression::PropertyAccess(pa) => {
            if let Some(Value::Node(node)) = row.get(&pa.variable) {
                return node
                    .properties
                    .get(&pa.property)
                    .cloned()
                    .unwrap_or(Value::Null);
            }

            if let Some(Value::Relationship(rel)) = row.get(&pa.variable) {
                return rel
                    .properties
                    .get(&pa.property)
                    .cloned()
                    .unwrap_or(Value::Null);
            }

            if let Some(Value::String(raw)) = row.get(&pa.variable)
                && let Some(temporal) = evaluator_temporal_parse::parse_temporal_string(raw)
                && let Some(v) = evaluate_temporal_accessor(raw, temporal, &pa.property)
            {
                return v;
            }

            // Get node/edge from row, then query property from snapshot
            if let Some(node_id) = row.get_node(&pa.variable) {
                return snapshot
                    .node_property(node_id, &pa.property)
                    .as_ref()
                    .map(convert_api_property_to_value)
                    .unwrap_or(Value::Null);
            }

            if let Some(edge) = row.get_edge(&pa.variable) {
                return snapshot
                    .edge_property(edge, &pa.property)
                    .as_ref()
                    .map(convert_api_property_to_value)
                    .unwrap_or(Value::Null);
            }

            if let Some(Value::Map(map)) = row.get(&pa.variable) {
                if matches!(map.get("__kind"), Some(Value::String(kind)) if kind == "duration")
                    && let Some(v) = evaluate_duration_accessor(map, &pa.property)
                {
                    return v;
                }
                return map.get(&pa.property).cloned().unwrap_or(Value::Null);
            }

            Value::Null
        }
        Expression::Parameter(name) => {
            // Get from params
            params.get(name).cloned().unwrap_or(Value::Null)
        }
        Expression::List(items) => Value::List(
            items
                .iter()
                .map(|e| evaluate_expression_value(e, row, snapshot, params))
                .collect(),
        ),
        Expression::Map(map) => {
            let mut out = std::collections::BTreeMap::new();
            for pair in &map.properties {
                out.insert(
                    pair.key.clone(),
                    evaluate_expression_value(&pair.value, row, snapshot, params),
                );
            }
            Value::Map(out)
        }
        Expression::Unary(u) => {
            let v = evaluate_expression_value(&u.operand, row, snapshot, params);
            match u.operator {
                UnaryOperator::Not => match v {
                    Value::Bool(b) => Value::Bool(!b),
                    Value::Null => Value::Null,
                    _ => Value::Null,
                },
                UnaryOperator::Negate => match v {
                    Value::Int(i) => i
                        .checked_neg()
                        .map(Value::Int)
                        .unwrap_or_else(|| Value::Float(-(i as f64))),
                    Value::Float(f) => Value::Float(-f),
                    Value::Null => Value::Null,
                    _ => Value::Null,
                },
            }
        }
        Expression::Binary(b) => {
            let left = evaluate_expression_value(&b.left, row, snapshot, params);
            let right = evaluate_expression_value(&b.right, row, snapshot, params);

            match b.operator {
                BinaryOperator::Equals => cypher_equals(&left, &right),
                BinaryOperator::NotEquals => match cypher_equals(&left, &right) {
                    Value::Bool(v) => Value::Bool(!v),
                    Value::Null => Value::Null,
                    _ => Value::Null,
                },
                BinaryOperator::And => match (left, right) {
                    (Value::Bool(false), _) | (_, Value::Bool(false)) => Value::Bool(false),
                    (Value::Bool(true), Value::Bool(true)) => Value::Bool(true),
                    (Value::Bool(true), Value::Null)
                    | (Value::Null, Value::Bool(true))
                    | (Value::Null, Value::Null)
                    | (Value::Bool(true), _)
                    | (_, Value::Bool(true))
                    | (Value::Null, _)
                    | (_, Value::Null) => Value::Null,
                    _ => Value::Null,
                },
                BinaryOperator::Or => match (left, right) {
                    (Value::Bool(true), _) | (_, Value::Bool(true)) => Value::Bool(true),
                    (Value::Bool(false), Value::Bool(false)) => Value::Bool(false),
                    (Value::Bool(false), Value::Null)
                    | (Value::Null, Value::Bool(false))
                    | (Value::Null, Value::Null)
                    | (Value::Bool(false), _)
                    | (_, Value::Bool(false))
                    | (Value::Null, _)
                    | (_, Value::Null) => Value::Null,
                    _ => Value::Null,
                },
                BinaryOperator::Xor => match (left, right) {
                    (Value::Bool(l), Value::Bool(r)) => Value::Bool(l ^ r),
                    (Value::Null, _) | (_, Value::Null) => Value::Null,
                    _ => Value::Null,
                },
                BinaryOperator::LessThan => compare_values(&left, &right, |ord| ord.is_lt()),
                BinaryOperator::LessEqual => {
                    compare_values(&left, &right, |ord| ord.is_lt() || ord.is_eq())
                }
                BinaryOperator::GreaterThan => compare_values(&left, &right, |ord| ord.is_gt()),

                BinaryOperator::GreaterEqual => {
                    compare_values(&left, &right, |ord| ord.is_gt() || ord.is_eq())
                }
                BinaryOperator::Add => add_values(&left, &right),
                BinaryOperator::Subtract => subtract_values(&left, &right),
                BinaryOperator::Multiply => multiply_values(&left, &right),
                BinaryOperator::Divide => divide_values(&left, &right),
                BinaryOperator::Modulo => numeric_mod(&left, &right),
                BinaryOperator::Power => numeric_pow(&left, &right),
                BinaryOperator::In => in_list(&left, &right),
                BinaryOperator::StartsWith => {
                    string_predicate(&left, &right, |l, r| l.starts_with(r))
                }
                BinaryOperator::EndsWith => string_predicate(&left, &right, |l, r| l.ends_with(r)),
                BinaryOperator::Contains => string_predicate(&left, &right, |l, r| l.contains(r)),
                BinaryOperator::HasLabel => evaluate_has_label(&left, &right, snapshot),
                BinaryOperator::IsNull => Value::Bool(matches!(left, Value::Null)),
                BinaryOperator::IsNotNull => Value::Bool(!matches!(left, Value::Null)),
            }
        }
        Expression::Case(case) => {
            for (cond, val) in &case.when_clauses {
                match evaluate_expression_value(cond, row, snapshot, params) {
                    Value::Bool(true) => {
                        return evaluate_expression_value(val, row, snapshot, params);
                    }
                    Value::Bool(false) | Value::Null => continue,
                    _ => continue,
                }
            }
            case.else_expression
                .as_ref()
                .map(|e| evaluate_expression_value(e, row, snapshot, params))
                .unwrap_or(Value::Null)
        }
        Expression::ListComprehension(comp) => {
            evaluate_list_comprehension(comp, row, snapshot, params)
        }
        Expression::FunctionCall(call) => {
            if call.name.starts_with("__quant_") {
                evaluate_quantifier(call, row, snapshot, params)
            } else {
                evaluate_function(call, row, snapshot, params)
            }
        }
        Expression::Exists(exists_expr) => match exists_expr.as_ref() {
            crate::ast::ExistsExpression::Pattern(pattern) => {
                evaluate_pattern_exists(pattern, row, snapshot, params)
            }
            crate::ast::ExistsExpression::Subquery(query) => {
                match crate::query_api::exists_subquery_has_rows(query, row, snapshot, params) {
                    Ok(has_rows) => Value::Bool(has_rows),
                    Err(_) => Value::Null,
                }
            }
        },
        Expression::PatternComprehension(pattern_comp) => {
            evaluate_pattern_comprehension(pattern_comp, row, snapshot, params)
        }
    }
}

fn evaluate_function<S: GraphSnapshot>(
    call: &crate::ast::FunctionCall,
    row: &Row,
    snapshot: &S,
    params: &Params,
) -> Value {
    let name = call.name.to_lowercase();
    let args: Vec<Value> = call
        .args
        .iter()
        .map(|arg| evaluate_expression_value(arg, row, snapshot, params))
        .collect();

    if let Some(value) = evaluate_collection_function(&name, &args, snapshot) {
        return value;
    }
    if let Some(value) = evaluate_scalar_function(&name, &args) {
        return value;
    }
    if let Some(value) = evaluate_graph_function(&name, &args, row, snapshot) {
        return value;
    }
    if let Some(value) = evaluate_temporal_function(&name, &args) {
        return value;
    }

    match name.as_str() {
        "tointeger" => cast_to_integer(args.first()),
        "tofloat" => cast_to_float(args.first()),
        "toboolean" => cast_to_boolean(args.first()),
        _ => Value::Null, // Unknown function
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DurationMode {
    Between,
    InMonths,
    InDays,
    InSeconds,
}

#[derive(Debug, Clone)]
struct TemporalAnchor {
    has_date: bool,
    date: NaiveDate,
    time: NaiveTime,
    offset: Option<FixedOffset>,
    zone_name: Option<String>,
}

#[derive(Debug, Clone)]
struct TemporalOperand {
    value: TemporalValue,
    zone_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LargeDate {
    year: i64,
    month: u32,
    day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LargeDateTime {
    date: LargeDate,
    hour: u32,
    minute: u32,
    second: u32,
    nanos: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LargeTemporal {
    Date(LargeDate),
    LocalDateTime(LargeDateTime),
}

pub fn order_compare(left: &Value, right: &Value) -> Ordering {
    match (left, right) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Greater,
        (_, Value::Null) => Ordering::Less,
        _ => order_compare_non_null(left, right).unwrap_or(Ordering::Equal),
    }
}

#[derive(Debug, Clone, Default)]
struct DurationParts {
    months: i32,
    days: i64,
    nanos: i64,
}

#[derive(Debug, Clone)]
enum TemporalValue {
    Date(NaiveDate),
    LocalTime(NaiveTime),
    Time {
        time: NaiveTime,
        offset: FixedOffset,
    },
    LocalDateTime(NaiveDateTime),
    DateTime(DateTime<FixedOffset>),
}

fn evaluate_temporal_accessor(raw: &str, temporal: TemporalValue, property: &str) -> Option<Value> {
    match temporal {
        TemporalValue::Date(date) => evaluate_date_accessor(date, property),
        TemporalValue::LocalTime(time) => match evaluate_time_accessor(time, property) {
            Some(v) => Some(v),
            None => match property {
                // localtime has no offset/timezone metadata.
                "timezone" | "offset" | "offsetMinutes" | "offsetSeconds" => Some(Value::Null),
                _ => None,
            },
        },
        TemporalValue::Time { time, offset } => {
            if let Some(v) = evaluate_time_accessor(time, property) {
                return Some(v);
            }
            match property {
                "timezone" | "offset" => {
                    Some(Value::String(evaluator_timezone::format_offset(offset)))
                }
                "offsetMinutes" => Some(Value::Int(i64::from(offset.local_minus_utc() / 60))),
                "offsetSeconds" => Some(Value::Int(i64::from(offset.local_minus_utc()))),
                _ => None,
            }
        }
        TemporalValue::LocalDateTime(dt) => {
            let date = dt.date();
            let time = dt.time();
            if let Some(v) = evaluate_date_accessor(date, property) {
                return Some(v);
            }
            if let Some(v) = evaluate_time_accessor(time, property) {
                return Some(v);
            }
            match property {
                "timezone" | "offset" | "offsetMinutes" | "offsetSeconds" | "epochSeconds"
                | "epochMillis" => Some(Value::Null),
                _ => None,
            }
        }
        TemporalValue::DateTime(dt) => {
            let local = dt.naive_local();
            if let Some(v) = evaluate_date_accessor(local.date(), property) {
                return Some(v);
            }
            if let Some(v) = evaluate_time_accessor(local.time(), property) {
                return Some(v);
            }

            let offset = *dt.offset();
            let offset_str = evaluator_timezone::format_offset(offset);
            match property {
                "timezone" => evaluator_temporal_parse::extract_timezone_name(raw)
                    .map(Value::String)
                    .or_else(|| Some(Value::String(offset_str))),
                "offset" => Some(Value::String(offset_str)),
                "offsetMinutes" => Some(Value::Int(i64::from(offset.local_minus_utc() / 60))),
                "offsetSeconds" => Some(Value::Int(i64::from(offset.local_minus_utc()))),
                "epochSeconds" => Some(Value::Int(dt.timestamp())),
                "epochMillis" => Some(Value::Int(dt.timestamp_millis())),
                _ => None,
            }
        }
    }
}

fn evaluate_date_accessor(date: NaiveDate, property: &str) -> Option<Value> {
    match property {
        "year" => Some(Value::Int(i64::from(date.year()))),
        "quarter" => Some(Value::Int(i64::from((date.month0() / 3) + 1))),
        "month" => Some(Value::Int(i64::from(date.month()))),
        "week" => Some(Value::Int(i64::from(date.iso_week().week()))),
        "weekYear" => Some(Value::Int(i64::from(date.iso_week().year()))),
        "day" => Some(Value::Int(i64::from(date.day()))),
        "ordinalDay" => Some(Value::Int(i64::from(date.ordinal()))),
        "weekDay" => Some(Value::Int(i64::from(date.weekday().number_from_monday()))),
        "dayOfQuarter" => {
            let quarter = (date.month0() / 3) + 1;
            let start_month = ((quarter - 1) * 3) + 1;
            let start = NaiveDate::from_ymd_opt(date.year(), start_month, 1)?;
            let delta = date.signed_duration_since(start).num_days() + 1;
            Some(Value::Int(delta))
        }
        _ => None,
    }
}

fn evaluate_time_accessor(time: NaiveTime, property: &str) -> Option<Value> {
    let nanos = i64::from(time.nanosecond());
    match property {
        "hour" => Some(Value::Int(i64::from(time.hour()))),
        "minute" => Some(Value::Int(i64::from(time.minute()))),
        "second" => Some(Value::Int(i64::from(time.second()))),
        "millisecond" => Some(Value::Int(nanos / 1_000_000)),
        "microsecond" => Some(Value::Int(nanos / 1_000)),
        "nanosecond" => Some(Value::Int(nanos)),
        _ => None,
    }
}

fn evaluate_duration_accessor(
    map: &std::collections::BTreeMap<String, Value>,
    property: &str,
) -> Option<Value> {
    let months = match map.get("months") {
        Some(Value::Int(v)) => *v,
        _ => return Some(Value::Null),
    };
    let days = match map.get("days") {
        Some(Value::Int(v)) => *v,
        _ => return Some(Value::Null),
    };
    let nanos = match map.get("nanos") {
        Some(Value::Int(v)) => *v,
        _ => return Some(Value::Null),
    };

    let years = months.div_euclid(12);
    let quarters = months.div_euclid(3);
    let months_of_year = months.rem_euclid(12);
    let quarters_of_year = months_of_year.div_euclid(3);
    let months_of_quarter = months_of_year.rem_euclid(3);

    let weeks = days.div_euclid(7);
    let days_of_week = days.rem_euclid(7);

    let hours = nanos.div_euclid(3_600_000_000_000);
    let minutes = nanos.div_euclid(60_000_000_000);
    let seconds = nanos.div_euclid(1_000_000_000);
    let total_seconds = days
        .saturating_mul(86_400)
        .saturating_add(nanos.div_euclid(1_000_000_000));
    let milliseconds = nanos.div_euclid(1_000_000);
    let microseconds = nanos.div_euclid(1_000);
    let nanoseconds = nanos;

    let minutes_of_hour = minutes.rem_euclid(60);
    let seconds_of_minute = seconds.rem_euclid(60);

    let nanos_of_second = nanos.rem_euclid(1_000_000_000);
    let milliseconds_of_second = nanos_of_second.div_euclid(1_000_000);
    let microseconds_of_second = nanos_of_second.div_euclid(1_000);
    let nanoseconds_of_second = nanos_of_second;

    match property {
        "years" => Some(Value::Int(years)),
        "quarters" => Some(Value::Int(quarters)),
        "months" => Some(Value::Int(months)),
        "weeks" => Some(Value::Int(weeks)),
        "days" => Some(Value::Int(days)),
        "hours" => Some(Value::Int(hours)),
        "minutes" => Some(Value::Int(minutes)),
        // NOTE: `duration.seconds` historically returned total seconds including `days` as 24h.
        // Keep this behaviour for now, since we already expose the same derived value in
        // `duration_value_wide` and our baseline tests rely on it.
        "seconds" => Some(Value::Int(total_seconds)),
        "milliseconds" => Some(Value::Int(milliseconds)),
        "microseconds" => Some(Value::Int(microseconds)),
        "nanoseconds" => Some(Value::Int(nanoseconds)),
        "quartersOfYear" => Some(Value::Int(quarters_of_year)),
        "monthsOfQuarter" => Some(Value::Int(months_of_quarter)),
        "monthsOfYear" => Some(Value::Int(months_of_year)),
        "daysOfWeek" => Some(Value::Int(days_of_week)),
        "minutesOfHour" => Some(Value::Int(minutes_of_hour)),
        "secondsOfMinute" => Some(Value::Int(seconds_of_minute)),
        "millisecondsOfSecond" => Some(Value::Int(milliseconds_of_second)),
        "microsecondsOfSecond" => Some(Value::Int(microseconds_of_second)),
        "nanosecondsOfSecond" => Some(Value::Int(nanoseconds_of_second)),
        _ => None,
    }
}

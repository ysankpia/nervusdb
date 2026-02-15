use super::evaluator_duration::{
    add_duration_parts, duration_from_value, duration_value, scale_duration_parts,
    sub_duration_parts,
};
use super::evaluator_numeric::{numeric_binop, numeric_div, value_as_f64};
use super::evaluator_temporal_parse::parse_temporal_string;
use super::{Value, add_temporal_string_with_duration, subtract_temporal_string_with_duration};

pub(super) fn add_values(left: &Value, right: &Value) -> Value {
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (Value::String(lhs), rhs)
            if duration_from_value(rhs).is_some() && parse_temporal_string(lhs).is_some() =>
        {
            add_temporal_string_with_duration(lhs, rhs)
                .map(Value::String)
                .unwrap_or(Value::Null)
        }
        (lhs, Value::String(rhs))
            if duration_from_value(lhs).is_some() && parse_temporal_string(rhs).is_some() =>
        {
            add_temporal_string_with_duration(rhs, lhs)
                .map(Value::String)
                .unwrap_or(Value::Null)
        }
        (lhs, rhs) => {
            if let (Some(l), Some(r)) = (duration_from_value(lhs), duration_from_value(rhs)) {
                return duration_value(add_duration_parts(&l, &r));
            }

            match (lhs, rhs) {
                (Value::String(l), Value::String(r)) => Value::String(format!("{l}{r}")),
                (Value::List(l), Value::List(r)) => {
                    let mut out = l.clone();
                    out.extend(r.clone());
                    Value::List(out)
                }
                (Value::List(l), r) => {
                    let mut out = l.clone();
                    out.push(r.clone());
                    Value::List(out)
                }
                (l, Value::List(r)) => {
                    let mut out = Vec::with_capacity(r.len() + 1);
                    out.push(l.clone());
                    out.extend(r.clone());
                    Value::List(out)
                }
                _ => numeric_binop(lhs, rhs, |l, r| l + r, |l, r| l + r),
            }
        }
    }
}

pub(super) fn subtract_values(left: &Value, right: &Value) -> Value {
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (Value::String(lhs), rhs)
            if duration_from_value(rhs).is_some() && parse_temporal_string(lhs).is_some() =>
        {
            subtract_temporal_string_with_duration(lhs, rhs)
                .map(Value::String)
                .unwrap_or(Value::Null)
        }
        (lhs, rhs) => {
            if let (Some(l), Some(r)) = (duration_from_value(lhs), duration_from_value(rhs)) {
                return duration_value(sub_duration_parts(&l, &r));
            }
            numeric_binop(lhs, rhs, |l, r| l - r, |l, r| l - r)
        }
    }
}

pub(super) fn multiply_values(left: &Value, right: &Value) -> Value {
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (lhs, rhs) => {
            if let (Some(parts), Some(factor)) = (duration_from_value(lhs), value_as_f64(rhs)) {
                return scale_duration_parts(parts, factor)
                    .map(duration_value)
                    .unwrap_or(Value::Null);
            }
            if let (Some(factor), Some(parts)) = (value_as_f64(lhs), duration_from_value(rhs)) {
                return scale_duration_parts(parts, factor)
                    .map(duration_value)
                    .unwrap_or(Value::Null);
            }
            numeric_binop(lhs, rhs, |l, r| l * r, |l, r| l * r)
        }
    }
}

pub(super) fn divide_values(left: &Value, right: &Value) -> Value {
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (lhs, rhs) => {
            if let (Some(parts), Some(divisor)) = (duration_from_value(lhs), value_as_f64(rhs)) {
                if divisor == 0.0 {
                    return Value::Null;
                }
                return scale_duration_parts(parts, 1.0 / divisor)
                    .map(duration_value)
                    .unwrap_or(Value::Null);
            }
            numeric_div(lhs, rhs)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Value, add_values, divide_values, multiply_values, subtract_values};

    #[test]
    fn multiply_int_overflow_does_not_panic() {
        let out = multiply_values(&Value::Int(i64::MAX), &Value::Int(2));
        match out {
            Value::Float(v) => assert!(v.is_finite()),
            other => panic!("expected finite float on int overflow, got {other:?}"),
        }
    }

    #[test]
    fn add_int_overflow_does_not_panic() {
        let out = add_values(&Value::Int(i64::MAX), &Value::Int(1));
        match out {
            Value::Float(v) => assert!(v.is_finite()),
            other => panic!("expected finite float on int overflow, got {other:?}"),
        }
    }

    #[test]
    fn subtract_int_overflow_does_not_panic() {
        let out = subtract_values(&Value::Int(i64::MIN), &Value::Int(1));
        match out {
            Value::Float(v) => assert!(v.is_finite()),
            other => panic!("expected finite float on int overflow, got {other:?}"),
        }
    }

    #[test]
    fn divide_int_min_by_negative_one_does_not_panic() {
        let out = divide_values(&Value::Int(i64::MIN), &Value::Int(-1));
        match out {
            Value::Float(v) => assert!(v.is_finite()),
            other => panic!("expected finite float on div overflow, got {other:?}"),
        }
    }
}

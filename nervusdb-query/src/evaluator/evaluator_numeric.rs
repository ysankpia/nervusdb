use super::Value;

pub(super) fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Int(v) => Some(*v as f64),
        Value::Float(v) => Some(*v),
        _ => None,
    }
}

pub(super) fn value_as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Int(i) => Some(*i),
        Value::Float(f) if f.fract() == 0.0 && *f >= i64::MIN as f64 && *f <= i64::MAX as f64 => {
            Some(*f as i64)
        }
        _ => None,
    }
}

pub(super) fn cast_to_integer(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    match value {
        Value::Null => Value::Null,
        Value::Int(i) => Value::Int(*i),
        Value::Float(f) => {
            if !f.is_finite() {
                return Value::Null;
            }
            let truncated = f.trunc();
            if truncated < i64::MIN as f64 || truncated > i64::MAX as f64 {
                Value::Null
            } else {
                Value::Int(truncated as i64)
            }
        }
        Value::String(s) => {
            if let Ok(i) = s.parse::<i64>() {
                return Value::Int(i);
            }
            if let Ok(f) = s.parse::<f64>() {
                return cast_to_integer(Some(&Value::Float(f)));
            }
            Value::Null
        }
        _ => Value::Null,
    }
}

pub(super) fn cast_to_float(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    match value {
        Value::Null => Value::Null,
        Value::Int(i) => Value::Float(*i as f64),
        Value::Float(f) => {
            if f.is_finite() {
                Value::Float(*f)
            } else {
                Value::Null
            }
        }
        Value::String(s) => s
            .parse::<f64>()
            .ok()
            .filter(|f| f.is_finite())
            .map(Value::Float)
            .unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

pub(super) fn cast_to_boolean(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    match value {
        Value::Null => Value::Null,
        Value::Bool(b) => Value::Bool(*b),
        Value::String(s) => {
            if s.eq_ignore_ascii_case("true") {
                Value::Bool(true)
            } else if s.eq_ignore_ascii_case("false") {
                Value::Bool(false)
            } else {
                Value::Null
            }
        }
        _ => Value::Null,
    }
}

pub(super) fn numeric_binop<FInt, FFloat>(
    left: &Value,
    right: &Value,
    int_op: FInt,
    float_op: FFloat,
) -> Value
where
    FInt: FnOnce(i64, i64) -> i64,
    FFloat: FnOnce(f64, f64) -> f64,
{
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (Value::Int(l), Value::Int(r)) => Value::Int(int_op(*l, *r)),
        (Value::Int(l), Value::Float(r)) => Value::Float(float_op(*l as f64, *r)),
        (Value::Float(l), Value::Int(r)) => Value::Float(float_op(*l, *r as f64)),
        (Value::Float(l), Value::Float(r)) => Value::Float(float_op(*l, *r)),
        _ => Value::Null,
    }
}

pub(super) fn numeric_div(left: &Value, right: &Value) -> Value {
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (Value::Int(_), Value::Int(0)) => Value::Null,
        (Value::Int(l), Value::Int(r)) => Value::Int(*l / *r),
        (Value::Int(l), Value::Float(r)) => Value::Float(*l as f64 / *r),
        (Value::Float(l), Value::Int(r)) => Value::Float(*l / *r as f64),
        (Value::Float(l), Value::Float(r)) => Value::Float(*l / *r),
        _ => Value::Null,
    }
}

pub(super) fn numeric_mod(left: &Value, right: &Value) -> Value {
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (_, Value::Int(0)) => Value::Null,
        (_, Value::Float(r)) if *r == 0.0 => Value::Null,
        (Value::Int(l), Value::Int(r)) => Value::Int(l % r),
        (Value::Int(l), Value::Float(r)) => Value::Float((*l as f64) % *r),
        (Value::Float(l), Value::Int(r)) => Value::Float(*l % (*r as f64)),
        (Value::Float(l), Value::Float(r)) => Value::Float(*l % *r),
        _ => Value::Null,
    }
}

pub(super) fn numeric_pow(left: &Value, right: &Value) -> Value {
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (Value::Int(l), Value::Int(r)) => Value::Float((*l as f64).powf(*r as f64)),
        (Value::Int(l), Value::Float(r)) => Value::Float((*l as f64).powf(*r)),
        (Value::Float(l), Value::Int(r)) => Value::Float(l.powf(*r as f64)),
        (Value::Float(l), Value::Float(r)) => Value::Float(l.powf(*r)),
        _ => Value::Null,
    }
}

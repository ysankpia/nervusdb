use super::Value;
use std::collections::BTreeMap;

pub(super) fn cypher_equals(left: &Value, right: &Value) -> Value {
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (Value::Int(l), Value::Int(r)) => Value::Bool(l == r),
        (Value::Int(l), Value::Float(r)) => Value::Bool(float_equals_int(*r, *l)),
        (Value::Float(l), Value::Int(r)) => Value::Bool(float_equals_int(*l, *r)),
        (Value::Float(l), Value::Float(r)) => {
            if l.is_nan() || r.is_nan() {
                Value::Bool(false)
            } else {
                Value::Bool(l == r)
            }
        }
        (Value::List(l), Value::List(r)) => cypher_equals_sequence(l, r),
        (Value::Map(l), Value::Map(r)) => cypher_equals_map(l, r),
        _ => Value::Bool(left == right),
    }
}

fn float_equals_int(float_value: f64, int_value: i64) -> bool {
    if float_value.is_nan() || !float_value.is_finite() {
        return false;
    }
    float_value == int_value as f64
}

fn cypher_equals_sequence(left: &[Value], right: &[Value]) -> Value {
    if left.len() != right.len() {
        return Value::Bool(false);
    }

    let mut saw_null = false;
    for (l, r) in left.iter().zip(right.iter()) {
        match cypher_equals(l, r) {
            Value::Bool(true) => {}
            Value::Bool(false) => return Value::Bool(false),
            Value::Null => saw_null = true,
            _ => return Value::Null,
        }
    }

    if saw_null {
        Value::Null
    } else {
        Value::Bool(true)
    }
}

fn cypher_equals_map(left: &BTreeMap<String, Value>, right: &BTreeMap<String, Value>) -> Value {
    if left.len() != right.len() {
        return Value::Bool(false);
    }

    let mut saw_null = false;
    for (key, left_value) in left {
        let Some(right_value) = right.get(key) else {
            return Value::Bool(false);
        };
        match cypher_equals(left_value, right_value) {
            Value::Bool(true) => {}
            Value::Bool(false) => return Value::Bool(false),
            Value::Null => saw_null = true,
            _ => return Value::Null,
        }
    }

    if saw_null {
        Value::Null
    } else {
        Value::Bool(true)
    }
}

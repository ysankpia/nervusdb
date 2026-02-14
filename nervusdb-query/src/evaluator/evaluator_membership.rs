use super::Value;
use super::evaluator_equality::cypher_equals;

pub(super) fn string_predicate<F>(left: &Value, right: &Value, pred: F) -> Value
where
    F: FnOnce(&str, &str) -> bool,
{
    match (left, right) {
        (Value::String(l), Value::String(r)) => Value::Bool(pred(l, r)),
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        _ => Value::Null,
    }
}

pub(super) fn in_list(left: &Value, right: &Value) -> Value {
    match (left, right) {
        (_, Value::Null) => Value::Null,
        (l, Value::List(items)) => {
            let mut saw_null = false;
            for item in items {
                match cypher_equals(l, item) {
                    Value::Bool(true) => return Value::Bool(true),
                    Value::Bool(false) => {}
                    Value::Null => saw_null = true,
                    _ => saw_null = true,
                }
            }
            if saw_null {
                Value::Null
            } else {
                Value::Bool(false)
            }
        }
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::in_list;
    use crate::executor::Value;

    #[test]
    fn null_in_empty_list_is_false() {
        assert_eq!(
            in_list(&Value::Null, &Value::List(vec![])),
            Value::Bool(false)
        );
    }

    #[test]
    fn null_in_non_empty_list_is_null() {
        assert_eq!(
            in_list(
                &Value::Null,
                &Value::List(vec![Value::Int(1), Value::Int(2), Value::Null])
            ),
            Value::Null
        );
    }
}

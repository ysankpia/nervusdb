use super::Value;
use super::evaluator_duration::{duration_from_value, duration_iso_components};
use crate::executor::PathValue;

pub(super) fn evaluate_scalar_function(name: &str, args: &[Value]) -> Option<Value> {
    match name {
        "__nervus_singleton_path" => Some(evaluate_singleton_path(args)),
        "rand" => Some(Value::Float(0.42)),
        "abs" => Some(evaluate_abs(args)),
        "tolower" => Some(evaluate_to_lower(args)),
        "toupper" => Some(evaluate_to_upper(args)),
        "reverse" => Some(evaluate_reverse(args)),
        "tostring" => Some(evaluate_to_string(args)),
        "trim" => Some(evaluate_trim(args)),
        "ltrim" => Some(evaluate_ltrim(args)),
        "rtrim" => Some(evaluate_rtrim(args)),
        "substring" => Some(evaluate_substring(args)),
        "left" => Some(evaluate_left(args)),
        "right" => Some(evaluate_right(args)),
        "replace" => Some(evaluate_replace(args)),
        "split" => Some(evaluate_split(args)),
        "coalesce" => Some(evaluate_coalesce(args)),
        "sqrt" => Some(evaluate_sqrt(args)),
        "sign" => Some(evaluate_sign(args)),
        "ceil" => Some(evaluate_ceil(args)),
        "floor" => Some(evaluate_floor(args)),
        "round" => Some(evaluate_round(args)),
        "log" => Some(evaluate_log(args)),
        "e" => Some(evaluate_e(args)),
        "pi" => Some(evaluate_pi(args)),
        _ => None,
    }
}

fn evaluate_singleton_path(args: &[Value]) -> Value {
    match args.first() {
        Some(Value::NodeId(id)) => Value::Path(PathValue {
            nodes: vec![*id],
            edges: vec![],
        }),
        Some(Value::Node(node)) => Value::Path(PathValue {
            nodes: vec![node.id],
            edges: vec![],
        }),
        _ => Value::Null,
    }
}

fn evaluate_abs(args: &[Value]) -> Value {
    if let Some(arg) = args.first() {
        match arg {
            Value::Int(i) => i
                .checked_abs()
                .map(Value::Int)
                .unwrap_or_else(|| Value::Float((*i as f64).abs())),
            Value::Float(f) => Value::Float(f.abs()),
            Value::Null => Value::Null,
            _ => Value::Null,
        }
    } else {
        Value::Null
    }
}

fn evaluate_to_lower(args: &[Value]) -> Value {
    if let Some(Value::String(s)) = args.first() {
        Value::String(s.to_lowercase())
    } else {
        Value::Null
    }
}

fn evaluate_to_upper(args: &[Value]) -> Value {
    if let Some(Value::String(s)) = args.first() {
        Value::String(s.to_uppercase())
    } else {
        Value::Null
    }
}

fn evaluate_reverse(args: &[Value]) -> Value {
    match args.first() {
        Some(Value::String(s)) => Value::String(s.chars().rev().collect()),
        Some(Value::List(items)) => {
            let mut out = items.clone();
            out.reverse();
            Value::List(out)
        }
        _ => Value::Null,
    }
}

fn evaluate_to_string(args: &[Value]) -> Value {
    if let Some(arg) = args.first() {
        match arg {
            Value::String(s) => Value::String(s.clone()),
            Value::Int(i) => Value::String(i.to_string()),
            Value::Float(f) => Value::String(f.to_string()),
            Value::Bool(b) => Value::String(b.to_string()),
            _ => duration_from_value(arg)
                .map(|parts| {
                    Value::String(duration_iso_components(
                        parts.months as i64,
                        parts.days,
                        parts.nanos,
                    ))
                })
                .unwrap_or(Value::Null),
        }
    } else {
        Value::Null
    }
}

fn evaluate_trim(args: &[Value]) -> Value {
    if let Some(Value::String(s)) = args.first() {
        Value::String(s.trim().to_string())
    } else {
        Value::Null
    }
}

fn evaluate_ltrim(args: &[Value]) -> Value {
    if let Some(Value::String(s)) = args.first() {
        Value::String(s.trim_start().to_string())
    } else {
        Value::Null
    }
}

fn evaluate_rtrim(args: &[Value]) -> Value {
    if let Some(Value::String(s)) = args.first() {
        Value::String(s.trim_end().to_string())
    } else {
        Value::Null
    }
}

fn evaluate_substring(args: &[Value]) -> Value {
    if let Some(Value::String(s)) = args.first() {
        if let Some(Value::Int(start)) = args.get(1) {
            let start = *start as usize;
            let len = if let Some(Value::Int(l)) = args.get(2) {
                Some(*l as usize)
            } else {
                None
            };

            let chars: Vec<char> = s.chars().collect();
            if start >= chars.len() {
                Value::String(String::new())
            } else {
                let end = if let Some(l) = len {
                    (start + l).min(chars.len())
                } else {
                    chars.len()
                };
                Value::String(chars[start..end].iter().collect())
            }
        } else {
            Value::Null
        }
    } else {
        Value::Null
    }
}

fn evaluate_left(args: &[Value]) -> Value {
    match (args.first(), args.get(1)) {
        (Some(Value::String(s)), Some(Value::Int(len))) => {
            if *len <= 0 {
                return Value::String(String::new());
            }
            let chars: Vec<char> = s.chars().collect();
            let take = (*len as usize).min(chars.len());
            Value::String(chars[..take].iter().collect())
        }
        (Some(Value::Null), _) | (_, Some(Value::Null)) => Value::Null,
        _ => Value::Null,
    }
}

fn evaluate_right(args: &[Value]) -> Value {
    match (args.first(), args.get(1)) {
        (Some(Value::String(s)), Some(Value::Int(len))) => {
            if *len <= 0 {
                return Value::String(String::new());
            }
            let chars: Vec<char> = s.chars().collect();
            let take = (*len as usize).min(chars.len());
            let start = chars.len().saturating_sub(take);
            Value::String(chars[start..].iter().collect())
        }
        (Some(Value::Null), _) | (_, Some(Value::Null)) => Value::Null,
        _ => Value::Null,
    }
}

fn evaluate_replace(args: &[Value]) -> Value {
    if let (
        Some(Value::String(orig)),
        Some(Value::String(search)),
        Some(Value::String(replacement)),
    ) = (args.first(), args.get(1), args.get(2))
    {
        Value::String(orig.replace(search, replacement))
    } else {
        Value::Null
    }
}

fn evaluate_split(args: &[Value]) -> Value {
    if let (Some(Value::String(orig)), Some(Value::String(delim))) = (args.first(), args.get(1)) {
        let parts: Vec<Value> = orig
            .split(delim)
            .map(|segment| Value::String(segment.to_string()))
            .collect();
        Value::List(parts)
    } else {
        Value::Null
    }
}

fn evaluate_coalesce(args: &[Value]) -> Value {
    for arg in args {
        if !matches!(arg, Value::Null) {
            return arg.clone();
        }
    }
    Value::Null
}

fn evaluate_sqrt(args: &[Value]) -> Value {
    match args.first() {
        Some(Value::Int(i)) => Value::Float((*i as f64).sqrt()),
        Some(Value::Float(f)) => Value::Float(f.sqrt()),
        _ => Value::Null,
    }
}

fn evaluate_sign(args: &[Value]) -> Value {
    match args.first() {
        Some(Value::Int(i)) => Value::Int(i.signum()),
        Some(Value::Float(f)) => Value::Int(if *f > 0.0 {
            1
        } else if *f < 0.0 {
            -1
        } else {
            0
        }),
        _ => Value::Null,
    }
}

fn evaluate_ceil(args: &[Value]) -> Value {
    match args.first() {
        Some(Value::Int(i)) => Value::Float(*i as f64),
        Some(Value::Float(f)) => Value::Float(f.ceil()),
        _ => Value::Null,
    }
}

fn evaluate_floor(args: &[Value]) -> Value {
    match args.first() {
        Some(Value::Int(i)) => Value::Float(*i as f64),
        Some(Value::Float(f)) => Value::Float(f.floor()),
        _ => Value::Null,
    }
}

fn evaluate_round(args: &[Value]) -> Value {
    match args.first() {
        Some(Value::Int(i)) => Value::Float(*i as f64),
        Some(Value::Float(f)) => Value::Float(f.round()),
        _ => Value::Null,
    }
}

fn value_as_positive_f64(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Int(i)) if *i > 0 => Some(*i as f64),
        Some(Value::Float(f)) if *f > 0.0 => Some(*f),
        _ => None,
    }
}

fn evaluate_log(args: &[Value]) -> Value {
    if args.is_empty() {
        return Value::Null;
    }

    let Some(first) = value_as_positive_f64(args.first()) else {
        return Value::Null;
    };

    if args.len() == 1 {
        return Value::Float(first.ln());
    }

    let Some(base) = value_as_positive_f64(args.first()) else {
        return Value::Null;
    };
    let Some(value) = value_as_positive_f64(args.get(1)) else {
        return Value::Null;
    };

    if (base - 1.0).abs() < f64::EPSILON {
        return Value::Null;
    }
    Value::Float(value.ln() / base.ln())
}

fn evaluate_e(args: &[Value]) -> Value {
    if args.is_empty() {
        Value::Float(std::f64::consts::E)
    } else {
        Value::Null
    }
}

fn evaluate_pi(args: &[Value]) -> Value {
    if args.is_empty() {
        Value::Float(std::f64::consts::PI)
    } else {
        Value::Null
    }
}

#[cfg(test)]
mod tests {
    use super::evaluate_scalar_function;
    use crate::evaluator::Value;

    #[test]
    fn sign_returns_expected_integer_signum() {
        assert_eq!(
            evaluate_scalar_function("sign", &[Value::Int(-10)]),
            Some(Value::Int(-1))
        );
        assert_eq!(
            evaluate_scalar_function("sign", &[Value::Int(0)]),
            Some(Value::Int(0))
        );
        assert_eq!(
            evaluate_scalar_function("sign", &[Value::Int(7)]),
            Some(Value::Int(1))
        );
    }

    #[test]
    fn ceil_returns_expected_rounded_value() {
        assert_eq!(
            evaluate_scalar_function("ceil", &[Value::Float(1.7)]),
            Some(Value::Float(2.0))
        );
        assert_eq!(
            evaluate_scalar_function("ceil", &[Value::Int(2)]),
            Some(Value::Float(2.0))
        );
    }

    #[test]
    fn floor_round_log_and_constants_work() {
        assert_eq!(
            evaluate_scalar_function("floor", &[Value::Float(2.7)]),
            Some(Value::Float(2.0))
        );
        assert_eq!(
            evaluate_scalar_function("round", &[Value::Float(2.5)]),
            Some(Value::Float(3.0))
        );

        let log = evaluate_scalar_function("log", &[Value::Int(1)]).unwrap();
        match log {
            Value::Float(v) => assert!(v.abs() < 1e-12),
            other => panic!("expected float for log(1), got {other:?}"),
        }

        let e = evaluate_scalar_function("e", &[]).unwrap();
        match e {
            Value::Float(v) => assert!((v - std::f64::consts::E).abs() < 1e-12),
            other => panic!("expected float for e(), got {other:?}"),
        }

        let pi = evaluate_scalar_function("pi", &[]).unwrap();
        match pi {
            Value::Float(v) => assert!((v - std::f64::consts::PI).abs() < 1e-12),
            other => panic!("expected float for pi(), got {other:?}"),
        }
    }

    #[test]
    fn left_and_right_return_expected_substrings() {
        assert_eq!(
            evaluate_scalar_function("left", &[Value::String("hello".into()), Value::Int(3)]),
            Some(Value::String("hel".into()))
        );
        assert_eq!(
            evaluate_scalar_function("right", &[Value::String("hello".into()), Value::Int(2)]),
            Some(Value::String("lo".into()))
        );
    }
}

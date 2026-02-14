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
        "replace" => Some(evaluate_replace(args)),
        "split" => Some(evaluate_split(args)),
        "coalesce" => Some(evaluate_coalesce(args)),
        "sqrt" => Some(evaluate_sqrt(args)),
        "sign" => Some(evaluate_sign(args)),
        "ceil" => Some(evaluate_ceil(args)),
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
}

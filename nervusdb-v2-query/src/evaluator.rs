use crate::ast::{BinaryOperator, Expression, Literal, UnaryOperator};
use crate::executor::{Row, Value};
use crate::query_api::Params;
use nervusdb_v2_api::{GraphSnapshot, PropertyValue as ApiPropertyValue};

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
            Literal::Number(n) => {
                // Try to parse as integer if it's a whole number
                if n.fract() == 0.0 {
                    Value::Int(*n as i64)
                } else {
                    Value::Float(*n)
                }
            }
            Literal::Boolean(b) => Value::Bool(*b),
            Literal::Null => Value::Null,
        },
        Expression::Variable(name) => {
            // Get value from row
            row.columns()
                .iter()
                .find_map(|(k, v)| if k == name { Some(v.clone()) } else { None })
                .unwrap_or(Value::Null)
        }
        Expression::PropertyAccess(pa) => {
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
                    Value::Int(i) => Value::Int(-i),
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
                BinaryOperator::Equals => Value::Bool(left == right),
                BinaryOperator::NotEquals => Value::Bool(left != right),
                BinaryOperator::And => match (left, right) {
                    (Value::Bool(l), Value::Bool(r)) => Value::Bool(l && r),
                    _ => Value::Null,
                },
                BinaryOperator::Or => match (left, right) {
                    (Value::Bool(l), Value::Bool(r)) => Value::Bool(l || r),
                    _ => Value::Null,
                },
                BinaryOperator::Xor => match (left, right) {
                    (Value::Bool(l), Value::Bool(r)) => Value::Bool(l ^ r),
                    _ => Value::Null,
                },
                BinaryOperator::LessThan => compare_values(&left, &right, |l, r| l < r),
                BinaryOperator::LessEqual => compare_values(&left, &right, |l, r| l <= r),
                BinaryOperator::GreaterThan => compare_values(&left, &right, |l, r| l > r),

                BinaryOperator::GreaterEqual => compare_values(&left, &right, |l, r| l >= r),
                BinaryOperator::Add => add_values(&left, &right),
                BinaryOperator::Subtract => {
                    numeric_binop(&left, &right, |l, r| l - r, |l, r| l - r)
                }
                BinaryOperator::Multiply => {
                    numeric_binop(&left, &right, |l, r| l * r, |l, r| l * r)
                }
                BinaryOperator::Divide => numeric_div(&left, &right),
                BinaryOperator::Modulo => numeric_mod(&left, &right),
                BinaryOperator::Power => numeric_pow(&left, &right),
                BinaryOperator::In => in_list(&left, &right),
                BinaryOperator::StartsWith => {
                    string_predicate(&left, &right, |l, r| l.starts_with(r))
                }
                BinaryOperator::EndsWith => string_predicate(&left, &right, |l, r| l.ends_with(r)),
                BinaryOperator::Contains => string_predicate(&left, &right, |l, r| l.contains(r)),
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
        _ => Value::Null, // Not supported yet
    }
}

fn compare_values<F>(left: &Value, right: &Value, cmp: F) -> Value
where
    F: Fn(f64, f64) -> bool,
{
    match (left, right) {
        (Value::Float(l), Value::Float(r)) => Value::Bool(cmp(*l, *r)),
        (Value::Int(l), Value::Float(r)) => Value::Bool(cmp(*l as f64, *r)),
        (Value::Float(l), Value::Int(r)) => Value::Bool(cmp(*l, *r as f64)),
        (Value::Int(l), Value::Int(r)) => Value::Bool(cmp(*l as f64, *r as f64)),
        (Value::Int(l), Value::String(r)) => {
            if let Ok(r_num) = r.parse::<f64>() {
                Value::Bool(cmp(*l as f64, r_num))
            } else {
                Value::Null
            }
        }
        (Value::String(l), Value::Int(r)) => {
            if let Ok(l_num) = l.parse::<f64>() {
                Value::Bool(cmp(l_num, *r as f64))
            } else {
                Value::Null
            }
        }
        (Value::String(l), Value::String(r)) => Value::Bool(cmp(
            l.parse::<f64>().unwrap_or(0.0),
            r.parse::<f64>().unwrap_or(0.0),
        )),
        _ => Value::Null,
    }
}

fn string_predicate<F>(left: &Value, right: &Value, pred: F) -> Value
where
    F: FnOnce(&str, &str) -> bool,
{
    match (left, right) {
        (Value::String(l), Value::String(r)) => Value::Bool(pred(l, r)),
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        _ => Value::Null,
    }
}

fn in_list(left: &Value, right: &Value) -> Value {
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (l, Value::List(items)) => Value::Bool(items.contains(l)),
        _ => Value::Null,
    }
}

fn add_values(left: &Value, right: &Value) -> Value {
    // Minimal Cypher-ish behavior:
    // - numeric + numeric
    // - string + string
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (Value::String(l), Value::String(r)) => Value::String(format!("{l}{r}")),
        _ => numeric_binop(left, right, |l, r| l + r, |l, r| l + r),
    }
}

fn numeric_binop<FInt, FFloat>(left: &Value, right: &Value, int_op: FInt, float_op: FFloat) -> Value
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

fn numeric_div(left: &Value, right: &Value) -> Value {
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (_, Value::Int(0)) => Value::Null,
        (_, Value::Float(r)) if *r == 0.0 => Value::Null,
        (Value::Int(l), Value::Int(r)) => Value::Float(*l as f64 / *r as f64),
        (Value::Int(l), Value::Float(r)) => Value::Float(*l as f64 / *r),
        (Value::Float(l), Value::Int(r)) => Value::Float(*l / *r as f64),
        (Value::Float(l), Value::Float(r)) => Value::Float(*l / *r),
        _ => Value::Null,
    }
}

fn numeric_mod(left: &Value, right: &Value) -> Value {
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

fn numeric_pow(left: &Value, right: &Value) -> Value {
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (Value::Int(l), Value::Int(r)) => Value::Float((*l as f64).powf(*r as f64)),
        (Value::Int(l), Value::Float(r)) => Value::Float((*l as f64).powf(*r)),
        (Value::Float(l), Value::Int(r)) => Value::Float(l.powf(*r as f64)),
        (Value::Float(l), Value::Float(r)) => Value::Float(l.powf(*r)),
        _ => Value::Null,
    }
}

fn convert_api_property_to_value(api_value: &ApiPropertyValue) -> Value {
    match api_value {
        ApiPropertyValue::Null => Value::Null,
        ApiPropertyValue::Bool(b) => Value::Bool(*b),
        ApiPropertyValue::Int(i) => Value::Int(*i),
        ApiPropertyValue::Float(f) => Value::Float(*f),
        ApiPropertyValue::String(s) => Value::String(s.clone()),
        ApiPropertyValue::DateTime(i) => Value::DateTime(*i),
        ApiPropertyValue::Blob(b) => Value::Blob(b.clone()),
        ApiPropertyValue::List(l) => {
            Value::List(l.iter().map(convert_api_property_to_value).collect())
        }
        ApiPropertyValue::Map(m) => Value::Map(
            m.iter()
                .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
                .collect(),
        ),
    }
}

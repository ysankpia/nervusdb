use crate::ast::{BinaryOperator, Expression, Literal};
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
                BinaryOperator::LessThan => compare_values(&left, &right, |l, r| l < r),
                BinaryOperator::LessEqual => compare_values(&left, &right, |l, r| l <= r),
                BinaryOperator::GreaterThan => compare_values(&left, &right, |l, r| l > r),

                BinaryOperator::GreaterEqual => compare_values(&left, &right, |l, r| l >= r),
                _ => Value::Null, // MVP: only support basic comparisons
            }
        }
        _ => Value::Null, // MVP: other expression types not supported yet
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

fn convert_api_property_to_value(api_value: &ApiPropertyValue) -> Value {
    match api_value {
        ApiPropertyValue::Null => Value::Null,
        ApiPropertyValue::Bool(b) => Value::Bool(*b),
        ApiPropertyValue::Int(i) => Value::Int(*i),
        ApiPropertyValue::Float(f) => Value::Float(*f),
        ApiPropertyValue::String(s) => Value::String(s.clone()),
    }
}

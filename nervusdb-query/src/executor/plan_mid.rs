use super::{
    Error, FilterIter, GraphSnapshot, Plan, PlanIterator, ProjectIter, Result, Row, Value,
    execute_plan,
};
use crate::ast::Expression;

fn runtime_type_error(code: &str) -> Error {
    Error::Other(format!("runtime error: {code}"))
}

fn is_duration_map_value(value: &Value) -> bool {
    matches!(
        value,
        Value::Map(map)
            if matches!(map.get("__kind"), Some(Value::String(kind)) if kind == "duration")
    )
}

fn ensure_runtime_function_call_compatible<S: GraphSnapshot>(
    call: &crate::ast::FunctionCall,
    row: &Row,
    snapshot: &S,
    params: &crate::query_api::Params,
) -> Result<()> {
    let name = call.name.to_ascii_lowercase();
    match name.as_str() {
        "__index" if call.args.len() == 2 => {
            let container =
                crate::evaluator::evaluate_expression_value(&call.args[0], row, snapshot, params);
            let index =
                crate::evaluator::evaluate_expression_value(&call.args[1], row, snapshot, params);

            if matches!(container, Value::Null) || matches!(index, Value::Null) {
                return Ok(());
            }

            let valid = matches!(
                (&container, &index),
                (Value::List(_), Value::Int(_))
                    | (Value::Map(_), Value::String(_))
                    | (Value::Node(_), Value::String(_))
                    | (Value::Relationship(_), Value::String(_))
                    | (Value::NodeId(_), Value::String(_))
                    | (Value::EdgeKey(_), Value::String(_))
            );
            if valid {
                Ok(())
            } else {
                Err(runtime_type_error("InvalidArgumentType"))
            }
        }
        "labels" if call.args.len() == 1 => {
            let value =
                crate::evaluator::evaluate_expression_value(&call.args[0], row, snapshot, params);
            if matches!(value, Value::Null | Value::Node(_) | Value::NodeId(_)) {
                Ok(())
            } else {
                Err(runtime_type_error("InvalidArgumentValue"))
            }
        }
        "type" if call.args.len() == 1 => {
            let value =
                crate::evaluator::evaluate_expression_value(&call.args[0], row, snapshot, params);
            if matches!(
                value,
                Value::Null | Value::Relationship(_) | Value::EdgeKey(_)
            ) {
                Ok(())
            } else {
                Err(runtime_type_error("InvalidArgumentValue"))
            }
        }
        "toboolean" if call.args.len() == 1 => {
            let value =
                crate::evaluator::evaluate_expression_value(&call.args[0], row, snapshot, params);
            if matches!(value, Value::Null | Value::Bool(_) | Value::String(_)) {
                Ok(())
            } else {
                Err(runtime_type_error("InvalidArgumentValue"))
            }
        }
        "tointeger" if call.args.len() == 1 => {
            let value =
                crate::evaluator::evaluate_expression_value(&call.args[0], row, snapshot, params);
            if matches!(
                value,
                Value::Null | Value::Int(_) | Value::Float(_) | Value::String(_)
            ) {
                Ok(())
            } else {
                Err(runtime_type_error("InvalidArgumentValue"))
            }
        }
        "tofloat" if call.args.len() == 1 => {
            let value =
                crate::evaluator::evaluate_expression_value(&call.args[0], row, snapshot, params);
            if matches!(
                value,
                Value::Null | Value::Int(_) | Value::Float(_) | Value::String(_)
            ) {
                Ok(())
            } else {
                Err(runtime_type_error("InvalidArgumentValue"))
            }
        }
        "range" if call.args.len() == 2 || call.args.len() == 3 => {
            let start =
                crate::evaluator::evaluate_expression_value(&call.args[0], row, snapshot, params);
            let end =
                crate::evaluator::evaluate_expression_value(&call.args[1], row, snapshot, params);
            let step = if call.args.len() == 3 {
                crate::evaluator::evaluate_expression_value(&call.args[2], row, snapshot, params)
            } else {
                Value::Int(1)
            };

            let (start, end, step) = match (start, end, step) {
                (Value::Int(s), Value::Int(e), Value::Int(st)) => (s, e, st),
                (Value::Null, _, _) | (_, Value::Null, _) | (_, _, Value::Null) => return Ok(()),
                _ => return Ok(()),
            };
            if step == 0 {
                return Ok(());
            }
            let observed = estimate_range_len(start, end, step);
            params.check_collection_size("Function(range)", observed)
        }
        "tostring" if call.args.len() == 1 => {
            let value =
                crate::evaluator::evaluate_expression_value(&call.args[0], row, snapshot, params);
            if matches!(
                value,
                Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_)
            ) || is_duration_map_value(&value)
            {
                Ok(())
            } else {
                Err(runtime_type_error("InvalidArgumentValue"))
            }
        }
        _ => Ok(()),
    }
}

fn estimate_range_len(start: i64, end: i64, step: i64) -> usize {
    if step > 0 && start > end {
        return 0;
    }
    if step < 0 && start < end {
        return 0;
    }

    let delta = if step > 0 {
        end.saturating_sub(start)
    } else {
        start.saturating_sub(end)
    };
    let step_abs = step.unsigned_abs();
    let len = (delta as u128 / step_abs as u128) + 1;
    usize::try_from(len).unwrap_or(usize::MAX)
}

pub(super) fn ensure_runtime_expression_compatible<S: GraphSnapshot>(
    expr: &Expression,
    row: &Row,
    snapshot: &S,
    params: &crate::query_api::Params,
) -> Result<()> {
    match expr {
        Expression::Unary(unary) => {
            ensure_runtime_expression_compatible(&unary.operand, row, snapshot, params)
        }
        Expression::Binary(binary) => {
            ensure_runtime_expression_compatible(&binary.left, row, snapshot, params)?;
            ensure_runtime_expression_compatible(&binary.right, row, snapshot, params)
        }
        Expression::FunctionCall(call) => {
            for arg in &call.args {
                ensure_runtime_expression_compatible(arg, row, snapshot, params)?;
            }
            ensure_runtime_function_call_compatible(call, row, snapshot, params)
        }
        Expression::List(items) => {
            for item in items {
                ensure_runtime_expression_compatible(item, row, snapshot, params)?;
            }
            Ok(())
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                ensure_runtime_expression_compatible(&pair.value, row, snapshot, params)?;
            }
            Ok(())
        }
        Expression::Case(case_expr) => {
            if let Some(test_expr) = &case_expr.expression {
                ensure_runtime_expression_compatible(test_expr, row, snapshot, params)?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                ensure_runtime_expression_compatible(when_expr, row, snapshot, params)?;
                ensure_runtime_expression_compatible(then_expr, row, snapshot, params)?;
            }
            if let Some(else_expr) = &case_expr.else_expression {
                ensure_runtime_expression_compatible(else_expr, row, snapshot, params)?;
            }
            Ok(())
        }
        Expression::ListComprehension(comp) => {
            ensure_runtime_expression_compatible(&comp.list, row, snapshot, params)?;
            let list_value =
                crate::evaluator::evaluate_expression_value(&comp.list, row, snapshot, params);
            if let Value::List(items) = list_value {
                for item in items {
                    let scoped_row = row.clone().with(comp.variable.as_str(), item);
                    if let Some(where_expr) = &comp.where_expression {
                        ensure_runtime_expression_compatible(
                            where_expr,
                            &scoped_row,
                            snapshot,
                            params,
                        )?;
                    }
                    if let Some(map_expr) = &comp.map_expression {
                        ensure_runtime_expression_compatible(
                            map_expr,
                            &scoped_row,
                            snapshot,
                            params,
                        )?;
                    }
                }
            }
            Ok(())
        }
        Expression::PatternComprehension(comp) => {
            if let Some(where_expr) = &comp.where_expression {
                ensure_runtime_expression_compatible(where_expr, row, snapshot, params)?;
            }
            ensure_runtime_expression_compatible(&comp.projection, row, snapshot, params)
        }
        _ => Ok(()),
    }
}

pub(super) fn execute_filter<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    predicate: &'a Expression,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let input_iter = execute_plan(snapshot, input, params);
    PlanIterator::Filter(FilterIter {
        snapshot,
        input: Box::new(input_iter),
        predicate,
        params,
    })
}

pub(super) fn execute_project<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    projections: &'a [(String, Expression)],
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let input_iter = execute_plan(snapshot, input, params);
    PlanIterator::Project(Box::new(ProjectIter {
        snapshot,
        input: Box::new(input_iter),
        projections,
        params,
    }))
}

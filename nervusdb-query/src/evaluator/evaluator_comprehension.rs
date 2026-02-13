use super::{Params, Row, Value, evaluate_expression_value};
use crate::ast::{Expression, FunctionCall, ListComprehension};
use nervusdb_api::GraphSnapshot;

pub(super) fn evaluate_list_comprehension<S: GraphSnapshot>(
    comp: &ListComprehension,
    row: &Row,
    snapshot: &S,
    params: &Params,
) -> Value {
    let var_name = comp.variable.clone();
    let list_value = evaluate_expression_value(&comp.list, row, snapshot, params);
    let predicate = comp.where_expression.as_ref();
    let projection = comp.map_expression.as_ref();

    let items = match list_value {
        Value::List(items) => items,
        Value::Null => return Value::Null,
        _ => return Value::Null,
    };

    let mut out = Vec::new();
    for item in items {
        let local_row = row.clone().with(var_name.clone(), item.clone());
        let predicate_pass = match predicate {
            None => true,
            Some(expr) => matches!(
                evaluate_expression_value(expr, &local_row, snapshot, params),
                Value::Bool(true)
            ),
        };
        if !predicate_pass {
            continue;
        }

        let projected = match projection {
            Some(expr) => evaluate_expression_value(expr, &local_row, snapshot, params),
            None => item.clone(),
        };
        out.push(projected);
    }
    Value::List(out)
}

pub(super) fn evaluate_quantifier<S: GraphSnapshot>(
    call: &FunctionCall,
    row: &Row,
    snapshot: &S,
    params: &Params,
) -> Value {
    if call.args.len() != 3 {
        return Value::Null;
    }

    let var_name = match &call.args[0] {
        Expression::Variable(v) => v.clone(),
        _ => return Value::Null,
    };

    let list_value = evaluate_expression_value(&call.args[1], row, snapshot, params);
    let predicate = &call.args[2];

    let items = match list_value {
        Value::List(items) => items,
        Value::Null => return Value::Null,
        _ => return Value::Null,
    };

    let eval_pred = |item: Value| -> Value {
        let local_row = row.clone().with(var_name.clone(), item);
        evaluate_expression_value(predicate, &local_row, snapshot, params)
    };

    match call.name.as_str() {
        "__quant_any" => {
            let mut saw_null = false;
            for item in items {
                match eval_pred(item) {
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
        "__quant_all" => {
            let mut saw_null = false;
            for item in items {
                match eval_pred(item) {
                    Value::Bool(true) => {}
                    Value::Bool(false) => return Value::Bool(false),
                    Value::Null => saw_null = true,
                    _ => saw_null = true,
                }
            }
            if saw_null {
                Value::Null
            } else {
                Value::Bool(true)
            }
        }
        "__quant_none" => {
            let mut saw_null = false;
            for item in items {
                match eval_pred(item) {
                    Value::Bool(true) => return Value::Bool(false),
                    Value::Bool(false) => {}
                    Value::Null => saw_null = true,
                    _ => saw_null = true,
                }
            }
            if saw_null {
                Value::Null
            } else {
                Value::Bool(true)
            }
        }
        "__quant_single" => {
            let mut match_count = 0usize;
            let mut saw_null = false;
            for item in items {
                match eval_pred(item) {
                    Value::Bool(true) => {
                        match_count += 1;
                        if match_count > 1 {
                            return Value::Bool(false);
                        }
                    }
                    Value::Bool(false) => {}
                    Value::Null => saw_null = true,
                    _ => saw_null = true,
                }
            }
            if saw_null && match_count < 2 {
                Value::Null
            } else if match_count == 1 {
                Value::Bool(true)
            } else {
                Value::Bool(false)
            }
        }
        _ => Value::Null,
    }
}

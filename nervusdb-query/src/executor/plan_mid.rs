use super::{
    Direction, EdgeKey, Error, FilterIter, GraphSnapshot, InternalNodeId, Plan, PlanIterator,
    ProjectIter, Result, ResultRowsIter, Row, Value, execute_plan,
};
use crate::ast::Expression;

fn value_node_id(value: &Value) -> Option<InternalNodeId> {
    match value {
        Value::NodeId(id) => Some(*id),
        Value::Node(node) => Some(node.id),
        _ => None,
    }
}

fn value_edge_key(value: &Value) -> Option<EdgeKey> {
    match value {
        Value::EdgeKey(edge) => Some(*edge),
        Value::Relationship(rel) => Some(rel.key),
        _ => None,
    }
}

fn binding_values_equal(candidate: &Value, outer: &Value) -> bool {
    if let (Some(candidate_id), Some(outer_id)) = (value_node_id(candidate), value_node_id(outer)) {
        return candidate_id == outer_id;
    }
    if let (Some(candidate_edge), Some(outer_edge)) =
        (value_edge_key(candidate), value_edge_key(outer))
    {
        return candidate_edge == outer_edge;
    }
    candidate == outer
}

fn row_contains_all_bindings(candidate: &Row, outer: &Row) -> bool {
    outer.columns().iter().all(|(key, outer_value)| {
        candidate
            .get(key)
            .is_some_and(|candidate_value| binding_values_equal(candidate_value, outer_value))
    })
}

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

pub(super) fn execute_optional_where_fixup<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    outer: &'a Plan,
    filtered: &'a Plan,
    null_aliases: &[String],
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let mut outer_rows: Vec<Row> = Vec::new();
    for item in execute_plan(snapshot, outer, params) {
        if let Err(err) = params.check_timeout("OptionalWhereFixup.outer") {
            return PlanIterator::ReturnOne(std::iter::once(Err(err)));
        }
        let row = match item {
            Ok(row) => row,
            Err(err) => return PlanIterator::ReturnOne(std::iter::once(Err(err))),
        };
        outer_rows.push(row);
        if let Err(err) = params.check_collection_size("OptionalWhereFixup.outer", outer_rows.len())
        {
            return PlanIterator::ReturnOne(std::iter::once(Err(err)));
        }
    }

    let mut filtered_rows: Vec<Row> = Vec::new();
    for item in execute_plan(snapshot, filtered, params) {
        if let Err(err) = params.check_timeout("OptionalWhereFixup.filtered") {
            return PlanIterator::ReturnOne(std::iter::once(Err(err)));
        }
        let row = match item {
            Ok(row) => row,
            Err(err) => return PlanIterator::ReturnOne(std::iter::once(Err(err))),
        };
        filtered_rows.push(row);
        if let Err(err) =
            params.check_collection_size("OptionalWhereFixup.filtered", filtered_rows.len())
        {
            return PlanIterator::ReturnOne(std::iter::once(Err(err)));
        }
    }

    let mut out: Vec<Result<Row>> = Vec::new();
    for outer_row in outer_rows {
        if let Err(err) = params.check_timeout("OptionalWhereFixup.merge") {
            return PlanIterator::ReturnOne(std::iter::once(Err(err)));
        }
        let mut matched = false;
        for row in &filtered_rows {
            if row_contains_all_bindings(row, &outer_row) {
                out.push(Ok(row.clone()));
                matched = true;
            }
        }
        if !matched {
            let mut null_row = outer_row;
            for alias in null_aliases {
                null_row = null_row.with(alias.clone(), Value::Null);
            }
            out.push(Ok(null_row));
        }
        if let Err(err) = params.check_collection_size("OptionalWhereFixup.output", out.len()) {
            return PlanIterator::ReturnOne(std::iter::once(Err(err)));
        }
    }

    PlanIterator::ResultRows(Box::new(ResultRowsIter {
        rows: out.into_iter(),
    }))
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

pub(super) fn execute_order_by<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    items: &[(Expression, Direction)],
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let input_iter = execute_plan(snapshot, input, params);
    let mut rows: Vec<Result<Row>> = Vec::new();
    for item in input_iter {
        if let Err(err) = params.check_timeout("OrderBy.collect") {
            return PlanIterator::ReturnOne(std::iter::once(Err(err)));
        }
        rows.push(item);
        if let Err(err) = params.check_collection_size("OrderBy.collect", rows.len()) {
            return PlanIterator::ReturnOne(std::iter::once(Err(err)));
        }
    }
    #[allow(clippy::type_complexity)]
    let mut sortable: Vec<(Result<Row>, Vec<(Value, Direction)>)> = rows
        .into_iter()
        .map(|row| match &row {
            Ok(r) => {
                for (expr, _) in items {
                    if let Err(err) =
                        ensure_runtime_expression_compatible(expr, r, snapshot, params)
                    {
                        return (Err(err), vec![]);
                    }
                }
                let sort_keys: Vec<(Value, Direction)> = items
                    .iter()
                    .map(|(expr, dir)| {
                        let val =
                            crate::evaluator::evaluate_expression_value(expr, r, snapshot, params);
                        (val, dir.clone())
                    })
                    .collect();
                (row, sort_keys)
            }
            Err(_) => (row, vec![]),
        })
        .collect();

    sortable.sort_by(|a, b| {
        for ((val_a, dir_a), (val_b, _)) in a.1.iter().zip(b.1.iter()) {
            let order = crate::evaluator::order_compare(val_a, val_b);
            if order == std::cmp::Ordering::Equal {
                continue;
            }
            return if *dir_a == Direction::Ascending {
                order
            } else {
                order.reverse()
            };
        }
        std::cmp::Ordering::Equal
    });

    let rows: Vec<Result<Row>> = sortable.into_iter().map(|(row, _)| row).collect();
    PlanIterator::ResultRows(Box::new(ResultRowsIter {
        rows: rows.into_iter(),
    }))
}

pub(super) fn execute_aggregate<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    group_by: &[String],
    aggregates: &[(crate::ast::AggregateFunction, String)],
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let input_iter = execute_plan(snapshot, input, params);
    let mut input_rows: Vec<Row> = Vec::new();
    for item in input_iter {
        if let Err(err) = params.check_timeout("Aggregate.collect") {
            return PlanIterator::ReturnOne(std::iter::once(Err(err)));
        }
        match item {
            Ok(row) => input_rows.push(row),
            Err(err) => return PlanIterator::ReturnOne(std::iter::once(Err(err))),
        }
        if let Err(err) = params.check_collection_size("Aggregate.input", input_rows.len()) {
            return PlanIterator::ReturnOne(std::iter::once(Err(err)));
        }
    }

    if input_rows.is_empty() && group_by.is_empty() && aggregates.is_empty() {
        return PlanIterator::ResultRows(Box::new(ResultRowsIter {
            rows: vec![].into_iter(),
        }));
    }

    let mut groups: std::collections::HashMap<Vec<Value>, Vec<Row>> =
        std::collections::HashMap::new();
    for row in input_rows {
        let key: Vec<Value> = group_by
            .iter()
            .map(|col| row.get(col).cloned().unwrap_or(Value::Null))
            .collect();
        groups.entry(key).or_default().push(row);
    }

    let mut result_rows: Vec<Row> = Vec::new();
    if groups.is_empty() {
        if !aggregates.is_empty() {
            let mut out = Row::default();
            for (func, alias) in aggregates {
                out = out.with(
                    alias.clone(),
                    compute_aggregate(func, &[], snapshot, params),
                );
            }
            result_rows.push(out);
        }
    } else {
        for (group_key, group_rows) in &groups {
            let mut out = Row::default();
            for (i, col) in group_by.iter().enumerate() {
                out = out.with(col.clone(), group_key[i].clone());
            }
            for (func, alias) in aggregates {
                out = out.with(
                    alias.clone(),
                    compute_aggregate(func, group_rows, snapshot, params),
                );
            }
            result_rows.push(out);
        }
    }

    PlanIterator::ResultRows(Box::new(ResultRowsIter {
        rows: result_rows
            .into_iter()
            .map(Ok)
            .collect::<Vec<_>>()
            .into_iter(),
    }))
}

fn compute_aggregate<S: GraphSnapshot>(
    func: &crate::ast::AggregateFunction,
    rows: &[Row],
    snapshot: &S,
    params: &crate::query_api::Params,
) -> Value {
    use crate::ast::AggregateFunction;
    match func {
        AggregateFunction::Count(None) => Value::Int(rows.len() as i64),
        AggregateFunction::Count(Some(expr)) => {
            let count = rows
                .iter()
                .filter(|r| {
                    let val =
                        crate::evaluator::evaluate_expression_value(expr, r, snapshot, params);
                    !matches!(val, Value::Null)
                })
                .count();
            Value::Int(count as i64)
        }
        AggregateFunction::Sum(expr) => {
            let mut sum: i64 = 0;
            let mut has_value = false;
            for row in rows {
                let val = crate::evaluator::evaluate_expression_value(expr, row, snapshot, params);
                if let Value::Int(n) = val {
                    sum = sum.saturating_add(n);
                    has_value = true;
                } else if let Value::Float(f) = val {
                    return compute_aggregate_float_sum(expr, rows, snapshot, params);
                }
            }
            if has_value {
                Value::Int(sum)
            } else {
                Value::Null
            }
        }
        AggregateFunction::Avg(expr) => {
            let mut sum: f64 = 0.0;
            let mut count: i64 = 0;
            for row in rows {
                let val = crate::evaluator::evaluate_expression_value(expr, row, snapshot, params);
                match val {
                    Value::Int(n) => {
                        sum += n as f64;
                        count += 1;
                    }
                    Value::Float(f) => {
                        sum += f;
                        count += 1;
                    }
                    _ => {}
                }
            }
            if count > 0 {
                Value::Float(sum / count as f64)
            } else {
                Value::Null
            }
        }
        AggregateFunction::Min(expr) => {
            let mut best: Option<Value> = None;
            for row in rows {
                let val = crate::evaluator::evaluate_expression_value(expr, row, snapshot, params);
                if !matches!(val, Value::Null)
                    && best.as_ref().is_none_or(|b| {
                        crate::evaluator::order_compare(&val, b) == std::cmp::Ordering::Less
                    })
                {
                    best = Some(val);
                }
            }
            best.unwrap_or(Value::Null)
        }
        AggregateFunction::Max(expr) => {
            let mut best: Option<Value> = None;
            for row in rows {
                let val = crate::evaluator::evaluate_expression_value(expr, row, snapshot, params);
                if !matches!(val, Value::Null)
                    && best.as_ref().is_none_or(|b| {
                        crate::evaluator::order_compare(&val, b) == std::cmp::Ordering::Greater
                    })
                {
                    best = Some(val);
                }
            }
            best.unwrap_or(Value::Null)
        }
        _ => Value::String(format!(
            "aggregate function {:?} not yet supported in 0.1",
            std::mem::discriminant(func)
        )),
    }
}

fn compute_aggregate_float_sum<S: GraphSnapshot>(
    expr: &crate::ast::Expression,
    rows: &[Row],
    snapshot: &S,
    params: &crate::query_api::Params,
) -> Value {
    let mut sum: f64 = 0.0;
    let mut has_value = false;
    for row in rows {
        let val = crate::evaluator::evaluate_expression_value(expr, row, snapshot, params);
        match val {
            Value::Int(n) => {
                sum += n as f64;
                has_value = true;
            }
            Value::Float(f) => {
                sum += f;
                has_value = true;
            }
            _ => {}
        }
    }
    if has_value {
        Value::Float(sum)
    } else {
        Value::Null
    }
}

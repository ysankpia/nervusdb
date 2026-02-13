use super::{Error, GraphSnapshot, Plan, PlanIterator, Row, Value, execute_plan};

fn evaluate_row_window_expression<S: GraphSnapshot>(
    snapshot: &S,
    expr: &crate::ast::Expression,
    params: &crate::query_api::Params,
) -> super::Result<usize> {
    let value =
        crate::evaluator::evaluate_expression_value(expr, &Row::default(), snapshot, params);
    match value {
        Value::Int(v) if v >= 0 => usize::try_from(v)
            .map_err(|_| Error::Other("syntax error: InvalidArgumentType".to_string())),
        Value::Int(_) => Err(Error::Other(
            "syntax error: NegativeIntegerArgument".to_string(),
        )),
        _ => Err(Error::Other(
            "syntax error: InvalidArgumentType".to_string(),
        )),
    }
}

pub(super) fn execute_skip<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    skip: &'a crate::ast::Expression,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let skip = match evaluate_row_window_expression(snapshot, skip, params) {
        Ok(value) => value,
        Err(err) => return PlanIterator::Dynamic(Box::new(std::iter::once(Err(err)))),
    };
    let input_iter = execute_plan(snapshot, input, params);
    PlanIterator::Dynamic(Box::new(input_iter.skip(skip)))
}

pub(super) fn execute_limit<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    limit: &'a crate::ast::Expression,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let limit = match evaluate_row_window_expression(snapshot, limit, params) {
        Ok(value) => value,
        Err(err) => return PlanIterator::Dynamic(Box::new(std::iter::once(Err(err)))),
    };
    let input_iter = execute_plan(snapshot, input, params);
    PlanIterator::Dynamic(Box::new(input_iter.take(limit)))
}

pub(super) fn execute_distinct<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let input_iter = execute_plan(snapshot, input, params);
    let mut seen = std::collections::HashSet::new();
    PlanIterator::Dynamic(Box::new(input_iter.filter(move |result| {
        if let Ok(row) = result {
            let key = row
                .columns()
                .iter()
                .map(|(_, v)| format!("{:?}", v))
                .collect::<Vec<_>>()
                .join(",");
            if seen.insert(key) {
                return true;
            }
        }
        false
    })))
}

pub(super) fn execute_unwind<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    expression: &'a crate::ast::Expression,
    alias: &'a str,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let input_iter = execute_plan(snapshot, input, params);
    let expression = expression.clone();
    let alias = alias.to_string();
    let params = params.clone();

    PlanIterator::Dynamic(Box::new(input_iter.flat_map(move |result| match result {
        Ok(row) => {
            let val =
                crate::evaluator::evaluate_expression_value(&expression, &row, snapshot, &params);
            match val {
                Value::List(list) => {
                    let mut rows = Vec::with_capacity(list.len());
                    for item in list {
                        rows.push(Ok(row.clone().with(alias.clone(), item)));
                    }
                    rows
                }
                Value::Null => vec![],
                _ => vec![Ok(row.clone().with(alias.clone(), val))],
            }
        }
        Err(e) => vec![Err(e)],
    })))
}

pub(super) fn execute_union<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    left: &'a Plan,
    right: &'a Plan,
    all: bool,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let left_iter = execute_plan(snapshot, left, params);
    let right_iter = execute_plan(snapshot, right, params);
    let chained = left_iter.chain(right_iter);

    if all {
        PlanIterator::Dynamic(Box::new(chained))
    } else {
        let mut seen = std::collections::HashSet::new();
        PlanIterator::Dynamic(Box::new(chained.filter(move |result| {
            if let Ok(row) = result {
                let key = row
                    .columns()
                    .iter()
                    .map(|(_, v)| format!("{:?}", v))
                    .collect::<Vec<_>>()
                    .join(",");
                if seen.insert(key) {
                    return true;
                }
            }
            false
        })))
    }
}

pub(super) fn write_only_plan_error<'a, S: GraphSnapshot + 'a>(
    message: &'static str,
) -> PlanIterator<'a, S> {
    PlanIterator::Dynamic(Box::new(std::iter::once(Err(Error::Other(message.into())))))
}

pub(super) fn execute_values<'a, S: GraphSnapshot + 'a>(rows: &[Row]) -> PlanIterator<'a, S> {
    let rows = rows.to_vec();
    PlanIterator::Dynamic(Box::new(rows.into_iter().map(Ok::<Row, super::Error>)))
}

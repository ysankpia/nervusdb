use super::{
    DistinctIter, Error, GraphSnapshot, Plan, PlanIterator, Row, UnionDistinctIter, Value,
    execute_plan,
};

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
    PlanIterator::Distinct(Box::new(DistinctIter {
        input: Box::new(input_iter),
        seen: std::collections::HashSet::new(),
    }))
}

pub(super) fn execute_unwind<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    expression: &'a crate::ast::Expression,
    alias: &'a str,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let input_iter = execute_plan(snapshot, input, params);

    PlanIterator::Dynamic(Box::new(input_iter.flat_map(
        move |result| -> Box<dyn Iterator<Item = super::Result<Row>>> {
            match result {
                Ok(row) => {
                    if let Err(err) = params.check_timeout("Unwind.eval") {
                        return Box::new(std::iter::once(Err(err)));
                    }
                    if let Err(err) = super::plan_mid::ensure_runtime_expression_compatible(
                        &expression,
                        &row,
                        snapshot,
                        &params,
                    ) {
                        return Box::new(std::iter::once(Err(err)));
                    }
                    let val = crate::evaluator::evaluate_expression_value(
                        &expression,
                        &row,
                        snapshot,
                        &params,
                    );
                    match val {
                        Value::List(list) => {
                            if let Err(err) =
                                params.check_collection_size("Unwind.list", list.len())
                            {
                                return Box::new(std::iter::once(Err(err)));
                            }
                            Box::new(
                                list.into_iter()
                                    .map(move |item| Ok(row.clone().with(alias, item))),
                            )
                        }
                        Value::Null => Box::new(std::iter::empty()),
                        _ => Box::new(std::iter::once(Ok(row.with(alias, val)))),
                    }
                }
                Err(e) => Box::new(std::iter::once(Err(e))),
            }
        },
    )))
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
        PlanIterator::UnionDistinct(Box::new(UnionDistinctIter {
            input: chained,
            seen: std::collections::HashSet::new(),
        }))
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

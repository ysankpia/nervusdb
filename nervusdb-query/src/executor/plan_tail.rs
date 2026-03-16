use super::{
    ChainIter, DistinctIter, Error, GraphSnapshot, LimitIter, Plan, PlanIterator, Row, SkipIter,
    UnionDistinctIter, UnwindIter, Value, ValuesIter, execute_plan,
};

fn evaluate_row_window_expression<S: GraphSnapshot>(
    snapshot: &S,
    expr: &crate::ast::Expression,
    params: &crate::query_api::Params,
) -> super::Result<usize> {
    super::plan_mid::ensure_runtime_expression_compatible(expr, &Row::default(), snapshot, params)?;
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
        Err(err) => return PlanIterator::ReturnOne(std::iter::once(Err(err))),
    };
    let input_iter = execute_plan(snapshot, input, params);
    PlanIterator::Skip(Box::new(SkipIter {
        input: Box::new(input_iter),
        remaining: skip,
    }))
}

pub(super) fn execute_limit<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    limit: &'a crate::ast::Expression,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let limit = match evaluate_row_window_expression(snapshot, limit, params) {
        Ok(value) => value,
        Err(err) => return PlanIterator::ReturnOne(std::iter::once(Err(err))),
    };
    let input_iter = execute_plan(snapshot, input, params);
    PlanIterator::Limit(Box::new(LimitIter {
        input: Box::new(input_iter),
        remaining: limit,
    }))
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
    PlanIterator::Unwind(Box::new(UnwindIter {
        snapshot,
        input: Box::new(input_iter),
        expression,
        alias,
        params,
        current_row: None,
        current_items: Vec::new().into_iter(),
    }))
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

    if all {
        PlanIterator::Chain(Box::new(ChainIter {
            left: Box::new(left_iter),
            right: Box::new(right_iter),
            draining_left: true,
        }))
    } else {
        let chained = left_iter.chain(right_iter);
        PlanIterator::UnionDistinct(Box::new(UnionDistinctIter {
            input: chained,
            seen: std::collections::HashSet::new(),
        }))
    }
}

pub(super) fn write_only_plan_error<'a, S: GraphSnapshot + 'a>(
    message: &'static str,
) -> PlanIterator<'a, S> {
    PlanIterator::ReturnOne(std::iter::once(Err(Error::Other(message.into()))))
}

pub(super) fn execute_values<'a, S: GraphSnapshot + 'a>(rows: &[Row]) -> PlanIterator<'a, S> {
    PlanIterator::Values(Box::new(ValuesIter {
        rows: Vec::from(rows).into_iter(),
    }))
}

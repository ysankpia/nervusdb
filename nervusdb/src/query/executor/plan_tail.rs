use super::{
    Error, GraphSnapshot, LimitIter, Plan, PlanIterator, Row, Value, ValuesIter, execute_plan,
};

fn evaluate_row_window_expression<S: GraphSnapshot>(
    snapshot: &S,
    expr: &crate::query::ast::Expression,
    params: &crate::query::query_api::Params,
) -> super::Result<usize> {
    super::plan_mid::ensure_runtime_expression_compatible(expr, &Row::default(), snapshot, params)?;
    let value =
        crate::query::evaluator::evaluate_expression_value(expr, &Row::default(), snapshot, params);
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

pub(super) fn execute_limit<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    limit: &'a crate::query::ast::Expression,
    params: &'a crate::query::query_api::Params,
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

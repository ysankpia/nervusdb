use super::{
    Direction, FilterIter, GraphSnapshot, Plan, PlanIterator, Result, Row, Value,
    execute_aggregate as execute_aggregate_impl, execute_plan, row_contains_all_bindings,
};
use crate::ast::Expression;

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
    let outer_rows: Vec<Row> = match execute_plan(snapshot, outer, params).collect() {
        Ok(rows) => rows,
        Err(err) => return PlanIterator::Dynamic(Box::new(std::iter::once(Err(err)))),
    };
    let filtered_rows: Vec<Row> = match execute_plan(snapshot, filtered, params).collect() {
        Ok(rows) => rows,
        Err(err) => return PlanIterator::Dynamic(Box::new(std::iter::once(Err(err)))),
    };

    let mut out: Vec<Result<Row>> = Vec::new();
    for outer_row in outer_rows {
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
    }

    PlanIterator::Dynamic(Box::new(out.into_iter()))
}

pub(super) fn execute_project<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    projections: &'a [(String, Expression)],
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let input_iter = execute_plan(snapshot, input, params);
    let projections = projections.to_vec();
    let params = params.clone();

    PlanIterator::Dynamic(Box::new(input_iter.map(move |result| {
        let row = result?;
        let mut new_row = Row::default();
        for (alias, expr) in &projections {
            let val = crate::evaluator::evaluate_expression_value(expr, &row, snapshot, &params);
            new_row = new_row.with(alias.clone(), val);
        }
        Ok(new_row)
    })))
}

pub(super) fn execute_aggregate<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    group_by: &[String],
    aggregates: &[(super::AggregateFunction, String)],
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let input_iter = execute_plan(snapshot, input, params);
    PlanIterator::Dynamic(execute_aggregate_impl(
        snapshot,
        Box::new(input_iter),
        group_by.to_vec(),
        aggregates.to_vec(),
        params,
    ))
}

pub(super) fn execute_order_by<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    items: &[(Expression, Direction)],
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let input_iter = execute_plan(snapshot, input, params);
    let rows: Vec<Result<Row>> = input_iter.collect();
    #[allow(clippy::type_complexity)]
    let mut sortable: Vec<(Result<Row>, Vec<(Value, Direction)>)> = rows
        .into_iter()
        .map(|row| match &row {
            Ok(r) => {
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

    PlanIterator::Dynamic(Box::new(sortable.into_iter().map(|(row, _)| row)))
}

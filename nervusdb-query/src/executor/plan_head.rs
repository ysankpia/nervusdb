use super::{
    ApplyIter, CartesianProductIter, GraphSnapshot, NodeScanIter, Plan, PlanIterator,
    ProcedureCallIter, Row, Value, execute_plan,
};
use crate::ast::Expression;

pub(super) fn execute_cartesian_product<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    left: &'a Plan,
    right: &'a Plan,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let left_iter = execute_plan(snapshot, left, params);
    PlanIterator::CartesianProduct(Box::new(CartesianProductIter {
        left_iter: Box::new(left_iter),
        right_plan: right,
        snapshot,
        params,
        current_left_row: None,
        current_right_iter: None,
    }))
}

pub(super) fn execute_apply<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    subquery: &'a Plan,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let input_iter = execute_plan(snapshot, input, params);
    PlanIterator::Apply(Box::new(ApplyIter {
        input_iter: Box::new(input_iter),
        subquery_plan: subquery,
        snapshot,
        base_params: params,
        current_outer_row: None,
        current_results: Vec::new().into_iter(),
    }))
}

pub(super) fn execute_procedure_call<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    name: &[String],
    args: &'a [Expression],
    yields: &'a [(String, Option<String>)],
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let input_iter = execute_plan(snapshot, input, params);
    PlanIterator::ProcedureCall(Box::new(ProcedureCallIter::new(
        Box::new(input_iter),
        name.join("."),
        args,
        yields,
        snapshot,
        params,
    )))
}

pub(super) fn write_only_foreach_error<'a, S: GraphSnapshot + 'a>() -> PlanIterator<'a, S> {
    PlanIterator::Dynamic(Box::new(std::iter::once(Err(crate::error::Error::Other(
        "FOREACH must be executed via execute_write".into(),
    )))))
}

pub(super) fn execute_node_scan<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    alias: &str,
    label: &'a Option<String>,
    optional: bool,
) -> PlanIterator<'a, S> {
    let label_id = if let Some(l) = label {
        match snapshot.resolve_label_id(l) {
            Some(id) => Some(id),
            None => {
                if optional {
                    let row = Row::new(vec![(alias.to_string(), Value::Null)]);
                    return PlanIterator::Dynamic(Box::new(std::iter::once(Ok(row))));
                }
                return PlanIterator::Dynamic(Box::new(std::iter::empty()));
            }
        }
    } else {
        None
    };

    let mut iter = NodeScanIter {
        snapshot,
        node_iter: Box::new(snapshot.nodes()),
        alias: alias.to_string(),
        label_id,
    };

    if optional {
        match iter.next() {
            Some(first) => PlanIterator::Dynamic(Box::new(std::iter::once(first).chain(iter))),
            None => {
                let row = Row::new(vec![(alias.to_string(), Value::Null)]);
                PlanIterator::Dynamic(Box::new(std::iter::once(Ok(row))))
            }
        }
    } else {
        PlanIterator::NodeScan(iter)
    }
}

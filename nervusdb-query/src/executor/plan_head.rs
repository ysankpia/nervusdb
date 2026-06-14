use super::{
    CartesianProductIter, ChainIter, GraphSnapshot, NodeScanIter, Plan, PlanIterator, Row, Value,
    execute_plan,
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

pub(super) fn execute_node_scan<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    alias: &'a str,
    label: &'a Option<String>,
    optional: bool,
) -> PlanIterator<'a, S> {
    let label_id = if let Some(l) = label {
        match snapshot.resolve_label_id(l) {
            Some(id) => Some(id),
            None => {
                if optional {
                    let row = Row::new(vec![(alias.to_owned(), Value::Null)]);
                    return PlanIterator::ReturnOne(std::iter::once(Ok(row)));
                }
                return PlanIterator::Values(Box::new(super::ValuesIter {
                    rows: Vec::new().into_iter(),
                }));
            }
        }
    } else {
        None
    };

    let mut iter = NodeScanIter {
        snapshot,
        node_iter: Box::new(snapshot.nodes()),
        alias,
        label_id,
    };

    if optional {
        match iter.next() {
            Some(first) => PlanIterator::Chain(Box::new(ChainIter {
                left: Box::new(PlanIterator::ReturnOne(std::iter::once(first))),
                right: Box::new(PlanIterator::NodeScan(iter)),
                draining_left: true,
            })),
            None => {
                let row = Row::new(vec![(alias.to_owned(), Value::Null)]);
                PlanIterator::ReturnOne(std::iter::once(Ok(row)))
            }
        }
    } else {
        PlanIterator::NodeScan(iter)
    }
}

use super::{CartesianProductIter, GraphSnapshot, NodeScanIter, Plan, PlanIterator, execute_plan};

pub(super) fn execute_cartesian_product<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    left: &'a Plan,
    right: &'a Plan,
    params: &'a crate::query::query_api::Params,
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
    property_eq: &'a Option<(String, crate::api::PropertyValue)>,
    _optional: bool,
) -> PlanIterator<'a, S> {
    let label_id = if let Some(l) = label {
        match snapshot.resolve_label_id(l) {
            Some(id) => Some(id),
            None => {
                return PlanIterator::Values(Box::new(super::ValuesIter {
                    rows: Vec::new().into_iter(),
                }));
            }
        }
    } else {
        None
    };

    let iter = NodeScanIter {
        snapshot,
        node_iter: match (label_id, property_eq) {
            (Some(lid), Some((key, value))) => {
                snapshot.nodes_with_label_and_property(lid, key, value)
            }
            (Some(lid), None) => snapshot.nodes_with_label(lid),
            (None, _) => snapshot.nodes(),
        },
        alias,
    };

    PlanIterator::NodeScan(iter)
}

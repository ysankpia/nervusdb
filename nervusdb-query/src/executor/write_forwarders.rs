use super::{
    Expression, GraphSnapshot, MergeOverlayState, Pattern, Plan, PropertyValue, Result, Row, Value,
    WriteableGraph, create_delete_ops, foreach_ops, merge_execution, write_path,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_merge_create_from_rows<S: GraphSnapshot>(
    snapshot: &S,
    input_rows: Vec<Row>,
    txn: &mut dyn WriteableGraph,
    pattern: &Pattern,
    params: &crate::query_api::Params,
    on_create_items: &[(String, String, Expression)],
    on_create_map_items: &[(String, Expression, bool)],
    on_match_items: &[(String, String, Expression)],
    on_match_map_items: &[(String, Expression, bool)],
    on_create_labels: &[(String, Vec<String>)],
    on_match_labels: &[(String, Vec<String>)],
    overlay: &mut MergeOverlayState,
) -> Result<(u32, Vec<Row>)> {
    merge_execution::execute_merge_create_from_rows(
        snapshot,
        input_rows,
        txn,
        pattern,
        params,
        on_create_items,
        on_create_map_items,
        on_match_items,
        on_match_map_items,
        on_create_labels,
        on_match_labels,
        overlay,
    )
}

pub(super) fn execute_foreach<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    variable: &str,
    list: &Expression,
    sub_plan: &Plan,
    params: &crate::query_api::Params,
) -> Result<u32> {
    foreach_ops::execute_foreach(snapshot, input, txn, variable, list, sub_plan, params)
}

pub(super) fn execute_delete_on_rows<S: GraphSnapshot>(
    snapshot: &S,
    rows: &[Row],
    txn: &mut dyn WriteableGraph,
    detach: bool,
    expressions: &[Expression],
    params: &crate::query_api::Params,
) -> Result<u32> {
    create_delete_ops::execute_delete_on_rows(snapshot, rows, txn, detach, expressions, params)
}

pub(super) fn execute_create_write_rows<S: GraphSnapshot>(
    plan: &Plan,
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    params: &crate::query_api::Params,
) -> Result<(u32, Vec<Row>)> {
    create_delete_ops::execute_create_write_rows(plan, snapshot, txn, params)
}

pub(super) fn execute_create<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    pattern: &Pattern,
    params: &crate::query_api::Params,
) -> Result<u32> {
    create_delete_ops::execute_create(snapshot, input, txn, pattern, params)
}

pub(super) fn execute_delete<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    detach: bool,
    expressions: &[Expression],
    params: &crate::query_api::Params,
) -> Result<u32> {
    create_delete_ops::execute_delete(snapshot, input, txn, detach, expressions, params)
}

pub(super) fn convert_executor_value_to_property(value: &Value) -> Result<PropertyValue> {
    write_path::convert_executor_value_to_property(value)
}

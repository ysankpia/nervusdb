use super::{
    Expression, GraphSnapshot, Pattern, Plan, PropertyValue, Result, Row, Value, WriteableGraph,
    create_delete_ops, write_path,
};

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

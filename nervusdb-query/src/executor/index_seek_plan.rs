use super::{
    GraphSnapshot, Plan, PlanIterator, Row, Value, evaluate_expression_value, execute_plan,
};

pub(super) fn execute_index_seek<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    alias: &str,
    label: &str,
    field: &str,
    value_expr: &'a crate::ast::Expression,
    fallback: &'a Plan,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let val = evaluate_expression_value(value_expr, &Row::default(), snapshot, params);

    let prop_val = match val {
        Value::Null => nervusdb_api::PropertyValue::Null,
        Value::Bool(b) => nervusdb_api::PropertyValue::Bool(b),
        Value::Int(i) => nervusdb_api::PropertyValue::Int(i),
        Value::Float(f) => nervusdb_api::PropertyValue::Float(f),
        Value::String(s) => nervusdb_api::PropertyValue::String(s),
        _ => {
            return execute_plan(snapshot, fallback, params);
        }
    };

    if let Some(mut node_ids) = snapshot.lookup_index(label, field, &prop_val) {
        node_ids.sort();
        let alias = alias.to_string();
        PlanIterator::Dynamic(Box::new(
            node_ids
                .into_iter()
                .map(move |iid| Ok(Row::default().with(alias.clone(), Value::NodeId(iid)))),
        ))
    } else {
        execute_plan(snapshot, fallback, params)
    }
}

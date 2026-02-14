use super::{
    Error, GraphSnapshot, Plan, Result, WriteableGraph, execute_create, execute_delete,
    execute_foreach, execute_remove, execute_remove_labels, execute_set, execute_set_from_maps,
    execute_set_labels,
};

pub(super) fn execute_write<S: GraphSnapshot>(
    plan: &Plan,
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    params: &crate::query_api::Params,
) -> Result<u32> {
    match plan {
        Plan::Create { input, pattern, .. } => {
            execute_create(snapshot, input, txn, pattern, params)
        }
        Plan::Delete {
            input,
            detach,
            expressions,
        } => execute_delete(snapshot, input, txn, *detach, expressions, params),
        Plan::SetProperty { input, items } => execute_set(snapshot, input, txn, items, params),
        Plan::SetPropertiesFromMap { input, items } => {
            execute_set_from_maps(snapshot, input, txn, items, params)
        }
        Plan::SetLabels { input, items } => execute_set_labels(snapshot, input, txn, items, params),
        Plan::RemoveProperty { input, items } => {
            execute_remove(snapshot, input, txn, items, params)
        }
        Plan::RemoveLabels { input, items } => {
            execute_remove_labels(snapshot, input, txn, items, params)
        }
        Plan::Foreach {
            input,
            variable,
            list,
            sub_plan,
        } => execute_foreach(snapshot, input, txn, variable, list, sub_plan, params),
        Plan::Filter { input, .. }
        | Plan::Project { input, .. }
        | Plan::Limit { input, .. }
        | Plan::Skip { input, .. }
        | Plan::OrderBy { input, .. }
        | Plan::Distinct { input }
        | Plan::Unwind { input, .. }
        | Plan::Aggregate { input, .. }
        | Plan::ProcedureCall { input, .. } => execute_write(input, snapshot, txn, params),
        Plan::OptionalWhereFixup {
            outer, filtered, ..
        } => execute_write(outer, snapshot, txn, params)
            .or_else(|_| execute_write(filtered, snapshot, txn, params)),
        Plan::IndexSeek { fallback, .. } => execute_write(fallback, snapshot, txn, params),
        Plan::MatchOut { input, .. }
        | Plan::MatchIn { input, .. }
        | Plan::MatchUndirected { input, .. }
        | Plan::MatchOutVarLen { input, .. } => {
            if let Some(inner) = input.as_deref() {
                execute_write(inner, snapshot, txn, params)
            } else {
                Err(Error::Other(
                    "write query plan has no mutable stage under match plan".to_string(),
                ))
            }
        }
        Plan::MatchBoundRel { input, .. } => execute_write(input, snapshot, txn, params),
        Plan::Apply {
            input, subquery, ..
        } => execute_write(input, snapshot, txn, params)
            .or_else(|_| execute_write(subquery, snapshot, txn, params)),
        Plan::CartesianProduct { left, right } | Plan::Union { left, right, .. } => {
            execute_write(left, snapshot, txn, params)
                .or_else(|_| execute_write(right, snapshot, txn, params))
        }
        _ => Err(Error::Other(
            "Only CREATE, DELETE, SET, REMOVE and FOREACH plans can be executed with execute_write"
                .into(),
        )),
    }
}

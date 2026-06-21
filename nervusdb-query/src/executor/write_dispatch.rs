use super::{
    Error, GraphSnapshot, Plan, Result, WriteableGraph, execute_create, execute_delete, execute_set,
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
        Plan::Filter { input, .. } | Plan::Project { input, .. } | Plan::Limit { input, .. } => {
            execute_write(input, snapshot, txn, params)
        }
        Plan::MatchOut { input, .. } => {
            if let Some(inner) = input.as_deref() {
                execute_write(inner, snapshot, txn, params)
            } else {
                Err(Error::Other(
                    "write query plan has no mutable stage under match plan".to_string(),
                ))
            }
        }
        Plan::MatchBoundRel { input, .. } => execute_write(input, snapshot, txn, params),
        Plan::CartesianProduct { left, right } => execute_write(left, snapshot, txn, params)
            .or_else(|_| execute_write(right, snapshot, txn, params)),
        _ => Err(Error::Other(
            "Only CREATE, DELETE, and SET plans can be executed with execute_write".into(),
        )),
    }
}

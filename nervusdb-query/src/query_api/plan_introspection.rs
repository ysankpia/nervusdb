use super::Plan;

pub(super) fn plan_contains_write(plan: &Plan) -> bool {
    match plan {
        Plan::Create { .. }
        | Plan::Delete { .. }
        | Plan::SetProperty { .. }
        | Plan::SetPropertiesFromMap { .. }
        | Plan::SetLabels { .. }
        | Plan::RemoveProperty { .. }
        | Plan::RemoveLabels { .. }
        | Plan::Foreach { .. } => true,
        Plan::Filter { input, .. }
        | Plan::Project { input, .. }
        | Plan::Limit { input, .. }
        | Plan::Skip { input, .. }
        | Plan::OrderBy { input, .. }
        | Plan::Distinct { input }
        | Plan::Unwind { input, .. }
        | Plan::Aggregate { input, .. }
        | Plan::ProcedureCall { input, .. }
        | Plan::MatchBoundRel { input, .. } => plan_contains_write(input),
        Plan::OptionalWhereFixup {
            outer, filtered, ..
        } => plan_contains_write(outer) || plan_contains_write(filtered),
        Plan::IndexSeek { fallback, .. } => plan_contains_write(fallback),
        Plan::MatchOut { input, .. }
        | Plan::MatchIn { input, .. }
        | Plan::MatchUndirected { input, .. }
        | Plan::MatchOutVarLen { input, .. } => input.as_deref().is_some_and(plan_contains_write),
        Plan::Apply {
            input, subquery, ..
        } => plan_contains_write(input) || plan_contains_write(subquery),
        Plan::CartesianProduct { left, right } | Plan::Union { left, right, .. } => {
            plan_contains_write(left) || plan_contains_write(right)
        }
        Plan::NodeScan { .. } | Plan::ReturnOne | Plan::Values { .. } => false,
    }
}

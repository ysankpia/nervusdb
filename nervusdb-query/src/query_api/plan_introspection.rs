use super::Plan;

pub(super) fn plan_contains_write(plan: &Plan) -> bool {
    match plan {
        Plan::Create { .. } | Plan::Delete { .. } | Plan::SetProperty { .. } => true,
        Plan::Filter { input, .. }
        | Plan::Project { input, .. }
        | Plan::Limit { input, .. }
        | Plan::MatchBoundRel { input, .. } => plan_contains_write(input),
        Plan::MatchOut { input, .. } => input.as_deref().is_some_and(plan_contains_write),
        Plan::CartesianProduct { left, right } => {
            plan_contains_write(left) || plan_contains_write(right)
        }
        Plan::NodeScan { .. } | Plan::ReturnOne | Plan::Values { .. } => false,
    }
}

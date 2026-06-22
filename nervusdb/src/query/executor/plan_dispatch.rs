use super::{
    GraphSnapshot, Plan, PlanIterator, Row, match_bound_rel_plan, match_out_plan, plan_head,
    plan_mid, plan_tail,
};

pub(super) fn execute_plan<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    plan: &'a Plan,
    params: &'a crate::query::query_api::Params,
) -> PlanIterator<'a, S> {
    match plan {
        Plan::ReturnOne => PlanIterator::ReturnOne(std::iter::once(Ok(Row::default()))),
        Plan::CartesianProduct { left, right } => {
            plan_head::execute_cartesian_product(snapshot, left, right, params)
        }
        Plan::NodeScan {
            alias,
            label,
            property_eq,
            optional,
        } => plan_head::execute_node_scan(snapshot, alias, label, property_eq, *optional),
        Plan::MatchOut {
            input,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            dst_labels,
            src_prebound,
            limit,
            project: _,
            project_external: _,
            optional,
            optional_unbind,
            path_alias,
        } => match_out_plan::execute_match_out(
            snapshot,
            input,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            dst_labels,
            *src_prebound,
            *limit,
            *optional,
            optional_unbind,
            path_alias,
            params,
        ),
        Plan::MatchBoundRel {
            input,
            rel_alias,
            src_alias,
            dst_alias,
            dst_labels,
            src_prebound,
            rels,
            direction,
            optional,
            optional_unbind,
            path_alias,
        } => match_bound_rel_plan::execute_match_bound_rel(
            snapshot,
            input,
            rel_alias,
            src_alias,
            dst_alias,
            dst_labels,
            *src_prebound,
            rels,
            direction,
            *optional,
            optional_unbind,
            path_alias,
            params,
        ),
        Plan::Filter { input, predicate } => {
            plan_mid::execute_filter(snapshot, input, predicate, params)
        }
        Plan::Project { input, projections } => {
            plan_mid::execute_project(snapshot, input, projections, params)
        }
        Plan::Limit { input, limit } => plan_tail::execute_limit(snapshot, input, limit, params),
        Plan::Create { .. } => {
            plan_tail::write_only_plan_error("CREATE must be executed via execute_write")
        }
        Plan::Delete { .. } => {
            plan_tail::write_only_plan_error("DELETE must be executed via execute_write")
        }
        Plan::SetProperty { .. } => {
            plan_tail::write_only_plan_error("SET must be executed via execute_write")
        }
        Plan::Values { rows } => plan_tail::execute_values(rows),
    }
}

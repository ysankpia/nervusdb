use super::{
    GraphSnapshot, Plan, PlanIterator, Row, index_seek_plan, match_bound_rel_plan,
    match_in_undirected_plan, match_out_plan, plan_head, plan_mid, plan_tail,
};

pub(super) fn execute_plan<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    plan: &'a Plan,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    match plan {
        Plan::ReturnOne => PlanIterator::ReturnOne(std::iter::once(Ok(Row::default()))),
        Plan::CartesianProduct { left, right } => {
            plan_head::execute_cartesian_product(snapshot, left, right, params)
        }
        Plan::Apply {
            input,
            subquery,
            alias: _,
        } => plan_head::execute_apply(snapshot, input, subquery, params),
        Plan::ProcedureCall {
            input,
            name,
            args,
            yields,
        } => plan_head::execute_procedure_call(snapshot, input, name, args, yields, params),
        Plan::Foreach { .. } => plan_head::write_only_foreach_error(),
        Plan::NodeScan {
            alias,
            label,
            optional,
        } => plan_head::execute_node_scan(snapshot, alias, label, *optional),
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
        Plan::MatchOutVarLen {
            input,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            dst_labels,
            src_prebound,
            direction,
            min_hops,
            max_hops,
            limit,
            project: _,
            project_external: _,
            optional,
            optional_unbind,
            path_alias,
        } => match_out_plan::execute_match_out_var_len(
            snapshot,
            input,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            dst_labels,
            *src_prebound,
            direction,
            *min_hops,
            *max_hops,
            *limit,
            *optional,
            optional_unbind,
            path_alias,
            params,
        ),
        Plan::MatchIn {
            input,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            dst_labels,
            src_prebound,
            limit: _,
            optional,
            optional_unbind,
            path_alias,
        } => match_in_undirected_plan::execute_match_in(
            snapshot,
            input,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            dst_labels,
            *src_prebound,
            *optional,
            optional_unbind,
            path_alias,
            params,
        ),
        Plan::MatchUndirected {
            input,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            dst_labels,
            src_prebound,
            limit,
            optional,
            optional_unbind,
            path_alias,
        } => match_in_undirected_plan::execute_match_undirected(
            snapshot,
            input,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            dst_labels,
            *src_prebound,
            limit.map(|n| n as usize),
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
        Plan::OptionalWhereFixup {
            outer,
            filtered,
            null_aliases,
        } => {
            plan_mid::execute_optional_where_fixup(snapshot, outer, filtered, null_aliases, params)
        }
        Plan::Project { input, projections } => {
            plan_mid::execute_project(snapshot, input, projections, params)
        }
        Plan::Aggregate {
            input,
            group_by,
            aggregates,
        } => plan_mid::execute_aggregate(snapshot, input, group_by, aggregates, params),
        Plan::OrderBy { input, items } => {
            plan_mid::execute_order_by(snapshot, input, items, params)
        }
        Plan::Skip { input, skip } => plan_tail::execute_skip(snapshot, input, skip, params),
        Plan::Limit { input, limit } => plan_tail::execute_limit(snapshot, input, limit, params),
        Plan::Distinct { input } => plan_tail::execute_distinct(snapshot, input, params),
        Plan::Unwind {
            input,
            expression,
            alias,
        } => plan_tail::execute_unwind(snapshot, input, expression, alias, params),
        Plan::Union { left, right, all } => {
            plan_tail::execute_union(snapshot, left, right, *all, params)
        }
        Plan::Create { .. } => {
            plan_tail::write_only_plan_error("CREATE must be executed via execute_write")
        }
        Plan::Delete { .. } => {
            plan_tail::write_only_plan_error("DELETE must be executed via execute_write")
        }
        Plan::SetProperty { .. } | Plan::SetPropertiesFromMap { .. } | Plan::SetLabels { .. } => {
            plan_tail::write_only_plan_error("SET must be executed via execute_write")
        }
        Plan::RemoveProperty { .. } | Plan::RemoveLabels { .. } => {
            plan_tail::write_only_plan_error("REMOVE must be executed via execute_write")
        }
        Plan::IndexSeek {
            alias,
            label,
            field,
            value_expr,
            fallback,
        } => index_seek_plan::execute_index_seek(
            snapshot, alias, label, field, value_expr, fallback, params,
        ),
        Plan::Values { rows } => plan_tail::execute_values(rows),
    }
}

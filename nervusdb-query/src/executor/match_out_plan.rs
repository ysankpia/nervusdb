use super::binding_utils::value_node_id;
use super::label_constraint::{node_matches_label_constraint, resolve_label_constraint};
use super::read_path::{ExpandIter, MatchOutIter, MatchOutVarLenIter};
use super::{
    GraphSnapshot, Plan, PlanIterator, RelTypeId, RelationshipDirection, Result, Row, execute_plan,
};

fn resolve_rel_ids<S: GraphSnapshot>(snapshot: &S, rels: &[String]) -> Option<Vec<RelTypeId>> {
    if rels.is_empty() {
        return None;
    }
    let mut ids = Vec::new();
    for rel in rels {
        if let Some(id) = snapshot.resolve_rel_type_id(rel) {
            ids.push(id);
        }
    }
    Some(ids)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_match_out<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Option<Box<Plan>>,
    src_alias: &'a str,
    rels: &[String],
    edge_alias: &'a Option<String>,
    dst_alias: &'a str,
    dst_labels: &[String],
    src_prebound: bool,
    limit: Option<u32>,
    optional: bool,
    optional_unbind: &[String],
    path_alias: &'a Option<String>,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let rel_ids = resolve_rel_ids(snapshot, rels);
    let dst_label_constraint = resolve_label_constraint(snapshot, dst_labels);

    if let Some(input_plan) = input {
        let input_iter = execute_plan(snapshot, input_plan, params);
        let expand = ExpandIter::new(
            snapshot,
            Box::new(input_iter),
            src_alias,
            rel_ids,
            edge_alias.as_deref(),
            dst_alias,
            optional,
            optional && src_prebound,
            optional_unbind.to_vec(),
            dst_label_constraint.clone(),
            path_alias.as_deref(),
        );
        if let Some(limit) = limit {
            PlanIterator::Dynamic(Box::new(expand.take(limit as usize)))
        } else {
            PlanIterator::Dynamic(Box::new(expand))
        }
    } else {
        let base = MatchOutIter::new(
            snapshot,
            src_alias,
            rel_ids,
            edge_alias.as_deref(),
            dst_alias,
            path_alias.as_deref(),
        );
        let filtered = base.filter(move |result| match result {
            Ok(row) => row
                .get(dst_alias)
                .and_then(value_node_id)
                .is_some_and(|id| {
                    node_matches_label_constraint(snapshot, id, &dst_label_constraint)
                }),
            Err(_) => true,
        });
        if let Some(limit) = limit {
            PlanIterator::Dynamic(Box::new(filtered.take(limit as usize)))
        } else {
            PlanIterator::Dynamic(Box::new(filtered))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_match_out_var_len<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Option<Box<Plan>>,
    src_alias: &'a str,
    rels: &[String],
    edge_alias: &'a Option<String>,
    dst_alias: &'a str,
    dst_labels: &[String],
    src_prebound: bool,
    direction: &RelationshipDirection,
    min_hops: u32,
    max_hops: Option<u32>,
    limit: Option<u32>,
    optional: bool,
    optional_unbind: &[String],
    path_alias: &'a Option<String>,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let input_iter = input
        .as_ref()
        .map(|plan| execute_plan(snapshot, plan, params));
    let rel_ids = resolve_rel_ids(snapshot, rels);
    let dst_label_constraint = resolve_label_constraint(snapshot, dst_labels);

    let base = MatchOutVarLenIter::new(
        snapshot,
        input_iter.map(|iter| Box::new(iter) as Box<dyn Iterator<Item = Result<Row>>>),
        src_alias,
        rel_ids,
        edge_alias.as_deref(),
        dst_alias,
        dst_label_constraint,
        direction.clone(),
        min_hops,
        max_hops,
        limit,
        optional,
        src_prebound,
        optional_unbind.to_vec(),
        path_alias.as_deref(),
    );
    if let Some(limit) = limit {
        PlanIterator::Dynamic(Box::new(base.take(limit as usize)))
    } else {
        PlanIterator::Dynamic(Box::new(base))
    }
}

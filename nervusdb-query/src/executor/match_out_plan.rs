use super::binding_utils::value_node_id;
use super::label_constraint::{node_matches_label_constraint, resolve_label_constraint};
use super::read_path::{ExpandIter, MatchOutIter, MatchOutVarLenIter};
use super::{
    GraphSnapshot, LabelConstraint, LimitIter, Plan, PlanIterator, RelTypeId,
    RelationshipDirection, Result, Row, execute_plan,
};

pub struct FilteredMatchOutIter<'a, S: GraphSnapshot + 'a> {
    snapshot: &'a S,
    inner: MatchOutIter<'a, S>,
    dst_alias: &'a str,
    dst_label_constraint: LabelConstraint,
}

impl<'a, S: GraphSnapshot + 'a> FilteredMatchOutIter<'a, S> {
    fn new(
        snapshot: &'a S,
        inner: MatchOutIter<'a, S>,
        dst_alias: &'a str,
        dst_label_constraint: LabelConstraint,
    ) -> Self {
        Self {
            snapshot,
            inner,
            dst_alias,
            dst_label_constraint,
        }
    }
}

impl<'a, S: GraphSnapshot + 'a> Iterator for FilteredMatchOutIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.inner.next()? {
                Ok(row) => {
                    let matches =
                        row.get(self.dst_alias)
                            .and_then(value_node_id)
                            .is_some_and(|id| {
                                node_matches_label_constraint(
                                    self.snapshot,
                                    id,
                                    &self.dst_label_constraint,
                                )
                            });
                    if matches {
                        return Some(Ok(row));
                    }
                }
                Err(err) => return Some(Err(err)),
            }
        }
    }
}

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
    edge_alias: &'a Option<std::sync::Arc<str>>,
    dst_alias: &'a str,
    dst_labels: &[String],
    src_prebound: bool,
    limit: Option<u32>,
    optional: bool,
    optional_unbind: &[String],
    path_alias: &'a Option<std::sync::Arc<str>>,
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
            PlanIterator::Limit(Box::new(LimitIter {
                input: Box::new(PlanIterator::Expand(Box::new(expand))),
                remaining: limit as usize,
            }))
        } else {
            PlanIterator::Expand(Box::new(expand))
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
        let filtered = PlanIterator::MatchOutFiltered(Box::new(FilteredMatchOutIter::new(
            snapshot,
            base,
            dst_alias,
            dst_label_constraint,
        )));
        if let Some(limit) = limit {
            PlanIterator::Limit(Box::new(LimitIter {
                input: Box::new(filtered),
                remaining: limit as usize,
            }))
        } else {
            filtered
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_match_out_var_len<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Option<Box<Plan>>,
    src_alias: &'a str,
    rels: &[String],
    edge_alias: &'a Option<std::sync::Arc<str>>,
    dst_alias: &'a str,
    dst_labels: &[String],
    src_prebound: bool,
    direction: &RelationshipDirection,
    min_hops: u32,
    max_hops: Option<u32>,
    limit: Option<u32>,
    optional: bool,
    optional_unbind: &[String],
    path_alias: &'a Option<std::sync::Arc<str>>,
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
        PlanIterator::Limit(Box::new(LimitIter {
            input: Box::new(PlanIterator::MatchOutVarLen(Box::new(base))),
            remaining: limit as usize,
        }))
    } else {
        PlanIterator::MatchOutVarLen(Box::new(base))
    }
}

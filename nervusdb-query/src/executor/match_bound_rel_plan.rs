use super::{
    GraphSnapshot, InternalNodeId, LabelConstraint, Plan, PlanIterator, RelationshipDirection,
    Result, Row, Value, apply_optional_unbinds_row, execute_plan, node_matches_label_constraint,
    path_alias_contains_edge, resolve_label_constraint,
};

pub struct MatchBoundRelIter<'a, S: GraphSnapshot + 'a> {
    snapshot: &'a S,
    input: Box<PlanIterator<'a, S>>,
    rel_alias: &'a str,
    src_alias: &'a str,
    dst_alias: &'a str,
    dst_label_constraint: LabelConstraint,
    src_prebound: bool,
    rel_ids: Option<Vec<super::RelTypeId>>,
    direction: RelationshipDirection,
    optional: bool,
    optional_unbind: &'a [String],
    path_alias: Option<&'a str>,
    pending: std::vec::IntoIter<Result<Row>>,
}

impl<'a, S: GraphSnapshot + 'a> MatchBoundRelIter<'a, S> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        snapshot: &'a S,
        input: Box<PlanIterator<'a, S>>,
        rel_alias: &'a str,
        src_alias: &'a str,
        dst_alias: &'a str,
        dst_label_constraint: LabelConstraint,
        src_prebound: bool,
        rel_ids: Option<Vec<super::RelTypeId>>,
        direction: RelationshipDirection,
        optional: bool,
        optional_unbind: &'a [String],
        path_alias: Option<&'a str>,
    ) -> Self {
        Self {
            snapshot,
            input,
            rel_alias,
            src_alias,
            dst_alias,
            dst_label_constraint,
            src_prebound,
            rel_ids,
            direction,
            optional,
            optional_unbind,
            path_alias,
            pending: Vec::new().into_iter(),
        }
    }
}

impl<'a, S: GraphSnapshot + 'a> Iterator for MatchBoundRelIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(next) = self.pending.next() {
                return Some(next);
            }

            let row = match self.input.next()? {
                Ok(row) => row,
                Err(err) => return Some(Err(err)),
            };

            let results = collect_bound_rel_rows(
                self.snapshot,
                &row,
                self.rel_alias,
                self.src_alias,
                self.dst_alias,
                &self.dst_label_constraint,
                self.src_prebound,
                &self.rel_ids,
                &self.direction,
                self.optional,
                self.optional_unbind,
                self.path_alias,
            );
            if results.is_empty() {
                continue;
            }
            self.pending = results.into_iter();
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_match_bound_rel<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    rel_alias: &'a str,
    src_alias: &'a str,
    dst_alias: &'a str,
    dst_labels: &'a [String],
    src_prebound: bool,
    rels: &'a [String],
    direction: &'a RelationshipDirection,
    optional: bool,
    optional_unbind: &'a [String],
    path_alias: &'a Option<std::sync::Arc<str>>,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let rel_ids = if rels.is_empty() {
        None
    } else {
        let mut ids = Vec::new();
        for r in rels {
            if let Some(id) = snapshot.resolve_rel_type_id(r) {
                ids.push(id);
            }
        }
        Some(ids)
    };
    let dst_label_constraint = resolve_label_constraint(snapshot, dst_labels);

    let input_iter = execute_plan(snapshot, input, params);
    PlanIterator::MatchBoundRel(Box::new(MatchBoundRelIter::new(
        snapshot,
        Box::new(input_iter),
        rel_alias,
        src_alias,
        dst_alias,
        dst_label_constraint,
        src_prebound,
        rel_ids,
        direction.clone(),
        optional,
        optional_unbind,
        path_alias.as_deref(),
    )))
}

#[allow(clippy::too_many_arguments)]
fn collect_bound_rel_rows<S: GraphSnapshot>(
    snapshot: &S,
    row: &Row,
    rel_alias: &str,
    src_alias: &str,
    dst_alias: &str,
    dst_label_constraint: &LabelConstraint,
    src_prebound: bool,
    rel_ids: &Option<Vec<super::RelTypeId>>,
    direction: &RelationshipDirection,
    optional: bool,
    optional_unbind: &[String],
    path_alias: Option<&str>,
) -> Vec<Result<Row>> {
    let mut out = Vec::new();
    let bound_edge = row.get_edge(rel_alias);

    if let Some(edge) = bound_edge
        && rel_ids.as_ref().is_none_or(|ids| ids.contains(&edge.rel))
    {
        let orientations: Vec<(InternalNodeId, InternalNodeId)> = match direction {
            RelationshipDirection::LeftToRight => vec![(edge.src, edge.dst)],
            RelationshipDirection::RightToLeft => vec![(edge.dst, edge.src)],
            RelationshipDirection::Undirected => {
                if edge.src == edge.dst {
                    vec![(edge.src, edge.dst)]
                } else {
                    vec![(edge.src, edge.dst), (edge.dst, edge.src)]
                }
            }
        };

        for (src_id, dst_id) in orientations {
            if path_alias_contains_edge(snapshot, row, path_alias, edge) {
                continue;
            }

            let src_ok = match row.get(src_alias) {
                Some(Value::NodeId(id)) => *id == src_id,
                Some(Value::Null) => false,
                Some(_) => false,
                None => true,
            };
            if !src_ok {
                continue;
            }

            let dst_ok = match row.get(dst_alias) {
                Some(Value::NodeId(id)) => *id == dst_id,
                Some(Value::Null) => false,
                Some(_) => false,
                None => true,
            };
            if !dst_ok || !node_matches_label_constraint(snapshot, dst_id, dst_label_constraint) {
                continue;
            }

            let mut new_row = row.clone();
            new_row = new_row.with(src_alias, Value::NodeId(src_id));
            new_row = new_row.with(dst_alias, Value::NodeId(dst_id));
            if let Some(pa) = path_alias {
                new_row.join_path(pa, src_id, edge, dst_id);
            }
            out.push(Ok(new_row));
        }
    }

    if out.is_empty() && optional && src_prebound {
        out.push(Ok(apply_optional_unbinds_row(row.clone(), optional_unbind)));
    }

    out
}

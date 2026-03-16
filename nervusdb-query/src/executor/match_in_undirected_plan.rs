use super::{
    EdgeKey, ErasedSnapshot, GraphSnapshot, LabelConstraint, Plan, PlanIterator, RelTypeId, Result,
    Row, Value, apply_optional_unbinds_row, execute_plan, node_matches_label_constraint,
    path_alias_contains_edge, resolve_label_constraint, row_matches_node_binding,
};

pub struct MatchInIter<'a, S: GraphSnapshot + 'a> {
    snapshot: &'a S,
    input: Box<PlanIterator<'a, S>>,
    src_alias: &'a str,
    rel_ids: Option<Vec<RelTypeId>>,
    edge_alias: Option<&'a str>,
    dst_alias: &'a str,
    dst_label_constraint: LabelConstraint,
    src_prebound: bool,
    optional: bool,
    optional_unbind: &'a [String],
    path_alias: Option<&'a str>,
    pending: std::vec::IntoIter<Result<Row>>,
}

impl<'a, S: GraphSnapshot + 'a> MatchInIter<'a, S> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        snapshot: &'a S,
        input: Box<PlanIterator<'a, S>>,
        src_alias: &'a str,
        rel_ids: Option<Vec<RelTypeId>>,
        edge_alias: Option<&'a str>,
        dst_alias: &'a str,
        dst_label_constraint: LabelConstraint,
        src_prebound: bool,
        optional: bool,
        optional_unbind: &'a [String],
        path_alias: Option<&'a str>,
    ) -> Self {
        Self {
            snapshot,
            input,
            src_alias,
            rel_ids,
            edge_alias,
            dst_alias,
            dst_label_constraint,
            src_prebound,
            optional,
            optional_unbind,
            path_alias,
            pending: Vec::new().into_iter(),
        }
    }
}

impl<'a, S: GraphSnapshot + 'a> Iterator for MatchInIter<'a, S> {
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

            let results = collect_match_in_rows(
                self.snapshot,
                &row,
                self.src_alias,
                &self.rel_ids,
                self.edge_alias,
                self.dst_alias,
                &self.dst_label_constraint,
                self.src_prebound,
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

pub struct MatchUndirectedIter<'a, S: GraphSnapshot + 'a> {
    snapshot: &'a S,
    input: Box<PlanIterator<'a, S>>,
    src_alias: &'a str,
    rel_ids: Option<Vec<RelTypeId>>,
    edge_alias: Option<&'a str>,
    dst_alias: &'a str,
    dst_label_constraint: LabelConstraint,
    src_prebound: bool,
    optional: bool,
    optional_unbind: &'a [String],
    path_alias: Option<&'a str>,
    pending: std::vec::IntoIter<Result<Row>>,
}

impl<'a, S: GraphSnapshot + 'a> MatchUndirectedIter<'a, S> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        snapshot: &'a S,
        input: Box<PlanIterator<'a, S>>,
        src_alias: &'a str,
        rel_ids: Option<Vec<RelTypeId>>,
        edge_alias: Option<&'a str>,
        dst_alias: &'a str,
        dst_label_constraint: LabelConstraint,
        src_prebound: bool,
        optional: bool,
        optional_unbind: &'a [String],
        path_alias: Option<&'a str>,
    ) -> Self {
        Self {
            snapshot,
            input,
            src_alias,
            rel_ids,
            edge_alias,
            dst_alias,
            dst_label_constraint,
            src_prebound,
            optional,
            optional_unbind,
            path_alias,
            pending: Vec::new().into_iter(),
        }
    }
}

impl<'a, S: GraphSnapshot + 'a> Iterator for MatchUndirectedIter<'a, S> {
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

            let results = collect_match_undirected_rows(
                self.snapshot,
                &row,
                self.src_alias,
                &self.rel_ids,
                self.edge_alias,
                self.dst_alias,
                &self.dst_label_constraint,
                self.src_prebound,
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

fn chain_incoming_candidates<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    target_iid: super::InternalNodeId,
    rel_ids: &Option<Vec<RelTypeId>>,
) -> Box<dyn Iterator<Item = EdgeKey> + 'a> {
    if let Some(rids) = rel_ids {
        let mut iter: Box<dyn Iterator<Item = EdgeKey>> = Box::new(std::iter::empty());
        for rid in rids {
            iter = Box::new(iter.chain(snapshot.incoming_neighbors_erased(target_iid, Some(*rid))));
        }
        iter
    } else {
        snapshot.incoming_neighbors_erased(target_iid, None)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_match_in<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Option<Box<Plan>>,
    src_alias: &'a str,
    rels: &'a [String],
    edge_alias: &'a Option<std::sync::Arc<str>>,
    dst_alias: &'a str,
    dst_labels: &'a [String],
    src_prebound: bool,
    optional: bool,
    optional_unbind: &'a [String],
    path_alias: &'a Option<std::sync::Arc<str>>,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let rel_ids = resolve_rel_ids(snapshot, rels);
    let dst_label_constraint = resolve_label_constraint(snapshot, dst_labels);
    let input_iter = input
        .as_ref()
        .map(|plan| execute_plan(snapshot, plan, params))
        .unwrap_or_else(|| PlanIterator::ReturnOne(std::iter::once(Ok(Row::default()))));

    PlanIterator::MatchIn(Box::new(MatchInIter::new(
        snapshot,
        Box::new(input_iter),
        src_alias,
        rel_ids,
        edge_alias.as_deref(),
        dst_alias,
        dst_label_constraint,
        src_prebound,
        optional,
        optional_unbind,
        path_alias.as_deref(),
    )))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_match_undirected<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Option<Box<Plan>>,
    src_alias: &'a str,
    rels: &'a [String],
    edge_alias: &'a Option<std::sync::Arc<str>>,
    dst_alias: &'a str,
    dst_labels: &'a [String],
    src_prebound: bool,
    limit: Option<usize>,
    optional: bool,
    optional_unbind: &'a [String],
    path_alias: &'a Option<std::sync::Arc<str>>,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let rel_ids = resolve_rel_ids(snapshot, rels);
    let dst_label_constraint = resolve_label_constraint(snapshot, dst_labels);
    let input_iter = input
        .as_ref()
        .map(|plan| execute_plan(snapshot, plan, params))
        .unwrap_or_else(|| PlanIterator::ReturnOne(std::iter::once(Ok(Row::default()))));
    let iter = PlanIterator::MatchUndirected(Box::new(MatchUndirectedIter::new(
        snapshot,
        Box::new(input_iter),
        src_alias,
        rel_ids,
        edge_alias.as_deref(),
        dst_alias,
        dst_label_constraint,
        src_prebound,
        optional,
        optional_unbind,
        path_alias.as_deref(),
    )));
    if let Some(limit) = limit {
        PlanIterator::Limit(Box::new(super::LimitIter {
            input: Box::new(iter),
            remaining: limit,
        }))
    } else {
        iter
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_match_in_rows<S: GraphSnapshot>(
    snapshot: &S,
    row: &Row,
    src_alias: &str,
    rel_ids: &Option<Vec<RelTypeId>>,
    edge_alias: Option<&str>,
    dst_alias: &str,
    dst_label_constraint: &LabelConstraint,
    src_prebound: bool,
    optional: bool,
    optional_unbind: &[String],
    path_alias: Option<&str>,
) -> Vec<Result<Row>> {
    let target_iid = match row.get(src_alias).cloned() {
        Some(Value::NodeId(id)) => id,
        Some(Value::Null) | None if optional => {
            return vec![Ok(apply_optional_unbinds_row(row.clone(), optional_unbind))];
        }
        _ => return Vec::new(),
    };

    let mut rows = Vec::new();
    for edge in chain_incoming_candidates(snapshot, target_iid, rel_ids) {
        if path_alias_contains_edge(snapshot, row, path_alias, edge) {
            continue;
        }
        if !row_matches_node_binding(row, dst_alias, edge.src) {
            continue;
        }
        if !node_matches_label_constraint(snapshot, edge.src, dst_label_constraint) {
            continue;
        }

        let mut new_row = row.clone();
        new_row = new_row.with(dst_alias, Value::NodeId(edge.src));
        if let Some(ea) = edge_alias {
            new_row = new_row.with(ea, Value::EdgeKey(edge));
        }
        if let Some(pa) = path_alias {
            new_row.join_path(pa, edge.dst, edge, edge.src);
        }
        rows.push(Ok(new_row));
    }

    if optional && rows.is_empty() && src_prebound {
        rows.push(Ok(apply_optional_unbinds_row(row.clone(), optional_unbind)));
    }

    rows
}

#[allow(clippy::too_many_arguments)]
fn collect_match_undirected_rows<S: GraphSnapshot>(
    snapshot: &S,
    row: &Row,
    src_alias: &str,
    rel_ids: &Option<Vec<RelTypeId>>,
    edge_alias: Option<&str>,
    dst_alias: &str,
    dst_label_constraint: &LabelConstraint,
    src_prebound: bool,
    optional: bool,
    optional_unbind: &[String],
    path_alias: Option<&str>,
) -> Vec<Result<Row>> {
    let src_iid = match row.get(src_alias).cloned() {
        Some(Value::NodeId(id)) => id,
        Some(Value::Null) | None if optional => {
            return vec![Ok(apply_optional_unbinds_row(row.clone(), optional_unbind))];
        }
        _ => return Vec::new(),
    };

    let mut rows = Vec::new();
    match rel_ids {
        Some(rids) => {
            for rid in rids {
                append_undirected_rows(
                    snapshot,
                    row,
                    src_iid,
                    Some(*rid),
                    edge_alias,
                    dst_alias,
                    dst_label_constraint,
                    path_alias,
                    &mut rows,
                );
            }
        }
        None => append_undirected_rows(
            snapshot,
            row,
            src_iid,
            None,
            edge_alias,
            dst_alias,
            dst_label_constraint,
            path_alias,
            &mut rows,
        ),
    }

    if optional && rows.is_empty() && src_prebound {
        rows.push(Ok(apply_optional_unbinds_row(row.clone(), optional_unbind)));
    }

    rows
}

#[allow(clippy::too_many_arguments)]
fn append_undirected_rows<S: GraphSnapshot>(
    snapshot: &S,
    row: &Row,
    src_iid: super::InternalNodeId,
    rel_id: Option<RelTypeId>,
    edge_alias: Option<&str>,
    dst_alias: &str,
    dst_label_constraint: &LabelConstraint,
    path_alias: Option<&str>,
    rows: &mut Vec<Result<Row>>,
) {
    for edge in snapshot.neighbors(src_iid, rel_id) {
        if path_alias_contains_edge(snapshot, row, path_alias, edge) {
            continue;
        }
        if !row_matches_node_binding(row, dst_alias, edge.dst) {
            continue;
        }
        if !node_matches_label_constraint(snapshot, edge.dst, dst_label_constraint) {
            continue;
        }
        let mut new_row = row.clone();
        new_row = new_row.with(dst_alias, Value::NodeId(edge.dst));
        if let Some(ea) = edge_alias {
            new_row = new_row.with(ea, Value::EdgeKey(edge));
        }
        if let Some(pa) = path_alias {
            new_row.join_path(pa, edge.src, edge, edge.dst);
        }
        rows.push(Ok(new_row));
    }

    for edge in snapshot.incoming_neighbors_erased(src_iid, rel_id) {
        if path_alias_contains_edge(snapshot, row, path_alias, edge) || edge.src == edge.dst {
            continue;
        }
        if !row_matches_node_binding(row, dst_alias, edge.src) {
            continue;
        }
        if !node_matches_label_constraint(snapshot, edge.src, dst_label_constraint) {
            continue;
        }
        let mut new_row = row.clone();
        new_row = new_row.with(dst_alias, Value::NodeId(edge.src));
        if let Some(ea) = edge_alias {
            new_row = new_row.with(ea, Value::EdgeKey(edge));
        }
        if let Some(pa) = path_alias {
            new_row.join_path(pa, edge.dst, edge, edge.src);
        }
        rows.push(Ok(new_row));
    }
}

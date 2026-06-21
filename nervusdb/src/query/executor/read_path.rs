use super::{
    EdgeKey, Error, InternalNodeId, LabelConstraint, RelTypeId, Result, Row, Value,
    node_matches_label_constraint,
};

fn edge_multiplicity<S: GraphSnapshot>(snapshot: &S, edge: EdgeKey) -> usize {
    let count = snapshot
        .neighbors(edge.src, Some(edge.rel))
        .filter(|candidate| candidate.dst == edge.dst)
        .count();
    count.max(1)
}

fn path_alias_contains_edge<S: GraphSnapshot>(
    snapshot: &S,
    row: &Row,
    path_alias: Option<&str>,
    edge: EdgeKey,
) -> bool {
    if let Some(alias) = path_alias
        && let Some(Value::Path(path)) = row.get(alias)
    {
        let used = path
            .edges
            .iter()
            .filter(|existing| **existing == edge)
            .count();
        if used == 0 {
            return false;
        }
        return used >= edge_multiplicity(snapshot, edge);
    }
    false
}

fn apply_optional_unbinds_row(mut row: Row, optional_unbind: &[String]) -> Row {
    for alias in optional_unbind {
        row = row.with(alias.clone(), Value::Null);
    }
    row
}

fn row_matches_node_binding(row: &Row, alias: &str, candidate: InternalNodeId) -> bool {
    match row.get(alias) {
        None => true,
        Some(Value::Null) => false,
        Some(value) => value_node_id(value).is_some_and(|id| id == candidate),
    }
}

fn value_node_id(value: &Value) -> Option<InternalNodeId> {
    match value {
        Value::NodeId(id) => Some(*id),
        Value::Node(node) => Some(node.id),
        _ => None,
    }
}
use crate::api::GraphSnapshot;

pub(super) struct MatchOutIter<'a, S: GraphSnapshot + 'a> {
    snapshot: &'a S,
    src_alias: &'a str,
    rels: Option<Vec<RelTypeId>>,
    edge_alias: Option<&'a str>,
    dst_alias: &'a str,
    node_iter: Box<dyn Iterator<Item = InternalNodeId> + 'a>,
    cur_src: Option<InternalNodeId>,
    cur_edges: Option<Box<dyn Iterator<Item = EdgeKey> + 'a>>,
    path_alias: Option<&'a str>,
}

impl<'a, S: GraphSnapshot + 'a> MatchOutIter<'a, S> {
    pub(super) fn new(
        snapshot: &'a S,
        src_alias: &'a str,
        rels: Option<Vec<RelTypeId>>,
        edge_alias: Option<&'a str>,
        dst_alias: &'a str,
        path_alias: Option<&'a str>,
    ) -> Self {
        Self {
            snapshot,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            node_iter: snapshot.nodes(),
            cur_src: None,
            cur_edges: None,
            path_alias,
        }
    }

    fn next_src(&mut self) -> Option<InternalNodeId> {
        for src in self.node_iter.by_ref() {
            if self.snapshot.is_tombstoned_node(src) {
                continue;
            }
            return Some(src);
        }
        None
    }
}

impl<'a, S: GraphSnapshot + 'a> Iterator for MatchOutIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.cur_edges.is_none() {
                let src = self.next_src()?;
                self.cur_src = Some(src);

                if let Some(rels) = &self.rels {
                    // Chain multiple iterators
                    let mut iter: Box<dyn Iterator<Item = EdgeKey> + 'a> =
                        Box::new(std::iter::empty());
                    for rel in rels {
                        // Note: Depending on impl, this might need optimizing.
                        // But for now we chain them.
                        // We must clone rel because it's owned by the Vec in struct? No, rel is Copy (RelTypeId).
                        let r = *rel;
                        let neighbors = self.snapshot.neighbors(src, Some(r));
                        iter = Box::new(iter.chain(neighbors));
                    }
                    self.cur_edges = Some(iter);
                } else {
                    // Match all
                    self.cur_edges = Some(Box::new(self.snapshot.neighbors(src, None)));
                }
            }

            let edges = self.cur_edges.as_mut().expect("cur_edges must exist");

            if let Some(edge) = edges.next() {
                let mut row = Row::default().with(self.src_alias, Value::NodeId(edge.src));
                if let Some(edge_alias) = self.edge_alias {
                    row = row.with(edge_alias, Value::EdgeKey(edge));
                }
                row = row.with(self.dst_alias, Value::NodeId(edge.dst));

                if let Some(path_alias) = self.path_alias {
                    row.join_path(path_alias, edge.src, edge, edge.dst);
                }

                // Always return full row - projection happens in Plan::Project
                return Some(Ok(row));
            }

            self.cur_edges = None;
            self.cur_src = None;
        }
    }
}

pub struct ExpandIter<'a, S: GraphSnapshot + 'a> {
    snapshot: &'a S,
    input: Box<dyn Iterator<Item = Result<Row>> + 'a>,
    src_alias: &'a str,
    rels: Option<Vec<RelTypeId>>,
    edge_alias: Option<&'a str>,
    dst_alias: &'a str,
    optional: bool,
    emit_on_miss: bool,
    optional_unbind: Vec<String>,
    dst_label_constraint: LabelConstraint,
    cur_row: Option<Row>,
    cur_edges: Option<Box<dyn Iterator<Item = EdgeKey> + 'a>>,
    yielded_any: bool,
    path_alias: Option<&'a str>,
}

impl<'a, S: GraphSnapshot + 'a> ExpandIter<'a, S> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        snapshot: &'a S,
        input: Box<dyn Iterator<Item = Result<Row>> + 'a>,
        src_alias: &'a str,
        rels: Option<Vec<RelTypeId>>,
        edge_alias: Option<&'a str>,
        dst_alias: &'a str,
        optional: bool,
        emit_on_miss: bool,
        optional_unbind: Vec<String>,
        dst_label_constraint: LabelConstraint,
        path_alias: Option<&'a str>,
    ) -> Self {
        Self {
            snapshot,
            input,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            optional,
            emit_on_miss,
            optional_unbind,
            dst_label_constraint,
            cur_row: None,
            cur_edges: None,
            yielded_any: false,
            path_alias,
        }
    }
}

impl<'a, S: GraphSnapshot + 'a> Iterator for ExpandIter<'a, S> {
    type Item = Result<Row>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.cur_edges.is_none() {
                match self.input.next() {
                    Some(Ok(row)) => {
                        self.cur_row = Some(row.clone());
                        let src_val = row
                            .cols
                            .iter()
                            .find(|(k, _)| k == self.src_alias)
                            .map(|(_, v)| v);
                        match src_val {
                            Some(Value::NodeId(id)) => {
                                if let Some(rels) = &self.rels {
                                    let mut iter: Box<dyn Iterator<Item = EdgeKey> + 'a> =
                                        Box::new(std::iter::empty());
                                    // Reverse iteration to maintain chain order? Or standard.
                                    for rel in rels {
                                        let neighbors = self.snapshot.neighbors(*id, Some(*rel));
                                        iter = Box::new(iter.chain(neighbors));
                                    }
                                    self.cur_edges = Some(iter);
                                } else {
                                    self.cur_edges =
                                        Some(Box::new(self.snapshot.neighbors(*id, None)));
                                }
                                self.yielded_any = false;
                            }
                            Some(Value::Null) => {
                                // Source is Null (e.g. from previous optional match)
                                if self.optional {
                                    // Propagate Nulls
                                    let row = apply_optional_unbinds_row(
                                        row.clone(),
                                        &self.optional_unbind,
                                    );
                                    self.cur_row = None; // Done with this row
                                    return Some(Ok(row));
                                } else {
                                    // Not optional: Filter out this row
                                    self.cur_row = None;
                                    continue;
                                }
                            }
                            Some(_) => {
                                return Some(Err(Error::Other(format!(
                                    "Variable {} is not a node",
                                    self.src_alias
                                ))));
                            }
                            None => {
                                return Some(Err(Error::Other(format!(
                                    "Variable {} not found",
                                    self.src_alias
                                ))));
                            }
                        }
                    }
                    Some(Err(e)) => return Some(Err(e)),
                    None => return None,
                }
            }

            let edges = self.cur_edges.as_mut().unwrap();
            if let Some(edge) = edges.next() {
                if path_alias_contains_edge(
                    self.snapshot,
                    self.cur_row.as_ref().unwrap(),
                    self.path_alias,
                    edge,
                ) {
                    continue;
                }
                if !row_matches_node_binding(
                    self.cur_row.as_ref().unwrap(),
                    self.dst_alias,
                    edge.dst,
                ) {
                    continue;
                }
                if !node_matches_label_constraint(
                    self.snapshot,
                    edge.dst,
                    &self.dst_label_constraint,
                ) {
                    continue;
                }
                self.yielded_any = true;
                let mut row = self.cur_row.as_ref().unwrap().clone();
                if let Some(ea) = self.edge_alias {
                    row = row.with(ea, Value::EdgeKey(edge));
                }
                row = row.with(self.dst_alias, Value::NodeId(edge.dst));

                if let Some(path_alias) = self.path_alias {
                    row.join_path(path_alias, edge.src, edge, edge.dst);
                }

                return Some(Ok(row));
            } else {
                if self.optional && self.emit_on_miss && !self.yielded_any {
                    self.yielded_any = true;
                    let row = apply_optional_unbinds_row(
                        self.cur_row.take().unwrap(),
                        &self.optional_unbind,
                    );
                    self.cur_edges = None;
                    return Some(Ok(row));
                }
                self.cur_edges = None;
                self.cur_row = None;
            }
        }
    }
}

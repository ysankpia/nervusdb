use super::{
    EdgeKey, ErasedSnapshot, Error, InternalNodeId, LabelConstraint, PathValue, RelTypeId, Result,
    Row, Value, apply_optional_unbinds_row, edge_multiplicity, node_matches_label_constraint,
    path_alias_contains_edge, row_matches_node_binding,
};
use crate::ast::RelationshipDirection;
use nervusdb_api::GraphSnapshot;
use std::collections::HashMap;

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

/// Variable-length path iterator using DFS
const DEFAULT_MAX_VAR_LEN_HOPS: u32 = 64;

type VarLenStackItem = (
    InternalNodeId,
    InternalNodeId,
    u32,
    Option<EdgeKey>,
    Option<PathValue>,
    Vec<EdgeKey>,
    Vec<EdgeKey>,
);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) struct MatchOutVarLenIter<'a, S: GraphSnapshot + 'a> {
    snapshot: &'a S,
    input: Option<Box<dyn Iterator<Item = Result<Row>> + 'a>>,
    cur_row: Option<Row>,
    src_alias: &'a str,
    rels: Option<Vec<RelTypeId>>,
    edge_alias: Option<&'a str>,
    dst_alias: &'a str,
    dst_label_constraint: LabelConstraint,
    direction: RelationshipDirection,
    min_hops: u32,
    max_hops: Option<u32>,
    limit: Option<u32>,
    node_iter: Option<Box<dyn Iterator<Item = InternalNodeId> + 'a>>,
    // DFS state: (start_node, current_node, current_depth, incoming_edge, current_path)
    stack: Vec<VarLenStackItem>,
    edge_constraint: Option<Vec<EdgeKey>>,
    edge_constraint_valid: bool,
    emitted: u32,
    yielded_any: bool,
    optional: bool,
    emit_on_miss: bool,
    optional_unbind: Vec<String>,
    path_alias: Option<&'a str>,
}

impl<'a, S: GraphSnapshot + 'a> MatchOutVarLenIter<'a, S> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        snapshot: &'a S,
        input: Option<Box<dyn Iterator<Item = Result<Row>> + 'a>>,
        src_alias: &'a str,
        rels: Option<Vec<RelTypeId>>,
        edge_alias: Option<&'a str>,
        dst_alias: &'a str,
        dst_label_constraint: LabelConstraint,
        direction: RelationshipDirection,
        min_hops: u32,
        max_hops: Option<u32>,
        limit: Option<u32>,
        optional: bool,
        emit_on_miss: bool,
        optional_unbind: Vec<String>,
        path_alias: Option<&'a str>,
    ) -> Self {
        let node_iter = if input.is_none() {
            Some(snapshot.nodes())
        } else {
            None
        };

        Self {
            snapshot,
            input,
            cur_row: None,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            dst_label_constraint,
            direction,
            min_hops,
            max_hops,
            limit,
            node_iter,
            stack: Vec::new(),
            edge_constraint: None,
            edge_constraint_valid: true,
            emitted: 0,
            yielded_any: false,
            optional,
            emit_on_miss,
            optional_unbind,
            path_alias,
        }
    }

    /// Start DFS from a node
    fn start_dfs(&mut self, start_node: InternalNodeId) {
        let (initial_path, initial_used_edges) = if let Some(alias) = self.path_alias
            && let Some(row) = &self.cur_row
        {
            match row.get(alias) {
                Some(Value::Path(p)) => (Some(p.clone()), p.edges.clone()),
                _ => (None, Vec::new()),
            }
        } else {
            (None, Vec::new())
        };

        // If it's a new path (not continuation), initialize it with the first node.
        // Actually, join_path will do that if we pass None initial_path.
        // But for DFS stack, we need to hold it.
        self.stack.push((
            start_node,
            start_node,
            0,
            None,
            initial_path,
            Vec::new(),
            initial_used_edges,
        ));
    }

    fn parse_bound_edge_constraint(value: &Value) -> Option<Vec<EdgeKey>> {
        match value {
            Value::Null => Some(Vec::new()),
            Value::EdgeKey(edge) => Some(vec![*edge]),
            Value::Relationship(rel) => Some(vec![rel.key]),
            Value::List(values) => {
                let mut out = Vec::with_capacity(values.len());
                for item in values {
                    match item {
                        Value::EdgeKey(edge) => out.push(*edge),
                        Value::Relationship(rel) => out.push(rel.key),
                        _ => return None,
                    }
                }
                Some(out)
            }
            _ => None,
        }
    }
}

impl<'a, S: GraphSnapshot + 'a> Iterator for MatchOutVarLenIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        let max_hops = self.max_hops.unwrap_or(DEFAULT_MAX_VAR_LEN_HOPS);

        // Check limit
        if let Some(limit) = self.limit
            && self.emitted >= limit
        {
            return None;
        }

        loop {
            // 1. Process Stack (DFS)
            if let Some((
                start_node,
                current_node,
                depth,
                _incoming_edge,
                current_path,
                path_edges,
                used_edges,
            )) = self.stack.pop()
            {
                // Expand
                if depth < max_hops && self.edge_constraint_valid {
                    let mut emitted_per_edge: HashMap<EdgeKey, usize> = HashMap::new();
                    let mut push_edge =
                        |edge: EdgeKey,
                         next_node: InternalNodeId,
                         stack: &mut Vec<VarLenStackItem>| {
                            if let Some(bound) = self.edge_constraint.as_ref() {
                                let index = depth as usize;
                                if index >= bound.len() || bound[index] != edge {
                                    return;
                                }
                            }

                            let used_count = used_edges
                                .iter()
                                .filter(|existing| **existing == edge)
                                .count();
                            let total_count = edge_multiplicity(self.snapshot, edge);
                            if used_count >= total_count {
                                return;
                            }

                            let remaining = total_count - used_count;
                            let already_emitted = emitted_per_edge.entry(edge).or_insert(0);
                            if *already_emitted >= remaining {
                                return;
                            }
                            *already_emitted += 1;

                            let mut next_path = current_path.clone();
                            if self.path_alias.is_some() {
                                if let Some(p) = &mut next_path {
                                    p.edges.push(edge);
                                    p.nodes.push(next_node);
                                } else {
                                    next_path = Some(PathValue {
                                        nodes: vec![start_node, next_node],
                                        edges: vec![edge],
                                    });
                                }
                            }
                            let mut next_used = used_edges.clone();
                            next_used.push(edge);
                            let mut next_path_edges = path_edges.clone();
                            next_path_edges.push(edge);
                            stack.push((
                                start_node,
                                next_node,
                                depth + 1,
                                Some(edge),
                                next_path,
                                next_path_edges,
                                next_used,
                            ));
                        };

                    match (&self.direction, self.rels.as_ref()) {
                        (RelationshipDirection::LeftToRight, Some(rels)) => {
                            for rel in rels {
                                for edge in self.snapshot.neighbors(current_node, Some(*rel)) {
                                    push_edge(edge, edge.dst, &mut self.stack);
                                }
                            }
                        }
                        (RelationshipDirection::LeftToRight, None) => {
                            for edge in self.snapshot.neighbors(current_node, None) {
                                push_edge(edge, edge.dst, &mut self.stack);
                            }
                        }

                        (RelationshipDirection::RightToLeft, Some(rels)) => {
                            for rel in rels {
                                for edge in self
                                    .snapshot
                                    .incoming_neighbors_erased(current_node, Some(*rel))
                                {
                                    if edge.src == edge.dst {
                                        continue;
                                    }
                                    push_edge(edge, edge.src, &mut self.stack);
                                }
                            }
                        }
                        (RelationshipDirection::RightToLeft, None) => {
                            for edge in self.snapshot.incoming_neighbors_erased(current_node, None)
                            {
                                if edge.src == edge.dst {
                                    continue;
                                }
                                push_edge(edge, edge.src, &mut self.stack);
                            }
                        }

                        (RelationshipDirection::Undirected, Some(rels)) => {
                            for rel in rels {
                                for edge in self.snapshot.neighbors(current_node, Some(*rel)) {
                                    push_edge(edge, edge.dst, &mut self.stack);
                                }
                                for edge in self
                                    .snapshot
                                    .incoming_neighbors_erased(current_node, Some(*rel))
                                {
                                    if edge.src == edge.dst {
                                        continue;
                                    }
                                    push_edge(edge, edge.src, &mut self.stack);
                                }
                            }
                        }
                        (RelationshipDirection::Undirected, None) => {
                            for edge in self.snapshot.neighbors(current_node, None) {
                                push_edge(edge, edge.dst, &mut self.stack);
                            }
                            for edge in self.snapshot.incoming_neighbors_erased(current_node, None)
                            {
                                if edge.src == edge.dst {
                                    continue;
                                }
                                push_edge(edge, edge.src, &mut self.stack);
                            }
                        }
                    }
                }

                // Emit check
                if depth >= self.min_hops {
                    if !self.edge_constraint_valid {
                        continue;
                    }
                    if let Some(bound) = self.edge_constraint.as_ref()
                        && path_edges.len() != bound.len()
                    {
                        continue;
                    }

                    let base_row = self.cur_row.clone().unwrap_or_default();
                    if !row_matches_node_binding(&base_row, self.dst_alias, current_node) {
                        continue;
                    }
                    if !node_matches_label_constraint(
                        self.snapshot,
                        current_node,
                        &self.dst_label_constraint,
                    ) {
                        continue;
                    }

                    let mut row = base_row;
                    row = row.with(self.src_alias, Value::NodeId(start_node));

                    if let Some(edge_alias) = self.edge_alias {
                        let edge_values = path_edges
                            .iter()
                            .copied()
                            .map(Value::EdgeKey)
                            .collect::<Vec<_>>();
                        row = row.with(edge_alias, Value::List(edge_values));
                    }
                    row = row.with(self.dst_alias, Value::NodeId(current_node));

                    if let Some(path_alias) = self.path_alias {
                        if let Some(p) = current_path {
                            row = row.with(path_alias, Value::Path(p));
                        } else if depth == 0 {
                            // Empty path starting with just the node?
                            // Cypher p = (n) where length(p) = 0.
                            row = row.with(
                                path_alias,
                                Value::Path(PathValue {
                                    nodes: vec![start_node],
                                    edges: vec![],
                                }),
                            );
                        }
                    }

                    self.emitted += 1;
                    self.yielded_any = true;
                    return Some(Ok(row));
                }
                continue;
            }

            // 2. Stack Empty: Check Optional Null emission
            if let Some(row) = &self.cur_row
                && self.optional
                && self.emit_on_miss
                && !self.yielded_any
                && self.input.is_some()
            {
                self.yielded_any = true;
                let null_row = apply_optional_unbinds_row(row.clone(), &self.optional_unbind);
                self.emitted += 1;
                return Some(Ok(null_row));
            }

            // 3. Get Next Start Node
            if let Some(input) = &mut self.input {
                match input.next() {
                    Some(Ok(row)) => {
                        self.cur_row = Some(row.clone());
                        self.yielded_any = false;
                        self.edge_constraint = None;
                        self.edge_constraint_valid = true;
                        if let Some(edge_alias) = self.edge_alias
                            && let Some(bound_value) = row.get(edge_alias)
                        {
                            if let Some(edges) = Self::parse_bound_edge_constraint(bound_value) {
                                self.edge_constraint = Some(edges);
                            } else {
                                self.edge_constraint = Some(Vec::new());
                                self.edge_constraint_valid = false;
                            }
                        }

                        if let Some(src_id) = row.get_node(self.src_alias) {
                            self.start_dfs(src_id);
                        } else {
                            match row.get(self.src_alias) {
                                Some(Value::Null) => {
                                    // Optional null source handled next iteration
                                }
                                Some(_) => {
                                    // Invalid source binding type; skip this row.
                                }
                                None => {
                                    if self.edge_constraint_valid {
                                        if let Some(bound_edges) = self.edge_constraint.as_ref() {
                                            if let Some(first_edge) = bound_edges.first() {
                                                let mut starts = vec![match self.direction {
                                                    RelationshipDirection::LeftToRight => {
                                                        first_edge.src
                                                    }
                                                    RelationshipDirection::RightToLeft => {
                                                        first_edge.dst
                                                    }
                                                    RelationshipDirection::Undirected => {
                                                        first_edge.src
                                                    }
                                                }];
                                                if matches!(
                                                    self.direction,
                                                    RelationshipDirection::Undirected
                                                ) && first_edge.src != first_edge.dst
                                                {
                                                    starts.push(first_edge.dst);
                                                }
                                                for start in starts {
                                                    if !self.snapshot.is_tombstoned_node(start) {
                                                        self.start_dfs(start);
                                                    }
                                                }
                                            } else if self.min_hops == 0 {
                                                let starts: Vec<_> = self
                                                    .snapshot
                                                    .nodes()
                                                    .filter(|id| {
                                                        !self.snapshot.is_tombstoned_node(*id)
                                                    })
                                                    .collect();
                                                for start in starts {
                                                    self.start_dfs(start);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some(Err(e)) => return Some(Err(e)),
                    None => return None, // Input exhausted
                }
            } else {
                // Scan mode
                if let Some(node_iter) = &mut self.node_iter {
                    match node_iter.next() {
                        Some(id) => {
                            if self.snapshot.is_tombstoned_node(id) {
                                continue;
                            }
                            self.cur_row = Some(Row::new(vec![(
                                self.src_alias.to_string(),
                                Value::NodeId(id),
                            )]));
                            self.yielded_any = false;
                            self.start_dfs(id);
                        }
                        None => return None,
                    }
                } else {
                    return None;
                }
            }
        }
    }
}

pub(super) struct ExpandIter<'a, S: GraphSnapshot + 'a> {
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

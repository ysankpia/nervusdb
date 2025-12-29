use crate::ast::{AggregateFunction, Direction, Expression, Literal, PathElement, Pattern};
use crate::error::{Error, Result};
pub use nervusdb_v2_api::LabelId;
use nervusdb_v2_api::{EdgeKey, ExternalId, GraphSnapshot, InternalNodeId, RelTypeId};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Value {
    NodeId(InternalNodeId),
    ExternalId(ExternalId),
    EdgeKey(EdgeKey),
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
}

// Custom Hash implementation for Value (since Float doesn't implement Hash)
impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Value::NodeId(id) => id.hash(state),
            Value::ExternalId(ext) => ext.hash(state),
            Value::EdgeKey(key) => key.hash(state),
            Value::Int(i) => i.hash(state),
            Value::Float(f) => {
                // Hash by bit pattern for consistency
                f.to_bits().hash(state);
            }
            Value::String(s) => s.hash(state),
            Value::Bool(b) => b.hash(state),
            Value::Null => 0u8.hash(state),
        }
    }
}

impl Eq for Value {}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row {
    // Small row: linear search is fine for MVP.
    cols: Vec<(String, Value)>,
}

impl Row {
    pub fn with(mut self, name: impl Into<String>, value: Value) -> Self {
        let name = name.into();
        if let Some((_k, v)) = self.cols.iter_mut().find(|(k, _)| *k == name) {
            *v = value;
        } else {
            self.cols.push((name, value));
        }
        self
    }

    pub fn get_node(&self, name: &str) -> Option<InternalNodeId> {
        self.cols.iter().find_map(|(k, v)| {
            if k == name {
                match v {
                    Value::NodeId(iid) => Some(*iid),
                    _ => None,
                }
            } else {
                None
            }
        })
    }

    pub fn get_edge(&self, name: &str) -> Option<EdgeKey> {
        self.cols.iter().find_map(|(k, v)| {
            if k == name {
                match v {
                    Value::EdgeKey(e) => Some(*e),
                    _ => None,
                }
            } else {
                None
            }
        })
    }

    pub fn project(&self, names: &[&str]) -> Row {
        let mut out = Row::default();
        for &name in names {
            if let Some((k, v)) = self.cols.iter().find(|(k, _)| k == name) {
                out.cols.push((k.clone(), v.clone()));
            } else {
                out.cols.push((name.to_string(), Value::Null));
            }
        }
        out
    }

    pub fn columns(&self) -> &[(String, Value)] {
        &self.cols
    }
}

#[derive(Debug, Clone)]
pub enum Plan {
    /// `RETURN 1`
    ReturnOne,
    /// `MATCH (n) RETURN ...`
    NodeScan { alias: String },
    /// `MATCH (a)-[:rel]->(b) RETURN ...`
    MatchOut {
        src_alias: String,
        rel: Option<RelTypeId>,
        edge_alias: Option<String>,
        dst_alias: String,
        limit: Option<u32>,
        // Note: project is kept for backward compatibility but projection
        // should happen after filtering (see Plan::Project)
        project: Vec<String>,
        project_external: bool,
    },
    /// `MATCH (a)-[:rel*min..max]->(b) RETURN ...` (variable length)
    MatchOutVarLen {
        src_alias: String,
        rel: Option<RelTypeId>,
        edge_alias: Option<String>,
        dst_alias: String,
        min_hops: u32,
        max_hops: Option<u32>,
        limit: Option<u32>,
        project: Vec<String>,
        project_external: bool,
    },
    /// `MATCH ... WHERE ... RETURN ...` (with filter)
    Filter {
        input: Box<Plan>,
        predicate: Expression,
    },
    /// Project columns from input row (runs after filtering)
    Project {
        input: Box<Plan>,
        columns: Vec<String>,
    },
    /// Aggregation: COUNT, SUM, AVG with optional grouping
    Aggregate {
        input: Box<Plan>,
        group_by: Vec<String>,                        // Variables to group by
        aggregates: Vec<(AggregateFunction, String)>, // (Function, Alias)
    },
    /// `ORDER BY` - sort results
    OrderBy {
        input: Box<Plan>,
        items: Vec<(String, Direction)>, // (column_name, ASC|DESC)
    },
    /// `SKIP` - skip first n rows
    Skip { input: Box<Plan>, skip: u32 },
    /// `LIMIT` - limit result count
    Limit { input: Box<Plan>, limit: u32 },
    /// `RETURN DISTINCT` - deduplicate results
    Distinct { input: Box<Plan> },
    /// `CREATE (n)-[:rel]->(m)` - create pattern
    Create { pattern: Pattern },
    /// `DELETE` - delete nodes/edges (with input plan for variable resolution)
    Delete {
        input: Box<Plan>,
        detach: bool,
        expressions: Vec<Expression>,
    },
}

pub fn execute_plan<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    plan: &'a Plan,
    params: &'a crate::query_api::Params,
) -> Box<dyn Iterator<Item = Result<Row>> + 'a> {
    match plan {
        Plan::ReturnOne => Box::new(std::iter::once(Ok(Row::default().with("1", Value::Int(1))))),
        Plan::NodeScan { alias } => {
            let alias = alias.clone();
            Box::new(snapshot.nodes().filter_map(move |iid| {
                if snapshot.is_tombstoned_node(iid) {
                    return None;
                }
                Some(Ok(Row::default().with(alias.clone(), Value::NodeId(iid))))
            }))
        }
        Plan::MatchOut {
            src_alias,
            rel,
            edge_alias,
            dst_alias,
            limit,
            project: _,
            project_external: _,
        } => {
            let base =
                MatchOutIter::new(snapshot, src_alias, *rel, edge_alias.as_deref(), dst_alias);
            if let Some(n) = limit {
                Box::new(base.take(*n as usize))
            } else {
                Box::new(base)
            }
        }
        Plan::MatchOutVarLen {
            src_alias,
            rel,
            edge_alias,
            dst_alias,
            min_hops,
            max_hops,
            limit,
            project: _,
            project_external: _,
        } => {
            let base = MatchOutVarLenIter::new(
                snapshot,
                src_alias,
                *rel,
                edge_alias.as_deref(),
                dst_alias,
                *min_hops,
                *max_hops,
                *limit,
            );
            if let Some(n) = limit {
                Box::new(base.take(*n as usize))
            } else {
                Box::new(base)
            }
        }
        Plan::Filter { input, predicate } => {
            let input_iter = execute_plan(snapshot, input, params);
            Box::new(input_iter.filter(move |result| {
                match result {
                    Ok(row) => {
                        crate::evaluator::evaluate_expression_bool(predicate, row, snapshot, params)
                    }
                    Err(_) => true, // Pass through errors
                }
            }))
        }
        Plan::Project { input, columns } => {
            let input_iter = execute_plan(snapshot, input, params);
            let names: Vec<&str> = columns.iter().map(|s| s.as_str()).collect();
            Box::new(input_iter.map(move |result| result.map(|row| row.project(&names))))
        }
        Plan::Aggregate {
            input,
            group_by,
            aggregates,
        } => {
            let input_iter = execute_plan(snapshot, input, params);
            execute_aggregate(input_iter, group_by.clone(), aggregates.clone())
        }
        Plan::OrderBy { input, items } => {
            let input_iter = execute_plan(snapshot, input, params);
            let rows: Vec<Result<Row>> = input_iter.collect();
            #[allow(clippy::type_complexity)]
            let mut sortable: Vec<(Result<Row>, Vec<(Value, Direction)>)> = rows
                .into_iter()
                .map(|row| {
                    let sort_keys: Vec<(Value, Direction)> = items
                        .iter()
                        .map(|(col, dir)| {
                            let val = row
                                .as_ref()
                                .ok()
                                .and_then(|r| r.cols.iter().find(|(n, _)| n == col))
                                .map(|(_, v)| v.clone())
                                .unwrap_or(Value::Null);
                            (val, dir.clone())
                        })
                        .collect();
                    (row, sort_keys)
                })
                .collect();

            sortable.sort_by(|a, b| {
                for ((val_a, dir_a), (val_b, _)) in a.1.iter().zip(b.1.iter()) {
                    match val_a.partial_cmp(val_b) {
                        Some(std::cmp::Ordering::Equal) => continue,
                        Some(order) => {
                            return if *dir_a == Direction::Ascending {
                                order
                            } else {
                                order.reverse()
                            };
                        }
                        None => return std::cmp::Ordering::Equal,
                    }
                }
                std::cmp::Ordering::Equal
            });

            Box::new(sortable.into_iter().map(|(row, _)| row))
        }
        Plan::Skip { input, skip } => {
            let input_iter = execute_plan(snapshot, input, params);
            Box::new(input_iter.skip(*skip as usize))
        }
        Plan::Limit { input, limit } => {
            let input_iter = execute_plan(snapshot, input, params);
            Box::new(input_iter.take(*limit as usize))
        }
        Plan::Distinct { input } => {
            let input_iter = execute_plan(snapshot, input, params);
            let mut seen = std::collections::HashSet::new();
            Box::new(input_iter.filter(move |result| {
                if let Ok(row) = result {
                    let key = row
                        .columns()
                        .iter()
                        .map(|(_, v)| format!("{:?}", v))
                        .collect::<Vec<_>>()
                        .join(",");
                    if seen.insert(key) {
                        return true;
                    }
                }
                false
            }))
        }
        Plan::Create { pattern: _ } => {
            // CREATE should be executed via execute_write, not execute_plan
            Box::new(std::iter::once(Err(Error::Other(
                "CREATE must be executed via execute_write".into(),
            ))))
        }
        Plan::Delete { .. } => {
            // DELETE should be executed via execute_write, not execute_plan
            Box::new(std::iter::once(Err(Error::Other(
                "DELETE must be executed via execute_write".into(),
            ))))
        }
    }
}

/// Execute a write plan (CREATE/DELETE) with a transaction
pub fn execute_write<S: GraphSnapshot>(
    plan: &Plan,
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    params: &crate::query_api::Params,
) -> Result<u32> {
    match plan {
        Plan::Create { pattern } => execute_create(txn, pattern, params),
        Plan::Delete {
            input,
            detach,
            expressions,
        } => execute_delete(snapshot, input, txn, *detach, expressions, params),
        _ => Err(Error::Other(
            "Only CREATE and DELETE plans can be executed with execute_write".into(),
        )),
    }
}

pub trait WriteableGraph {
    fn create_node(&mut self, external_id: ExternalId, label_id: LabelId)
    -> Result<InternalNodeId>;
    fn create_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> Result<()>;
    fn set_node_property(
        &mut self,
        node: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()>;
    fn set_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()>;
    fn tombstone_node(&mut self, node: InternalNodeId) -> Result<()>;
    fn tombstone_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> Result<()>;
}

pub use nervusdb_v2_storage::property::PropertyValue;

// Implement WriteableGraph for nervusdb-v2-storage::engine::WriteTxn
// This is allowed because `nervusdb-v2-storage` is a dependency of `nervusdb-v2-query`
mod txn_engine_impl {
    use super::*;
    use nervusdb_v2_storage::engine::WriteTxn as EngineWriteTxn;

    impl<'a> WriteableGraph for EngineWriteTxn<'a> {
        fn create_node(
            &mut self,
            external_id: ExternalId,
            label_id: LabelId,
        ) -> Result<InternalNodeId> {
            EngineWriteTxn::create_node(self, external_id, label_id)
                .map_err(|e| Error::Other(e.to_string()))
        }

        fn create_edge(
            &mut self,
            src: InternalNodeId,
            rel: RelTypeId,
            dst: InternalNodeId,
        ) -> Result<()> {
            EngineWriteTxn::create_edge(self, src, rel, dst);
            Ok(())
        }

        fn set_node_property(
            &mut self,
            node: InternalNodeId,
            key: String,
            value: PropertyValue,
        ) -> Result<()> {
            EngineWriteTxn::set_node_property(self, node, key, value);
            Ok(())
        }

        fn set_edge_property(
            &mut self,
            src: InternalNodeId,
            rel: RelTypeId,
            dst: InternalNodeId,
            key: String,
            value: PropertyValue,
        ) -> Result<()> {
            EngineWriteTxn::set_edge_property(self, src, rel, dst, key, value);
            Ok(())
        }

        fn tombstone_node(&mut self, node: InternalNodeId) -> Result<()> {
            EngineWriteTxn::tombstone_node(self, node);
            Ok(())
        }

        fn tombstone_edge(
            &mut self,
            src: InternalNodeId,
            rel: RelTypeId,
            dst: InternalNodeId,
        ) -> Result<()> {
            EngineWriteTxn::tombstone_edge(self, src, rel, dst);
            Ok(())
        }
    }
}

fn execute_create(
    txn: &mut dyn WriteableGraph,
    pattern: &Pattern,
    params: &crate::query_api::Params,
) -> Result<u32> {
    let mut created_count = 0u32;

    // First pass: collect all node patterns and relationship patterns with their indices
    let mut node_patterns: Vec<(usize, &crate::ast::NodePattern)> = Vec::new();
    let mut rel_patterns: Vec<(usize, &crate::ast::RelationshipPattern)> = Vec::new();

    for (idx, element) in pattern.elements.iter().enumerate() {
        match element {
            PathElement::Node(n) => node_patterns.push((idx, n)),
            PathElement::Relationship(r) => rel_patterns.push((idx, r)),
        }
    }

    // Create all nodes first
    let mut node_ids: Vec<(usize, InternalNodeId)> = Vec::new();
    for (idx, node_pat) in &node_patterns {
        let external_id = ExternalId::from(
            created_count as u64 + chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64,
        );
        let node_id = txn.create_node(external_id, 0)?;
        created_count += 1;

        // Set properties if any
        if let Some(props) = &node_pat.properties {
            for prop in &props.properties {
                let value = evaluate_property_value(&prop.value, params)?;
                txn.set_node_property(node_id, prop.key.clone(), value)?;
            }
        }

        node_ids.push((*idx, node_id));
    }

    // Now create all relationships
    for (idx, rel_pat) in &rel_patterns {
        let rel_type: RelTypeId = rel_pat
            .types
            .first()
            .and_then(|t| t.parse().ok())
            .ok_or_else(|| Error::Other("relationship type must be numeric in v2 M3".into()))?;

        // For single-hop patterns, find nodes at idx-1 (src) and idx+1 (dst)
        let src_id = node_ids
            .iter()
            .find(|(n_idx, _)| *n_idx == *idx - 1)
            .map(|(_, id)| *id)
            .ok_or_else(|| Error::Other("CREATE relationship requires preceding node".into()))?;

        let dst_id = node_ids
            .iter()
            .find(|(n_idx, _)| *n_idx == *idx + 1)
            .map(|(_, id)| *id)
            .ok_or_else(|| Error::Other("CREATE relationship requires following node".into()))?;

        // Create the edge
        txn.create_edge(src_id, rel_type, dst_id)?;
        created_count += 1;

        // Set properties if any
        if let Some(props) = &rel_pat.properties {
            for prop in &props.properties {
                let value = evaluate_property_value(&prop.value, params)?;
                txn.set_edge_property(src_id, rel_type, dst_id, prop.key.clone(), value)?;
            }
        }
    }

    Ok(created_count)
}

fn execute_delete<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    detach: bool,
    expressions: &[Expression],
    params: &crate::query_api::Params,
) -> Result<u32> {
    const MAX_DELETE_TARGETS: usize = 100_000;

    let mut deleted_count = 0u32;
    let mut nodes_to_delete: Vec<InternalNodeId> = Vec::new();
    let mut seen_nodes: std::collections::HashSet<InternalNodeId> =
        std::collections::HashSet::new();

    // Stream input rows and collect delete targets without materializing all rows.
    for row in execute_plan(snapshot, input, params) {
        let row = row?;
        for expr in expressions {
            match expr {
                Expression::Variable(var_name) => {
                    if let Some(node_id) = row.get_node(var_name)
                        && seen_nodes.insert(node_id)
                    {
                        nodes_to_delete.push(node_id);
                        if nodes_to_delete.len() > MAX_DELETE_TARGETS {
                            return Err(Error::Other(format!(
                                "DELETE target limit exceeded ({MAX_DELETE_TARGETS}); batch your deletes"
                            )));
                        }
                    }
                    // TODO: Support deleting edges by variable once we expose edge bindings in Row API.
                }
                Expression::PropertyAccess(_pa) => {
                    return Err(Error::NotImplemented(
                        "DELETE property not implemented in v2 M3",
                    ));
                }
                _ => {
                    return Err(Error::Other(
                        "DELETE only supports variable expressions in v2 M3".to_string(),
                    ));
                }
            }
        }
    }

    // If detach=true, delete all edges connected to nodes being deleted
    if detach {
        for &node_id in &nodes_to_delete {
            // Get all edges connected to this node and delete them
            for edge in snapshot.neighbors(node_id, None) {
                txn.tombstone_edge(edge.src, edge.rel, edge.dst)?;
                deleted_count += 1;
            }
        }
    }

    // Delete the nodes
    for node_id in nodes_to_delete {
        txn.tombstone_node(node_id)?;
        deleted_count += 1;
    }

    Ok(deleted_count)
}

fn evaluate_property_value(
    expr: &Expression,
    params: &crate::query_api::Params,
) -> Result<PropertyValue> {
    match expr {
        Expression::Literal(lit) => match lit {
            Literal::Null => Ok(PropertyValue::Null),
            Literal::Boolean(b) => Ok(PropertyValue::Bool(*b)),
            Literal::Number(n) => {
                if n.fract() == 0.0 {
                    Ok(PropertyValue::Int(*n as i64))
                } else {
                    Ok(PropertyValue::Float(*n))
                }
            }
            Literal::String(s) => Ok(PropertyValue::String(s.clone())),
        },
        Expression::Parameter(name) => {
            if let Some(value) = params.get(name) {
                convert_executor_value_to_property(value)
            } else {
                Ok(PropertyValue::Null)
            }
        }
        _ => Err(Error::NotImplemented(
            "complex expressions in property values not supported in v2 M3",
        )),
    }
}

fn convert_executor_value_to_property(value: &Value) -> Result<PropertyValue> {
    match value {
        Value::Null => Ok(PropertyValue::Null),
        Value::Bool(b) => Ok(PropertyValue::Bool(*b)),
        Value::Int(i) => Ok(PropertyValue::Int(*i)),
        Value::String(s) => Ok(PropertyValue::String(s.clone())),
        Value::Float(_) => Err(Error::NotImplemented(
            "float values in properties not supported in v2 M3",
        )),
        Value::NodeId(_) | Value::ExternalId(_) | Value::EdgeKey(_) => Err(Error::NotImplemented(
            "node/edge values in properties not supported in v2 M3",
        )),
    }
}

struct MatchOutIter<'a, S: GraphSnapshot + 'a> {
    snapshot: &'a S,
    src_alias: &'a str,
    rel: Option<RelTypeId>,
    edge_alias: Option<&'a str>,
    dst_alias: &'a str,
    node_iter: Box<dyn Iterator<Item = InternalNodeId> + 'a>,
    cur_src: Option<InternalNodeId>,
    cur_edges: Option<S::Neighbors<'a>>,
}

impl<'a, S: GraphSnapshot + 'a> MatchOutIter<'a, S> {
    fn new(
        snapshot: &'a S,
        src_alias: &'a str,
        rel: Option<RelTypeId>,
        edge_alias: Option<&'a str>,
        dst_alias: &'a str,
    ) -> Self {
        Self {
            snapshot,
            src_alias,
            rel,
            edge_alias,
            dst_alias,
            node_iter: snapshot.nodes(),
            cur_src: None,
            cur_edges: None,
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
                self.cur_edges = Some(self.snapshot.neighbors(src, self.rel));
            }

            let edges = self.cur_edges.as_mut().expect("cur_edges must exist");

            if let Some(edge) = edges.next() {
                let mut row = Row::default().with(self.src_alias, Value::NodeId(edge.src));
                if let Some(edge_alias) = self.edge_alias {
                    row = row.with(edge_alias, Value::EdgeKey(edge));
                }
                row = row.with(self.dst_alias, Value::NodeId(edge.dst));

                // Always return full row - projection happens in Plan::Project
                return Some(Ok(row));
            }

            self.cur_edges = None;
            self.cur_src = None;
        }
    }
}

/// Variable-length path iterator using DFS
const DEFAULT_MAX_VAR_LEN_HOPS: u32 = 5;

#[allow(clippy::too_many_arguments)]
struct MatchOutVarLenIter<'a, S: GraphSnapshot + 'a> {
    snapshot: &'a S,
    src_alias: &'a str,
    rel: Option<RelTypeId>,
    edge_alias: Option<&'a str>,
    dst_alias: &'a str,
    min_hops: u32,
    max_hops: Option<u32>,
    limit: Option<u32>,
    node_iter: Box<dyn Iterator<Item = InternalNodeId> + 'a>,
    // DFS state: (start_node, current_node, current_depth)
    stack: Vec<(InternalNodeId, InternalNodeId, u32)>,
    emitted: u32,
}

impl<'a, S: GraphSnapshot + 'a> MatchOutVarLenIter<'a, S> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        snapshot: &'a S,
        src_alias: &'a str,
        rel: Option<RelTypeId>,
        edge_alias: Option<&'a str>,
        dst_alias: &'a str,
        min_hops: u32,
        max_hops: Option<u32>,
        limit: Option<u32>,
    ) -> Self {
        Self {
            snapshot,
            src_alias,
            rel,
            edge_alias,
            dst_alias,
            min_hops,
            max_hops,
            limit,
            node_iter: snapshot.nodes(),
            stack: Vec::new(),
            emitted: 0,
        }
    }

    fn next_start_node(&mut self) -> Option<InternalNodeId> {
        for src in self.node_iter.by_ref() {
            if self.snapshot.is_tombstoned_node(src) {
                continue;
            }
            return Some(src);
        }
        None
    }

    /// Start DFS from a node
    fn start_dfs(&mut self, start_node: InternalNodeId) {
        self.stack.push((start_node, start_node, 0));
    }
}

impl<'a, S: GraphSnapshot + 'a> Iterator for MatchOutVarLenIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        let max_hops = Some(self.max_hops.unwrap_or(DEFAULT_MAX_VAR_LEN_HOPS));

        // Check limit
        if let Some(limit) = self.limit
            && self.emitted >= limit
        {
            return None;
        }

        loop {
            // If stack is empty, get next start node
            if self.stack.is_empty() {
                let start_node = self.next_start_node()?;
                self.start_dfs(start_node);
            }

            // Pop next path to explore
            let (start_node, current_node, depth) = match self.stack.pop() {
                Some(state) => state,
                None => continue,
            };

            // Get neighbors
            let neighbors: Vec<_> = self.snapshot.neighbors(current_node, self.rel).collect();

            // For each neighbor, check if we should emit this path
            for edge in neighbors {
                let next_node = edge.dst;
                let next_depth = depth + 1;

                // Check max hops constraint
                if let Some(max) = max_hops
                    && next_depth > max
                {
                    continue;
                }

                // Check min hops and emit
                if next_depth >= self.min_hops {
                    let mut row = Row::default().with(self.src_alias, Value::NodeId(start_node));
                    if let Some(edge_alias) = self.edge_alias {
                        row = row.with(edge_alias, Value::EdgeKey(edge));
                    }
                    row = row.with(self.dst_alias, Value::NodeId(next_node));

                    self.emitted += 1;
                    return Some(Ok(row));
                }

                // Continue DFS
                self.stack.push((start_node, next_node, next_depth));
            }
        }
    }
}

/// Simple aggregation executor that collects all input, groups, and computes aggregates
fn execute_aggregate<'a>(
    input: Box<dyn Iterator<Item = Result<Row>> + 'a>,
    group_by: Vec<String>,
    aggregates: Vec<(AggregateFunction, String)>,
) -> Box<dyn Iterator<Item = Result<Row>> + 'a> {
    // Collect all rows and group them
    let mut groups: std::collections::HashMap<Vec<Value>, Vec<Row>> =
        std::collections::HashMap::new();

    for item in input {
        let row = match item {
            Ok(r) => r,
            Err(e) => return Box::new(std::iter::once(Err(e))),
        };

        let key: Vec<Value> = group_by
            .iter()
            .filter_map(|var| {
                row.cols
                    .iter()
                    .find(|(k, _)| k == var)
                    .map(|(_, v)| v.clone())
            })
            .collect();

        groups.entry(key).or_default().push(row);
    }

    // Convert to result rows
    let results: Vec<Result<Row>> = groups
        .into_iter()
        .map(|(key, rows)| {
            // Build group key row
            let mut result = Row::default();
            for (i, var) in group_by.iter().enumerate() {
                if i < key.len() {
                    result = result.with(var, key[i].clone());
                }
            }

            // Compute aggregates
            for (func, alias) in &aggregates {
                let value = match func {
                    AggregateFunction::Count(None) => {
                        // COUNT(*)
                        Value::Float(rows.len() as f64)
                    }
                    AggregateFunction::Count(Some(expr)) => {
                        // COUNT(expr) - count non-null values
                        let count = rows
                            .iter()
                            .filter(|r| !matches!(evaluate_expression(expr, r), Value::Null))
                            .count();
                        Value::Float(count as f64)
                    }
                    AggregateFunction::Sum(expr) => {
                        let sum: f64 = rows
                            .iter()
                            .filter_map(|r| {
                                if let Value::Float(f) = evaluate_expression(expr, r) {
                                    Some(f)
                                } else {
                                    None
                                }
                            })
                            .sum();
                        Value::Float(sum)
                    }
                    AggregateFunction::Avg(expr) => {
                        let values: Vec<f64> = rows
                            .iter()
                            .filter_map(|r| {
                                if let Value::Float(f) = evaluate_expression(expr, r) {
                                    Some(f)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if values.is_empty() {
                            Value::Null
                        } else {
                            Value::Float(values.iter().sum::<f64>() / values.len() as f64)
                        }
                    }
                };
                result = result.with(alias, value);
            }

            Ok(result)
        })
        .collect();

    Box::new(results.into_iter())
}

/// Simple expression evaluator for aggregate functions
fn evaluate_expression(expr: &Expression, row: &Row) -> Value {
    match expr {
        Expression::Variable(name) => row
            .cols
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or(Value::Null),
        Expression::PropertyAccess(prop) => {
            // For now, just return the node value (property access not fully implemented)
            row.cols
                .iter()
                .find(|(k, _)| k == &prop.variable)
                .map(|(_, v)| v.clone())
                .unwrap_or(Value::Null)
        }
        Expression::Literal(Literal::Number(n)) => Value::Float(*n),
        Expression::Literal(Literal::String(s)) => Value::String(s.clone()),
        _ => Value::Null,
    }
}

pub fn parse_u32_identifier(name: &str) -> Result<u32> {
    name.parse::<u32>()
        .map_err(|_| Error::NotImplemented("non-numeric label/rel identifiers in M3"))
}

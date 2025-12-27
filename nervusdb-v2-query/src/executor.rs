use crate::ast::{Expression, Literal, PathElement, Pattern};
use crate::error::{Error, Result};
pub use nervusdb_v2_api::LabelId;
use nervusdb_v2_api::{EdgeKey, ExternalId, GraphSnapshot, InternalNodeId, RelTypeId};

#[derive(Debug, Clone, PartialEq)]
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

// Implement WriteableGraph for nervusdb-v2 WriteTxn
mod txn_adapter {
    use super::*;
    use nervusdb_v2::WriteTxn as DbWriteTxn;

    impl<'a> WriteableGraph for DbWriteTxn<'a> {
        fn create_node(
            &mut self,
            external_id: ExternalId,
            _label_id: LabelId,
        ) -> Result<InternalNodeId> {
            // MVP: ignore label_id, just create node
            DbWriteTxn::create_node(self, external_id, 0).map_err(|e| Error::Other(e.to_string()))
        }

        fn create_edge(
            &mut self,
            src: InternalNodeId,
            rel: RelTypeId,
            dst: InternalNodeId,
        ) -> Result<()> {
            DbWriteTxn::create_edge(self, src, rel, dst);
            Ok(())
        }

        fn set_node_property(
            &mut self,
            node: InternalNodeId,
            key: String,
            value: PropertyValue,
        ) -> Result<()> {
            let value = convert_to_storage(value);
            let _ = DbWriteTxn::set_node_property(self, node, key, value);
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
            let value = convert_to_storage(value);
            let _ = DbWriteTxn::set_edge_property(self, src, rel, dst, key, value);
            Ok(())
        }

        fn tombstone_node(&mut self, node: InternalNodeId) -> Result<()> {
            DbWriteTxn::tombstone_node(self, node);
            Ok(())
        }

        fn tombstone_edge(
            &mut self,
            src: InternalNodeId,
            rel: RelTypeId,
            dst: InternalNodeId,
        ) -> Result<()> {
            DbWriteTxn::tombstone_edge(self, src, rel, dst);
            Ok(())
        }
    }

    fn convert_to_storage(v: PropertyValue) -> nervusdb_v2::PropertyValue {
        match v {
            PropertyValue::Null => nervusdb_v2::PropertyValue::Null,
            PropertyValue::Bool(b) => nervusdb_v2::PropertyValue::Bool(b),
            PropertyValue::Int(i) => nervusdb_v2::PropertyValue::Int(i),
            PropertyValue::Float(f) => nervusdb_v2::PropertyValue::Float(f),
            PropertyValue::String(s) => nervusdb_v2::PropertyValue::String(s),
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
    let mut deleted_count = 0u32;
    let mut nodes_to_delete: Vec<InternalNodeId> = Vec::new();
    let mut edges_to_delete: Vec<EdgeKey> = Vec::new();

    // Execute the input plan (MATCH) to get rows with variable bindings
    let rows: Vec<_> = execute_plan(snapshot, input, params)
        .filter_map(|r| r.ok())
        .collect();

    // Collect nodes/edges to delete from the rows
    for row in &rows {
        for expr in expressions {
            match expr {
                Expression::Variable(var_name) => {
                    // Try to get node ID from row
                    if let Some(node_id) = row.get_node(var_name) {
                        if !nodes_to_delete.contains(&node_id) {
                            nodes_to_delete.push(node_id);
                        }
                    }
                    // Note: edge variables would be handled similarly if we had get_edge method
                }
                Expression::PropertyAccess(_pa) => {
                    // DELETE n.property - set to null (not implemented)
                    return Err(Error::NotImplemented(
                        "DELETE property not implemented in v2 M3".into(),
                    ));
                }
                _ => {
                    return Err(Error::Other(
                        "DELETE only supports variable expressions in v2 M3".into(),
                    ));
                }
            }
        }
    }

    // If detach=true, delete all edges connected to nodes being deleted
    if detach {
        for &node_id in &nodes_to_delete {
            // Get all edges connected to this node and delete them
            let neighbors: Vec<_> = snapshot.neighbors(node_id, None).collect();
            for edge in neighbors {
                if !edges_to_delete.contains(&edge) {
                    edges_to_delete.push(edge);
                }
            }
        }

        // Delete the edges
        for edge in &edges_to_delete {
            txn.tombstone_edge(edge.src, edge.rel, edge.dst)?;
            deleted_count += 1;
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
            "complex expressions in property values not supported in v2 M3".into(),
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
            "float values in properties not supported in v2 M3".into(),
        )),
        Value::NodeId(_) | Value::ExternalId(_) | Value::EdgeKey(_) => Err(Error::NotImplemented(
            "node/edge values in properties not supported in v2 M3".into(),
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

pub fn parse_u32_identifier(name: &str) -> Result<u32> {
    name.parse::<u32>()
        .map_err(|_| Error::NotImplemented("non-numeric label/rel identifiers in M3"))
}

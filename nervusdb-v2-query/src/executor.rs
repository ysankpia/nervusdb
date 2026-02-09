use crate::ast::{
    AggregateFunction, Direction, Expression, Literal, PathElement, Pattern, RelationshipDirection,
};
use crate::error::{Error, Result};
use crate::evaluator::evaluate_expression_value;
pub use nervusdb_v2_api::LabelId;
use nervusdb_v2_api::{EdgeKey, ExternalId, GraphSnapshot, InternalNodeId, RelTypeId};
use serde::ser::{SerializeMap, SerializeSeq};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

pub trait Procedure: Send + Sync {
    fn execute(&self, snapshot: &dyn ErasedSnapshot, args: Vec<Value>) -> Result<Vec<Row>>;
}

pub trait ErasedSnapshot {
    fn neighbors_erased(
        &self,
        src: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Box<dyn Iterator<Item = EdgeKey> + '_>;
    fn incoming_neighbors_erased(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Box<dyn Iterator<Item = EdgeKey> + '_>;
    fn node_property_erased(
        &self,
        iid: InternalNodeId,
        key: &str,
    ) -> Option<nervusdb_v2_api::PropertyValue>;
    fn resolve_label_name_erased(&self, id: LabelId) -> Option<String>;
    fn resolve_rel_type_name_erased(&self, id: RelTypeId) -> Option<String>;
    fn resolve_node_labels_erased(&self, iid: InternalNodeId) -> Option<Vec<LabelId>>;
    fn node_properties_erased(
        &self,
        iid: InternalNodeId,
    ) -> Option<std::collections::BTreeMap<String, nervusdb_v2_api::PropertyValue>>;
    fn edge_properties_erased(
        &self,
        key: EdgeKey,
    ) -> Option<std::collections::BTreeMap<String, nervusdb_v2_api::PropertyValue>>;
}

impl<S: GraphSnapshot> ErasedSnapshot for S {
    fn neighbors_erased(
        &self,
        src: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Box<dyn Iterator<Item = EdgeKey> + '_> {
        Box::new(self.neighbors(src, rel))
    }
    fn incoming_neighbors_erased(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Box<dyn Iterator<Item = EdgeKey> + '_> {
        Box::new(self.incoming_neighbors(dst, rel))
    }
    fn node_property_erased(
        &self,
        iid: InternalNodeId,
        key: &str,
    ) -> Option<nervusdb_v2_api::PropertyValue> {
        self.node_property(iid, key)
    }
    fn resolve_label_name_erased(&self, id: LabelId) -> Option<String> {
        self.resolve_label_name(id)
    }
    fn resolve_rel_type_name_erased(&self, id: RelTypeId) -> Option<String> {
        self.resolve_rel_type_name(id)
    }
    fn resolve_node_labels_erased(&self, iid: InternalNodeId) -> Option<Vec<LabelId>> {
        self.resolve_node_labels(iid)
    }
    fn node_properties_erased(
        &self,
        iid: InternalNodeId,
    ) -> Option<std::collections::BTreeMap<String, nervusdb_v2_api::PropertyValue>> {
        self.node_properties(iid)
    }
    fn edge_properties_erased(
        &self,
        key: EdgeKey,
    ) -> Option<std::collections::BTreeMap<String, nervusdb_v2_api::PropertyValue>> {
        self.edge_properties(key)
    }
}

pub struct ProcedureRegistry {
    handlers: HashMap<String, Arc<dyn Procedure>>,
}

impl Default for ProcedureRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcedureRegistry {
    pub fn new() -> Self {
        let mut handlers: HashMap<String, Arc<dyn Procedure>> = HashMap::new();
        // Register built-ins
        handlers.insert("db.info".to_string(), Arc::new(DbInfoProcedure));
        handlers.insert("math.add".to_string(), Arc::new(MathAddProcedure));
        Self { handlers }
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Procedure>> {
        self.handlers.get(name).cloned()
    }
}

pub static GLOBAL_PROCEDURE_REGISTRY: OnceLock<ProcedureRegistry> = OnceLock::new();

pub fn get_procedure_registry() -> &'static ProcedureRegistry {
    GLOBAL_PROCEDURE_REGISTRY.get_or_init(ProcedureRegistry::new)
}

struct DbInfoProcedure;
impl Procedure for DbInfoProcedure {
    fn execute(&self, _snapshot: &dyn ErasedSnapshot, _args: Vec<Value>) -> Result<Vec<Row>> {
        Ok(vec![Row::new(vec![(
            "version".to_string(),
            Value::String("2.0.0".to_string()),
        )])])
    }
}

struct MathAddProcedure;
impl Procedure for MathAddProcedure {
    fn execute(&self, _snapshot: &dyn ErasedSnapshot, args: Vec<Value>) -> Result<Vec<Row>> {
        if args.len() != 2 {
            return Err(Error::Other("math.add requires 2 arguments".to_string()));
        }
        let a = match &args[0] {
            Value::Int(i) => *i as f64,
            Value::Float(f) => *f,
            _ => {
                return Err(Error::Other(
                    "math.add requires numeric arguments".to_string(),
                ));
            }
        };
        let b = match &args[1] {
            Value::Int(i) => *i as f64,
            Value::Float(f) => *f,
            _ => {
                return Err(Error::Other(
                    "math.add requires numeric arguments".to_string(),
                ));
            }
        };
        Ok(vec![Row::new(vec![(
            "result".to_string(),
            Value::Float(a + b),
        )])])
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Hash)]
pub struct NodeValue {
    pub id: InternalNodeId,
    pub labels: Vec<String>,
    pub properties: std::collections::BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Hash)]
pub struct RelationshipValue {
    pub key: EdgeKey,
    pub rel_type: String,
    pub properties: std::collections::BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Hash)]
pub struct PathValue {
    pub nodes: Vec<InternalNodeId>,
    pub edges: Vec<EdgeKey>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Hash)]
pub struct ReifiedPathValue {
    pub nodes: Vec<NodeValue>,
    pub relationships: Vec<RelationshipValue>,
}

impl serde::Serialize for NodeValue {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("labels", &self.labels)?;
        map.serialize_entry("properties", &self.properties)?;
        map.end()
    }
}

impl serde::Serialize for RelationshipValue {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(4))?;
        map.serialize_entry("src", &self.key.src)?;
        map.serialize_entry("rel", &self.key.rel)?;
        map.serialize_entry("dst", &self.key.dst)?;
        map.serialize_entry("properties", &self.properties)?;
        map.end()
    }
}

impl serde::Serialize for PathValue {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("nodes", &self.nodes)?;
        map.serialize_entry("edges", &self.edges)?;
        map.end()
    }
}

impl serde::Serialize for ReifiedPathValue {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("nodes", &self.nodes)?;
        map.serialize_entry("relationships", &self.relationships)?;
        map.end()
    }
}

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
    List(Vec<Value>),
    DateTime(i64),
    Blob(Vec<u8>),
    Map(std::collections::BTreeMap<String, Value>),
    Path(PathValue),
    Node(NodeValue),
    Relationship(RelationshipValue),
    ReifiedPath(ReifiedPathValue),
}

impl serde::Serialize for Value {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Value::NodeId(iid) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "node_id")?;
                map.serialize_entry("id", iid)?;
                map.end()
            }
            Value::ExternalId(id) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "external_id")?;
                map.serialize_entry("id", id)?;
                map.end()
            }
            Value::EdgeKey(e) => {
                let mut map = serializer.serialize_map(Some(4))?;
                map.serialize_entry("type", "edge_key")?;
                map.serialize_entry("src", &e.src)?;
                map.serialize_entry("rel", &e.rel)?;
                map.serialize_entry("dst", &e.dst)?;
                map.end()
            }
            Value::Int(i) => serializer.serialize_i64(*i),
            Value::Float(f) => serializer.serialize_f64(*f),
            Value::String(s) => serializer.serialize_str(s),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::Null => serializer.serialize_none(),
            Value::List(list) => {
                let mut seq = serializer.serialize_seq(Some(list.len()))?;
                for item in list {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Value::DateTime(i) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "datetime")?;
                map.serialize_entry("value", i)?;
                map.end()
            }
            Value::Blob(_) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "blob")?;
                map.serialize_entry("data", "<binary>")?;
                map.end()
            }
            Value::Map(map) => {
                let mut ser = serializer.serialize_map(Some(map.len()))?;
                for (k, v) in map {
                    ser.serialize_entry(k, v)?;
                }
                ser.end()
            }
            Value::Path(p) => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "path")?;
                map.serialize_entry("nodes", &p.nodes)?;
                map.serialize_entry("edges", &p.edges)?;
                map.end()
            }
            Value::Node(n) => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "node")?;
                map.serialize_entry("id", &n.id)?;
                map.serialize_entry("labels", &n.labels)?;
                map.end()
            }
            Value::Relationship(r) => {
                let mut map = serializer.serialize_map(Some(4))?;
                map.serialize_entry("type", "relationship")?;
                map.serialize_entry("src", &r.key.src)?;
                map.serialize_entry("rel", &r.key.rel)?;
                map.serialize_entry("dst", &r.key.dst)?;
                map.end()
            }
            Value::ReifiedPath(p) => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "reified_path")?;
                map.serialize_entry("nodes", &p.nodes)?;
                map.serialize_entry("relationships", &p.relationships)?;
                map.end()
            }
        }
    }
}

impl Value {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn reify(&self, snapshot: &dyn ErasedSnapshot) -> Result<Value> {
        match self {
            Value::NodeId(id) => {
                let mut labels = Vec::new();
                if let Some(label_ids) = snapshot.resolve_node_labels_erased(*id) {
                    for lid in label_ids {
                        if let Some(name) = snapshot.resolve_label_name_erased(lid) {
                            labels.push(name);
                        }
                    }
                }

                let mut properties = std::collections::BTreeMap::new();
                if let Some(props) = snapshot.node_properties_erased(*id) {
                    for (k, v) in props {
                        properties.insert(k, convert_api_property_to_value(&v));
                    }
                }

                Ok(Value::Node(NodeValue {
                    id: *id,
                    labels,
                    properties,
                }))
            }
            Value::EdgeKey(key) => {
                let rel_type = snapshot
                    .resolve_rel_type_name_erased(key.rel)
                    .unwrap_or_else(|| format!("<{}>", key.rel));

                let mut properties = std::collections::BTreeMap::new();
                if let Some(props) = snapshot.edge_properties_erased(*key) {
                    for (k, v) in props {
                        properties.insert(k, convert_api_property_to_value(&v));
                    }
                }

                Ok(Value::Relationship(RelationshipValue {
                    key: *key,
                    rel_type,
                    properties,
                }))
            }
            Value::Path(p) => {
                let mut nodes = Vec::new();
                for nid in &p.nodes {
                    if let Value::Node(n) = Value::NodeId(*nid).reify(snapshot)? {
                        nodes.push(n);
                    }
                }
                let mut relationships = Vec::new();
                for ekey in &p.edges {
                    if let Value::Relationship(r) = Value::EdgeKey(*ekey).reify(snapshot)? {
                        relationships.push(r);
                    }
                }
                Ok(Value::ReifiedPath(ReifiedPathValue {
                    nodes,
                    relationships,
                }))
            }
            Value::List(l) => {
                let mut out = Vec::new();
                for v in l {
                    out.push(v.reify(snapshot)?);
                }
                Ok(Value::List(out))
            }
            Value::Map(m) => {
                let mut out = std::collections::BTreeMap::new();
                for (k, v) in m {
                    out.insert(k.clone(), v.reify(snapshot)?);
                }
                Ok(Value::Map(out))
            }
            _ => Ok(self.clone()),
        }
    }
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
            Value::List(l) => l.hash(state),
            Value::DateTime(i) => i.hash(state),
            Value::Blob(b) => b.hash(state),
            Value::Map(m) => m.hash(state),
            Value::Path(p) => p.hash(state),
            Value::Node(n) => n.hash(state),
            Value::Relationship(r) => r.hash(state),
            Value::ReifiedPath(p) => p.hash(state),
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
    pub fn new(cols: Vec<(String, Value)>) -> Self {
        Self { cols }
    }

    pub fn reify(&self, snapshot: &dyn ErasedSnapshot) -> Result<Row> {
        let mut cols = Vec::with_capacity(self.cols.len());
        for (k, v) in &self.cols {
            cols.push((k.clone(), v.reify(snapshot)?));
        }
        Ok(Row { cols })
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.cols.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

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

    pub fn join(&self, other: &Row) -> Row {
        let mut out = self.clone();
        out.cols.extend(other.cols.clone());
        out
    }

    pub fn join_path(
        &mut self,
        alias: &str,
        src: InternalNodeId,
        edge: EdgeKey,
        dst: InternalNodeId,
    ) {
        let path = match self.get(alias) {
            Some(Value::Path(p)) => {
                let mut p = p.clone();
                p.edges.push(edge);
                p.nodes.push(dst);
                Value::Path(p)
            }
            _ => {
                // Initialize path
                Value::Path(PathValue {
                    nodes: vec![src, dst],
                    edges: vec![edge],
                })
            }
        };
        self.with_mut(alias, path);
    }

    fn with_mut(&mut self, name: &str, value: Value) {
        if let Some((_, v)) = self.cols.iter_mut().find(|(k, _)| k == name) {
            *v = value;
        } else {
            self.cols.push((name.to_string(), value));
        }
    }
}

#[derive(Debug, Clone)]
pub enum Plan {
    /// `RETURN 1`
    ReturnOne,
    /// `MATCH (n) RETURN ...`
    NodeScan {
        alias: String,
        label: Option<String>,
    },
    /// `MATCH (a)-[:rel]->(b) RETURN ...`
    MatchOut {
        input: Option<Box<Plan>>,
        src_alias: String,
        rels: Vec<String>,
        edge_alias: Option<String>,
        dst_alias: String,
        limit: Option<u32>,
        // Note: project is kept for backward compatibility but projection
        // should happen after filtering (see Plan::Project)
        project: Vec<String>,
        project_external: bool,
        optional: bool,
        path_alias: Option<String>,
    },
    /// `MATCH (a)-[:rel*min..max]->(b) RETURN ...` (variable length)
    MatchOutVarLen {
        input: Option<Box<Plan>>,
        src_alias: String,
        rels: Vec<String>,
        edge_alias: Option<String>,
        dst_alias: String,
        direction: RelationshipDirection,
        min_hops: u32,
        max_hops: Option<u32>,
        limit: Option<u32>,
        project: Vec<String>,
        project_external: bool,
        optional: bool,
        path_alias: Option<String>,
    },
    MatchIn {
        input: Option<Box<Plan>>,
        src_alias: String,
        rels: Vec<String>,
        edge_alias: Option<String>,
        dst_alias: String,
        limit: Option<u32>,
        optional: bool,
        path_alias: Option<String>,
    },
    MatchUndirected {
        input: Option<Box<Plan>>,
        src_alias: String,
        rels: Vec<String>,
        edge_alias: Option<String>,
        dst_alias: String,
        limit: Option<u32>,
        optional: bool,
        path_alias: Option<String>,
    },
    /// `MATCH ... WHERE ... RETURN ...` (with filter)
    Filter {
        input: Box<Plan>,
        predicate: Expression,
    },
    /// Project expressions to new variables
    Project {
        input: Box<Plan>,
        projections: Vec<(String, Expression)>, // (Result/Alias Name, Expression to Eval)
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
        items: Vec<(Expression, Direction)>, // (Expression to sort by, ASC|DESC)
    },
    /// `SKIP` - skip first n rows
    Skip {
        input: Box<Plan>,
        skip: u32,
    },
    /// `LIMIT` - limit result count
    Limit {
        input: Box<Plan>,
        limit: u32,
    },
    /// `RETURN DISTINCT` - deduplicate results
    Distinct {
        input: Box<Plan>,
    },
    /// `UNWIND` - expand a list into multiple rows
    Unwind {
        input: Box<Plan>,
        expression: Expression,
        alias: String,
    },
    /// `UNION` / `UNION ALL` - combine results from two queries
    Union {
        left: Box<Plan>,
        right: Box<Plan>,
        all: bool, // true = UNION ALL (keep duplicates), false = UNION (distinct)
    },

    /// `DELETE` - delete nodes/edges (with input plan for variable resolution)
    Delete {
        input: Box<Plan>,
        detach: bool,
        expressions: Vec<Expression>,
    },
    /// `SetProperty` - update properties on nodes
    SetProperty {
        input: Box<Plan>,
        items: Vec<(String, String, Expression)>, // (variable, key, value_expression)
    },
    /// `REMOVE n.prop` - remove properties from nodes/edges
    RemoveProperty {
        input: Box<Plan>,
        items: Vec<(String, String)>, // (variable, key)
    },
    /// `IndexSeek` - optimize scan using index if available, else fallback
    IndexSeek {
        alias: String,
        label: String,
        field: String,
        value_expr: Expression,
        fallback: Box<Plan>,
    },
    /// `CartesianProduct` - multiply two plans (join without shared variables)
    CartesianProduct {
        left: Box<Plan>,
        right: Box<Plan>,
    },
    /// `Apply` - execute subquery for each row (Correlated Subquery)
    Apply {
        input: Box<Plan>,
        subquery: Box<Plan>,
        alias: Option<String>, // Optional alias for subquery result? usually subquery projects...
    },
    /// `CALL namespace.name(args) YIELD x, y`
    ProcedureCall {
        input: Box<Plan>,
        name: Vec<String>,
        args: Vec<Expression>,
        yields: Vec<(String, Option<String>)>, // (field_name, alias)
    },
    Foreach {
        input: Box<Plan>,
        variable: String,
        list: Expression,
        sub_plan: Box<Plan>,
    },
    // Injects specific rows into the pipeline (used for FOREACH context and constructing literal rows)
    Values {
        rows: Vec<Row>,
    },
    Create {
        input: Box<Plan>,
        pattern: Pattern,
    },
}

pub enum PlanIterator<'a, S: GraphSnapshot> {
    ReturnOne(std::iter::Once<Result<Row>>),
    NodeScan(NodeScanIter<'a, S>),
    Filter(FilterIter<'a, S>),
    CartesianProduct(Box<CartesianProductIter<'a, S>>),
    Apply(Box<ApplyIter<'a, S>>),
    ProcedureCall(Box<ProcedureCallIter<'a, S>>),

    Dynamic(Box<dyn Iterator<Item = Result<Row>> + 'a>),
}

impl<'a, S: GraphSnapshot> Iterator for PlanIterator<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            PlanIterator::ReturnOne(iter) => iter.next(),
            PlanIterator::NodeScan(iter) => iter.next(),
            PlanIterator::Filter(iter) => iter.next(),
            PlanIterator::CartesianProduct(iter) => iter.next(),
            PlanIterator::Apply(iter) => iter.next(),
            PlanIterator::ProcedureCall(iter) => iter.next(),

            PlanIterator::Dynamic(iter) => iter.next(),
        }
    }
}

pub struct NodeScanIter<'a, S: GraphSnapshot> {
    snapshot: &'a S,
    node_iter: Box<dyn Iterator<Item = InternalNodeId> + 'a>, // Still boxed internally for now as nodes() returns abstract iter
    alias: String,
    label_id: Option<LabelId>,
}

impl<'a, S: GraphSnapshot> Iterator for NodeScanIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        for iid in self.node_iter.by_ref() {
            if self.snapshot.is_tombstoned_node(iid) {
                continue;
            }
            if let Some(lid) = self.label_id
                && self.snapshot.node_label(iid) != Some(lid)
            {
                continue;
            }
            return Some(Ok(
                Row::default().with(self.alias.clone(), Value::NodeId(iid))
            ));
        }
        None
    }
}

pub struct FilterIter<'a, S: GraphSnapshot> {
    snapshot: &'a S,
    input: Box<PlanIterator<'a, S>>,
    predicate: &'a Expression,
    params: &'a crate::query_api::Params,
}

impl<'a, S: GraphSnapshot> Iterator for FilterIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.input.next() {
                Some(Ok(row)) => {
                    let pass = crate::evaluator::evaluate_expression_bool(
                        self.predicate,
                        &row,
                        self.snapshot,
                        self.params,
                    );
                    if pass {
                        return Some(Ok(row));
                    } else {
                        continue;
                    }
                }
                Some(Err(e)) => return Some(Err(e)),
                None => return None,
            }
        }
    }
}

pub struct CartesianProductIter<'a, S: GraphSnapshot> {
    pub left_iter: Box<PlanIterator<'a, S>>,
    pub right_plan: &'a Plan,
    pub snapshot: &'a S,
    pub params: &'a crate::query_api::Params,

    pub current_left_row: Option<Row>,
    pub current_right_iter: Option<Box<PlanIterator<'a, S>>>,
}

impl<'a, S: GraphSnapshot> Iterator for CartesianProductIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current_left_row.is_none() {
                match self.left_iter.next() {
                    Some(Ok(row)) => {
                        self.current_left_row = Some(row);
                        self.current_right_iter = Some(Box::new(execute_plan(
                            self.snapshot,
                            self.right_plan,
                            self.params,
                        )));
                    }
                    Some(Err(e)) => return Some(Err(e)),
                    None => return None,
                }
            }

            if let Some(right_iter) = &mut self.current_right_iter {
                match right_iter.next() {
                    Some(Ok(right_row)) => {
                        let left_row = self.current_left_row.as_ref().unwrap();
                        return Some(Ok(left_row.join(&right_row)));
                    }
                    Some(Err(e)) => return Some(Err(e)),
                    None => {
                        self.current_left_row = None;
                        self.current_right_iter = None;
                        continue;
                    }
                }
            }
        }
    }
}

pub fn execute_plan<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    plan: &'a Plan,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    match plan {
        Plan::ReturnOne => {
            PlanIterator::ReturnOne(std::iter::once(Ok(Row::default().with("1", Value::Int(1)))))
        }
        Plan::CartesianProduct { left, right } => {
            let left_iter = execute_plan(snapshot, left, params);
            PlanIterator::CartesianProduct(Box::new(CartesianProductIter {
                left_iter: Box::new(left_iter),
                right_plan: right,
                snapshot,
                params,
                current_left_row: None,
                current_right_iter: None,
            }))
        }
        Plan::Apply {
            input,
            subquery,
            alias: _,
        } => {
            let input_iter = execute_plan(snapshot, input, params);
            PlanIterator::Apply(Box::new(ApplyIter {
                input_iter: Box::new(input_iter),
                subquery_plan: subquery,
                snapshot,
                base_params: params,
                current_outer_row: None,
                current_results: Vec::new().into_iter(),
            }))
        }
        Plan::ProcedureCall {
            input,
            name,
            args,
            yields,
        } => {
            let input_iter = execute_plan(snapshot, input, params);
            PlanIterator::ProcedureCall(Box::new(ProcedureCallIter::new(
                Box::new(input_iter),
                name.join("."),
                args,
                yields,
                snapshot,
                params,
            )))
        }
        Plan::Foreach { .. } => {
            // FOREACH should be executed via execute_write
            PlanIterator::Dynamic(Box::new(std::iter::once(Err(crate::error::Error::Other(
                "FOREACH must be executed via execute_write".into(),
            )))))
        }
        Plan::NodeScan { alias, label } => {
            let label_id = if let Some(l) = label {
                match snapshot.resolve_label_id(l) {
                    Some(id) => Some(id),
                    None => return PlanIterator::Dynamic(Box::new(std::iter::empty())),
                }
            } else {
                None
            };

            PlanIterator::NodeScan(NodeScanIter {
                snapshot,
                node_iter: Box::new(snapshot.nodes()), // Logic moved to NodeScanIter
                alias: alias.clone(),
                label_id,
            })
        }
        Plan::MatchOut {
            input,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            limit,
            project: _,
            project_external: _,
            optional,
            path_alias,
        } => {
            let rel_ids = if rels.is_empty() {
                None
            } else {
                let mut ids = Vec::new();
                for r in rels {
                    if let Some(id) = snapshot.resolve_rel_type_id(r) {
                        ids.push(id);
                    }
                }
                if ids.is_empty() {
                    // None of the requested types exist
                    return PlanIterator::Dynamic(Box::new(std::iter::empty()));
                }
                Some(ids)
            };

            if let Some(input_plan) = input {
                let input_iter = execute_plan(snapshot, input_plan, params);
                let expand = ExpandIter {
                    snapshot,
                    input: Box::new(input_iter),
                    src_alias,
                    rels: rel_ids,
                    edge_alias: edge_alias.as_deref(),
                    dst_alias,
                    optional: *optional,
                    cur_row: None,
                    cur_edges: None,
                    yielded_any: false,
                    path_alias: path_alias.as_deref(),
                };
                if let Some(n) = limit {
                    PlanIterator::Dynamic(Box::new(expand.take(*n as usize)))
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
                if let Some(n) = limit {
                    PlanIterator::Dynamic(Box::new(base.take(*n as usize)))
                } else {
                    PlanIterator::Dynamic(Box::new(base))
                }
            }
        }
        Plan::MatchOutVarLen {
            input,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            direction,
            min_hops,
            max_hops,
            limit,
            project: _,
            project_external: _,
            optional,
            path_alias,
        } => {
            let input_iter = input.as_ref().map(|p| execute_plan(snapshot, p, params));

            let rel_ids = if rels.is_empty() {
                None
            } else {
                let mut ids = Vec::new();
                for r in rels {
                    if let Some(id) = snapshot.resolve_rel_type_id(r) {
                        ids.push(id);
                    }
                }
                if ids.is_empty() {
                    return PlanIterator::Dynamic(Box::new(std::iter::empty()));
                }
                Some(ids)
            };

            let base = MatchOutVarLenIter::new(
                snapshot,
                input_iter.map(|i| Box::new(i) as Box<dyn Iterator<Item = Result<Row>>>),
                src_alias,
                rel_ids,
                edge_alias.as_deref(),
                dst_alias,
                direction.clone(),
                *min_hops,
                *max_hops,
                *limit,
                *optional,
                path_alias.as_deref(),
            );
            if let Some(n) = limit {
                PlanIterator::Dynamic(Box::new(base.take(*n as usize)))
            } else {
                PlanIterator::Dynamic(Box::new(base))
            }
        }
        Plan::MatchIn {
            input,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            limit: _,
            optional,
            path_alias,
        } => {
            let rel_ids = if rels.is_empty() {
                None
            } else {
                let mut ids = Vec::new();
                for r in rels {
                    if let Some(id) = snapshot.resolve_rel_type_id(r) {
                        ids.push(id);
                    }
                }
                if ids.is_empty() {
                    return PlanIterator::Dynamic(Box::new(std::iter::empty()));
                }
                Some(ids)
            };

            let input_iter: Box<dyn Iterator<Item = Result<Row>>> = if let Some(input_plan) = input
            {
                Box::new(execute_plan(snapshot, input_plan, params))
            } else {
                Box::new(std::iter::once(Ok(Row::default())))
            };

            let src_alias = src_alias.clone();
            let dst_alias = dst_alias.clone();
            let edge_alias = edge_alias.clone();
            let optional = *optional;

            PlanIterator::Dynamic(Box::new(input_iter.flat_map(move |result| {
                match result {
                    Ok(row) => {
                        let node_val = row.get(&src_alias).cloned();
                        let target_iid = match node_val {
                            Some(Value::NodeId(id)) => id,
                            _ => {
                                return Box::new(std::iter::empty())
                                    as Box<dyn Iterator<Item = Result<Row>>>;
                            }
                        };

                        let rel_ids = rel_ids.clone(); // Capture for closure
                        let candidates: Box<dyn Iterator<Item = EdgeKey>> =
                            if let Some(rids) = &rel_ids {
                                let mut iter: Box<dyn Iterator<Item = EdgeKey>> =
                                    Box::new(std::iter::empty());
                                for rid in rids {
                                    iter = Box::new(iter.chain(
                                        snapshot.incoming_neighbors_erased(target_iid, Some(*rid)),
                                    ));
                                }
                                iter
                            } else {
                                snapshot.incoming_neighbors_erased(target_iid, None)
                            };

                        let dst_alias_binding = dst_alias.clone();
                        let edge_alias_binding = edge_alias.clone();
                        let path_alias = path_alias.clone();
                        let row_for_map = row.clone();

                        let mapped = candidates.map(move |edge| {
                            let mut new_row = row_for_map.clone();
                            new_row =
                                new_row.with(dst_alias_binding.clone(), Value::NodeId(edge.src));
                            if let Some(ea) = &edge_alias_binding {
                                new_row = new_row.with(ea.clone(), Value::EdgeKey(edge));
                            }
                            if let Some(pa) = &path_alias {
                                new_row.join_path(pa, edge.dst, edge, edge.src);
                            }
                            Ok(new_row)
                        });

                        if optional {
                            let results: Vec<_> = mapped.collect();
                            if results.is_empty() {
                                let mut new_row = row.clone();
                                new_row = new_row.with(dst_alias.clone(), Value::Null);
                                if let Some(ea) = &edge_alias {
                                    new_row = new_row.with(ea.clone(), Value::Null);
                                }
                                Box::new(std::iter::once(Ok(new_row)))
                                    as Box<dyn Iterator<Item = Result<Row>>>
                            } else {
                                Box::new(results.into_iter())
                                    as Box<dyn Iterator<Item = Result<Row>>>
                            }
                        } else {
                            Box::new(mapped) as Box<dyn Iterator<Item = Result<Row>>>
                        }
                    }
                    Err(e) => {
                        Box::new(std::iter::once(Err(e))) as Box<dyn Iterator<Item = Result<Row>>>
                    }
                }
            })))
        }
        Plan::MatchUndirected {
            input,
            src_alias,
            rels,
            edge_alias,
            dst_alias,
            limit,
            optional,
            path_alias,
        } => {
            let rel_ids = if rels.is_empty() {
                None
            } else {
                let mut ids = Vec::new();
                for r in rels {
                    if let Some(id) = snapshot.resolve_rel_type_id(r) {
                        ids.push(id);
                    }
                }
                if ids.is_empty() {
                    return PlanIterator::Dynamic(Box::new(std::iter::empty()));
                }
                Some(ids)
            };

            let input_iter: Box<dyn Iterator<Item = Result<Row>>> = if let Some(input_plan) = input
            {
                Box::new(execute_plan(snapshot, input_plan, params))
            } else {
                Box::new(std::iter::once(Ok(Row::default())))
            };

            // Collect successfully to avoid cloning Error
            let rows_result: Result<Vec<Row>> = input_iter.collect();
            let rows = match rows_result {
                Ok(r) => r,
                Err(e) => return PlanIterator::Dynamic(Box::new(std::iter::once(Err(e)))),
            };

            let src_alias = src_alias.clone();
            let dst_alias = dst_alias.clone();
            let edge_alias = edge_alias.clone();
            let _optional = *optional;
            let rel_ids_out = rel_ids.clone();

            // Outgoing Iterator
            let rows_out = rows.clone();
            let src_alias_out = src_alias.clone();
            let dst_alias_out = dst_alias.clone();
            let edge_alias_out = edge_alias.clone();

            let out_iter = rows_out.into_iter().map(Ok).flat_map(move |res| match res {
                Ok(row) => {
                    let node_val = row.get(&src_alias_out).cloned();
                    let src_iid = match node_val {
                        Some(Value::NodeId(id)) => id,
                        _ => {
                            return Box::new(std::iter::empty())
                                as Box<dyn Iterator<Item = Result<Row>>>;
                        }
                    };

                    let neighbors: Box<dyn Iterator<Item = EdgeKey>> = if let Some(rids) =
                        &rel_ids_out
                    {
                        let mut iter: Box<dyn Iterator<Item = EdgeKey>> =
                            Box::new(std::iter::empty());
                        for rid in rids {
                            iter = Box::new(iter.chain(snapshot.neighbors(src_iid, Some(*rid))));
                        }
                        iter
                    } else {
                        Box::new(snapshot.neighbors(src_iid, None))
                    };

                    let dst_alias_inner = dst_alias_out.clone();
                    let edge_alias_inner = edge_alias_out.clone();

                    let mapped = neighbors.map(move |edge| {
                        let mut new_row = row.clone();
                        new_row = new_row.with(dst_alias_inner.clone(), Value::NodeId(edge.dst));
                        if let Some(ea) = &edge_alias_inner {
                            new_row = new_row.with(ea.clone(), Value::EdgeKey(edge));
                        }
                        Ok(new_row)
                    });
                    Box::new(mapped) as Box<dyn Iterator<Item = Result<Row>>>
                }
                Err(e) => {
                    Box::new(std::iter::once(Err(e))) as Box<dyn Iterator<Item = Result<Row>>>
                }
            });

            let rows_in = rows;
            // src_alias, dst_alias, edge_alias are still owned by this function scope (mostly),
            // but were cloned for out_iter. Original vars can be moved into in_iter.
            // Actually, `src_alias.clone()` above didn't consume `src_alias`.

            let in_iter = rows_in.into_iter().map(Ok).flat_map(move |res| match res {
                Ok(row) => {
                    let node_val = row.get(&src_alias).cloned();
                    let target_iid = match node_val {
                        Some(Value::NodeId(id)) => id,
                        _ => {
                            return Box::new(std::iter::empty())
                                as Box<dyn Iterator<Item = Result<Row>>>;
                        }
                    };

                    let rel_ids_in = rel_ids.clone();
                    let candidates: Box<dyn Iterator<Item = EdgeKey>> =
                        if let Some(rids) = &rel_ids_in {
                            let mut iter: Box<dyn Iterator<Item = EdgeKey>> =
                                Box::new(std::iter::empty());
                            for rid in rids {
                                iter = Box::new(iter.chain(
                                    snapshot.incoming_neighbors_erased(target_iid, Some(*rid)),
                                ));
                            }
                            iter
                        } else {
                            snapshot.incoming_neighbors_erased(target_iid, None)
                        };

                    let dst_alias_inner = dst_alias.clone();
                    let edge_alias_inner = edge_alias.clone();
                    let path_alias_inner = path_alias.clone();

                    let mapped = candidates.map(move |edge| {
                        let mut new_row = row.clone();
                        new_row = new_row.with(dst_alias_inner.clone(), Value::NodeId(edge.src));
                        if let Some(ea) = &edge_alias_inner {
                            new_row = new_row.with(ea.clone(), Value::EdgeKey(edge));
                        }
                        if let Some(pa) = &path_alias_inner {
                            new_row.join_path(pa, edge.dst, edge, edge.src);
                        }
                        Ok(new_row)
                    });
                    Box::new(mapped) as Box<dyn Iterator<Item = Result<Row>>>
                }
                Err(e) => {
                    Box::new(std::iter::once(Err(e))) as Box<dyn Iterator<Item = Result<Row>>>
                }
            });

            let combined = out_iter.chain(in_iter);

            if let Some(n) = limit {
                PlanIterator::Dynamic(Box::new(combined.take(*n as usize)))
            } else {
                PlanIterator::Dynamic(Box::new(combined))
            }
        }
        Plan::Filter { input, predicate } => {
            let input_iter = execute_plan(snapshot, input, params);
            PlanIterator::Filter(FilterIter {
                snapshot,
                input: Box::new(input_iter),
                predicate,
                params,
            })
        }
        Plan::Project { input, projections } => {
            let input_iter = execute_plan(snapshot, input, params);
            let projections = projections.clone();
            let params = params.clone();
            // We need to capture snapshot. But snapshot is &S within reference 'a.
            // Check if we can capture it in move closure?
            // Yes, &S is Copy.

            PlanIterator::Dynamic(Box::new(input_iter.map(move |result| {
                let row = result?;
                let mut new_row = crate::executor::Row::default();
                for (alias, expr) in &projections {
                    let val =
                        crate::evaluator::evaluate_expression_value(expr, &row, snapshot, &params);
                    new_row = new_row.with(alias.clone(), val);
                }
                Ok(new_row)
            })))
        }
        Plan::Aggregate {
            input,
            group_by,
            aggregates,
        } => {
            let input_iter = execute_plan(snapshot, input, params);
            PlanIterator::Dynamic(execute_aggregate(
                snapshot,
                Box::new(input_iter),
                group_by.clone(),
                aggregates.clone(),
                params,
            ))
        }
        Plan::OrderBy { input, items } => {
            let input_iter = execute_plan(snapshot, input, params);
            let rows: Vec<Result<Row>> = input_iter.collect();
            #[allow(clippy::type_complexity)]
            let mut sortable: Vec<(Result<Row>, Vec<(Value, Direction)>)> = rows
                .into_iter()
                .map(|row| {
                    match &row {
                        Ok(r) => {
                            let sort_keys: Vec<(Value, Direction)> = items
                                .iter()
                                .map(|(expr, dir)| {
                                    let val = crate::evaluator::evaluate_expression_value(
                                        expr, r, snapshot, params,
                                    );
                                    (val, dir.clone())
                                })
                                .collect();
                            (row, sort_keys)
                        }
                        Err(_) => (row, vec![]), // Error rows sort arbitrarily (usually bubble up)
                    }
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

            PlanIterator::Dynamic(Box::new(sortable.into_iter().map(|(row, _)| row)))
        }
        Plan::Skip { input, skip } => {
            let input_iter = execute_plan(snapshot, input, params);
            PlanIterator::Dynamic(Box::new(input_iter.skip(*skip as usize)))
        }
        Plan::Limit { input, limit } => {
            let input_iter = execute_plan(snapshot, input, params);
            PlanIterator::Dynamic(Box::new(input_iter.take(*limit as usize)))
        }
        Plan::Distinct { input } => {
            let input_iter = execute_plan(snapshot, input, params);
            let mut seen = std::collections::HashSet::new();
            PlanIterator::Dynamic(Box::new(input_iter.filter(move |result| {
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
            })))
        }
        Plan::Unwind {
            input,
            expression,
            alias,
        } => {
            let input_iter = execute_plan(snapshot, input, params);
            // Must clone expression because it's used in closure
            let expression = expression.clone();
            let alias = alias.clone();
            let params = params.clone();
            // Capture snapshot in closure

            PlanIterator::Dynamic(Box::new(input_iter.flat_map(move |result| {
                match result {
                    Ok(row) => {
                        let val = crate::evaluator::evaluate_expression_value(
                            &expression,
                            &row,
                            snapshot,
                            &params,
                        );
                        match val {
                            Value::List(list) => {
                                // Expand list
                                let mut rows = Vec::with_capacity(list.len());
                                for item in list {
                                    rows.push(Ok(row.clone().with(alias.clone(), item)));
                                }
                                rows
                            }
                            Value::Null => {
                                // Null unwinds to 0 rows
                                vec![]
                            }
                            _ => {
                                // Scalar unwinds to 1 row
                                vec![Ok(row.clone().with(alias.clone(), val))]
                            }
                        }
                    }
                    Err(e) => vec![Err(e)],
                }
            })))
        }
        Plan::Union { left, right, all } => {
            let left_iter = execute_plan(snapshot, left, params);
            let right_iter = execute_plan(snapshot, right, params);
            let chained = left_iter.chain(right_iter);

            if *all {
                // UNION ALL: keep all rows
                PlanIterator::Dynamic(Box::new(chained))
            } else {
                // UNION: deduplicate
                let mut seen = std::collections::HashSet::new();
                PlanIterator::Dynamic(Box::new(chained.filter(move |result| {
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
                })))
            }
        }
        Plan::Create { .. } => {
            // CREATE should be executed via execute_write, not execute_plan
            PlanIterator::Dynamic(Box::new(std::iter::once(Err(Error::Other(
                "CREATE must be executed via execute_write".into(),
            )))))
        }
        Plan::Delete { .. } => {
            // DELETE should be executed via execute_write, not execute_plan
            PlanIterator::Dynamic(Box::new(std::iter::once(Err(Error::Other(
                "DELETE must be executed via execute_write".into(),
            )))))
        }
        Plan::SetProperty { .. } => {
            // SET should be executed via execute_write, not execute_plan
            PlanIterator::Dynamic(Box::new(std::iter::once(Err(Error::Other(
                "SET must be executed via execute_write".into(),
            )))))
        }
        Plan::RemoveProperty { .. } => {
            // REMOVE should be executed via execute_write, not execute_plan
            PlanIterator::Dynamic(Box::new(std::iter::once(Err(Error::Other(
                "REMOVE must be executed via execute_write".into(),
            )))))
        }
        Plan::IndexSeek {
            alias,
            label,
            field,
            value_expr,
            fallback,
        } => {
            // 1. Evaluate key value
            let val = evaluate_expression_value(value_expr, &Row::default(), snapshot, params);
            // evaluate_expression_value does not return Result, it returns Value directly.
            // But we need to handle errors? evaluate_expression_value swallows errors (returns Null).
            // That's MVP logic in evaluator.rs.

            // 2. Convert to PropertyValue
            let prop_val = match val {
                Value::Null => nervusdb_v2_api::PropertyValue::Null,
                Value::Bool(b) => nervusdb_v2_api::PropertyValue::Bool(b),
                Value::Int(i) => nervusdb_v2_api::PropertyValue::Int(i),
                Value::Float(f) => nervusdb_v2_api::PropertyValue::Float(f),
                Value::String(s) => nervusdb_v2_api::PropertyValue::String(s),
                _ => {
                    // Index does not support NodeId/ExternalId/EdgeKey/List values
                    // Fallback to scan
                    return execute_plan(snapshot, fallback, params);
                }
            };

            // 3. Try Index Lookup
            if let Some(mut node_ids) = snapshot.lookup_index(label, field, &prop_val) {
                // Sort IDs for consistent output (optional but good)
                node_ids.sort();
                let alias = alias.clone();
                PlanIterator::Dynamic(Box::new(
                    node_ids
                        .into_iter()
                        .map(move |iid| Ok(Row::default().with(alias.clone(), Value::NodeId(iid)))),
                ))
            } else {
                // 4. Fallback if index missing
                execute_plan(snapshot, fallback, params)
            }
        }
        Plan::Values { rows } => {
            let rows = rows.clone();
            PlanIterator::Dynamic(Box::new(rows.into_iter().map(Ok)))
        }
    }
}

/// Execute a write plan (CREATE/DELETE/SET/REMOVE) with a transaction
pub fn execute_write<S: GraphSnapshot>(
    plan: &Plan,
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    params: &crate::query_api::Params,
) -> Result<u32> {
    match plan {
        Plan::Create { input, pattern } => execute_create(snapshot, input, txn, pattern, params),
        Plan::Delete {
            input,
            detach,
            expressions,
        } => execute_delete(snapshot, input, txn, *detach, expressions, params),
        Plan::SetProperty { input, items } => execute_set(snapshot, input, txn, items, params),
        Plan::RemoveProperty { input, items } => {
            execute_remove(snapshot, input, txn, items, params)
        }
        Plan::Foreach {
            input,
            variable,
            list,
            sub_plan,
        } => execute_foreach(snapshot, input, txn, variable, list, sub_plan, params),
        _ => Err(Error::Other(
            "Only CREATE, DELETE, SET, REMOVE and FOREACH plans can be executed with execute_write"
                .into(),
        )),
    }
}

/// Find the CREATE part of a plan (for MERGE support)
fn find_create_plan<'a>(plan: &'a Plan) -> Option<&'a Plan> {
    match plan {
        Plan::Create { .. } => Some(plan),
        Plan::Filter { input, .. }
        | Plan::Project { input, .. }
        | Plan::Limit { input, .. }
        | Plan::Skip { input, .. }
        | Plan::OrderBy { input, .. }
        | Plan::Distinct { input }
        | Plan::Unwind { input, .. }
        | Plan::Aggregate { input, .. }
        | Plan::Delete { input, .. }
        | Plan::SetProperty { input, .. }
        | Plan::RemoveProperty { input, .. }
        | Plan::Foreach { input, .. } => find_create_plan(input),
        Plan::CartesianProduct { left, right } => {
            find_create_plan(left).or(find_create_plan(right))
        }
        Plan::Union { left, right, .. } => find_create_plan(left).or(find_create_plan(right)),
        Plan::Apply {
            input, subquery, ..
        } => find_create_plan(input).or(find_create_plan(subquery)),
        Plan::ProcedureCall { input, .. } => find_create_plan(input),
        Plan::MatchOut { input, .. }
        | Plan::MatchIn { input, .. }
        | Plan::MatchUndirected { input, .. }
        | Plan::MatchOutVarLen { input, .. } => input.as_deref().and_then(find_create_plan),
        Plan::IndexSeek { fallback, .. } => find_create_plan(fallback),
        Plan::NodeScan { .. } | Plan::Values { .. } | Plan::ReturnOne => None,
    }
}

pub(crate) fn execute_merge<S: GraphSnapshot>(
    plan: &Plan,
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    params: &crate::query_api::Params,
    on_create_items: &[(String, String, Expression)],
    on_match_items: &[(String, String, Expression)],
) -> Result<u32> {
    let create_plan = find_create_plan(plan)
        .ok_or_else(|| Error::Other("MERGE plan must contain a CREATE stage".into()))?;
    let Plan::Create { pattern, .. } = create_plan else {
        return Err(Error::Other(
            "MERGE CREATE stage is not available in compiled plan".into(),
        ));
    };

    fn apply_merge_set_items<S: GraphSnapshot>(
        snapshot: &S,
        txn: &mut dyn WriteableGraph,
        row: &Row,
        items: &[(String, String, Expression)],
        params: &crate::query_api::Params,
    ) -> Result<()> {
        for (var, key, expr) in items {
            let val = evaluate_expression_value(expr, row, snapshot, params);
            let prop_val = convert_executor_value_to_property(&val)?;
            if let Some(node_id) = row.get_node(var) {
                txn.set_node_property(node_id, key.clone(), prop_val)?;
            } else if let Some(edge) = row.get_edge(var) {
                txn.set_edge_property(edge.src, edge.rel, edge.dst, key.clone(), prop_val)?;
            } else {
                return Err(Error::Other(format!("Variable {} not found in row", var)));
            }
        }
        Ok(())
    }

    #[derive(Clone)]
    struct OverlayNode {
        label: Option<String>,
        props: Vec<(String, PropertyValue)>,
        iid: InternalNodeId,
    }

    fn eval_props(
        props: &crate::ast::PropertyMap,
        params: &crate::query_api::Params,
    ) -> Result<Vec<(String, PropertyValue)>> {
        let mut out = Vec::with_capacity(props.properties.len());
        for prop in &props.properties {
            let v = evaluate_property_value(&prop.value, params)?;
            // NULL values are allowed in MERGE properties
            out.push((prop.key.clone(), v));
        }
        Ok(out)
    }

    fn overlay_lookup(
        overlay: &[OverlayNode],
        label: &Option<String>,
        expected: &[(String, PropertyValue)],
    ) -> Option<InternalNodeId> {
        overlay.iter().find_map(|n| {
            if &n.label != label {
                return None;
            }
            for (k, v) in expected {
                if n.props.iter().find(|(kk, _)| kk == k).map(|(_, vv)| vv) != Some(v) {
                    return None;
                }
            }
            Some(n.iid)
        })
    }

    fn find_existing_node<S: GraphSnapshot>(
        snapshot: &S,
        label: &Option<String>,
        expected: &[(String, PropertyValue)],
    ) -> Option<InternalNodeId> {
        fn to_api(v: &PropertyValue) -> nervusdb_v2_api::PropertyValue {
            match v {
                PropertyValue::Null => nervusdb_v2_api::PropertyValue::Null,
                PropertyValue::Bool(b) => nervusdb_v2_api::PropertyValue::Bool(*b),
                PropertyValue::Int(i) => nervusdb_v2_api::PropertyValue::Int(*i),
                PropertyValue::Float(f) => nervusdb_v2_api::PropertyValue::Float(*f),
                PropertyValue::String(s) => nervusdb_v2_api::PropertyValue::String(s.clone()),
                PropertyValue::DateTime(i) => nervusdb_v2_api::PropertyValue::DateTime(*i),
                PropertyValue::Blob(b) => nervusdb_v2_api::PropertyValue::Blob(b.clone()),
                PropertyValue::List(l) => {
                    nervusdb_v2_api::PropertyValue::List(l.iter().map(to_api).collect())
                }
                PropertyValue::Map(m) => nervusdb_v2_api::PropertyValue::Map(
                    m.iter().map(|(k, v)| (k.clone(), to_api(v))).collect(),
                ),
            }
        }

        let label_id = match label {
            None => None,
            Some(name) => match snapshot.resolve_label_id(name) {
                Some(id) => Some(id),
                None => return None,
            },
        };

        for iid in snapshot.nodes() {
            if snapshot.is_tombstoned_node(iid) {
                continue;
            }
            if let Some(lid) = label_id
                && snapshot.node_label(iid) != Some(lid)
            {
                continue;
            }
            let mut ok = true;
            for (k, v) in expected {
                if snapshot.node_property(iid, k) != Some(to_api(v)) {
                    ok = false;
                    break;
                }
            }
            if ok {
                return Some(iid);
            }
        }
        None
    }

    fn create_node(
        txn: &mut dyn WriteableGraph,
        label: &Option<String>,
        props: &[(String, PropertyValue)],
        created_count: &mut u32,
    ) -> Result<InternalNodeId> {
        let external_id = ExternalId::from(
            *created_count as u64 + chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64,
        );
        let label_id = if let Some(l) = label {
            txn.get_or_create_label_id(l)?
        } else {
            0
        };

        let iid = txn.create_node(external_id, label_id)?;
        *created_count += 1;
        for (k, v) in props {
            txn.set_node_property(iid, k.clone(), v.clone())?;
        }
        Ok(iid)
    }

    fn find_or_create_node<S: GraphSnapshot>(
        snapshot: &S,
        txn: &mut dyn WriteableGraph,
        node: &crate::ast::NodePattern,
        overlay: &mut Vec<OverlayNode>,
        params: &crate::query_api::Params,
        created_count: &mut u32,
    ) -> Result<(InternalNodeId, bool)> {
        let label = node.labels.first().cloned();
        // MERGE can operate with or without properties
        let props = node.properties.as_ref();
        let expected = if let Some(props) = props {
            eval_props(props, params)?
        } else {
            Vec::new() // No properties to match on
        };

        if let Some(iid) = overlay_lookup(overlay, &label, &expected) {
            return Ok((iid, false));
        }
        if let Some(iid) = find_existing_node(snapshot, &label, &expected) {
            return Ok((iid, false));
        }

        let iid = create_node(txn, &label, &expected, created_count)?;
        overlay.push(OverlayNode {
            label,
            props: expected,
            iid,
        });
        Ok((iid, true))
    }

    let mut created_count = 0u32;
    let mut overlay: Vec<OverlayNode> = Vec::new();

    // Support arbitrary length patterns: single node, single-hop, multi-hop chains
    // Process pattern elements sequentially: node-rel-node-rel-node...
    let mut current_iids = Vec::new(); // Track node IDs at each position
    let mut created_any = false;
    let mut merge_row = Row::default();

    // Iterate through pattern elements
    let mut i = 0;
    while i < pattern.elements.len() {
        match &pattern.elements[i] {
            PathElement::Node(node_pat) => {
                // Find or create node
                let (iid, node_created) = find_or_create_node(
                    snapshot,
                    txn,
                    node_pat,
                    &mut overlay,
                    params,
                    &mut created_count,
                )?;
                if node_created {
                    created_any = true;
                }
                // Ensure current_iids has capacity for this index
                if current_iids.len() <= i {
                    current_iids.resize(i + 1, None);
                }
                current_iids[i] = Some(iid);
                if let Some(var) = &node_pat.variable {
                    merge_row = merge_row.with(var.clone(), Value::NodeId(iid));
                }
                i += 1;
            }
            PathElement::Relationship(rel_pat) => {
                // Relationship must be followed by a node
                if i + 1 >= pattern.elements.len() {
                    return Err(Error::Other("relationship must be followed by node".into()));
                }
                if let PathElement::Node(dst_node) = &pattern.elements[i + 1] {
                    if i == 0 {
                        return Err(Error::Other(
                            "relationship cannot be the first element in MERGE pattern".into(),
                        ));
                    }
                    // Get source node ID from previous element (must be a node)
                    let src_iid = current_iids.get(i - 1).and_then(|x| *x).ok_or_else(|| {
                        Error::Other("missing source node for relationship".into())
                    })?;

                    // Handle variable-length relationships
                    if rel_pat.variable_length.is_some() {
                        return Err(Error::NotImplemented(
                            "MERGE variable-length relationships need multi-hop expansion",
                        ));
                    }

                    // Get/create relationship type
                    let rel_type_name = rel_pat.types.first().cloned().ok_or_else(|| {
                        Error::Other("MERGE relationship requires a type for creation".into())
                    })?;
                    let rel_type = txn.get_or_create_rel_type_id(&rel_type_name)?;

                    // Find or create destination node
                    let (dst_iid, dst_created) = find_or_create_node(
                        snapshot,
                        txn,
                        dst_node,
                        &mut overlay,
                        params,
                        &mut created_count,
                    )?;
                    if dst_created {
                        created_any = true;
                    }

                    // Check if edge already exists
                    let mut exists = false;
                    for edge in snapshot.neighbors(src_iid, Some(rel_type)) {
                        if edge.dst == dst_iid {
                            exists = true;
                            break;
                        }
                    }

                    if !exists {
                        txn.create_edge(src_iid, rel_type, dst_iid)?;
                        created_count += 1;
                        created_any = true;
                    }

                    // Extend row bindings for ON CREATE / ON MATCH SET
                    if let Some(var) = &rel_pat.variable {
                        merge_row = merge_row.with(
                            var.clone(),
                            Value::EdgeKey(EdgeKey {
                                src: src_iid,
                                rel: rel_type,
                                dst: dst_iid,
                            }),
                        );
                    }
                    // Include source and destination node variables if present
                    // We can get the source node variable from the previous element in pattern
                    if let PathElement::Node(src_node) = &pattern.elements[i - 1] {
                        if let Some(src_var) = &src_node.variable {
                            merge_row = merge_row.with(src_var.clone(), Value::NodeId(src_iid));
                        }
                    }
                    // Destination node variable
                    if let Some(dst_var) = &dst_node.variable {
                        merge_row = merge_row.with(dst_var.clone(), Value::NodeId(dst_iid));
                    }

                    // Store destination node ID for next hop
                    if current_iids.len() <= i + 1 {
                        current_iids.resize(i + 2, None);
                    }
                    current_iids[i + 1] = Some(dst_iid);

                    i += 2; // Skip relationship and destination node
                } else {
                    return Err(Error::Other("relationship must be followed by node".into()));
                }
            }
        }
    }

    // Apply ON CREATE / ON MATCH updates once per MERGE execution.
    let set_items = if created_any {
        on_create_items
    } else {
        on_match_items
    };
    if !set_items.is_empty() {
        apply_merge_set_items(snapshot, txn, &merge_row, set_items, params)?;
    }

    Ok(created_count)
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
    fn remove_node_property(&mut self, node: InternalNodeId, key: &str) -> Result<()>;
    fn remove_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: &str,
    ) -> Result<()>;
    fn tombstone_node(&mut self, node: InternalNodeId) -> Result<()>;
    fn tombstone_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> Result<()>;

    // T65: Dynamic schema support
    fn get_or_create_label_id(&mut self, name: &str) -> Result<LabelId>;
    fn get_or_create_rel_type_id(&mut self, name: &str) -> Result<RelTypeId>;
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

        fn remove_node_property(&mut self, node: InternalNodeId, key: &str) -> Result<()> {
            EngineWriteTxn::remove_node_property(self, node, key);
            Ok(())
        }

        fn remove_edge_property(
            &mut self,
            src: InternalNodeId,
            rel: RelTypeId,
            dst: InternalNodeId,
            key: &str,
        ) -> Result<()> {
            EngineWriteTxn::remove_edge_property(self, src, rel, dst, key);
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

        fn get_or_create_label_id(&mut self, name: &str) -> Result<LabelId> {
            EngineWriteTxn::get_or_create_label(self, name).map_err(|e| Error::Other(e.to_string()))
        }

        fn get_or_create_rel_type_id(&mut self, name: &str) -> Result<RelTypeId> {
            EngineWriteTxn::get_or_create_rel_type(self, name)
                .map_err(|e| Error::Other(e.to_string()))
        }
    }
}

fn execute_foreach<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    variable: &str,
    list: &Expression,
    sub_plan: &Plan,
    params: &crate::query_api::Params,
) -> Result<u32> {
    let mut total_mods = 0;

    // We must collect rows first if needed, but execute_plan yields independent rows?
    // Actually execute_plan captures reference to S.
    // And we borrow traverse input plan.
    for row in execute_plan(snapshot, input, params) {
        let row = row?;

        let list_val = evaluate_expression_value(list, &row, snapshot, params);

        let items = match list_val {
            Value::List(l) => l,
            _ => {
                return Err(Error::Other(format!(
                    "FOREACH expression must evaluate to a list, got {:?}",
                    list_val
                )));
            }
        };

        for item in items {
            let sub_row = row.clone().with(variable, item.clone());
            let mut current_sub_plan = sub_plan.clone();
            inject_rows(&mut current_sub_plan, vec![sub_row]);
            let mods = execute_write(&current_sub_plan, snapshot, txn, params)?;
            total_mods += mods;
        }
    }

    Ok(total_mods)
}

fn execute_create<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
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

    // Iterate over input rows
    for row in execute_plan(snapshot, input, params) {
        let row = row?;

        // Scope: IDs created in this row's context
        // Maps pattern index -> InternalNodeId
        let mut row_node_ids: std::collections::HashMap<usize, InternalNodeId> =
            std::collections::HashMap::new();

        // Create all nodes first
        for (idx, node_pat) in &node_patterns {
            let external_id = ExternalId::from(
                created_count as u64 + chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64,
            );

            // Resolve or create label
            let label_id = if let Some(label) = node_pat.labels.first() {
                txn.get_or_create_label_id(label)?
            } else {
                0
            };

            let node_id = txn.create_node(external_id, label_id)?;
            created_count += 1;
            row_node_ids.insert(*idx, node_id);

            // Set properties if any
            if let Some(props) = &node_pat.properties {
                for prop in &props.properties {
                    // Use evaluate_expression_value to support variables
                    let val = evaluate_expression_value(&prop.value, &row, snapshot, params);
                    let prop_val = convert_executor_value_to_property(&val)?;
                    txn.set_node_property(node_id, prop.key.clone(), prop_val)?;
                }
            }
        }

        // Now create all relationships
        for (idx, rel_pat) in &rel_patterns {
            let rel_type_name = rel_pat
                .types
                .first()
                .ok_or_else(|| Error::Other("CREATE relationship requires a type".into()))?;

            let rel_type = txn.get_or_create_rel_type_id(rel_type_name)?;

            // Find src and dst
            let src_id = *row_node_ids
                .get(&(idx - 1))
                .ok_or(Error::Other("CREATE relationship src node missing".into()))?;

            let dst_id = *row_node_ids
                .get(&(idx + 1))
                .ok_or(Error::Other("CREATE relationship dst node missing".into()))?;

            // Create the edge
            txn.create_edge(src_id, rel_type, dst_id)?;
            created_count += 1;

            // Set properties if any
            if let Some(props) = &rel_pat.properties {
                for prop in &props.properties {
                    let val = evaluate_expression_value(&prop.value, &row, snapshot, params);
                    let prop_val = convert_executor_value_to_property(&val)?;
                    txn.set_edge_property(src_id, rel_type, dst_id, prop.key.clone(), prop_val)?;
                }
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
    let mut edges_to_delete: Vec<EdgeKey> = Vec::new();
    let mut seen_edges: std::collections::HashSet<EdgeKey> = std::collections::HashSet::new();

    // Stream input rows and collect delete targets without materializing all rows.
    for row in execute_plan(snapshot, input, params) {
        let row = row?;
        for expr in expressions {
            match expr {
                Expression::Variable(var_name) => {
                    if let Some(node_id) = row.get_node(var_name) {
                        if seen_nodes.insert(node_id) {
                            nodes_to_delete.push(node_id);
                        }
                    } else if let Some(edge) = row.get_edge(var_name) {
                        if seen_edges.insert(edge) {
                            edges_to_delete.push(edge);
                        }
                    } else {
                        return Err(Error::Other(format!(
                            "Variable {} not found in row",
                            var_name
                        )));
                    }

                    if nodes_to_delete.len() + edges_to_delete.len() > MAX_DELETE_TARGETS {
                        return Err(Error::Other(format!(
                            "DELETE target limit exceeded ({MAX_DELETE_TARGETS}); batch your deletes"
                        )));
                    }
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

    // Delete explicitly targeted edges.
    for edge in edges_to_delete {
        txn.tombstone_edge(edge.src, edge.rel, edge.dst)?;
        deleted_count += 1;
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

fn execute_set<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    items: &[(String, String, Expression)],
    params: &crate::query_api::Params,
) -> Result<u32> {
    let mut count = 0;
    // Iterate over input rows
    for row in execute_plan(snapshot, input, params) {
        let row = row?;
        for (var, key, expr) in items {
            // Evaluate expression
            let val = evaluate_expression_value(expr, &row, snapshot, params);

            // Convert value to PropertyValue
            let prop_val = convert_executor_value_to_property(&val)?;

            // Set property (node or edge)
            if let Some(node_id) = row.get_node(var) {
                txn.set_node_property(node_id, key.clone(), prop_val)?;
            } else if let Some(edge) = row.get_edge(var) {
                txn.set_edge_property(edge.src, edge.rel, edge.dst, key.clone(), prop_val)?;
            } else {
                return Err(Error::Other(format!("Variable {} not found in row", var)));
            }
            count += 1;
        }
    }
    Ok(count)
}

fn execute_remove<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    items: &[(String, String)],
    params: &crate::query_api::Params,
) -> Result<u32> {
    let mut count = 0;
    for row in execute_plan(snapshot, input, params) {
        let row = row?;
        for (var, key) in items {
            if let Some(node_id) = row.get_node(var) {
                txn.remove_node_property(node_id, key)?;
            } else if let Some(edge) = row.get_edge(var) {
                txn.remove_edge_property(edge.src, edge.rel, edge.dst, key)?;
            } else {
                return Err(Error::Other(format!("Variable {} not found in row", var)));
            }
            count += 1;
        }
    }
    Ok(count)
}

fn convert_executor_value_to_property(value: &Value) -> Result<PropertyValue> {
    match value {
        Value::Null => Ok(PropertyValue::Null),
        Value::Bool(b) => Ok(PropertyValue::Bool(*b)),
        Value::Int(i) => Ok(PropertyValue::Int(*i)),
        Value::String(s) => Ok(PropertyValue::String(s.clone())),
        Value::Float(f) => Ok(PropertyValue::Float(*f)),
        Value::DateTime(i) => Ok(PropertyValue::DateTime(*i)),
        Value::Blob(b) => Ok(PropertyValue::Blob(b.clone())),
        Value::Path(_) => Err(Error::Other(
            "Path value cannot be stored as property".to_string(),
        )),
        Value::List(l) => {
            let mut list = Vec::with_capacity(l.len());
            for v in l {
                list.push(convert_executor_value_to_property(v)?);
            }
            Ok(PropertyValue::List(list))
        }
        Value::Map(m) => {
            let mut map = std::collections::BTreeMap::new();
            for (k, v) in m {
                map.insert(k.clone(), convert_executor_value_to_property(v)?);
            }
            Ok(PropertyValue::Map(map))
        }
        Value::Node(_) | Value::Relationship(_) | Value::ReifiedPath(_) => {
            Err(Error::NotImplemented(
                "node/relationship/path objects as property values are not supported",
            ))
        }
        Value::NodeId(_) | Value::ExternalId(_) | Value::EdgeKey(_) => Err(Error::NotImplemented(
            "node/edge identifiers as property values are not supported",
        )),
    }
}

pub fn convert_api_property_to_value(api_value: &nervusdb_v2_api::PropertyValue) -> Value {
    match api_value {
        nervusdb_v2_api::PropertyValue::Null => Value::Null,
        nervusdb_v2_api::PropertyValue::Bool(b) => Value::Bool(*b),
        nervusdb_v2_api::PropertyValue::Int(i) => Value::Int(*i),
        nervusdb_v2_api::PropertyValue::Float(f) => Value::Float(*f),
        nervusdb_v2_api::PropertyValue::String(s) => Value::String(s.clone()),
        nervusdb_v2_api::PropertyValue::DateTime(i) => Value::DateTime(*i),
        nervusdb_v2_api::PropertyValue::Blob(b) => Value::Blob(b.clone()),
        nervusdb_v2_api::PropertyValue::List(l) => {
            Value::List(l.iter().map(convert_api_property_to_value).collect())
        }
        nervusdb_v2_api::PropertyValue::Map(m) => Value::Map(
            m.iter()
                .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
                .collect(),
        ),
    }
}

fn inject_rows(plan: &mut Plan, rows: Vec<Row>) {
    match plan {
        Plan::Values { rows: target_rows } => {
            *target_rows = rows;
        }
        Plan::Create { input, .. }
        | Plan::Delete { input, .. }
        | Plan::SetProperty { input, .. }
        | Plan::RemoveProperty { input, .. }
        | Plan::Foreach { input, .. }
        | Plan::Filter { input, .. }
        | Plan::Project { input, .. }
        | Plan::Limit { input, .. }
        | Plan::Skip { input, .. }
        | Plan::OrderBy { input, .. }
        | Plan::Distinct { input }
        | Plan::Unwind { input, .. }
        | Plan::Aggregate { input, .. } => inject_rows(input, rows),

        // Binary ops: injection usually goes to Left? Or ambiguous?
        // For FOREACH updates, it's linear.
        // CartesianProduct/Union shouldn't appear in strictly update chains (v2 MVP).
        // But if they do, we default to injecting to LEFT side (primary flow).
        Plan::CartesianProduct { left, .. } | Plan::Union { left, .. } => inject_rows(left, rows),

        Plan::Apply { input, .. } => inject_rows(input, rows),

        _ => {
            // Leaf plans like Scan, ReturnOne - cannot inject.
            // If we reached here without matching Values, it means the plan doesn't start with Values placeholder.
            // This is fine if FOREACH body doesn't actually use the input (e.g. standalone CREATE without vars).
            // But query_api ensures Foreach body starts with Values placeholder.
        }
    }
}

struct MatchOutIter<'a, S: GraphSnapshot + 'a> {
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
    fn new(
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
const DEFAULT_MAX_VAR_LEN_HOPS: u32 = 5;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
struct MatchOutVarLenIter<'a, S: GraphSnapshot + 'a> {
    snapshot: &'a S,
    input: Option<Box<dyn Iterator<Item = Result<Row>> + 'a>>,
    cur_row: Option<Row>,
    src_alias: &'a str,
    rels: Option<Vec<RelTypeId>>,
    edge_alias: Option<&'a str>,
    dst_alias: &'a str,
    direction: RelationshipDirection,
    min_hops: u32,
    max_hops: Option<u32>,
    limit: Option<u32>,
    node_iter: Option<Box<dyn Iterator<Item = InternalNodeId> + 'a>>,
    // DFS state: (start_node, current_node, current_depth, incoming_edge, current_path)
    stack: Vec<(
        InternalNodeId,
        InternalNodeId,
        u32,
        Option<EdgeKey>,
        Option<PathValue>,
    )>,
    emitted: u32,
    yielded_any: bool,
    optional: bool,
    path_alias: Option<&'a str>,
}

impl<'a, S: GraphSnapshot + 'a> MatchOutVarLenIter<'a, S> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        snapshot: &'a S,
        input: Option<Box<dyn Iterator<Item = Result<Row>> + 'a>>,
        src_alias: &'a str,
        rels: Option<Vec<RelTypeId>>,
        edge_alias: Option<&'a str>,
        dst_alias: &'a str,
        direction: RelationshipDirection,
        min_hops: u32,
        max_hops: Option<u32>,
        limit: Option<u32>,
        optional: bool,
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
            direction,
            min_hops,
            max_hops,
            limit,
            node_iter,
            stack: Vec::new(),
            emitted: 0,
            yielded_any: false,
            optional,
            path_alias,
        }
    }

    /// Start DFS from a node
    fn start_dfs(&mut self, start_node: InternalNodeId) {
        let initial_path = if let Some(alias) = self.path_alias
            && let Some(row) = &self.cur_row
        {
            match row.get(alias) {
                Some(Value::Path(p)) => Some(p.clone()),
                _ => None,
            }
        } else {
            None
        };

        // If it's a new path (not continuation), initialize it with the first node.
        // Actually, join_path will do that if we pass None initial_path.
        // But for DFS stack, we need to hold it.
        self.stack
            .push((start_node, start_node, 0, None, initial_path));
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
            if let Some((start_node, current_node, depth, incoming_edge, current_path)) =
                self.stack.pop()
            {
                // Expand
                if depth < max_hops {
                    let push_edge = |edge: EdgeKey,
                                     next_node: InternalNodeId,
                                     stack: &mut Vec<(
                        InternalNodeId,
                        InternalNodeId,
                        u32,
                        Option<EdgeKey>,
                        Option<PathValue>,
                    )>| {
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
                        stack.push((start_node, next_node, depth + 1, Some(edge), next_path));
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
                                    push_edge(edge, edge.src, &mut self.stack);
                                }
                            }
                        }
                        (RelationshipDirection::RightToLeft, None) => {
                            for edge in self.snapshot.incoming_neighbors_erased(current_node, None)
                            {
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
                                push_edge(edge, edge.src, &mut self.stack);
                            }
                        }
                    }
                }

                // Emit check
                if depth >= self.min_hops {
                    let mut row = self.cur_row.clone().unwrap_or_default();
                    row = row.with(self.src_alias, Value::NodeId(start_node));

                    if let Some(edge_alias) = self.edge_alias {
                        if let Some(e) = incoming_edge {
                            row = row.with(edge_alias, Value::EdgeKey(e));
                        } else {
                            row = row.with(edge_alias, Value::Null);
                        }
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
                && !self.yielded_any
                && self.input.is_some()
            {
                self.yielded_any = true;
                let mut null_row = row.clone();
                null_row = null_row.with(self.dst_alias, Value::Null);
                if let Some(ea) = self.edge_alias {
                    null_row = null_row.with(ea, Value::Null);
                }
                self.emitted += 1;
                return Some(Ok(null_row));
            }

            // 3. Get Next Start Node
            if let Some(input) = &mut self.input {
                match input.next() {
                    Some(Ok(row)) => {
                        self.cur_row = Some(row.clone());
                        self.yielded_any = false;

                        let src_val = row.get(self.src_alias);
                        match src_val {
                            Some(Value::NodeId(id)) => self.start_dfs(*id),
                            Some(Value::Null) => {
                                // Optional null source handled next iteration
                            }
                            _ => {} // Invalid, skip
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

/// Simple aggregation executor that collects all input, groups, and computes aggregates
/// Simple aggregation executor that collects all input, groups, and computes aggregates
fn execute_aggregate<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: Box<dyn Iterator<Item = Result<Row>> + 'a>,
    group_by: Vec<String>,
    aggregates: Vec<(AggregateFunction, String)>,
    params: &'a crate::query_api::Params,
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
                            .filter(|r| {
                                !matches!(
                                    evaluate_expression_value(expr, r, snapshot, params),
                                    Value::Null
                                )
                            })
                            .count();
                        Value::Float(count as f64)
                    }
                    AggregateFunction::Sum(expr) => {
                        let sum: f64 = rows
                            .iter()
                            .filter_map(|r| {
                                match evaluate_expression_value(expr, r, snapshot, params) {
                                    Value::Float(f) => Some(f),
                                    Value::Int(i) => Some(i as f64),
                                    _ => None,
                                }
                            })
                            .sum();
                        Value::Float(sum)
                    }
                    AggregateFunction::Avg(expr) => {
                        let values: Vec<f64> = rows
                            .iter()
                            .filter_map(|r| {
                                match evaluate_expression_value(expr, r, snapshot, params) {
                                    Value::Float(f) => Some(f),
                                    Value::Int(i) => Some(i as f64),
                                    _ => None,
                                }
                            })
                            .collect();
                        if values.is_empty() {
                            Value::Null
                        } else {
                            Value::Float(values.iter().sum::<f64>() / values.len() as f64)
                        }
                    }
                    AggregateFunction::Min(expr) => {
                        let min_val = rows
                            .iter()
                            .filter_map(|r| {
                                let v = evaluate_expression_value(expr, r, snapshot, params);
                                if v == Value::Null { None } else { Some(v) }
                            })
                            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                        min_val.unwrap_or(Value::Null)
                    }
                    AggregateFunction::Max(expr) => {
                        let max_val = rows
                            .iter()
                            .filter_map(|r| {
                                let v = evaluate_expression_value(expr, r, snapshot, params);
                                if v == Value::Null { None } else { Some(v) }
                            })
                            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                        max_val.unwrap_or(Value::Null)
                    }
                    AggregateFunction::Collect(expr) => {
                        let values: Vec<Value> = rows
                            .iter()
                            .map(|r| evaluate_expression_value(expr, r, snapshot, params))
                            .filter(|v| *v != Value::Null)
                            .collect();
                        Value::List(values)
                    }
                };
                result = result.with(alias, value);
            }

            Ok(result)
        })
        .collect();

    Box::new(results.into_iter())
}

pub fn parse_u32_identifier(name: &str) -> Result<u32> {
    name.parse::<u32>()
        .map_err(|_| Error::NotImplemented("non-numeric label/rel identifiers in M3"))
}

struct ExpandIter<'a, S: GraphSnapshot + 'a> {
    snapshot: &'a S,
    input: Box<dyn Iterator<Item = Result<Row>> + 'a>,
    src_alias: &'a str,
    rels: Option<Vec<RelTypeId>>,
    edge_alias: Option<&'a str>,
    dst_alias: &'a str,
    optional: bool,
    cur_row: Option<Row>,
    cur_edges: Option<Box<dyn Iterator<Item = EdgeKey> + 'a>>,
    yielded_any: bool,
    path_alias: Option<&'a str>,
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
                                    let mut row = row.clone();
                                    if let Some(ea) = self.edge_alias {
                                        row = row.with(ea, Value::Null);
                                    }
                                    row = row.with(self.dst_alias, Value::Null);
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
                if self.optional && !self.yielded_any {
                    self.yielded_any = true;
                    let mut row = self.cur_row.take().unwrap();
                    if let Some(ea) = self.edge_alias {
                        row = row.with(ea, Value::Null);
                    }
                    row = row.with(self.dst_alias, Value::Null);
                    self.cur_edges = None;
                    return Some(Ok(row));
                }
                self.cur_edges = None;
                self.cur_row = None;
            }
        }
    }
}

pub struct ApplyIter<'a, S: GraphSnapshot> {
    pub input_iter: Box<PlanIterator<'a, S>>,
    pub subquery_plan: &'a Plan,
    pub snapshot: &'a S,
    pub base_params: &'a crate::query_api::Params,
    pub current_outer_row: Option<Row>,
    pub current_results: std::vec::IntoIter<Row>,
}

impl<'a, S: GraphSnapshot> Iterator for ApplyIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // 1. Try to yield from current subquery results
            if let Some(inner_row) = self.current_results.next() {
                if let Some(outer) = &self.current_outer_row {
                    return Some(Ok(outer.join(&inner_row)));
                } else {
                    return Some(Err(Error::Other("Lost outer row in Apply".into())));
                }
            }

            // 2. Consume next outer row
            match self.input_iter.next() {
                Some(Ok(outer_row)) => {
                    self.current_outer_row = Some(outer_row.clone());

                    // Prepare params
                    // We need to merge base_params and outer_row
                    let mut extended_params = self.base_params.clone();
                    for (k, v) in &outer_row.cols {
                        extended_params.insert(k.clone(), v.clone());
                    }

                    // Execute subquery
                    // We must materialize to avoid lifetime issues with local extended_params
                    // Note: execute_plan returns an Iterator. We consume it immediately.
                    let iter = execute_plan(self.snapshot, self.subquery_plan, &extended_params);

                    let results: Vec<Row> = match iter.collect() {
                        Ok(rows) => rows,
                        Err(e) => return Some(Err(e)),
                    };

                    self.current_results = results.into_iter();
                    // Loop will continue and pick up the first result
                }
                Some(Err(e)) => return Some(Err(e)),
                None => return None, // Input exhausted
            }
        }
    }
}

pub struct ProcedureCallIter<'a, S: GraphSnapshot + 'a> {
    input_iter: Box<PlanIterator<'a, S>>,
    proc_name: String,
    args: &'a [Expression],
    yields: &'a [(String, Option<String>)],
    snapshot: &'a S,
    params: &'a crate::query_api::Params,
    current_outer_row: Option<Row>,
    current_results: std::vec::IntoIter<Row>,
}

impl<'a, S: GraphSnapshot + 'a> ProcedureCallIter<'a, S> {
    pub fn new(
        input_iter: Box<PlanIterator<'a, S>>,
        proc_name: String,
        args: &'a [Expression],
        yields: &'a [(String, Option<String>)],
        snapshot: &'a S,
        params: &'a crate::query_api::Params,
    ) -> Self {
        Self {
            input_iter,
            proc_name,
            args,
            yields,
            snapshot,
            params,
            current_outer_row: None,
            current_results: Vec::new().into_iter(),
        }
    }
}

impl<'a, S: GraphSnapshot + 'a> Iterator for ProcedureCallIter<'a, S> {
    type Item = Result<Row>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // 1. Try to yield from current sub-results
            if let Some(proc_row) = self.current_results.next()
                && let Some(outer_row) = &self.current_outer_row
            {
                // Start with outer row
                let mut joined = outer_row.clone();
                // Merge proc_row into joined, applying YIELD aliases
                if self.yields.is_empty() {
                    // If no yields specified, just merge all?
                    // Actually in Cypher, if no YIELD is specified, it might be an error or return all.
                    // For NervusDB MVP: if yields is empty, assume we return everything from proc_row.
                    for (k, v) in proc_row.cols {
                        joined = joined.with(k, v);
                    }
                } else {
                    for (field, alias) in self.yields {
                        if let Some(val) = proc_row.get(field) {
                            joined = joined.with(alias.as_ref().unwrap_or(field), val.clone());
                        }
                    }
                }
                return Some(Ok(joined));
            }

            // 2. Fetch next outer row
            match self.input_iter.next() {
                Some(Ok(outer_row)) => {
                    // 3. Evaluate arguments
                    let mut eval_args = Vec::with_capacity(self.args.len());
                    for arg in self.args {
                        let v =
                            evaluate_expression_value(arg, &outer_row, self.snapshot, self.params);
                        eval_args.push(v);
                    }

                    // 4. Call procedure
                    let registry = get_procedure_registry();
                    if let Some(proc) = registry.get(&self.proc_name) {
                        match proc.execute(self.snapshot as &dyn ErasedSnapshot, eval_args) {
                            Ok(results) => {
                                self.current_outer_row = Some(outer_row);
                                self.current_results = results.into_iter();
                                // Loop continues to yield from current_results
                            }
                            Err(e) => return Some(Err(e)),
                        }
                    } else {
                        return Some(Err(Error::Other(format!(
                            "Procedure {} not found",
                            self.proc_name
                        ))));
                    }
                }
                Some(Err(e)) => return Some(Err(e)),
                None => return None,
            }
        }
    }
}

use super::{EdgeKey, ExternalId, InternalNodeId};
use serde::ser::{SerializeMap, SerializeSeq};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// A graph node with its labels and properties.
///
/// Returned from queries when a node variable is projected (e.g. `RETURN n`).
/// The `id` field is the internal node ID, `labels` lists all labels, and
/// `properties` maps property names to values.
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash)]
pub struct NodeValue {
    pub id: InternalNodeId,
    pub labels: Vec<String>,
    pub properties: std::collections::BTreeMap<String, Value>,
}

/// A directed edge with its relationship type and properties.
///
/// Returned from queries when an edge variable is projected. The `key` encodes
/// the source node ID, relationship type ID, and destination node ID.
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash)]
pub struct RelationshipValue {
    pub key: EdgeKey,
    pub rel_type: String,
    pub properties: std::collections::BTreeMap<String, Value>,
}

/// A variable-length path stored as raw node and edge IDs.
///
/// See [`ReifiedPathValue`] for the reified variant with resolved labels
/// and properties.
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash)]
pub struct PathValue {
    pub nodes: Vec<InternalNodeId>,
    pub edges: Vec<EdgeKey>,
}

/// A variable-length path with fully resolved node and relationship data.
///
/// Unlike [`PathValue`], this variant carries labels, relationship types,
/// and properties for every element in the path.
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

/// A typed value in the NervusDB query engine.
///
/// Values appear as column entries in query result rows. The enum captures
/// Cypher-compatible types: primitives, graph elements (nodes, edges, paths),
/// and containers (lists, maps).
///
/// Use helper methods like [`as_string`](Value::as_string) or match directly
/// on variants when you need to extract a specific type.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Value {
    /// An internal node identifier.
    NodeId(InternalNodeId),
    /// An external (user-assigned) node identifier.
    ExternalId(ExternalId),
    /// An edge key encoding the source, relationship type, and destination.
    EdgeKey(EdgeKey),
    /// A 64-bit signed integer.
    Int(i64),
    /// A 64-bit floating point number.
    Float(f64),
    /// A UTF-8 string.
    String(String),
    /// A boolean.
    Bool(bool),
    /// The Cypher null value.
    Null,
    /// A list of values (heterogeneous).
    List(Vec<Value>),
    /// A datetime value stored as epoch milliseconds.
    DateTime(i64),
    /// Raw binary data.
    Blob(Vec<u8>),
    /// A map / dictionary of named values.
    Map(std::collections::BTreeMap<String, Value>),
    /// A variable-length path (raw IDs only).
    Path(PathValue),
    /// A graph node with labels and properties.
    Node(NodeValue),
    /// A directed edge with type and properties.
    Relationship(RelationshipValue),
    /// A variable-length path with resolved element data.
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
    /// Returns the string value if this is a `Value::String`, else `None`.
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
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

/// A single row of query results.
///
/// Each row is an ordered list of named columns `(name, value)`. Columns
/// can be accessed by name using [`get`](Row::get) or iterated via
/// [`columns`](Row::columns).
///
/// # Example
///
/// ```ignore
/// for row in &rows {
///     if let Some(Value::String(name)) = row.get("n.name") {
///         println!("{name}");
///     }
/// }
/// ```
#[derive(Debug)]
pub struct Row {
    pub(crate) cols: SmallVec<[(String, Value); 8]>,
    index: Option<HashMap<String, usize>>,
}

impl Clone for Row {
    fn clone(&self) -> Self {
        Self {
            cols: self.cols.clone(),
            index: self.index.clone(),
        }
    }
}

impl Default for Row {
    fn default() -> Self {
        Self {
            cols: SmallVec::new(),
            index: None,
        }
    }
}

impl PartialEq for Row {
    fn eq(&self, other: &Self) -> bool {
        self.cols == other.cols
    }
}

impl Row {
    /// Creates a new row from a vector of `(column_name, value)` pairs.
    pub fn new(cols: Vec<(String, Value)>) -> Self {
        let mut row = Self {
            cols: SmallVec::from_vec(cols),
            index: None,
        };
        row.maybe_rebuild_index();
        row
    }

    /// Returns the value for a named column, or `None` if the column
    /// does not exist in this row.
    pub fn get(&self, name: &str) -> Option<&Value> {
        if let Some(index) = &self.index
            && let Some(idx) = index.get(name)
        {
            return self.cols.get(*idx).map(|(_, v)| v);
        }
        for (k, v) in &self.cols {
            if k == name {
                return Some(v);
            }
        }
        None
    }

    pub fn with(mut self, name: impl Into<String>, value: Value) -> Self {
        let name = name.into();
        if let Some(index) = &self.index
            && let Some(idx) = index.get(&name).copied()
        {
            self.cols[idx].1 = value;
            return self;
        }
        for (k, v) in &mut self.cols {
            if *k == name {
                *v = value;
                return self;
            }
        }
        self.cols.push((name, value));
        self.maybe_rebuild_index();
        self
    }

    pub fn get_node(&self, name: &str) -> Option<InternalNodeId> {
        match self.get(name) {
            Some(Value::NodeId(iid)) => Some(*iid),
            Some(Value::Node(node)) => Some(node.id),
            _ => None,
        }
    }

    pub fn get_edge(&self, name: &str) -> Option<EdgeKey> {
        match self.get(name) {
            Some(Value::EdgeKey(e)) => Some(*e),
            Some(Value::Relationship(rel)) => Some(rel.key),
            _ => None,
        }
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

    /// Returns all columns as a slice of `(name, value)` pairs.
    pub fn columns(&self) -> &[(String, Value)] {
        self.cols.as_slice()
    }

    pub fn value_key(&self) -> Vec<Value> {
        self.cols.iter().map(|(_, v)| v.clone()).collect()
    }

    pub fn join(&self, other: &Row) -> Row {
        let mut out = Row {
            cols: self.cols.clone(),
            index: self.index.clone(),
        };
        out.cols.reserve(other.cols.len());
        out.cols.extend(other.cols.iter().cloned());
        out.maybe_rebuild_index();
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
        if let Some(index) = &self.index
            && let Some(idx) = index.get(name).copied()
        {
            self.cols[idx].1 = value;
            return;
        }
        for (k, v) in &mut self.cols {
            if k == name {
                *v = value;
                return;
            }
        }
        self.cols.push((name.to_string(), value));
        self.maybe_rebuild_index();
    }

    fn maybe_rebuild_index(&mut self) {
        if self.cols.len() <= 8 {
            self.index = None;
            return;
        }

        let mut index = HashMap::with_capacity(self.cols.len());
        for (idx, (key, _)) in self.cols.iter().enumerate() {
            index.insert(key.clone(), idx);
        }
        self.index = Some(index);
    }
}

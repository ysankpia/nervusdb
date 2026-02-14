use super::{
    EdgeKey, ErasedSnapshot, ExternalId, InternalNodeId, Result, convert_api_property_to_value,
};
use serde::ser::{SerializeMap, SerializeSeq};
use std::hash::{Hash, Hasher};

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
    pub(crate) cols: Vec<(String, Value)>,
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
                    Value::Node(node) => Some(node.id),
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
                    Value::Relationship(rel) => Some(rel.key),
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

use std::collections::BTreeMap;

pub type ExternalId = u64;
pub type InternalNodeId = u32;
pub type LabelId = u32;
pub type RelTypeId = u32;

/// Property value types for nodes and edges.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeKey {
    pub src: InternalNodeId,
    pub rel: RelTypeId,
    pub dst: InternalNodeId,
}

pub trait GraphStore {
    type Snapshot: GraphSnapshot;

    fn snapshot(&self) -> Self::Snapshot;
}

pub trait GraphSnapshot {
    type Neighbors<'a>: Iterator<Item = EdgeKey> + 'a
    where
        Self: 'a;

    fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> Self::Neighbors<'_>;

    fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        Box::new(std::iter::empty())
    }

    fn resolve_external(&self, _iid: InternalNodeId) -> Option<ExternalId> {
        None
    }

    fn node_label(&self, _iid: InternalNodeId) -> Option<LabelId> {
        None
    }

    fn is_tombstoned_node(&self, _iid: InternalNodeId) -> bool {
        false
    }

    /// Get a property value for a node.
    /// Returns the value from the most recent transaction that set it.
    fn node_property(&self, _iid: InternalNodeId, _key: &str) -> Option<PropertyValue> {
        None
    }

    /// Get a property value for an edge.
    /// Returns the value from the most recent transaction that set it.
    fn edge_property(&self, _edge: EdgeKey, _key: &str) -> Option<PropertyValue> {
        None
    }

    /// Get all properties for a node.
    /// Returns properties merged from all runs (newest takes precedence).
    fn node_properties(&self, _iid: InternalNodeId) -> Option<BTreeMap<String, PropertyValue>> {
        None
    }

    /// Get all properties for an edge.
    /// Returns properties merged from all runs (newest takes precedence).
    fn edge_properties(&self, _edge: EdgeKey) -> Option<BTreeMap<String, PropertyValue>> {
        None
    }
}

use std::collections::BTreeMap;

/// External identifier for a node, assigned by the user.
///
/// This is a stable ID that users can use to reference nodes across transactions.
/// Maps to an internal `InternalNodeId` for storage efficiency.
pub type ExternalId = u64;

/// Internal node identifier used for storage and lookups.
///
/// This is an auto-incremented ID used internally. Users typically work with
/// `ExternalId` through the ID map.
pub type InternalNodeId = u32;

/// Label identifier for node classification.
///
/// Used to identify node types/labels in the graph.
pub type LabelId = u32;

/// Relationship type identifier.
///
/// Used to identify relationship types (e.g., `:KNOWS`, `:1`).
pub type RelTypeId = u32;

/// Property value types for nodes and edges.
///
/// Supports basic and complex types needed for Cypher property expressions:
/// - Null: NULL values
/// - Bool: true/false
/// - Int: 64-bit signed integers
/// - Float: 64-bit floating point
/// - String: UTF-8 strings
/// - DateTime: 64-bit signed microseconds since Unix epoch
/// - Blob: Raw binary data
/// - List: Ordered list of PropertyValues
/// - Map: String-keyed map of PropertyValues
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    DateTime(i64),
    Blob(Vec<u8>),
    List(Vec<PropertyValue>),
    Map(BTreeMap<String, PropertyValue>),
}

/// A directed edge from a source node to a destination node with a relationship type.
///
/// Used as the key type for neighbor lookups and edge operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeKey {
    pub src: InternalNodeId,
    pub rel: RelTypeId,
    pub dst: InternalNodeId,
}

/// Provides access to a snapshot of the graph at a point in time.
///
/// Implementors must ensure that the returned snapshot is immutable and
/// reflects a consistent state of the graph.
pub trait GraphStore {
    type Snapshot: GraphSnapshot;

    /// Creates a snapshot of the current graph state.
    ///
    /// The snapshot is independent of any writes that occur after creation.
    fn snapshot(&self) -> Self::Snapshot;
}

/// A read-only snapshot of the graph state.
///
/// Snapshots are immutable and provide consistent views of the graph
/// at the time of creation. Multiple snapshots can coexist.
pub trait GraphSnapshot {
    /// Iterator type for neighbors of a node.
    type Neighbors<'a>: Iterator<Item = EdgeKey> + 'a
    where
        Self: 'a;

    /// Get outgoing neighbors of a node, optionally filtered by relationship type.
    ///
    /// Returns an iterator over `EdgeKey`s representing outgoing edges.
    /// If `rel` is `Some`, only edges of that type are returned.
    /// If `rel` is `None`, all outgoing edges are returned.
    fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> Self::Neighbors<'_>;

    /// Get an iterator over all non-tombstoned nodes.
    ///
    /// Returns an iterator over all internal node IDs that are not tombstoned.
    /// The default implementation returns an empty iterator.
    fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        Box::new(std::iter::empty())
    }

    /// Lookup nodes using an index.
    ///
    /// Returns `Some(Vec<InternalNodeId>)` if the index exists and the lookup succeeds.
    /// Returns `None` if the index does not exist.
    ///
    /// # Arguments
    /// * `label` - The label name (e.g., "Person")
    /// * `field` - The property field name (e.g., "name")
    /// * `value` - The value to match
    fn lookup_index(
        &self,
        _label: &str,
        _field: &str,
        _value: &PropertyValue,
    ) -> Option<Vec<InternalNodeId>> {
        None
    }

    /// Resolve an internal node ID to its external ID.
    ///
    /// Returns `Some(external_id)` if the node exists and has an external ID,
    /// or `None` if the node doesn't exist or has no external ID.
    fn resolve_external(&self, _iid: InternalNodeId) -> Option<ExternalId> {
        None
    }

    /// Get the label ID for a node.
    ///
    /// Returns `Some(label_id)` if the node exists, `None` otherwise.
    fn node_label(&self, _iid: InternalNodeId) -> Option<LabelId> {
        None
    }

    /// Check if a node is tombstoned (soft-deleted).
    ///
    /// Tombstoned nodes are not returned by `neighbors()` or `nodes()`.
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
    /// Get all edge properties merged from all runs (newest takes precedence).
    fn edge_properties(&self, _edge: EdgeKey) -> Option<BTreeMap<String, PropertyValue>> {
        None
    }

    /// Resolve a label name to its ID.
    fn resolve_label_id(&self, _name: &str) -> Option<LabelId> {
        None
    }

    /// Resolve a relationship type name to its ID.
    fn resolve_rel_type_id(&self, _name: &str) -> Option<RelTypeId> {
        None
    }

    /// Resolve a label ID to its name.
    fn resolve_label_name(&self, _id: LabelId) -> Option<String> {
        None
    }

    /// Resolve a relationship type ID to its name.
    fn resolve_rel_type_name(&self, _id: RelTypeId) -> Option<String> {
        None
    }

    /// Get the estimated number of nodes, optionally filtered by label.
    fn node_count(&self, _label: Option<LabelId>) -> u64 {
        0
    }

    /// Get the estimated number of edges, optionally filtered by relationship type.
    fn edge_count(&self, _rel: Option<RelTypeId>) -> u64 {
        0
    }
}

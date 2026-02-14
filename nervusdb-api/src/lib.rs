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

impl From<&str> for PropertyValue {
    fn from(s: &str) -> Self {
        PropertyValue::String(s.to_string())
    }
}

impl From<String> for PropertyValue {
    fn from(s: String) -> Self {
        PropertyValue::String(s)
    }
}

impl From<i64> for PropertyValue {
    fn from(i: i64) -> Self {
        PropertyValue::Int(i)
    }
}

impl From<f64> for PropertyValue {
    fn from(f: f64) -> Self {
        PropertyValue::Float(f)
    }
}

impl From<bool> for PropertyValue {
    fn from(b: bool) -> Self {
        PropertyValue::Bool(b)
    }
}

impl PropertyValue {
    /// Encode property value to bytes for WAL/property-store persistence.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            PropertyValue::Null => vec![0],
            PropertyValue::Bool(b) => {
                let mut out = vec![1];
                out.push(if *b { 1 } else { 0 });
                out
            }
            PropertyValue::Int(i) => {
                let mut out = vec![2];
                out.extend_from_slice(&i.to_le_bytes());
                out
            }
            PropertyValue::Float(f) => {
                let mut out = vec![3];
                out.extend_from_slice(&f.to_le_bytes());
                out
            }
            PropertyValue::String(s) => {
                let mut out = vec![4];
                let bytes = s.as_bytes();
                let len = u32::try_from(bytes.len()).expect("string length should fit in u32");
                out.extend_from_slice(&len.to_le_bytes());
                out.extend_from_slice(bytes);
                out
            }
            PropertyValue::DateTime(i) => {
                let mut out = vec![5];
                out.extend_from_slice(&i.to_le_bytes());
                out
            }
            PropertyValue::Blob(b) => {
                let mut out = vec![6];
                let len = u32::try_from(b.len()).expect("blob length should fit in u32");
                out.extend_from_slice(&len.to_le_bytes());
                out.extend_from_slice(b);
                out
            }
            PropertyValue::List(l) => {
                let mut out = vec![7];
                let len = u32::try_from(l.len()).expect("list length should fit in u32");
                out.extend_from_slice(&len.to_le_bytes());
                for item in l {
                    out.extend_from_slice(&item.encode());
                }
                out
            }
            PropertyValue::Map(m) => {
                let mut out = vec![8];
                let len = u32::try_from(m.len()).expect("map length should fit in u32");
                out.extend_from_slice(&len.to_le_bytes());
                for (k, v) in m {
                    let k_bytes = k.as_bytes();
                    let k_len = u32::try_from(k_bytes.len()).expect("key length should fit in u32");
                    out.extend_from_slice(&k_len.to_le_bytes());
                    out.extend_from_slice(k_bytes);
                    out.extend_from_slice(&v.encode());
                }
                out
            }
        }
    }

    /// Decode property value from bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let (value, _) = Self::decode_recursive(bytes)?;
        Ok(value)
    }

    fn decode_recursive(bytes: &[u8]) -> Result<(Self, usize), DecodeError> {
        if bytes.is_empty() {
            return Err(DecodeError::Empty);
        }
        let ty = bytes[0];
        match ty {
            0 => Ok((PropertyValue::Null, 1)),
            1 => {
                if bytes.len() < 2 {
                    return Err(DecodeError::InvalidLength);
                }
                Ok((PropertyValue::Bool(bytes[1] != 0), 2))
            }
            2 => {
                if bytes.len() < 9 {
                    return Err(DecodeError::InvalidLength);
                }
                let i = i64::from_le_bytes(bytes[1..9].try_into().expect("slice length checked"));
                Ok((PropertyValue::Int(i), 9))
            }
            3 => {
                if bytes.len() < 9 {
                    return Err(DecodeError::InvalidLength);
                }
                let f = f64::from_le_bytes(bytes[1..9].try_into().expect("slice length checked"));
                Ok((PropertyValue::Float(f), 9))
            }
            4 => {
                if bytes.len() < 5 {
                    return Err(DecodeError::InvalidLength);
                }
                let len = u32::from_le_bytes(bytes[1..5].try_into().expect("slice length checked"))
                    as usize;
                if bytes.len() < 5 + len {
                    return Err(DecodeError::InvalidLength);
                }
                let s = String::from_utf8(bytes[5..5 + len].to_vec())
                    .map_err(|_| DecodeError::InvalidUtf8)?;
                Ok((PropertyValue::String(s), 5 + len))
            }
            5 => {
                if bytes.len() < 9 {
                    return Err(DecodeError::InvalidLength);
                }
                let i = i64::from_le_bytes(bytes[1..9].try_into().expect("slice length checked"));
                Ok((PropertyValue::DateTime(i), 9))
            }
            6 => {
                if bytes.len() < 5 {
                    return Err(DecodeError::InvalidLength);
                }
                let len = u32::from_le_bytes(bytes[1..5].try_into().expect("slice length checked"))
                    as usize;
                if bytes.len() < 5 + len {
                    return Err(DecodeError::InvalidLength);
                }
                Ok((PropertyValue::Blob(bytes[5..5 + len].to_vec()), 5 + len))
            }
            7 => {
                if bytes.len() < 5 {
                    return Err(DecodeError::InvalidLength);
                }
                let count =
                    u32::from_le_bytes(bytes[1..5].try_into().expect("slice length checked"))
                        as usize;
                let mut pos = 5;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    let (item, consumed) = Self::decode_recursive(&bytes[pos..])?;
                    items.push(item);
                    pos += consumed;
                }
                Ok((PropertyValue::List(items), pos))
            }
            8 => {
                if bytes.len() < 5 {
                    return Err(DecodeError::InvalidLength);
                }
                let count =
                    u32::from_le_bytes(bytes[1..5].try_into().expect("slice length checked"))
                        as usize;
                let mut pos = 5;
                let mut map = BTreeMap::new();
                for _ in 0..count {
                    if bytes.len() < pos + 4 {
                        return Err(DecodeError::InvalidLength);
                    }
                    let k_len = u32::from_le_bytes(
                        bytes[pos..pos + 4]
                            .try_into()
                            .expect("slice length checked"),
                    ) as usize;
                    pos += 4;
                    if bytes.len() < pos + k_len {
                        return Err(DecodeError::InvalidLength);
                    }
                    let key = String::from_utf8(bytes[pos..pos + k_len].to_vec())
                        .map_err(|_| DecodeError::InvalidUtf8)?;
                    pos += k_len;
                    let (val, consumed) = Self::decode_recursive(&bytes[pos..])?;
                    map.insert(key, val);
                    pos += consumed;
                }
                Ok((PropertyValue::Map(map), pos))
            }
            _ => Err(DecodeError::UnknownType(ty)),
        }
    }

    /// Returns float value if this variant is `Float`.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            PropertyValue::Float(f) => Some(*f),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum DecodeError {
    Empty,
    InvalidLength,
    InvalidUtf8,
    UnknownType(u8),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Empty => write!(f, "empty property value bytes"),
            DecodeError::InvalidLength => write!(f, "invalid property value length"),
            DecodeError::InvalidUtf8 => write!(f, "invalid UTF-8 in string property"),
            DecodeError::UnknownType(ty) => write!(f, "unknown property value type: {ty}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// A directed edge from a source node to a destination node with a relationship type.
///
/// Used as the key type for neighbor lookups and edge operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
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

    /// Get incoming neighbors of a node, optionally filtered by relationship type.
    ///
    /// Returns an iterator over `EdgeKey`s representing incoming edges.
    /// If `rel` is `Some`, only edges of that type are returned.
    /// If `rel` is `None`, all incoming edges are returned.
    fn incoming_neighbors(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Self::Neighbors<'_>;

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

    /// Get all label IDs for a node.
    fn resolve_node_labels(&self, _iid: InternalNodeId) -> Option<Vec<LabelId>> {
        self.node_label(_iid).map(|l| vec![l])
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

#[cfg(test)]
mod tests {
    use super::PropertyValue;
    use std::collections::BTreeMap;

    #[test]
    fn property_value_roundtrip_nested_structures() {
        let value = PropertyValue::Map(BTreeMap::from([(
            "k".to_string(),
            PropertyValue::List(vec![
                PropertyValue::Int(7),
                PropertyValue::Bool(true),
                PropertyValue::Null,
            ]),
        )]));

        let encoded = value.encode();
        let decoded = PropertyValue::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded, value);
    }

    #[test]
    fn property_value_decode_rejects_unknown_type_tag() {
        let err = PropertyValue::decode(&[255]).expect_err("unknown type tag should fail");
        assert_eq!(err.to_string(), "unknown property value type: 255");
    }

    #[test]
    fn property_value_as_float_only_for_float_variant() {
        assert_eq!(PropertyValue::Float(3.5).as_float(), Some(3.5));
        assert_eq!(PropertyValue::Int(3).as_float(), None);
    }
}

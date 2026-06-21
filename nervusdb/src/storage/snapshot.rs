use crate::api::{
    EdgeKey, ExternalId, GraphSnapshot, InternalNodeId, LabelId, PropertyValue, RelTypeId,
};
use crate::storage::engine::{
    KEY_FLAG_TOMBSTONE, Keyspaces, decode_node_value, edge_key_from_adj_in, edge_key_from_adj_out,
    edge_prefix, key_u32, node_prop_prefix, parse_iid_key, parse_label_node_key,
    parse_node_prop_key, parse_node_value, parse_prop_value,
};
use fjall::Readable;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Snapshot {
    inner: fjall::Snapshot,
    keyspaces: Keyspaces,
}

pub type StorageSnapshot = Snapshot;

impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Snapshot").finish_non_exhaustive()
    }
}

impl Snapshot {
    pub(crate) fn new(inner: fjall::Snapshot, keyspaces: Keyspaces) -> Self {
        Self { inner, keyspaces }
    }

    fn get(&self, keyspace: &fjall::Keyspace, key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
        self.inner
            .get(keyspace, key)
            .ok()
            .flatten()
            .map(|value| value.as_ref().to_vec())
    }

    pub(crate) fn node_is_live(&self, iid: InternalNodeId) -> bool {
        self.get(&self.keyspaces.nodes, key_u32(iid))
            .and_then(|value| parse_node_value(&value))
            .is_some_and(|(_, flags)| flags & KEY_FLAG_TOMBSTONE == 0)
    }

    fn collect_prefix_keys(&self, keyspace: &fjall::Keyspace, prefix: Vec<u8>) -> Vec<Vec<u8>> {
        self.inner
            .prefix(keyspace, prefix)
            .filter_map(|guard| guard.key().ok().map(|key| key.as_ref().to_vec()))
            .collect()
    }

    pub fn neighbors(
        &self,
        src: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> impl Iterator<Item = EdgeKey> + '_ {
        let mut prefix = key_u32(src);
        if let Some(rel) = rel {
            prefix.extend_from_slice(&rel.to_be_bytes());
        }

        self.collect_prefix_keys(&self.keyspaces.adj_out, prefix)
            .into_iter()
            .filter_map(|key| edge_key_from_adj_out(&key))
            .filter(|edge| self.node_is_live(edge.src) && self.node_is_live(edge.dst))
    }

    pub fn incoming_neighbors(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> impl Iterator<Item = EdgeKey> + '_ {
        let mut prefix = key_u32(dst);
        if let Some(rel) = rel {
            prefix.extend_from_slice(&rel.to_be_bytes());
        }

        self.collect_prefix_keys(&self.keyspaces.adj_in, prefix)
            .into_iter()
            .filter_map(|key| edge_key_from_adj_in(&key))
            .filter(|edge| self.node_is_live(edge.src) && self.node_is_live(edge.dst))
    }

    pub fn resolve_label_id(&self, name: &str) -> Option<LabelId> {
        self.get(&self.keyspaces.labels, name_key(name))
            .and_then(|v| decode_u32(&v))
    }

    pub fn resolve_rel_type_id(&self, name: &str) -> Option<RelTypeId> {
        self.get(&self.keyspaces.reltypes, name_key(name))
            .and_then(|v| decode_u32(&v))
    }

    pub fn resolve_label_name(&self, id: LabelId) -> Option<String> {
        self.get(&self.keyspaces.labels, id_key(id))
            .and_then(|v| String::from_utf8(v).ok())
    }

    pub fn resolve_rel_type_name(&self, id: RelTypeId) -> Option<String> {
        self.get(&self.keyspaces.reltypes, id_key(id))
            .and_then(|v| String::from_utf8(v).ok())
    }

    pub fn node_label(&self, iid: InternalNodeId) -> Option<LabelId> {
        self.node_labels(iid).into_iter().next()
    }

    pub fn node_labels(&self, iid: InternalNodeId) -> Vec<LabelId> {
        self.collect_prefix_keys(&self.keyspaces.node_labels, key_u32(iid))
            .into_iter()
            .filter_map(|key| {
                if key.len() == 8 {
                    decode_u32(&key[4..8])
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn node_property(&self, node: InternalNodeId, key: &str) -> Option<PropertyValue> {
        self.get(&self.keyspaces.node_props, node_prop_prefix(node, key))
            .and_then(|value| parse_prop_value(&value).ok())
    }

    pub fn edge_property(&self, edge: EdgeKey, key: &str) -> Option<PropertyValue> {
        let mut storage_key = edge_prefix(edge);
        storage_key.extend_from_slice(&(key.len() as u32).to_be_bytes());
        storage_key.extend_from_slice(key.as_bytes());
        self.get(&self.keyspaces.edge_props, storage_key)
            .and_then(|value| parse_prop_value(&value).ok())
    }

    pub(crate) fn edge_is_live(&self, edge: EdgeKey) -> bool {
        self.node_is_live(edge.src)
            && self.node_is_live(edge.dst)
            && self
                .get(&self.keyspaces.adj_out, edge_prefix(edge))
                .is_some()
    }

    pub fn node_properties(&self, iid: InternalNodeId) -> Option<BTreeMap<String, PropertyValue>> {
        let mut props = BTreeMap::new();
        for guard in self.inner.prefix(&self.keyspaces.node_props, key_u32(iid)) {
            let Ok((key, value)) = guard.into_inner() else {
                continue;
            };
            let Some(prop_key) = parse_node_prop_key(key.as_ref(), iid) else {
                continue;
            };
            let Ok(prop_value) = parse_prop_value(value.as_ref()) else {
                continue;
            };
            props.insert(prop_key, prop_value);
        }

        if props.is_empty() { None } else { Some(props) }
    }

    pub fn edge_properties(&self, edge: EdgeKey) -> Option<BTreeMap<String, PropertyValue>> {
        let mut props = BTreeMap::new();
        let prefix = edge_prefix(edge);
        for guard in self.inner.prefix(&self.keyspaces.edge_props, prefix) {
            let Ok((key, value)) = guard.into_inner() else {
                continue;
            };
            let Some(prop_key) = parse_edge_prop_key(key.as_ref()) else {
                continue;
            };
            let Ok(prop_value) = parse_prop_value(value.as_ref()) else {
                continue;
            };
            props.insert(prop_key, prop_value);
        }

        if props.is_empty() { None } else { Some(props) }
    }

    pub(crate) fn collect_edge_property_keys(&self, edge: EdgeKey) -> Vec<Vec<u8>> {
        self.collect_prefix_keys(&self.keyspaces.edge_props, edge_prefix(edge))
    }

    pub(crate) fn collect_node_property_keys(&self, node: InternalNodeId) -> Vec<Vec<u8>> {
        self.collect_prefix_keys(&self.keyspaces.node_props, key_u32(node))
    }

    pub(crate) fn collect_raw_outgoing_edges(&self, node: InternalNodeId) -> Vec<EdgeKey> {
        self.collect_prefix_keys(&self.keyspaces.adj_out, key_u32(node))
            .into_iter()
            .filter_map(|key| edge_key_from_adj_out(&key))
            .collect()
    }

    pub(crate) fn collect_raw_incoming_edges(&self, node: InternalNodeId) -> Vec<EdgeKey> {
        self.collect_prefix_keys(&self.keyspaces.adj_in, key_u32(node))
            .into_iter()
            .filter_map(|key| edge_key_from_adj_in(&key))
            .collect()
    }

    pub fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        let nodes: Vec<_> = self
            .inner
            .iter(&self.keyspaces.nodes)
            .filter_map(|guard| guard.key().ok())
            .filter_map(|key| parse_iid_key(key.as_ref()))
            .filter(|iid| self.node_is_live(*iid))
            .collect();
        Box::new(nodes.into_iter())
    }

    pub fn is_tombstoned_node(&self, iid: InternalNodeId) -> bool {
        self.get(&self.keyspaces.nodes, key_u32(iid))
            .and_then(|value| parse_node_value(&value))
            .is_some_and(|(_, flags)| flags & KEY_FLAG_TOMBSTONE != 0)
    }
}

impl GraphSnapshot for Snapshot {
    type Neighbors<'a>
        = Box<dyn Iterator<Item = EdgeKey> + 'a>
    where
        Self: 'a;

    fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> Self::Neighbors<'_> {
        Box::new(self.neighbors(src, rel))
    }

    fn incoming_neighbors(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Self::Neighbors<'_> {
        Box::new(self.incoming_neighbors(dst, rel))
    }

    fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        self.nodes()
    }

    fn nodes_with_label(&self, label: LabelId) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        let nodes: Vec<_> = self
            .collect_prefix_keys(&self.keyspaces.label_nodes, key_u32(label))
            .into_iter()
            .filter_map(|key| parse_label_node_key(&key))
            .filter(|iid| self.node_is_live(*iid))
            .collect();
        Box::new(nodes.into_iter())
    }

    fn resolve_external(&self, iid: InternalNodeId) -> Option<ExternalId> {
        if !self.node_is_live(iid) {
            return None;
        }
        self.get(&self.keyspaces.nodes, key_u32(iid))
            .and_then(|value| decode_node_value(&value).map(|(external, _)| external))
    }

    fn node_label(&self, iid: InternalNodeId) -> Option<LabelId> {
        self.node_label(iid)
    }

    fn resolve_node_labels(&self, iid: InternalNodeId) -> Option<Vec<LabelId>> {
        let labels = self.node_labels(iid);
        if labels.is_empty() {
            None
        } else {
            Some(labels)
        }
    }

    fn is_tombstoned_node(&self, iid: InternalNodeId) -> bool {
        self.is_tombstoned_node(iid)
    }

    fn node_property(&self, iid: InternalNodeId, key: &str) -> Option<PropertyValue> {
        self.node_property(iid, key)
    }

    fn edge_property(&self, edge: EdgeKey, key: &str) -> Option<PropertyValue> {
        self.edge_property(edge, key)
    }

    fn node_properties(&self, iid: InternalNodeId) -> Option<BTreeMap<String, PropertyValue>> {
        self.node_properties(iid)
    }

    fn edge_properties(&self, edge: EdgeKey) -> Option<BTreeMap<String, PropertyValue>> {
        self.edge_properties(edge)
    }

    fn resolve_label_id(&self, name: &str) -> Option<LabelId> {
        self.resolve_label_id(name)
    }

    fn resolve_rel_type_id(&self, name: &str) -> Option<RelTypeId> {
        self.resolve_rel_type_id(name)
    }

    fn resolve_label_name(&self, id: LabelId) -> Option<String> {
        self.resolve_label_name(id)
    }

    fn resolve_rel_type_name(&self, id: RelTypeId) -> Option<String> {
        self.resolve_rel_type_name(id)
    }

    fn node_count(&self, label: Option<LabelId>) -> u64 {
        match label {
            Some(label) => self.nodes_with_label(label).count() as u64,
            None => self.nodes().count() as u64,
        }
    }

    fn edge_count(&self, rel: Option<RelTypeId>) -> u64 {
        self.neighbors_for_count(rel).count() as u64
    }
}

impl Snapshot {
    fn neighbors_for_count(&self, rel: Option<RelTypeId>) -> impl Iterator<Item = EdgeKey> + '_ {
        self.inner
            .iter(&self.keyspaces.adj_out)
            .filter_map(|guard| guard.key().ok())
            .filter_map(|key| edge_key_from_adj_out(key.as_ref()))
            .filter(move |edge| rel.map_or(true, |r| edge.rel == r))
            .filter(|edge| self.node_is_live(edge.src) && self.node_is_live(edge.dst))
    }
}

pub(crate) fn name_key(name: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + name.len());
    key.push(b'n');
    key.extend_from_slice(name.as_bytes());
    key
}

pub(crate) fn id_key(id: u32) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + 4);
    key.push(b'i');
    key.extend_from_slice(&id.to_be_bytes());
    key
}

fn decode_u32(bytes: &[u8]) -> Option<u32> {
    let raw: [u8; 4] = bytes.try_into().ok()?;
    Some(u32::from_be_bytes(raw))
}

fn parse_edge_prop_key(key: &[u8]) -> Option<String> {
    if key.len() < 16 {
        return None;
    }
    let raw_len: [u8; 4] = key[12..16].try_into().ok()?;
    let len = u32::from_be_bytes(raw_len) as usize;
    if key.len() != 16 + len {
        return None;
    }
    String::from_utf8(key[16..].to_vec()).ok()
}

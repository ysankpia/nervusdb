use crate::api::{
    EdgeKey, ExternalId, GraphSnapshot, InternalNodeId, LabelId, PropertyValue, RelTypeId,
};
use crate::storage::engine::Keyspaces;
use crate::storage::layout::*;
use crate::storage::profile;
use fjall::Readable;
use std::collections::BTreeMap;
use std::time::Instant;

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
        self.get(&self.keyspaces.graph_data, node_key(iid))
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
        EdgeScan::new(
            "neighbors",
            self.inner
                .prefix(&self.keyspaces.adj_out, adj_out_prefix(src, rel)),
            EdgeScanDirection::Outgoing { src, rel },
        )
    }

    pub fn incoming_neighbors(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> impl Iterator<Item = EdgeKey> + '_ {
        EdgeScan::new(
            "incoming_neighbors",
            self.inner
                .prefix(&self.keyspaces.adj_in, adj_in_prefix(dst, rel)),
            EdgeScanDirection::Incoming { dst, rel },
        )
    }

    pub fn resolve_label_id(&self, name: &str) -> Option<LabelId> {
        self.get(&self.keyspaces.graph_data, label_name_key(name))
            .and_then(|v| decode_u32(&v))
    }

    pub fn resolve_rel_type_id(&self, name: &str) -> Option<RelTypeId> {
        self.get(&self.keyspaces.graph_data, rel_name_key(name))
            .and_then(|v| decode_u32(&v))
    }

    pub fn resolve_label_name(&self, id: LabelId) -> Option<String> {
        self.get(&self.keyspaces.graph_data, label_id_key(id))
            .and_then(|v| String::from_utf8(v).ok())
    }

    pub fn resolve_rel_type_name(&self, id: RelTypeId) -> Option<String> {
        self.get(&self.keyspaces.graph_data, rel_id_key(id))
            .and_then(|v| String::from_utf8(v).ok())
    }

    pub fn node_label(&self, iid: InternalNodeId) -> Option<LabelId> {
        self.node_labels(iid).into_iter().next()
    }

    pub fn node_labels(&self, iid: InternalNodeId) -> Vec<LabelId> {
        self.collect_prefix_keys(&self.keyspaces.graph_data, node_label_prefix(iid))
            .into_iter()
            .filter_map(|key| parse_node_label_key(&key).map(|(_, label)| label))
            .collect()
    }

    pub fn node_property(&self, node: InternalNodeId, key: &str) -> Option<PropertyValue> {
        self.get(&self.keyspaces.graph_data, node_prop_key(node, key))
            .and_then(|value| parse_prop_value(&value).ok())
    }

    pub fn edge_property(&self, edge: EdgeKey, key: &str) -> Option<PropertyValue> {
        self.get(&self.keyspaces.graph_data, edge_prop_key(edge, key))
            .and_then(|value| parse_prop_value(&value).ok())
    }

    pub(crate) fn edge_is_live(&self, edge: EdgeKey) -> bool {
        self.node_is_live(edge.src)
            && self.node_is_live(edge.dst)
            && self
                .get(&self.keyspaces.adj_out, adj_out_key(edge))
                .is_some()
    }

    pub fn node_properties(&self, iid: InternalNodeId) -> Option<BTreeMap<String, PropertyValue>> {
        let mut props = BTreeMap::new();
        for guard in self
            .inner
            .prefix(&self.keyspaces.graph_data, node_prop_prefix(iid))
        {
            let Ok((key, value)) = guard.into_inner() else {
                continue;
            };
            let Some(prop_key) = parse_node_prop_key_for_node(key.as_ref(), iid) else {
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
        let prefix = edge_prop_prefix(edge);
        for guard in self.inner.prefix(&self.keyspaces.graph_data, prefix) {
            let Ok((key, value)) = guard.into_inner() else {
                continue;
            };
            let Some(prop_key) = parse_edge_prop_key_for_edge(key.as_ref(), edge) else {
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
        self.collect_prefix_keys(&self.keyspaces.graph_data, edge_prop_prefix(edge))
    }

    pub(crate) fn collect_node_property_keys(&self, node: InternalNodeId) -> Vec<Vec<u8>> {
        self.collect_prefix_keys(&self.keyspaces.graph_data, node_prop_prefix(node))
    }

    pub(crate) fn collect_raw_outgoing_edges(&self, node: InternalNodeId) -> Vec<EdgeKey> {
        self.inner
            .prefix(&self.keyspaces.adj_out, adj_out_prefix(node, None))
            .filter_map(|guard| guard.key().ok())
            .filter_map(|key| edge_key_from_adj_out(key.as_ref()))
            .collect()
    }

    pub(crate) fn collect_raw_incoming_edges(&self, node: InternalNodeId) -> Vec<EdgeKey> {
        self.inner
            .prefix(&self.keyspaces.adj_in, adj_in_prefix(node, None))
            .filter_map(|guard| guard.key().ok())
            .filter_map(|key| edge_key_from_adj_in(key.as_ref()))
            .collect()
    }

    pub fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        Box::new(
            self.inner
                .prefix(&self.keyspaces.graph_data, node_scan_prefix())
                .filter_map(|guard| guard.key().ok())
                .filter_map(|key| parse_node_key(key.as_ref()))
                .filter(|iid| self.node_is_live(*iid)),
        )
    }

    pub fn is_tombstoned_node(&self, iid: InternalNodeId) -> bool {
        self.get(&self.keyspaces.graph_data, node_key(iid))
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
        Box::new(
            self.inner
                .prefix(&self.keyspaces.graph_data, label_node_prefix(label))
                .filter_map(|guard| guard.key().ok())
                .filter_map(|key| parse_label_node_key(key.as_ref()).map(|(_, node)| node))
                .filter(|iid| self.node_is_live(*iid)),
        )
    }

    fn nodes_with_label_and_property(
        &self,
        label: LabelId,
        key: &str,
        value: &PropertyValue,
    ) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        Box::new(
            self.inner
                .prefix(
                    &self.keyspaces.graph_data,
                    node_prop_index_prefix(label, key, value),
                )
                .filter_map(|guard| guard.key().ok())
                .filter_map(|key| parse_node_prop_index_node(key.as_ref()))
                .filter(|iid| self.node_is_live(*iid)),
        )
    }

    fn resolve_external(&self, iid: InternalNodeId) -> Option<ExternalId> {
        if !self.node_is_live(iid) {
            return None;
        }
        self.get(&self.keyspaces.graph_data, node_key(iid))
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
        let started = profile::start();
        let count = match label {
            Some(label) => self.nodes_with_label(label).count() as u64,
            None => self.nodes().count() as u64,
        };
        profile::event_since(
            "node_count",
            started,
            &[
                ("label_filter", u64::from(label.is_some())),
                ("count", count),
            ],
        );
        count
    }

    fn edge_count(&self, rel: Option<RelTypeId>) -> u64 {
        let started = profile::start();
        let count = self.neighbors_for_count(rel).count() as u64;
        profile::event_since(
            "edge_count",
            started,
            &[("rel_filter", u64::from(rel.is_some())), ("count", count)],
        );
        count
    }
}

impl Snapshot {
    fn neighbors_for_count(&self, rel: Option<RelTypeId>) -> impl Iterator<Item = EdgeKey> + '_ {
        self.inner
            .prefix(&self.keyspaces.adj_out, adj_out_scan_prefix())
            .filter_map(|guard| guard.key().ok())
            .filter_map(|key| edge_key_from_adj_out(key.as_ref()))
            .filter(move |edge| rel.map_or(true, |r| edge.rel == r))
    }
}

enum EdgeScanDirection {
    Outgoing {
        src: InternalNodeId,
        rel: Option<RelTypeId>,
    },
    Incoming {
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    },
}

struct EdgeScan {
    stage: &'static str,
    iter: fjall::Iter,
    direction: EdgeScanDirection,
    started: Option<Instant>,
    scanned: u64,
    decoded: u64,
    live: u64,
}

impl EdgeScan {
    fn new(stage: &'static str, iter: fjall::Iter, direction: EdgeScanDirection) -> Self {
        Self {
            stage,
            iter,
            direction,
            started: profile::start(),
            scanned: 0,
            decoded: 0,
            live: 0,
        }
    }
}

impl Iterator for EdgeScan {
    type Item = EdgeKey;

    fn next(&mut self) -> Option<Self::Item> {
        for guard in self.iter.by_ref() {
            if self.started.is_some() {
                self.scanned += 1;
            }
            let Ok(key) = guard.key() else {
                continue;
            };
            let edge = match self.direction {
                EdgeScanDirection::Outgoing {
                    src,
                    rel: Some(rel),
                } => dst_from_adj_out_key(key.as_ref()).map(|dst| EdgeKey { src, rel, dst }),
                EdgeScanDirection::Incoming {
                    dst,
                    rel: Some(rel),
                } => src_from_adj_in_key(key.as_ref()).map(|src| EdgeKey { src, rel, dst }),
                EdgeScanDirection::Outgoing { rel: None, .. } => {
                    edge_key_from_adj_out(key.as_ref())
                }
                EdgeScanDirection::Incoming { rel: None, .. } => edge_key_from_adj_in(key.as_ref()),
            };
            let Some(edge) = edge else {
                continue;
            };
            if self.started.is_some() {
                self.decoded += 1;
            }
            if self.started.is_some() {
                self.live += 1;
            }
            return Some(edge);
        }
        None
    }
}

impl Drop for EdgeScan {
    fn drop(&mut self) {
        profile::edge_scan(
            self.stage,
            self.started,
            self.scanned,
            self.decoded,
            self.live,
        );
    }
}

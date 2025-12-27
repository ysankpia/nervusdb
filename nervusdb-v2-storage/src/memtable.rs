use crate::idmap::InternalNodeId;
use crate::property::PropertyValue;
use crate::snapshot::{EdgeKey, L0Run, RelTypeId};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Default)]
pub struct MemTable {
    out: HashMap<InternalNodeId, BTreeSet<EdgeKey>>,
    tombstoned_nodes: BTreeSet<InternalNodeId>,
    tombstoned_edges: BTreeSet<EdgeKey>,
    // Node properties: node_id -> { key -> value }
    node_properties: HashMap<InternalNodeId, HashMap<String, PropertyValue>>,
    // Edge properties: edge_key -> { key -> value }
    edge_properties: HashMap<EdgeKey, HashMap<String, PropertyValue>>,
}

impl MemTable {
    pub fn create_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        let key = EdgeKey { src, rel, dst };
        self.tombstoned_edges.remove(&key);
        self.out.entry(src).or_default().insert(key);
    }

    pub fn tombstone_node(&mut self, node: InternalNodeId) {
        self.tombstoned_nodes.insert(node);
    }

    pub fn tombstone_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        let key = EdgeKey { src, rel, dst };
        if let Some(set) = self.out.get_mut(&src) {
            set.remove(&key);
            if set.is_empty() {
                self.out.remove(&src);
            }
        }
        self.tombstoned_edges.insert(key);
    }

    pub fn set_node_property(&mut self, node: InternalNodeId, key: String, value: PropertyValue) {
        self.node_properties
            .entry(node)
            .or_default()
            .insert(key, value);
    }

    pub fn remove_node_property(&mut self, node: InternalNodeId, key: &str) {
        if let Some(props) = self.node_properties.get_mut(&node) {
            props.remove(key);
            if props.is_empty() {
                self.node_properties.remove(&node);
            }
        }
    }

    pub fn set_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) {
        let edge = EdgeKey { src, rel, dst };
        self.edge_properties
            .entry(edge)
            .or_default()
            .insert(key, value);
    }

    pub fn remove_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: &str,
    ) {
        let edge = EdgeKey { src, rel, dst };
        if let Some(props) = self.edge_properties.get_mut(&edge) {
            props.remove(key);
            if props.is_empty() {
                self.edge_properties.remove(&edge);
            }
        }
    }

    /// Get all node properties for WAL writing.
    pub fn node_properties_for_wal(&self) -> Vec<(InternalNodeId, String, PropertyValue)> {
        self.node_properties
            .iter()
            .flat_map(|(node, props)| {
                props
                    .iter()
                    .map(move |(key, value)| (*node, key.clone(), value.clone()))
            })
            .collect()
    }

    /// Get all edge properties for WAL writing.
    pub fn edge_properties_for_wal(
        &self,
    ) -> Vec<(
        InternalNodeId,
        RelTypeId,
        InternalNodeId,
        String,
        PropertyValue,
    )> {
        self.edge_properties
            .iter()
            .flat_map(|(edge, props)| {
                props.iter().map(move |(key, value)| {
                    (edge.src, edge.rel, edge.dst, key.clone(), value.clone())
                })
            })
            .collect()
    }

    pub fn freeze_into_run(self, txid: u64) -> L0Run {
        let mut edges_by_src: BTreeMap<InternalNodeId, Vec<EdgeKey>> = BTreeMap::new();
        for (src, edges) in self.out {
            edges_by_src.insert(src, edges.into_iter().collect());
        }

        // Convert HashMap to BTreeMap for L0Run
        let node_properties: BTreeMap<_, _> = self
            .node_properties
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect();
        let edge_properties: BTreeMap<_, _> = self
            .edge_properties
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect();

        L0Run::new(
            txid,
            edges_by_src,
            self.tombstoned_nodes,
            self.tombstoned_edges,
            node_properties,
            edge_properties,
        )
    }
}

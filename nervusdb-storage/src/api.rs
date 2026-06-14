use crate::engine::GraphEngine;
use crate::idmap::I2eRecord;
use crate::index::btree::BTree;
use crate::index::catalog::IndexDef;
use crate::index::ordered_key::encode_ordered_value;
use crate::pager::Pager;
use crate::read_path_api_stats::{edge_count_from_stats, node_count_from_stats};
use crate::read_path_property_store::{
    extend_edge_properties_from_store, extend_node_properties_from_store,
    read_edge_property_from_store, read_node_property_from_store,
};
use crate::snapshot;
use nervusdb_api::{
    EdgeKey, ExternalId, GraphSnapshot, GraphStore, InternalNodeId, LabelId, PropertyValue,
    RelTypeId,
};
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug)]
pub struct StorageSnapshot {
    inner: snapshot::Snapshot,
    i2e: Arc<Vec<I2eRecord>>,
    tombstoned_nodes: Arc<HashSet<InternalNodeId>>,
    pager: Arc<RwLock<Pager>>,
    index_entries: Arc<BTreeMap<String, IndexDef>>,
    stats_cache: Mutex<Option<crate::stats::GraphStatistics>>,
}

impl StorageSnapshot {
    fn cached_stats_clone(&self) -> Option<crate::stats::GraphStatistics> {
        let mut cache = self.stats_cache.lock().unwrap();
        if cache.is_none() {
            let pager = self.pager.read().unwrap();
            if let Ok(stats) = self.inner.get_statistics(&pager) {
                *cache = Some(stats);
            }
        }
        cache.clone()
    }
}

impl GraphStore for GraphEngine {
    type Snapshot = StorageSnapshot;

    fn snapshot(&self) -> Self::Snapshot {
        let i2e = self.get_published_i2e();
        let inner = self.begin_read();
        StorageSnapshot {
            inner,
            i2e,
            tombstoned_nodes: self.get_published_tombstoned_nodes(),
            pager: self.get_pager(),
            index_entries: self.get_published_index_entries(),
            stats_cache: Mutex::new(None),
        }
    }
}

impl GraphSnapshot for StorageSnapshot {
    type Neighbors<'a>
        = Box<dyn Iterator<Item = EdgeKey> + 'a>
    where
        Self: 'a;

    fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> Self::Neighbors<'_> {
        Box::new(self.inner.neighbors(src, rel))
    }

    fn incoming_neighbors(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Self::Neighbors<'_> {
        Box::new(self.inner.incoming_neighbors(dst, rel))
    }

    fn lookup_index(
        &self,
        label: &str,
        field: &str,
        value: &PropertyValue,
    ) -> Option<Vec<InternalNodeId>> {
        // MVP Convention: Index name = "Label.Property"
        let index_name = format!("{}.{}", label, field);

        let def = self.index_entries.get(&index_name)?.clone();
        let tree = BTree::load(def.root);

        // Construct prefix: [index_id (4B)] [encoded_value]
        let mut prefix = Vec::new();
        prefix.extend_from_slice(&def.id.to_be_bytes());
        prefix.extend_from_slice(&encode_ordered_value(value));

        let pager = self.pager.read().unwrap();
        let mut cursor = tree.cursor_lower_bound(&pager, &prefix).ok()?;

        let mut results = Vec::new();
        while let Ok(valid) = cursor.is_valid() {
            if !valid {
                break;
            }
            let key = cursor.key().ok()?;
            if !key.starts_with(&prefix) {
                break;
            }
            if let Ok(payload) = cursor.payload() {
                results.push(payload as u32);
                if !cursor.advance().ok()? {
                    break;
                }
            }
        }

        if results.is_empty() {
            None
        } else {
            Some(results)
        }
    }

    fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        let len = self.i2e.len();
        let tombstoned = self.tombstoned_nodes.clone();
        Box::new((0..len).filter_map(move |iid_usize| {
            let iid = u32::try_from(iid_usize).ok()?;
            if tombstoned.contains(&iid) {
                return None;
            }
            Some(iid)
        }))
    }

    fn resolve_external(&self, iid: InternalNodeId) -> Option<ExternalId> {
        let r = self.i2e.get(iid as usize)?;
        if r.external_id == 0 {
            None
        } else {
            Some(r.external_id)
        }
    }

    fn node_label(&self, iid: InternalNodeId) -> Option<LabelId> {
        self.i2e.get(iid as usize).map(|r| r.label_id)
    }

    fn resolve_node_labels(&self, iid: InternalNodeId) -> Option<Vec<LabelId>> {
        self.inner.node_labels(iid)
    }

    fn is_tombstoned_node(&self, iid: InternalNodeId) -> bool {
        self.tombstoned_nodes.contains(&iid)
    }

    fn node_property(&self, iid: InternalNodeId, key: &str) -> Option<PropertyValue> {
        if let Some(v) = self.inner.node_property(iid, key) {
            return Some(v);
        }

        if self.inner.properties_root == 0 {
            return None;
        }

        let pager = self.pager.read().unwrap();
        read_node_property_from_store(&pager, self.inner.properties_root, iid, key)
    }

    fn edge_property(&self, edge: EdgeKey, key: &str) -> Option<PropertyValue> {
        if let Some(v) = self.inner.edge_property(edge, key) {
            return Some(v);
        }

        if self.inner.properties_root == 0 {
            return None;
        }

        let pager = self.pager.read().unwrap();
        read_edge_property_from_store(&pager, self.inner.properties_root, edge, key)
    }

    fn node_properties(&self, iid: InternalNodeId) -> Option<BTreeMap<String, PropertyValue>> {
        let mut props = self.inner.node_properties(iid).unwrap_or_default();

        if self.inner.properties_root != 0 {
            let pager = self.pager.read().unwrap();
            extend_node_properties_from_store(&pager, self.inner.properties_root, iid, &mut props)?;
        }

        if props.is_empty() { None } else { Some(props) }
    }

    fn edge_properties(&self, edge: EdgeKey) -> Option<BTreeMap<String, PropertyValue>> {
        let mut props = self.inner.edge_properties(edge).unwrap_or_default();

        if self.inner.properties_root != 0 {
            let pager = self.pager.read().unwrap();
            extend_edge_properties_from_store(
                &pager,
                self.inner.properties_root,
                edge,
                &mut props,
            )?;
        }

        if props.is_empty() { None } else { Some(props) }
    }

    fn resolve_label_id(&self, name: &str) -> Option<LabelId> {
        self.inner.resolve_label_id(name)
    }

    fn resolve_rel_type_id(&self, name: &str) -> Option<RelTypeId> {
        self.inner.resolve_rel_type_id(name)
    }

    fn resolve_label_name(&self, id: LabelId) -> Option<String> {
        self.inner.resolve_label_name(id)
    }

    fn resolve_rel_type_name(&self, id: RelTypeId) -> Option<String> {
        self.inner.resolve_rel_type_name(id)
    }

    fn node_count(&self, label: Option<LabelId>) -> u64 {
        let stats = self.cached_stats_clone();
        node_count_from_stats(stats.as_ref(), label)
    }

    fn edge_count(&self, rel: Option<RelTypeId>) -> u64 {
        let stats = self.cached_stats_clone();
        edge_count_from_stats(stats.as_ref(), rel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nervusdb_api::{GraphSnapshot, GraphStore};
    use tempfile::tempdir;

    #[test]
    fn snapshot_keeps_old_i2e_view_after_new_commit() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("snapshot-i2e.ndb");
        let wal = dir.path().join("snapshot-i2e.wal");
        let engine = GraphEngine::open(&ndb, &wal).unwrap();

        {
            let mut tx = engine.begin_write();
            tx.create_node(10, 1).unwrap();
            tx.commit().unwrap();
        }

        let snap_before = engine.snapshot();
        assert_eq!(snap_before.resolve_external(0), Some(10));
        assert_eq!(snap_before.resolve_external(1), None);

        {
            let mut tx = engine.begin_write();
            tx.create_node(20, 1).unwrap();
            tx.commit().unwrap();
        }

        let snap_after = engine.snapshot();
        assert_eq!(snap_before.resolve_external(0), Some(10));
        assert_eq!(snap_before.resolve_external(1), None);
        assert_eq!(snap_after.resolve_external(0), Some(10));
        assert_eq!(snap_after.resolve_external(1), Some(20));
    }

    #[test]
    fn snapshot_keeps_old_node_labels_after_label_mutation() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("snapshot-labels.ndb");
        let wal = dir.path().join("snapshot-labels.wal");
        let engine = GraphEngine::open(&ndb, &wal).unwrap();

        let node = {
            let mut tx = engine.begin_write();
            let node = tx.create_node(10, 1).unwrap();
            tx.commit().unwrap();
            node
        };

        let snap_before = engine.snapshot();
        assert_eq!(snap_before.resolve_node_labels(node), Some(vec![1]));

        {
            let mut tx = engine.begin_write();
            tx.add_node_label(node, 5).unwrap();
            tx.commit().unwrap();
        }

        let snap_after = engine.snapshot();
        assert_eq!(snap_before.resolve_node_labels(node), Some(vec![1]));
        assert_eq!(snap_after.resolve_node_labels(node), Some(vec![1, 5]));
    }
}

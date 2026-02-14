use crate::engine::GraphEngine;
use crate::idmap::I2eRecord;
use crate::index::btree::BTree;
use crate::index::catalog::IndexCatalog;
use crate::index::ordered_key::encode_ordered_value;
use crate::pager::Pager;
use crate::read_path_api_stats::{edge_count_from_stats, node_count_from_stats};
use crate::read_path_convert::{
    api_edge_to_internal, convert_property_map_to_api, convert_property_to_api,
    convert_property_to_storage, internal_edge_to_api,
};
use crate::read_path_property_store::{
    extend_edge_properties_from_store, extend_node_properties_from_store,
    read_edge_property_from_store, read_node_property_from_store,
};
use crate::read_path_tombstones::collect_tombstoned_nodes;
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
    index_catalog: Arc<Mutex<IndexCatalog>>,
    stats_cache: Mutex<Option<crate::stats::GraphStatistics>>,
}

impl StorageSnapshot {
    fn ensure_stats_cache_loaded(&self) {
        let mut cache = self.stats_cache.lock().unwrap();
        if cache.is_none() {
            let pager = self.pager.read().unwrap();
            if let Ok(stats) = self.inner.get_statistics(&pager) {
                *cache = Some(stats);
            }
        }
    }

    fn cached_stats_clone(&self) -> Option<crate::stats::GraphStatistics> {
        self.ensure_stats_cache_loaded();
        self.stats_cache.lock().unwrap().clone()
    }
}

impl GraphStore for GraphEngine {
    type Snapshot = StorageSnapshot;

    fn snapshot(&self) -> Self::Snapshot {
        let i2e = Arc::new(self.scan_i2e_records());
        let inner = self.begin_read();
        let tombstoned_nodes: HashSet<InternalNodeId> = collect_tombstoned_nodes(inner.runs());
        StorageSnapshot {
            inner,
            i2e,
            tombstoned_nodes: Arc::new(tombstoned_nodes),
            pager: self.get_pager(),
            index_catalog: self.get_index_catalog(),
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
        Box::new(
            self.inner
                .neighbors(src, rel)
                .map(internal_edge_to_api as fn(snapshot::EdgeKey) -> EdgeKey),
        )
    }

    fn incoming_neighbors(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Self::Neighbors<'_> {
        Box::new(
            self.inner
                .incoming_neighbors(dst, rel)
                .map(internal_edge_to_api as fn(snapshot::EdgeKey) -> EdgeKey),
        )
    }

    fn lookup_index(
        &self,
        label: &str,
        field: &str,
        value: &PropertyValue,
    ) -> Option<Vec<InternalNodeId>> {
        // MVP Convention: Index name = "Label.Property"
        let index_name = format!("{}.{}", label, field);

        let def = {
            let catalog = self.index_catalog.lock().unwrap();
            catalog.get(&index_name)?.clone()
        };
        let tree = BTree::load(def.root);

        // Use storage-level PropertyValue for encoding
        let storage_value = convert_property_to_storage(value.clone());

        // Construct prefix: [index_id (4B)] [encoded_value]
        let mut prefix = Vec::new();
        prefix.extend_from_slice(&def.id.to_be_bytes());
        prefix.extend_from_slice(&encode_ordered_value(&storage_value));

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
            return Some(convert_property_to_api(v));
        }

        if self.inner.properties_root == 0 {
            return None;
        }

        let pager = self.pager.read().unwrap();
        let storage_val =
            read_node_property_from_store(&pager, self.inner.properties_root, iid, key)?;
        Some(convert_property_to_api(storage_val))
    }

    fn edge_property(&self, edge: EdgeKey, key: &str) -> Option<PropertyValue> {
        let snapshot_edge = api_edge_to_internal(edge);
        if let Some(v) = self.inner.edge_property(snapshot_edge, key) {
            return Some(convert_property_to_api(v));
        }

        if self.inner.properties_root == 0 {
            return None;
        }

        let pager = self.pager.read().unwrap();
        let storage_val =
            read_edge_property_from_store(&pager, self.inner.properties_root, edge, key)?;
        Some(convert_property_to_api(storage_val))
    }

    fn node_properties(&self, iid: InternalNodeId) -> Option<BTreeMap<String, PropertyValue>> {
        let mut props = self.inner.node_properties(iid).unwrap_or_default();

        if self.inner.properties_root != 0 {
            let pager = self.pager.read().unwrap();
            extend_node_properties_from_store(&pager, self.inner.properties_root, iid, &mut props)?;
        }

        if props.is_empty() {
            None
        } else {
            Some(convert_property_map_to_api(props))
        }
    }

    fn edge_properties(&self, edge: EdgeKey) -> Option<BTreeMap<String, PropertyValue>> {
        let snapshot_edge = api_edge_to_internal(edge);
        let mut props = self
            .inner
            .edge_properties(snapshot_edge)
            .unwrap_or_default();

        if self.inner.properties_root != 0 {
            let pager = self.pager.read().unwrap();
            extend_edge_properties_from_store(
                &pager,
                self.inner.properties_root,
                edge,
                &mut props,
            )?;
        }

        if props.is_empty() {
            None
        } else {
            Some(convert_property_map_to_api(props))
        }
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

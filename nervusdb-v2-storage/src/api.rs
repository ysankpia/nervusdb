use crate::engine::GraphEngine;
use crate::idmap::I2eRecord;
use crate::index::btree::BTree;
use crate::index::catalog::IndexCatalog;
use crate::index::ordered_key::encode_ordered_value;
use crate::pager::Pager;
use crate::snapshot;
use nervusdb_v2_api::{
    EdgeKey, ExternalId, GraphSnapshot, GraphStore, InternalNodeId, LabelId, PropertyValue,
    RelTypeId,
};
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct StorageSnapshot {
    inner: snapshot::Snapshot,
    i2e: Arc<Vec<I2eRecord>>,
    tombstoned_nodes: Arc<HashSet<InternalNodeId>>,
    pager: Arc<Mutex<Pager>>,
    index_catalog: Arc<Mutex<IndexCatalog>>,
    stats_cache: Mutex<Option<crate::stats::GraphStatistics>>,
}

impl GraphStore for GraphEngine {
    type Snapshot = StorageSnapshot;

    fn snapshot(&self) -> Self::Snapshot {
        let i2e = Arc::new(
            self.scan_i2e_records()
                .expect("scan_i2e_records must succeed after open()"),
        );
        let inner = self.begin_read();
        let tombstoned_nodes: HashSet<InternalNodeId> = inner
            .runs()
            .iter()
            .flat_map(|r| r.iter_tombstoned_nodes())
            .collect();
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
        = std::iter::Map<snapshot::NeighborsIter, fn(snapshot::EdgeKey) -> EdgeKey>
    where
        Self: 'a;

    fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> Self::Neighbors<'_> {
        fn conv(e: snapshot::EdgeKey) -> EdgeKey {
            EdgeKey {
                src: e.src,
                rel: e.rel,
                dst: e.dst,
            }
        }
        self.inner
            .neighbors(src, rel)
            .map(conv as fn(snapshot::EdgeKey) -> EdgeKey)
    }

    fn lookup_index(
        &self,
        label: &str,
        field: &str,
        value: &PropertyValue,
    ) -> Option<Vec<InternalNodeId>> {
        let catalog = self.index_catalog.lock().unwrap();
        // MVP Convention: Index name = "Label.Property"
        let index_name = format!("{}.{}", label, field);

        let def = catalog.get(&index_name)?;
        let tree = BTree::load(def.root);

        // Use storage-level PropertyValue for encoding
        let storage_value = to_storage(value.clone());

        // Construct prefix: [index_id (4B)] [encoded_value]
        let mut prefix = Vec::new();
        prefix.extend_from_slice(&def.id.to_be_bytes());
        prefix.extend_from_slice(&encode_ordered_value(&storage_value));

        let mut pager = self.pager.lock().unwrap();
        let mut cursor = tree.cursor_lower_bound(&mut pager, &prefix).ok()?;

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

    fn is_tombstoned_node(&self, iid: InternalNodeId) -> bool {
        self.tombstoned_nodes.contains(&iid)
    }

    fn node_property(&self, iid: InternalNodeId, key: &str) -> Option<PropertyValue> {
        if let Some(v) = self.inner.node_property(iid, key) {
            return Some(convert_property_value(&v));
        }

        if self.inner.properties_root == 0 {
            return None;
        }

        let mut pager = self.pager.lock().unwrap();
        let tree =
            crate::index::btree::BTree::load(crate::pager::PageId::new(self.inner.properties_root));

        let mut btree_key = Vec::with_capacity(1 + 4 + 4 + key.len());
        btree_key.push(0u8); // Tag 0: Node Property
        btree_key.extend_from_slice(&iid.to_be_bytes());
        btree_key.extend_from_slice(&(key.len() as u32).to_be_bytes());
        btree_key.extend_from_slice(key.as_bytes());

        let blob_id = {
            let mut cursor = tree.cursor_lower_bound(&mut pager, &btree_key).ok()?;
            if cursor.is_valid().ok()? {
                let got_key = cursor.key().ok()?;
                if got_key == btree_key {
                    cursor.payload().ok()
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(blob_id) = blob_id {
            let bytes = crate::blob_store::BlobStore::read(&mut pager, blob_id).ok()?;
            let storage_val = crate::property::PropertyValue::decode(&bytes).ok()?;
            return Some(convert_property_value(&storage_val));
        }
        None
    }

    fn edge_property(&self, edge: EdgeKey, key: &str) -> Option<PropertyValue> {
        let snapshot_edge = snapshot::EdgeKey {
            src: edge.src,
            rel: edge.rel,
            dst: edge.dst,
        };
        if let Some(v) = self.inner.edge_property(snapshot_edge, key) {
            return Some(convert_property_value(&v));
        }

        if self.inner.properties_root == 0 {
            return None;
        }

        let mut pager = self.pager.lock().unwrap();
        let tree =
            crate::index::btree::BTree::load(crate::pager::PageId::new(self.inner.properties_root));

        let mut btree_key = Vec::with_capacity(1 + 4 + 4 + 4 + 4 + key.len());
        btree_key.push(1u8); // Tag 1: Edge Property
        btree_key.extend_from_slice(&edge.src.to_be_bytes());
        btree_key.extend_from_slice(&edge.rel.to_be_bytes());
        btree_key.extend_from_slice(&edge.dst.to_be_bytes());
        btree_key.extend_from_slice(&(key.len() as u32).to_be_bytes());
        btree_key.extend_from_slice(key.as_bytes());

        let blob_id = {
            let mut cursor = tree.cursor_lower_bound(&mut pager, &btree_key).ok()?;
            if cursor.is_valid().ok()? {
                let got_key = cursor.key().ok()?;
                if got_key == btree_key {
                    cursor.payload().ok()
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(blob_id) = blob_id {
            let bytes = crate::blob_store::BlobStore::read(&mut pager, blob_id).ok()?;
            let storage_val = crate::property::PropertyValue::decode(&bytes).ok()?;
            return Some(convert_property_value(&storage_val));
        }
        None
    }

    fn node_properties(&self, iid: InternalNodeId) -> Option<BTreeMap<String, PropertyValue>> {
        let mut props = self.inner.node_properties(iid).unwrap_or_default();

        if self.inner.properties_root != 0 {
            let mut pager = self.pager.lock().unwrap();
            let tree = crate::index::btree::BTree::load(crate::pager::PageId::new(
                self.inner.properties_root,
            ));

            // Prefix search for [tag=0: 1B][node: u32 BE]
            let mut prefix = Vec::with_capacity(5);
            prefix.push(0u8);
            prefix.extend_from_slice(&iid.to_be_bytes());

            let mut to_fetch = Vec::new();
            {
                let mut cursor = tree.cursor_lower_bound(&mut pager, &prefix).ok()?;
                while cursor.is_valid().ok()? {
                    let key = cursor.key().ok()?;
                    if !key.starts_with(&prefix) {
                        break;
                    }

                    // Key format: [tag: 1B][node: 4B][key_len: 4B][key_bytes]
                    if key.len() < 9 {
                        break;
                    }
                    let key_len = u32::from_be_bytes(key[5..9].try_into().unwrap()) as usize;
                    let key_name = String::from_utf8(key[9..9 + key_len].to_vec()).ok()?;

                    if !props.contains_key(&key_name) {
                        to_fetch.push((key_name, cursor.payload().ok()?));
                    }

                    if !cursor.advance().ok()? {
                        break;
                    }
                }
            }

            for (key_name, blob_id) in to_fetch {
                let bytes = crate::blob_store::BlobStore::read(&mut pager, blob_id).ok()?;
                let storage_val = crate::property::PropertyValue::decode(&bytes).ok()?;
                props.insert(key_name, storage_val);
            }
        }

        if props.is_empty() {
            None
        } else {
            Some(
                props
                    .into_iter()
                    .map(|(k, v)| (k, convert_property_value(&v)))
                    .collect(),
            )
        }
    }

    fn edge_properties(&self, edge: EdgeKey) -> Option<BTreeMap<String, PropertyValue>> {
        let snapshot_edge = snapshot::EdgeKey {
            src: edge.src,
            rel: edge.rel,
            dst: edge.dst,
        };
        let mut props = self
            .inner
            .edge_properties(snapshot_edge)
            .unwrap_or_default();

        if self.inner.properties_root != 0 {
            let mut pager = self.pager.lock().unwrap();
            let tree = crate::index::btree::BTree::load(crate::pager::PageId::new(
                self.inner.properties_root,
            ));

            // Prefix search for [tag=1: 1B][src: 4B][rel: 4B][dst: 4B]
            let mut prefix = Vec::with_capacity(13);
            prefix.push(1u8);
            prefix.extend_from_slice(&edge.src.to_be_bytes());
            prefix.extend_from_slice(&edge.rel.to_be_bytes());
            prefix.extend_from_slice(&edge.dst.to_be_bytes());

            let mut to_fetch = Vec::new();
            {
                let mut cursor = tree.cursor_lower_bound(&mut pager, &prefix).ok()?;
                while cursor.is_valid().ok()? {
                    let key = cursor.key().ok()?;
                    if !key.starts_with(&prefix) {
                        break;
                    }

                    // Key format: [tag: 1B][src: 4B][rel: 4B][dst: 4B][key_len: 4B][key_bytes]
                    if key.len() < 17 {
                        break;
                    }
                    let key_len = u32::from_be_bytes(key[13..17].try_into().unwrap()) as usize;
                    let key_name = String::from_utf8(key[17..17 + key_len].to_vec()).ok()?;

                    if !props.contains_key(&key_name) {
                        to_fetch.push((key_name, cursor.payload().ok()?));
                    }

                    if !cursor.advance().ok()? {
                        break;
                    }
                }
            }

            for (key_name, blob_id) in to_fetch {
                let bytes = crate::blob_store::BlobStore::read(&mut pager, blob_id).ok()?;
                let storage_val = crate::property::PropertyValue::decode(&bytes).ok()?;
                props.insert(key_name, storage_val);
            }
        }

        if props.is_empty() {
            None
        } else {
            Some(
                props
                    .into_iter()
                    .map(|(k, v)| (k, convert_property_value(&v)))
                    .collect(),
            )
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
        let mut cache = self.stats_cache.lock().unwrap();
        if cache.is_none() {
            let mut pager = self.pager.lock().unwrap();
            if let Ok(stats) = self.inner.get_statistics(&mut pager) {
                *cache = Some(stats);
            }
        }

        if let Some(stats) = cache.as_ref() {
            if let Some(lid) = label {
                stats.node_counts_by_label.get(&lid).copied().unwrap_or(0)
            } else {
                stats.total_nodes
            }
        } else {
            0
        }
    }

    fn edge_count(&self, rel: Option<RelTypeId>) -> u64 {
        let mut cache = self.stats_cache.lock().unwrap();
        if cache.is_none() {
            let mut pager = self.pager.lock().unwrap();
            if let Ok(stats) = self.inner.get_statistics(&mut pager) {
                *cache = Some(stats);
            }
        }

        if let Some(stats) = cache.as_ref() {
            if let Some(rid) = rel {
                stats.edge_counts_by_type.get(&rid).copied().unwrap_or(0)
            } else {
                stats.total_edges
            }
        } else {
            0
        }
    }
}

fn convert_property_value(v: &crate::property::PropertyValue) -> PropertyValue {
    match v {
        crate::property::PropertyValue::Null => PropertyValue::Null,
        crate::property::PropertyValue::Bool(b) => PropertyValue::Bool(*b),
        crate::property::PropertyValue::Int(i) => PropertyValue::Int(*i),
        crate::property::PropertyValue::Float(f) => PropertyValue::Float(*f),
        crate::property::PropertyValue::String(s) => PropertyValue::String(s.clone()),
        crate::property::PropertyValue::DateTime(i) => PropertyValue::DateTime(*i),
        crate::property::PropertyValue::Blob(b) => PropertyValue::Blob(b.clone()),
        crate::property::PropertyValue::List(l) => {
            PropertyValue::List(l.iter().map(convert_property_value).collect())
        }
        crate::property::PropertyValue::Map(m) => PropertyValue::Map(
            m.iter()
                .map(|(k, v)| (k.clone(), convert_property_value(v)))
                .collect(),
        ),
    }
}

pub(crate) fn to_storage(v: PropertyValue) -> crate::property::PropertyValue {
    match v {
        PropertyValue::Null => crate::property::PropertyValue::Null,
        PropertyValue::Bool(b) => crate::property::PropertyValue::Bool(b),
        PropertyValue::Int(i) => crate::property::PropertyValue::Int(i),
        PropertyValue::Float(f) => crate::property::PropertyValue::Float(f),
        PropertyValue::String(s) => crate::property::PropertyValue::String(s),
        PropertyValue::DateTime(i) => crate::property::PropertyValue::DateTime(i),
        PropertyValue::Blob(b) => crate::property::PropertyValue::Blob(b),
        PropertyValue::List(l) => {
            crate::property::PropertyValue::List(l.into_iter().map(to_storage).collect())
        }
        PropertyValue::Map(m) => crate::property::PropertyValue::Map(
            m.into_iter().map(|(k, v)| (k, to_storage(v))).collect(),
        ),
    }
}

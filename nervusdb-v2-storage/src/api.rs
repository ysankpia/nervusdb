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

#[derive(Debug, Clone)]
pub struct StorageSnapshot {
    inner: snapshot::Snapshot,
    i2e: Arc<Vec<I2eRecord>>,
    tombstoned_nodes: Arc<HashSet<InternalNodeId>>,
    pager: Arc<Mutex<Pager>>,
    index_catalog: Arc<Mutex<IndexCatalog>>,
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
        let storage_value = match value {
            PropertyValue::Null => crate::property::PropertyValue::Null,
            PropertyValue::Bool(b) => crate::property::PropertyValue::Bool(*b),
            PropertyValue::Int(i) => crate::property::PropertyValue::Int(*i),
            PropertyValue::Float(f) => crate::property::PropertyValue::Float(*f),
            PropertyValue::String(s) => crate::property::PropertyValue::String(s.clone()),
        };

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
        self.inner
            .node_property(iid, key)
            .map(|v| convert_property_value(&v))
    }

    fn edge_property(&self, edge: EdgeKey, key: &str) -> Option<PropertyValue> {
        let snapshot_edge = snapshot::EdgeKey {
            src: edge.src,
            rel: edge.rel,
            dst: edge.dst,
        };
        self.inner
            .edge_property(snapshot_edge, key)
            .map(|v| convert_property_value(&v))
    }

    fn node_properties(&self, iid: InternalNodeId) -> Option<BTreeMap<String, PropertyValue>> {
        self.inner.node_properties(iid).map(|props| {
            props
                .into_iter()
                .map(|(k, v)| (k, convert_property_value(&v)))
                .collect()
        })
    }

    fn edge_properties(&self, edge: EdgeKey) -> Option<BTreeMap<String, PropertyValue>> {
        let snapshot_edge = snapshot::EdgeKey {
            src: edge.src,
            rel: edge.rel,
            dst: edge.dst,
        };
        self.inner.edge_properties(snapshot_edge).map(|props| {
            props
                .into_iter()
                .map(|(k, v)| (k, convert_property_value(&v)))
                .collect()
        })
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
}

fn convert_property_value(v: &crate::property::PropertyValue) -> PropertyValue {
    match v {
        crate::property::PropertyValue::Null => PropertyValue::Null,
        crate::property::PropertyValue::Bool(b) => PropertyValue::Bool(*b),
        crate::property::PropertyValue::Int(i) => PropertyValue::Int(*i),
        crate::property::PropertyValue::Float(f) => PropertyValue::Float(*f),
        crate::property::PropertyValue::String(s) => PropertyValue::String(s.clone()),
    }
}

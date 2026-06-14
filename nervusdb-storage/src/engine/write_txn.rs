use super::GraphEngine;
use crate::idmap::{ExternalId, InternalNodeId, LabelId};
use crate::index::ordered_key::encode_ordered_value;
use crate::memtable::{MemTable, WalPropertyOp};
use crate::snapshot::RelTypeId;
use crate::wal::WalRecord;
use crate::{Error, Result};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub struct WriteTxn<'a> {
    pub(super) engine: &'a GraphEngine,
    pub(super) _guard: std::sync::MutexGuard<'a, ()>,
    pub(super) txid: u64,
    pub(super) created_nodes: Vec<(ExternalId, LabelId, InternalNodeId)>,
    pub(super) pending_label_additions: Vec<(InternalNodeId, LabelId)>,
    pub(super) pending_label_removals: Vec<(InternalNodeId, LabelId)>,
    pub(super) created_external_ids: std::collections::HashSet<ExternalId>,
    pub(super) memtable: MemTable,
}

impl<'a> WriteTxn<'a> {
    pub fn create_node(
        &mut self,
        external_id: ExternalId,
        label_id: LabelId,
    ) -> Result<InternalNodeId> {
        if self.engine.lookup_internal_id(external_id).is_some() {
            return Err(Error::WalProtocol("external id already exists"));
        }

        if !self.created_external_ids.insert(external_id) {
            return Err(Error::WalProtocol("duplicate external id in same tx"));
        }

        let base_next = {
            let idmap = self.engine.idmap.lock().unwrap();
            idmap.next_internal_id()
        };
        let internal_id = base_next + self.created_nodes.len() as u32;

        self.created_nodes
            .push((external_id, label_id, internal_id));
        Ok(internal_id)
    }

    pub fn add_node_label(&mut self, node: InternalNodeId, label_id: LabelId) -> Result<()> {
        self.pending_label_additions.push((node, label_id));
        Ok(())
    }

    pub fn remove_node_label(&mut self, node: InternalNodeId, label_id: LabelId) -> Result<()> {
        self.pending_label_removals.push((node, label_id));
        Ok(())
    }

    pub fn create_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        self.memtable.create_edge(src, rel, dst);
    }

    pub fn get_or_create_label(&self, name: &str) -> Result<LabelId> {
        self.engine.get_or_create_label(name)
    }

    pub fn get_or_create_rel_type(&self, name: &str) -> Result<RelTypeId> {
        self.engine.get_or_create_label(name)
    }

    pub fn tombstone_node(&mut self, node: InternalNodeId) {
        self.memtable.tombstone_node(node);
    }

    pub fn tombstone_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        self.memtable.tombstone_edge(src, rel, dst);
    }

    pub fn set_node_property(
        &mut self,
        node: InternalNodeId,
        key: String,
        value: crate::property::PropertyValue,
    ) {
        self.memtable.set_node_property(node, key, value);
    }

    pub fn set_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: String,
        value: crate::property::PropertyValue,
    ) {
        self.memtable.set_edge_property(src, rel, dst, key, value);
    }

    pub fn remove_node_property(&mut self, node: InternalNodeId, key: &str) {
        self.memtable.remove_node_property(node, key);
    }

    pub fn remove_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: &str,
    ) {
        self.memtable.remove_edge_property(src, rel, dst, key);
    }

    pub fn staged_created_nodes_with_labels(&self) -> Vec<(InternalNodeId, Vec<String>)> {
        let mut labels_by_node: BTreeMap<InternalNodeId, std::collections::BTreeSet<LabelId>> =
            BTreeMap::new();

        for (_, label_id, node_id) in &self.created_nodes {
            if *label_id != LabelId::MAX {
                labels_by_node
                    .entry(*node_id)
                    .or_default()
                    .insert(*label_id);
            } else {
                labels_by_node.entry(*node_id).or_default();
            }
        }

        for (node_id, label_id) in &self.pending_label_additions {
            labels_by_node
                .entry(*node_id)
                .or_default()
                .insert(*label_id);
        }

        for (node_id, label_id) in &self.pending_label_removals {
            if let Some(labels) = labels_by_node.get_mut(node_id) {
                labels.remove(label_id);
            }
        }

        let labels = self.engine.published_state.load_full().labels.clone();
        self.created_nodes
            .iter()
            .map(|(_, _, node_id)| {
                let node_label_names = labels_by_node
                    .get(node_id)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|label_id| labels.get_name(label_id).map(ToOwned::to_owned))
                    .collect::<Vec<_>>();
                (*node_id, node_label_names)
            })
            .collect()
    }

    pub fn commit(self) -> Result<()> {
        enum IndexOp {
            Insert(String, crate::property::PropertyValue),
            Update(
                String,
                Option<crate::property::PropertyValue>,
                crate::property::PropertyValue,
            ),
            Remove(String, Option<crate::property::PropertyValue>),
        }
        let mut index_ops = Vec::new();
        let published_state = self.engine.published_state.load_full();
        let label_snapshot = published_state.labels.clone();
        let index_defs = published_state.index_entries.clone();
        let created_node_labels: std::collections::HashMap<InternalNodeId, LabelId> = self
            .created_nodes
            .iter()
            .map(|(_, label_id, internal_id)| (*internal_id, *label_id))
            .collect();
        let read_snapshot = self.engine.begin_read();
        let run = {
            let mut wal = self.engine.wal.lock().unwrap();
            wal.append(&WalRecord::BeginTx { txid: self.txid })?;

            for (external_id, label_id, internal_id) in &self.created_nodes {
                wal.append(&WalRecord::CreateNode {
                    external_id: *external_id,
                    label_id: *label_id,
                    internal_id: *internal_id,
                })?;
            }
            for (node, label_id) in &self.pending_label_additions {
                wal.append(&WalRecord::AddNodeLabel {
                    node: *node,
                    label_id: *label_id,
                })?;
            }
            for (node, label_id) in &self.pending_label_removals {
                wal.append(&WalRecord::RemoveNodeLabel {
                    node: *node,
                    label_id: *label_id,
                })?;
            }

            self.memtable.try_for_each_wal_property_op(|op| match op {
                WalPropertyOp::SetNode { node, key, value } => wal
                    .append(&WalRecord::SetNodeProperty {
                        node,
                        key: key.to_string(),
                        value: value.clone(),
                    })
                    .map(|_| ()),
                WalPropertyOp::RemoveNode { node, key } => wal
                    .append(&WalRecord::RemoveNodeProperty {
                        node,
                        key: key.to_string(),
                    })
                    .map(|_| ()),
                WalPropertyOp::SetEdge { edge, key, value } => wal
                    .append(&WalRecord::SetEdgeProperty {
                        src: edge.src,
                        rel: edge.rel,
                        dst: edge.dst,
                        key: key.to_string(),
                        value: value.clone(),
                    })
                    .map(|_| ()),
                WalPropertyOp::RemoveEdge { edge, key } => wal
                    .append(&WalRecord::RemoveEdgeProperty {
                        src: edge.src,
                        rel: edge.rel,
                        dst: edge.dst,
                        key: key.to_string(),
                    })
                    .map(|_| ()),
            })?;

            self.memtable.for_each_node_property(|node, key, value| {
                let label_id = created_node_labels
                    .get(&node)
                    .copied()
                    .or_else(|| read_snapshot.node_label(node));
                let is_new = created_node_labels.contains_key(&node);

                if let Some(lid) = label_id
                    && let Some(label_name) = label_snapshot.get_name(lid)
                {
                    let index_name = format!("{}.{}", label_name, key);
                    if index_defs.contains_key(&index_name) {
                        if is_new {
                            index_ops.push((IndexOp::Insert(index_name, value.clone()), node));
                        } else {
                            let old_value = read_snapshot.node_property(node, key);
                            index_ops.push((
                                IndexOp::Update(index_name, old_value, value.clone()),
                                node,
                            ));
                        }
                    }
                }
            });

            self.memtable.for_each_removed_node_property(|node, key| {
                let is_new = created_node_labels.contains_key(&node);
                if is_new {
                    return;
                }

                let label_id = read_snapshot.node_label(node);
                if let Some(lid) = label_id
                    && let Some(label_name) = label_snapshot.get_name(lid)
                {
                    let index_name = format!("{}.{}", label_name, key);
                    if index_defs.contains_key(&index_name) {
                        let old_value = read_snapshot.node_property(node, key);
                        index_ops.push((IndexOp::Remove(index_name, old_value), node));
                    }
                }
            });

            let run = self.memtable.freeze_into_run(self.txid);

            for edge in run.iter_edges() {
                wal.append(&WalRecord::CreateEdge {
                    src: edge.src,
                    rel: edge.rel,
                    dst: edge.dst,
                })?;
            }
            for node in run.iter_tombstoned_nodes() {
                wal.append(&WalRecord::TombstoneNode { node })?;
            }
            for edge in run.iter_tombstoned_edges() {
                wal.append(&WalRecord::TombstoneEdge {
                    src: edge.src,
                    rel: edge.rel,
                    dst: edge.dst,
                })?;
            }

            wal.append(&WalRecord::CommitTx { txid: self.txid })?;
            wal.fsync()?;
            run
        };

        let has_new_nodes = !self.created_nodes.is_empty();
        let has_label_additions = !self.pending_label_additions.is_empty();
        let has_label_removals = !self.pending_label_removals.is_empty();

        if !index_ops.is_empty() {
            self.engine.with_catalog_pager(|catalog, pager| {
                let index_ops = index_ops;
                for (op, node_id) in index_ops {
                    match op {
                        IndexOp::Insert(name, val) => {
                            if let Some(re) = catalog.entries.get_mut(&name) {
                                let mut tree = crate::index::btree::BTree::load(re.root);

                                let mut key = Vec::new();
                                key.extend_from_slice(&re.id.to_be_bytes());
                                key.extend_from_slice(&encode_ordered_value(&val));

                                let _ = tree.insert(pager, &key, node_id as u64);
                                re.root = tree.root();
                            }
                        }
                        IndexOp::Update(name, old_val_opt, new_val) => {
                            if let Some(re) = catalog.entries.get_mut(&name) {
                                let mut tree = crate::index::btree::BTree::load(re.root);

                                if let Some(old_val) = old_val_opt {
                                    let mut old_key = Vec::new();
                                    old_key.extend_from_slice(&re.id.to_be_bytes());
                                    old_key.extend_from_slice(&encode_ordered_value(&old_val));
                                    let _ = tree.delete(pager, &old_key, node_id as u64);
                                }

                                let mut new_key = Vec::new();
                                new_key.extend_from_slice(&re.id.to_be_bytes());
                                new_key.extend_from_slice(&encode_ordered_value(&new_val));

                                let _ = tree.insert(pager, &new_key, node_id as u64);
                                re.root = tree.root();
                            }
                        }
                        IndexOp::Remove(name, old_val_opt) => {
                            if let Some(re) = catalog.entries.get_mut(&name)
                                && let Some(old_val) = old_val_opt
                            {
                                let mut tree = crate::index::btree::BTree::load(re.root);
                                let mut old_key = Vec::new();
                                old_key.extend_from_slice(&re.id.to_be_bytes());
                                old_key.extend_from_slice(&encode_ordered_value(&old_val));
                                let _ = tree.delete(pager, &old_key, node_id as u64);
                                re.root = tree.root();
                            }
                        }
                    }
                }
                catalog.flush(pager)?;
                self.engine.publish_index_entries_snapshot(catalog);
                Ok(())
            })?;
        }

        {
            let mut pager = self.engine.pager.write().unwrap();
            {
                let mut idmap = self.engine.idmap.lock().unwrap();
                for (external_id, label_id, internal_id) in self.created_nodes {
                    idmap.apply_create_node(&mut pager, external_id, label_id, internal_id)?;
                }
                for (node, label_id) in self.pending_label_additions {
                    idmap.apply_add_label(&mut pager, node, label_id)?;
                }
                for (node, label_id) in self.pending_label_removals {
                    idmap.apply_remove_label(&mut pager, node, label_id)?;
                }
            }

            pager.sync()?;
        }

        let has_label_mutations = has_new_nodes || has_label_additions || has_label_removals;
        if has_label_mutations {
            self.engine.update_published_idmap_snapshots();
        }

        if !run.is_empty() {
            self.engine.publish_run(Arc::new(run));
        }

        self.engine.next_txid.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }
}

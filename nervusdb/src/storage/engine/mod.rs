use crate::api::{
    EdgeKey, ExternalId, GraphSnapshot, GraphStore, InternalNodeId, LabelId, PropertyValue,
    RelTypeId,
};
use crate::storage::layout::*;
use crate::storage::profile;
use crate::storage::snapshot::Snapshot;
use crate::storage::{Error, Result, STORAGE_FORMAT_EPOCH};
use fjall::{Database, Keyspace, KeyspaceCreateOptions, PersistMode};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

const META_FORMAT_EPOCH: &[u8] = b"format_epoch";
const META_NEXT_NODE_ID: &[u8] = b"next_node_id";
const META_NEXT_LABEL_ID: &[u8] = b"next_label_id";
const META_NEXT_REL_TYPE_ID: &[u8] = b"next_rel_type_id";

#[derive(Clone)]
pub(crate) struct Keyspaces {
    pub(crate) meta: Keyspace,
    pub(crate) graph_data: Keyspace,
    pub(crate) adj_out: Keyspace,
    pub(crate) adj_in: Keyspace,
}

impl std::fmt::Debug for Keyspaces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keyspaces").finish_non_exhaustive()
    }
}

pub struct GraphEngine {
    pub(crate) path: PathBuf,
    pub(crate) db: Database,
    pub(crate) keyspaces: Keyspaces,
    pub(crate) write_lock: Mutex<()>,
}

impl std::fmt::Debug for GraphEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphEngine")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl GraphEngine {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let started = profile::start();
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)?;
        let db_started = profile::start();
        let db = Database::builder(&path).open()?;
        profile::event_since("GraphEngine::open.database", db_started, &[]);
        let keyspaces_started = profile::start();
        let meta = db.keyspace("meta", KeyspaceCreateOptions::default)?;
        ensure_meta(&db, &meta)?;
        let keyspaces = open_keyspaces(&db, meta)?;
        profile::event_since(
            "GraphEngine::open.keyspaces",
            keyspaces_started,
            &[("keyspaces", 4)],
        );

        let engine = Self {
            path,
            db,
            keyspaces,
            write_lock: Mutex::new(()),
        };
        profile::event_since("GraphEngine::open", started, &[]);
        Ok(engine)
    }

    #[inline]
    pub fn storage_dir(&self) -> &Path {
        &self.path
    }

    pub fn snapshot(&self) -> Snapshot {
        self.begin_read()
    }

    pub fn begin_read(&self) -> Snapshot {
        Snapshot::new(self.db.snapshot(), self.keyspaces.clone())
    }

    pub fn begin_write(&self) -> WriteTxn<'_> {
        let guard = self.write_lock.lock().unwrap();
        WriteTxn {
            engine: self,
            _guard: guard,
            created_nodes: Vec::new(),
            created_node_ids: HashSet::new(),
            pending_next_node_id: None,
            staged_external_ids: HashSet::new(),
            label_additions: Vec::new(),
            label_removals: Vec::new(),
            created_edges: Vec::new(),
            tombstoned_nodes: BTreeSet::new(),
            tombstoned_edges: BTreeSet::new(),
            node_props: HashMap::new(),
            edge_props: HashMap::new(),
            removed_node_props: Vec::new(),
            removed_edge_props: Vec::new(),
        }
    }

    pub fn lookup_internal_id(&self, external_id: ExternalId) -> Option<InternalNodeId> {
        let iid = self
            .keyspaces
            .graph_data
            .get(ext2node_key(external_id))
            .ok()
            .flatten()
            .and_then(|v| decode_u32(v.as_ref()))?;
        if self.begin_read().is_tombstoned_node(iid) {
            None
        } else {
            Some(iid)
        }
    }

    pub fn get_or_create_label(&self, name: &str) -> Result<LabelId> {
        let _guard = self.write_lock.lock().unwrap();
        self.get_or_create_name(label_name_key, label_id_key, META_NEXT_LABEL_ID, name)
    }

    pub fn get_or_create_rel_type(&self, name: &str) -> Result<RelTypeId> {
        let _guard = self.write_lock.lock().unwrap();
        self.get_or_create_name(rel_name_key, rel_id_key, META_NEXT_REL_TYPE_ID, name)
    }

    pub fn get_label_id(&self, name: &str) -> Option<LabelId> {
        self.get_name_id(label_name_key, name)
    }

    pub fn get_rel_type_id(&self, name: &str) -> Option<RelTypeId> {
        self.get_name_id(rel_name_key, name)
    }

    pub fn get_label_name(&self, id: LabelId) -> Option<String> {
        self.get_id_name(label_id_key, id)
    }

    pub fn get_rel_type_name(&self, id: RelTypeId) -> Option<String> {
        self.get_id_name(rel_id_key, id)
    }

    pub fn persist(&self) -> Result<()> {
        self.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }

    pub fn checkpoint_on_close(&self) -> Result<()> {
        self.persist()?;
        let started = profile::start();
        self.keyspaces.meta.rotate_memtable_and_wait()?;
        self.keyspaces.graph_data.rotate_memtable_and_wait()?;
        self.keyspaces.adj_out.rotate_memtable_and_wait()?;
        self.keyspaces.adj_in.rotate_memtable_and_wait()?;
        profile::event_since("GraphEngine::close.flush_keyspaces", started, &[]);
        Ok(())
    }

    fn get_name_id(&self, name_key: fn(&str) -> Vec<u8>, name: &str) -> Option<u32> {
        self.keyspaces
            .graph_data
            .get(name_key(name))
            .ok()
            .flatten()
            .and_then(|v| decode_u32(v.as_ref()))
    }

    fn get_id_name(&self, id_key: fn(u32) -> Vec<u8>, id: u32) -> Option<String> {
        self.keyspaces
            .graph_data
            .get(id_key(id))
            .ok()
            .flatten()
            .and_then(|v| String::from_utf8(v.as_ref().to_vec()).ok())
    }

    fn get_or_create_name(
        &self,
        name_key: fn(&str) -> Vec<u8>,
        id_key: fn(u32) -> Vec<u8>,
        counter_key: &[u8],
        name: &str,
    ) -> Result<u32> {
        if let Some(id) = self.get_name_id(name_key, name) {
            return Ok(id);
        }

        let id = self.next_counter(counter_key)?;
        let mut batch = self.db.batch().durability(Some(PersistMode::SyncAll));
        batch.insert(&self.keyspaces.graph_data, name_key(name), id.to_be_bytes());
        batch.insert(&self.keyspaces.graph_data, id_key(id), name.as_bytes());
        batch.commit()?;
        Ok(id)
    }

    fn next_counter(&self, key: &[u8]) -> Result<u32> {
        let current = read_meta_u64(&self.keyspaces.meta, key)?.unwrap_or(0);
        if current > u32::MAX as u64 {
            return Err(Error::StorageCorrupted(format!(
                "counter {} exceeds u32",
                String::from_utf8_lossy(key)
            )));
        }
        let next = current + 1;
        let mut batch = self.db.batch().durability(Some(PersistMode::SyncAll));
        batch.insert(&self.keyspaces.meta, key, next.to_be_bytes());
        batch.commit()?;
        Ok(current as u32)
    }
}

impl GraphStore for GraphEngine {
    type Snapshot = Snapshot;

    fn snapshot(&self) -> Self::Snapshot {
        GraphEngine::snapshot(self)
    }
}

#[derive(Debug)]
struct CreatedNode {
    iid: InternalNodeId,
    external_id: ExternalId,
    labels: BTreeSet<LabelId>,
}

#[derive(Debug)]
pub struct WriteTxn<'a> {
    engine: &'a GraphEngine,
    _guard: MutexGuard<'a, ()>,
    created_nodes: Vec<CreatedNode>,
    created_node_ids: HashSet<InternalNodeId>,
    pending_next_node_id: Option<u64>,
    staged_external_ids: HashSet<ExternalId>,
    label_additions: Vec<(InternalNodeId, LabelId)>,
    label_removals: Vec<(InternalNodeId, LabelId)>,
    created_edges: Vec<EdgeKey>,
    tombstoned_nodes: BTreeSet<InternalNodeId>,
    tombstoned_edges: BTreeSet<EdgeKey>,
    node_props: HashMap<(InternalNodeId, String), PropertyValue>,
    edge_props: HashMap<(EdgeKey, String), PropertyValue>,
    removed_node_props: Vec<(InternalNodeId, String)>,
    removed_edge_props: Vec<(EdgeKey, String)>,
}

#[derive(Debug, Default)]
struct NodeCleanup {
    label_keys: Vec<Vec<u8>>,
    label_node_keys: Vec<Vec<u8>>,
    node_prop_keys: Vec<Vec<u8>>,
    node_prop_index_keys: Vec<Vec<u8>>,
    incident_edges: BTreeSet<EdgeKey>,
}

pub(crate) fn scalar_indexable_value(value: &PropertyValue) -> bool {
    !matches!(value, PropertyValue::List(_) | PropertyValue::Map(_))
}

fn final_node_labels(
    node: InternalNodeId,
    snapshot: &Snapshot,
    created_node_labels: &HashMap<InternalNodeId, BTreeSet<LabelId>>,
    label_additions: &[(InternalNodeId, LabelId)],
    label_removals: &[(InternalNodeId, LabelId)],
) -> BTreeSet<LabelId> {
    let mut labels: BTreeSet<LabelId> = created_node_labels
        .get(&node)
        .cloned()
        .unwrap_or_else(|| snapshot.node_labels(node).into_iter().collect());

    for (label_node, label) in label_additions {
        if *label_node == node {
            labels.insert(*label);
        }
    }
    for (label_node, label) in label_removals {
        if *label_node == node {
            labels.remove(label);
        }
    }
    labels
}

fn final_node_properties(
    node: InternalNodeId,
    snapshot: &Snapshot,
    node_props: &HashMap<(InternalNodeId, String), PropertyValue>,
    removed_node_props: &[(InternalNodeId, String)],
) -> BTreeMap<String, PropertyValue> {
    let mut props = snapshot.node_properties(node).unwrap_or_default();
    for ((prop_node, key), value) in node_props {
        if *prop_node == node {
            props.insert(key.clone(), value.clone());
        }
    }
    for (prop_node, key) in removed_node_props {
        if *prop_node == node {
            props.remove(key);
        }
    }
    props
}

fn node_property_removed_in_txn(
    node: InternalNodeId,
    key: &str,
    removed_node_props: &[(InternalNodeId, String)],
) -> bool {
    removed_node_props
        .iter()
        .any(|(removed_node, removed_key)| *removed_node == node && removed_key == key)
}

fn snapshot_node_property_index_keys(node: InternalNodeId, snapshot: &Snapshot) -> Vec<Vec<u8>> {
    let labels = snapshot.node_labels(node);
    let Some(props) = snapshot.node_properties(node) else {
        return Vec::new();
    };
    node_property_index_keys_for_props(node, &labels, &props)
}

fn node_property_index_keys_for_props(
    node: InternalNodeId,
    labels: &[LabelId],
    props: &BTreeMap<String, PropertyValue>,
) -> Vec<Vec<u8>> {
    let mut keys = Vec::new();
    for label in labels {
        for (key, value) in props {
            if scalar_indexable_value(value) {
                keys.push(node_prop_index_key(*label, key, value, node));
            }
        }
    }
    keys
}

fn snapshot_node_property_index_keys_for_label(
    node: InternalNodeId,
    label: LabelId,
    snapshot: &Snapshot,
) -> Vec<Vec<u8>> {
    let Some(props) = snapshot.node_properties(node) else {
        return Vec::new();
    };
    node_property_index_keys_for_props(node, &[label], &props)
}

fn snapshot_node_property_index_keys_for_property(
    node: InternalNodeId,
    key: &str,
    snapshot: &Snapshot,
) -> Vec<Vec<u8>> {
    let Some(value) = snapshot.node_property(node, key) else {
        return Vec::new();
    };
    if !scalar_indexable_value(&value) {
        return Vec::new();
    }
    snapshot
        .node_labels(node)
        .into_iter()
        .map(|label| node_prop_index_key(label, key, &value, node))
        .collect()
}

impl<'a> WriteTxn<'a> {
    fn edge_not_found(edge: EdgeKey) -> Error {
        Error::EdgeNotFound {
            src: edge.src,
            rel: edge.rel,
            dst: edge.dst,
        }
    }

    fn node_deleted_in_txn(&self, node: InternalNodeId) -> bool {
        self.tombstoned_nodes.contains(&node)
    }

    fn edge_deleted_in_txn(&self, edge: EdgeKey) -> bool {
        self.tombstoned_edges.contains(&edge)
            || self.tombstoned_nodes.contains(&edge.src)
            || self.tombstoned_nodes.contains(&edge.dst)
    }

    fn node_live_in_txn(&self, node: InternalNodeId) -> bool {
        if self.node_deleted_in_txn(node) {
            return false;
        }
        if self.created_node_ids.contains(&node) {
            return true;
        }
        self.engine.begin_read().node_is_live(node)
    }

    fn edge_live_in_txn(&self, edge: EdgeKey) -> bool {
        if self.edge_deleted_in_txn(edge) {
            return false;
        }
        if !self.node_live_in_txn(edge.src) || !self.node_live_in_txn(edge.dst) {
            return false;
        }
        self.created_edges.contains(&edge) || self.engine.begin_read().edge_is_live(edge)
    }

    fn node_live_for_commit(&self, node: InternalNodeId, snapshot: &Snapshot) -> bool {
        if self.node_deleted_in_txn(node) {
            return false;
        }
        self.created_node_ids.contains(&node) || snapshot.node_is_live(node)
    }

    fn edge_live_for_commit(
        &self,
        edge: EdgeKey,
        snapshot: &Snapshot,
        created_edges: &[EdgeKey],
    ) -> bool {
        if self.edge_deleted_in_txn(edge) {
            return false;
        }
        self.node_live_for_commit(edge.src, snapshot)
            && self.node_live_for_commit(edge.dst, snapshot)
            && (created_edges.contains(&edge) || snapshot.edge_is_live(edge))
    }

    fn edge_known_before_delete(
        &self,
        edge: EdgeKey,
        snapshot: &Snapshot,
        created_edges: &[EdgeKey],
    ) -> bool {
        created_edges.contains(&edge) || snapshot.edge_is_live(edge)
    }

    fn external_id_for_commit(
        &self,
        node: InternalNodeId,
        snapshot: &Snapshot,
    ) -> Option<ExternalId> {
        self.created_nodes
            .iter()
            .find(|created| created.iid == node)
            .map(|created| created.external_id)
            .or_else(|| snapshot.resolve_external(node))
    }

    fn ensure_node_live(&self, node: InternalNodeId) -> Result<()> {
        if self.node_live_in_txn(node) {
            Ok(())
        } else {
            Err(Error::NodeNotFound(node))
        }
    }

    fn ensure_edge_live(&self, edge: EdgeKey) -> Result<()> {
        if self.edge_live_in_txn(edge) {
            Ok(())
        } else {
            Err(Self::edge_not_found(edge))
        }
    }

    pub fn create_node(
        &mut self,
        external_id: ExternalId,
        label_id: LabelId,
    ) -> Result<InternalNodeId> {
        if external_id == 0 {
            return Err(Error::StorageCorrupted(
                "external id 0 is reserved".to_string(),
            ));
        }
        if self.staged_external_ids.contains(&external_id)
            || self.engine.lookup_internal_id(external_id).is_some()
        {
            return Err(Error::DuplicateExternalId(external_id));
        }

        let next_node_id = match self.pending_next_node_id {
            Some(next) => next,
            None => read_meta_u64(&self.engine.keyspaces.meta, META_NEXT_NODE_ID)?.unwrap_or(0),
        };
        if next_node_id > u32::MAX as u64 {
            return Err(Error::StorageCorrupted(format!(
                "counter {} exceeds u32",
                String::from_utf8_lossy(META_NEXT_NODE_ID)
            )));
        }

        let iid = next_node_id as u32;
        self.pending_next_node_id = Some(next_node_id + 1);
        self.staged_external_ids.insert(external_id);
        self.created_node_ids.insert(iid);
        self.created_nodes.push(CreatedNode {
            iid,
            external_id,
            labels: BTreeSet::from([label_id]),
        });
        Ok(iid)
    }

    pub fn add_node_label(&mut self, node: InternalNodeId, label_id: LabelId) -> Result<()> {
        self.ensure_node_live(node)?;
        if let Some(created) = self.created_nodes.iter_mut().find(|n| n.iid == node) {
            created.labels.insert(label_id);
        } else {
            self.label_additions.push((node, label_id));
        }
        Ok(())
    }

    pub fn remove_node_label(&mut self, node: InternalNodeId, label_id: LabelId) -> Result<()> {
        self.ensure_node_live(node)?;
        if let Some(created) = self.created_nodes.iter_mut().find(|n| n.iid == node) {
            created.labels.remove(&label_id);
        } else {
            self.label_removals.push((node, label_id));
        }
        Ok(())
    }

    pub fn create_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> Result<()> {
        self.ensure_node_live(src)?;
        self.ensure_node_live(dst)?;
        self.created_edges.push(EdgeKey { src, rel, dst });
        Ok(())
    }

    pub fn tombstone_node(&mut self, node: InternalNodeId) -> Result<()> {
        self.ensure_node_live(node)?;
        self.tombstoned_nodes.insert(node);
        Ok(())
    }

    pub fn tombstone_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> Result<()> {
        let edge = EdgeKey { src, rel, dst };
        self.ensure_edge_live(edge)?;
        self.tombstoned_edges.insert(edge);
        Ok(())
    }

    pub fn set_node_property(
        &mut self,
        node: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()> {
        self.ensure_node_live(node)?;
        self.node_props.insert((node, key), value);
        Ok(())
    }

    pub fn set_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()> {
        let edge = EdgeKey { src, rel, dst };
        self.ensure_edge_live(edge)?;
        self.edge_props.insert((edge, key), value);
        Ok(())
    }

    pub fn remove_node_property(&mut self, node: InternalNodeId, key: &str) -> Result<()> {
        self.ensure_node_live(node)?;
        self.removed_node_props.push((node, key.to_string()));
        Ok(())
    }

    pub fn remove_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: &str,
    ) -> Result<()> {
        let edge = EdgeKey { src, rel, dst };
        self.ensure_edge_live(edge)?;
        self.removed_edge_props.push((edge, key.to_string()));
        Ok(())
    }

    pub fn get_or_create_label(&mut self, name: &str) -> Result<LabelId> {
        self.engine
            .get_or_create_name(label_name_key, label_id_key, META_NEXT_LABEL_ID, name)
    }

    pub fn get_or_create_rel_type(&mut self, name: &str) -> Result<RelTypeId> {
        self.engine
            .get_or_create_name(rel_name_key, rel_id_key, META_NEXT_REL_TYPE_ID, name)
    }

    pub fn staged_created_nodes_with_labels(&self) -> Vec<(InternalNodeId, Vec<String>)> {
        self.created_nodes
            .iter()
            .map(|node| {
                let labels = node
                    .labels
                    .iter()
                    .filter_map(|id| self.engine.get_label_name(*id))
                    .collect();
                (node.iid, labels)
            })
            .collect()
    }

    pub fn commit(self) -> Result<()> {
        let commit_started = profile::start();
        let mut batch = self
            .engine
            .db
            .batch()
            .durability(Some(PersistMode::SyncAll));
        let snapshot = self.engine.begin_read();
        let created_node_labels: HashMap<InternalNodeId, BTreeSet<LabelId>> = self
            .created_nodes
            .iter()
            .map(|created| (created.iid, created.labels.clone()))
            .collect();
        let validation_started = profile::start();
        let mut all_created_edges = self.created_edges.clone();
        all_created_edges.sort_unstable();
        all_created_edges.dedup();
        let mut created_edges = all_created_edges.clone();
        created_edges.retain(|edge| !self.edge_deleted_in_txn(*edge));

        for edge in &created_edges {
            if !self.node_live_for_commit(edge.src, &snapshot)
                || !self.node_live_for_commit(edge.dst, &snapshot)
            {
                return Err(Self::edge_not_found(*edge));
            }
        }

        for edge in &self.tombstoned_edges {
            if !self.edge_known_before_delete(*edge, &snapshot, &all_created_edges) {
                return Err(Self::edge_not_found(*edge));
            }
        }

        for (node, _) in &self.label_additions {
            if !self.node_live_for_commit(*node, &snapshot) {
                return Err(Error::NodeNotFound(*node));
            }
        }
        for (node, _) in &self.label_removals {
            if !self.node_live_for_commit(*node, &snapshot) {
                return Err(Error::NodeNotFound(*node));
            }
        }
        for (node, _) in self.node_props.keys() {
            if !self.node_live_for_commit(*node, &snapshot) {
                return Err(Error::NodeNotFound(*node));
            }
        }
        for (node, _) in &self.removed_node_props {
            if !self.node_live_for_commit(*node, &snapshot) {
                return Err(Error::NodeNotFound(*node));
            }
        }
        for (edge, _) in self.edge_props.keys() {
            if !self.edge_live_for_commit(*edge, &snapshot, &created_edges) {
                return Err(Self::edge_not_found(*edge));
            }
        }
        for (edge, _) in &self.removed_edge_props {
            if !self.edge_live_for_commit(*edge, &snapshot, &created_edges) {
                return Err(Self::edge_not_found(*edge));
            }
        }
        profile::event_since(
            "WriteTxn::commit.validation",
            validation_started,
            &[
                ("created_edges", created_edges.len() as u64),
                ("tombstoned_edges", self.tombstoned_edges.len() as u64),
                ("tombstoned_nodes", self.tombstoned_nodes.len() as u64),
                ("node_props", self.node_props.len() as u64),
                ("edge_props", self.edge_props.len() as u64),
            ],
        );

        let cleanup_started = profile::start();
        let mut node_cleanups: HashMap<InternalNodeId, NodeCleanup> = HashMap::new();
        let mut detached_edges: BTreeSet<EdgeKey> = BTreeSet::new();
        for node in &self.tombstoned_nodes {
            let mut cleanup = NodeCleanup::default();
            for label in snapshot.node_labels(*node) {
                cleanup.label_keys.push(node_label_key(*node, label));
                cleanup.label_node_keys.push(label_node_key(label, *node));
            }
            cleanup
                .node_prop_keys
                .extend(snapshot.collect_node_property_keys(*node));
            cleanup
                .node_prop_index_keys
                .extend(snapshot_node_property_index_keys(*node, &snapshot));
            cleanup
                .incident_edges
                .extend(snapshot.collect_raw_outgoing_edges(*node));
            cleanup
                .incident_edges
                .extend(snapshot.collect_raw_incoming_edges(*node));
            for edge in &created_edges {
                if edge.src == *node || edge.dst == *node {
                    cleanup.incident_edges.insert(*edge);
                }
            }
            detached_edges.extend(cleanup.incident_edges.iter().copied());
            node_cleanups.insert(*node, cleanup);
        }
        profile::event_since(
            "WriteTxn::commit.cleanup_collection",
            cleanup_started,
            &[
                ("nodes", self.tombstoned_nodes.len() as u64),
                ("detached_edges", detached_edges.len() as u64),
            ],
        );

        if let Some(next_node_id) = self.pending_next_node_id {
            batch.insert(
                &self.engine.keyspaces.meta,
                META_NEXT_NODE_ID,
                next_node_id.to_be_bytes(),
            );
        }

        let created_node_writes_started = profile::start();
        let mut created_node_writes = 0u64;
        for node in &self.created_nodes {
            if self.tombstoned_nodes.contains(&node.iid) {
                continue;
            }
            created_node_writes += 1;
            batch.insert(
                &self.engine.keyspaces.graph_data,
                node_key(node.iid),
                encode_node_value(node.external_id, 0),
            );
            batch.insert(
                &self.engine.keyspaces.graph_data,
                ext2node_key(node.external_id),
                key_u32(node.iid),
            );
            for label in &node.labels {
                batch.insert(
                    &self.engine.keyspaces.graph_data,
                    node_label_key(node.iid, *label),
                    [],
                );
                batch.insert(
                    &self.engine.keyspaces.graph_data,
                    label_node_key(*label, node.iid),
                    [],
                );
            }
        }
        profile::event_since(
            "WriteTxn::commit.created_node_writes",
            created_node_writes_started,
            &[("nodes", created_node_writes)],
        );

        let property_index_writes_started = profile::start();
        for (node, label) in &self.label_additions {
            if self.tombstoned_nodes.contains(node) {
                continue;
            }
            if !final_node_labels(
                *node,
                &snapshot,
                &created_node_labels,
                &self.label_additions,
                &self.label_removals,
            )
            .contains(label)
            {
                continue;
            }
            batch.insert(
                &self.engine.keyspaces.graph_data,
                node_label_key(*node, *label),
                [],
            );
            batch.insert(
                &self.engine.keyspaces.graph_data,
                label_node_key(*label, *node),
                [],
            );
            let props =
                final_node_properties(*node, &snapshot, &self.node_props, &self.removed_node_props);
            for (key, value) in props {
                if scalar_indexable_value(&value) {
                    batch.insert(
                        &self.engine.keyspaces.graph_data,
                        node_prop_index_key(*label, &key, &value, *node),
                        [],
                    );
                }
            }
        }

        for (node, label) in &self.label_removals {
            if self.tombstoned_nodes.contains(node) {
                continue;
            }
            batch.remove(
                &self.engine.keyspaces.graph_data,
                node_label_key(*node, *label),
            );
            batch.remove(
                &self.engine.keyspaces.graph_data,
                label_node_key(*label, *node),
            );
            for key in snapshot_node_property_index_keys_for_label(*node, *label, &snapshot) {
                batch.remove(&self.engine.keyspaces.graph_data, key);
            }
        }

        let edge_writes_started = profile::start();
        for edge in &created_edges {
            batch.insert(&self.engine.keyspaces.adj_out, adj_out_key(*edge), []);
            batch.insert(&self.engine.keyspaces.adj_in, adj_in_key(*edge), []);
        }

        for edge in &self.tombstoned_edges {
            batch.remove(&self.engine.keyspaces.adj_out, adj_out_key(*edge));
            batch.remove(&self.engine.keyspaces.adj_in, adj_in_key(*edge));
            for key in snapshot.collect_edge_property_keys(*edge) {
                batch.remove(&self.engine.keyspaces.graph_data, key);
            }
        }

        for node in &self.tombstoned_nodes {
            if let Some(cleanup) = node_cleanups.get(node) {
                for key in &cleanup.label_keys {
                    batch.remove(&self.engine.keyspaces.graph_data, key);
                }
                for key in &cleanup.label_node_keys {
                    batch.remove(&self.engine.keyspaces.graph_data, key);
                }
                for key in &cleanup.node_prop_keys {
                    batch.remove(&self.engine.keyspaces.graph_data, key);
                }
                for key in &cleanup.node_prop_index_keys {
                    batch.remove(&self.engine.keyspaces.graph_data, key);
                }
            }

            if let Some(external_id) = self.external_id_for_commit(*node, &snapshot) {
                batch.insert(
                    &self.engine.keyspaces.graph_data,
                    node_key(*node),
                    encode_node_value(external_id, KEY_FLAG_TOMBSTONE),
                );
            }
        }

        for edge in &detached_edges {
            batch.remove(&self.engine.keyspaces.adj_out, adj_out_key(*edge));
            batch.remove(&self.engine.keyspaces.adj_in, adj_in_key(*edge));
            for key in snapshot.collect_edge_property_keys(*edge) {
                batch.remove(&self.engine.keyspaces.graph_data, key);
            }
        }
        profile::event_since(
            "WriteTxn::commit.edge_writes",
            edge_writes_started,
            &[
                ("created_edges", created_edges.len() as u64),
                ("tombstoned_edges", self.tombstoned_edges.len() as u64),
                ("detached_edges", detached_edges.len() as u64),
            ],
        );

        for ((node, key), value) in &self.node_props {
            if self.tombstoned_nodes.contains(node) {
                continue;
            }
            if !self.created_node_ids.contains(node) {
                for old_key in snapshot_node_property_index_keys_for_property(*node, key, &snapshot)
                {
                    batch.remove(&self.engine.keyspaces.graph_data, old_key);
                }
            }
            if node_property_removed_in_txn(*node, key, &self.removed_node_props) {
                continue;
            }
            batch.insert(
                &self.engine.keyspaces.graph_data,
                node_prop_key(*node, key),
                value.encode(),
            );
            if scalar_indexable_value(value) {
                for label in final_node_labels(
                    *node,
                    &snapshot,
                    &created_node_labels,
                    &self.label_additions,
                    &self.label_removals,
                ) {
                    batch.insert(
                        &self.engine.keyspaces.graph_data,
                        node_prop_index_key(label, key, value, *node),
                        [],
                    );
                }
            }
        }

        for ((edge, key), value) in &self.edge_props {
            if detached_edges.contains(edge) || self.tombstoned_edges.contains(edge) {
                continue;
            }
            batch.insert(
                &self.engine.keyspaces.graph_data,
                edge_prop_key(*edge, key),
                value.encode(),
            );
        }

        for (node, key) in &self.removed_node_props {
            if self.tombstoned_nodes.contains(node) {
                continue;
            }
            for old_key in snapshot_node_property_index_keys_for_property(*node, key, &snapshot) {
                batch.remove(&self.engine.keyspaces.graph_data, old_key);
            }
            batch.remove(&self.engine.keyspaces.graph_data, node_prop_key(*node, key));
        }

        for (edge, key) in &self.removed_edge_props {
            if detached_edges.contains(edge) || self.tombstoned_edges.contains(edge) {
                continue;
            }
            batch.remove(&self.engine.keyspaces.graph_data, edge_prop_key(*edge, key));
        }
        profile::event_since(
            "WriteTxn::commit.property_index_writes",
            property_index_writes_started,
            &[
                ("label_additions", self.label_additions.len() as u64),
                ("label_removals", self.label_removals.len() as u64),
                ("node_props", self.node_props.len() as u64),
                ("removed_node_props", self.removed_node_props.len() as u64),
                ("edge_props", self.edge_props.len() as u64),
                ("removed_edge_props", self.removed_edge_props.len() as u64),
            ],
        );

        let batch_commit_started = profile::start();
        batch.commit()?;
        profile::event_since("WriteTxn::commit.batch_commit", batch_commit_started, &[]);
        profile::event_since("WriteTxn::commit", commit_started, &[]);
        Ok(())
    }
}

fn open_keyspaces(db: &Database, meta: Keyspace) -> Result<Keyspaces> {
    Ok(Keyspaces {
        meta,
        graph_data: db.keyspace("graph_data", KeyspaceCreateOptions::default)?,
        adj_out: db.keyspace("adj_out", KeyspaceCreateOptions::default)?,
        adj_in: db.keyspace("adj_in", KeyspaceCreateOptions::default)?,
    })
}

fn ensure_meta(db: &Database, meta: &Keyspace) -> Result<()> {
    let meta_started = profile::start();
    if let Some(found) = read_meta_u64(meta, META_FORMAT_EPOCH)? {
        if found != STORAGE_FORMAT_EPOCH {
            return Err(Error::StorageFormatMismatch {
                expected: STORAGE_FORMAT_EPOCH,
                found,
            });
        }
        profile::event_since("GraphEngine::open.meta", meta_started, &[]);
        return Ok(());
    }

    let mut batch = db.batch().durability(Some(PersistMode::SyncAll));
    batch.insert(meta, META_FORMAT_EPOCH, STORAGE_FORMAT_EPOCH.to_be_bytes());
    batch.insert(meta, META_NEXT_NODE_ID, 0u64.to_be_bytes());
    batch.insert(meta, META_NEXT_LABEL_ID, 1u64.to_be_bytes());
    batch.insert(meta, META_NEXT_REL_TYPE_ID, 1u64.to_be_bytes());
    batch.commit()?;
    profile::event_since("GraphEngine::open.meta", meta_started, &[]);
    Ok(())
}

fn read_meta_u64(meta: &Keyspace, key: &[u8]) -> Result<Option<u64>> {
    meta.get(key)?
        .map(|value| {
            let bytes = value.as_ref();
            if bytes.len() != 8 {
                return Err(Error::StorageCorrupted(format!(
                    "meta key {} has invalid length {}",
                    String::from_utf8_lossy(key),
                    bytes.len()
                )));
            }
            let mut raw = [0u8; 8];
            raw.copy_from_slice(bytes);
            Ok(u64::from_be_bytes(raw))
        })
        .transpose()
}

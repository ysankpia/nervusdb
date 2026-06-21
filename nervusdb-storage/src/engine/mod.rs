use crate::snapshot::{Snapshot, id_key, name_key};
use crate::{Error, Result, STORAGE_FORMAT_EPOCH};
use fjall::{Database, Keyspace, KeyspaceCreateOptions, PersistMode};
use nervusdb_api::{
    EdgeKey, ExternalId, GraphSnapshot, GraphStore, InternalNodeId, LabelId, PropertyValue,
    RelTypeId,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

pub(crate) const KEY_FLAG_TOMBSTONE: u8 = 0b0000_0001;

const META_FORMAT_EPOCH: &[u8] = b"format_epoch";
const META_NEXT_NODE_ID: &[u8] = b"next_node_id";
const META_NEXT_LABEL_ID: &[u8] = b"next_label_id";
const META_NEXT_REL_TYPE_ID: &[u8] = b"next_rel_type_id";

#[derive(Clone)]
pub(crate) struct Keyspaces {
    pub(crate) meta: Keyspace,
    pub(crate) nodes: Keyspace,
    pub(crate) ext2node: Keyspace,
    pub(crate) labels: Keyspace,
    pub(crate) reltypes: Keyspace,
    pub(crate) node_labels: Keyspace,
    pub(crate) label_nodes: Keyspace,
    pub(crate) adj_out: Keyspace,
    pub(crate) adj_in: Keyspace,
    pub(crate) node_props: Keyspace,
    pub(crate) edge_props: Keyspace,
}

impl std::fmt::Debug for Keyspaces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keyspaces").finish_non_exhaustive()
    }
}

pub struct GraphEngine {
    path: PathBuf,
    db: Database,
    keyspaces: Keyspaces,
    write_lock: Mutex<()>,
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
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)?;
        let db = Database::builder(&path).open()?;
        let keyspaces = open_keyspaces(&db)?;
        ensure_meta(&db, &keyspaces)?;

        Ok(Self {
            path,
            db,
            keyspaces,
            write_lock: Mutex::new(()),
        })
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
            staged_external_ids: HashSet::new(),
            label_additions: Vec::new(),
            label_removals: Vec::new(),
            created_edges: BTreeSet::new(),
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
            .ext2node
            .get(key_u64(external_id))
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
        self.get_or_create_name(&self.keyspaces.labels, META_NEXT_LABEL_ID, name)
    }

    pub fn get_or_create_rel_type(&self, name: &str) -> Result<RelTypeId> {
        let _guard = self.write_lock.lock().unwrap();
        self.get_or_create_name(&self.keyspaces.reltypes, META_NEXT_REL_TYPE_ID, name)
    }

    pub fn get_label_id(&self, name: &str) -> Option<LabelId> {
        self.get_name_id(&self.keyspaces.labels, name)
    }

    pub fn get_rel_type_id(&self, name: &str) -> Option<RelTypeId> {
        self.get_name_id(&self.keyspaces.reltypes, name)
    }

    pub fn get_label_name(&self, id: LabelId) -> Option<String> {
        self.get_id_name(&self.keyspaces.labels, id)
    }

    pub fn get_rel_type_name(&self, id: RelTypeId) -> Option<String> {
        self.get_id_name(&self.keyspaces.reltypes, id)
    }

    pub fn persist(&self) -> Result<()> {
        self.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }

    pub fn checkpoint_on_close(&self) -> Result<()> {
        self.persist()
    }

    fn get_name_id(&self, keyspace: &Keyspace, name: &str) -> Option<u32> {
        keyspace
            .get(name_key(name))
            .ok()
            .flatten()
            .and_then(|v| decode_u32(v.as_ref()))
    }

    fn get_id_name(&self, keyspace: &Keyspace, id: u32) -> Option<String> {
        keyspace
            .get(id_key(id))
            .ok()
            .flatten()
            .and_then(|v| String::from_utf8(v.as_ref().to_vec()).ok())
    }

    fn get_or_create_name(
        &self,
        keyspace: &Keyspace,
        counter_key: &[u8],
        name: &str,
    ) -> Result<u32> {
        if let Some(id) = self.get_name_id(keyspace, name) {
            return Ok(id);
        }

        let id = self.next_counter(counter_key)?;
        let mut batch = self.db.batch().durability(Some(PersistMode::SyncAll));
        batch.insert(keyspace, name_key(name), id.to_be_bytes());
        batch.insert(keyspace, id_key(id), name.as_bytes());
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
    staged_external_ids: HashSet<ExternalId>,
    label_additions: Vec<(InternalNodeId, LabelId)>,
    label_removals: Vec<(InternalNodeId, LabelId)>,
    created_edges: BTreeSet<EdgeKey>,
    tombstoned_nodes: BTreeSet<InternalNodeId>,
    tombstoned_edges: BTreeSet<EdgeKey>,
    node_props: HashMap<(InternalNodeId, String), PropertyValue>,
    edge_props: HashMap<(EdgeKey, String), PropertyValue>,
    removed_node_props: Vec<(InternalNodeId, String)>,
    removed_edge_props: Vec<(EdgeKey, String)>,
}

impl<'a> WriteTxn<'a> {
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

        let iid = self.engine.next_counter(META_NEXT_NODE_ID)?;
        self.staged_external_ids.insert(external_id);
        self.created_nodes.push(CreatedNode {
            iid,
            external_id,
            labels: BTreeSet::from([label_id]),
        });
        Ok(iid)
    }

    pub fn add_node_label(&mut self, node: InternalNodeId, label_id: LabelId) -> Result<()> {
        if let Some(created) = self.created_nodes.iter_mut().find(|n| n.iid == node) {
            created.labels.insert(label_id);
        } else {
            self.label_additions.push((node, label_id));
        }
        Ok(())
    }

    pub fn remove_node_label(&mut self, node: InternalNodeId, label_id: LabelId) -> Result<()> {
        if let Some(created) = self.created_nodes.iter_mut().find(|n| n.iid == node) {
            created.labels.remove(&label_id);
        } else {
            self.label_removals.push((node, label_id));
        }
        Ok(())
    }

    pub fn create_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        self.created_edges.insert(EdgeKey { src, rel, dst });
    }

    pub fn tombstone_node(&mut self, node: InternalNodeId) {
        self.tombstoned_nodes.insert(node);
    }

    pub fn tombstone_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        self.tombstoned_edges.insert(EdgeKey { src, rel, dst });
    }

    pub fn set_node_property(&mut self, node: InternalNodeId, key: String, value: PropertyValue) {
        self.node_props.insert((node, key), value);
    }

    pub fn set_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) {
        self.edge_props
            .insert((EdgeKey { src, rel, dst }, key), value);
    }

    pub fn remove_node_property(&mut self, node: InternalNodeId, key: &str) {
        self.removed_node_props.push((node, key.to_string()));
    }

    pub fn remove_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: &str,
    ) {
        self.removed_edge_props
            .push((EdgeKey { src, rel, dst }, key.to_string()));
    }

    pub fn get_or_create_label(&mut self, name: &str) -> Result<LabelId> {
        self.engine
            .get_or_create_name(&self.engine.keyspaces.labels, META_NEXT_LABEL_ID, name)
    }

    pub fn get_or_create_rel_type(&mut self, name: &str) -> Result<RelTypeId> {
        self.engine
            .get_or_create_name(&self.engine.keyspaces.reltypes, META_NEXT_REL_TYPE_ID, name)
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
        let mut batch = self
            .engine
            .db
            .batch()
            .durability(Some(PersistMode::SyncAll));
        let snapshot = self.engine.begin_read();

        for node in &self.created_nodes {
            batch.insert(
                &self.engine.keyspaces.nodes,
                key_u32(node.iid),
                encode_node_value(node.external_id, 0),
            );
            batch.insert(
                &self.engine.keyspaces.ext2node,
                key_u64(node.external_id),
                key_u32(node.iid),
            );
            for label in &node.labels {
                batch.insert(
                    &self.engine.keyspaces.node_labels,
                    node_label_key(node.iid, *label),
                    [],
                );
                batch.insert(
                    &self.engine.keyspaces.label_nodes,
                    label_node_key(*label, node.iid),
                    [],
                );
            }
        }

        for (node, label) in &self.label_additions {
            batch.insert(
                &self.engine.keyspaces.node_labels,
                node_label_key(*node, *label),
                [],
            );
            batch.insert(
                &self.engine.keyspaces.label_nodes,
                label_node_key(*label, *node),
                [],
            );
        }

        for (node, label) in &self.label_removals {
            batch.remove(
                &self.engine.keyspaces.node_labels,
                node_label_key(*node, *label),
            );
            batch.remove(
                &self.engine.keyspaces.label_nodes,
                label_node_key(*label, *node),
            );
        }

        for edge in &self.created_edges {
            batch.insert(&self.engine.keyspaces.adj_out, adj_out_key(*edge), []);
            batch.insert(&self.engine.keyspaces.adj_in, adj_in_key(*edge), []);
        }

        for edge in &self.tombstoned_edges {
            batch.remove(&self.engine.keyspaces.adj_out, adj_out_key(*edge));
            batch.remove(&self.engine.keyspaces.adj_in, adj_in_key(*edge));
            for key in snapshot.collect_edge_property_keys(*edge) {
                batch.remove(&self.engine.keyspaces.edge_props, key);
            }
        }

        for node in &self.tombstoned_nodes {
            if let Some(external_id) = snapshot.resolve_external(*node) {
                batch.insert(
                    &self.engine.keyspaces.nodes,
                    key_u32(*node),
                    encode_node_value(external_id, KEY_FLAG_TOMBSTONE),
                );
            }
        }

        for ((node, key), value) in &self.node_props {
            batch.insert(
                &self.engine.keyspaces.node_props,
                node_prop_prefix(*node, key),
                value.encode(),
            );
        }

        for ((edge, key), value) in &self.edge_props {
            let mut storage_key = edge_prefix(*edge);
            storage_key.extend_from_slice(&(key.len() as u32).to_be_bytes());
            storage_key.extend_from_slice(key.as_bytes());
            batch.insert(
                &self.engine.keyspaces.edge_props,
                storage_key,
                value.encode(),
            );
        }

        for (node, key) in &self.removed_node_props {
            batch.remove(
                &self.engine.keyspaces.node_props,
                node_prop_prefix(*node, key),
            );
        }

        for (edge, key) in &self.removed_edge_props {
            let mut storage_key = edge_prefix(*edge);
            storage_key.extend_from_slice(&(key.len() as u32).to_be_bytes());
            storage_key.extend_from_slice(key.as_bytes());
            batch.remove(&self.engine.keyspaces.edge_props, storage_key);
        }

        batch.commit()?;
        Ok(())
    }
}

fn open_keyspaces(db: &Database) -> Result<Keyspaces> {
    Ok(Keyspaces {
        meta: db.keyspace("meta", KeyspaceCreateOptions::default)?,
        nodes: db.keyspace("nodes", KeyspaceCreateOptions::default)?,
        ext2node: db.keyspace("ext2node", KeyspaceCreateOptions::default)?,
        labels: db.keyspace("labels", KeyspaceCreateOptions::default)?,
        reltypes: db.keyspace("reltypes", KeyspaceCreateOptions::default)?,
        node_labels: db.keyspace("node_labels", KeyspaceCreateOptions::default)?,
        label_nodes: db.keyspace("label_nodes", KeyspaceCreateOptions::default)?,
        adj_out: db.keyspace("adj_out", KeyspaceCreateOptions::default)?,
        adj_in: db.keyspace("adj_in", KeyspaceCreateOptions::default)?,
        node_props: db.keyspace("node_props", KeyspaceCreateOptions::default)?,
        edge_props: db.keyspace("edge_props", KeyspaceCreateOptions::default)?,
    })
}

fn ensure_meta(db: &Database, keyspaces: &Keyspaces) -> Result<()> {
    if let Some(found) = read_meta_u64(&keyspaces.meta, META_FORMAT_EPOCH)? {
        if found != STORAGE_FORMAT_EPOCH {
            return Err(Error::StorageFormatMismatch {
                expected: STORAGE_FORMAT_EPOCH,
                found,
            });
        }
        return Ok(());
    }

    let mut batch = db.batch().durability(Some(PersistMode::SyncAll));
    batch.insert(
        &keyspaces.meta,
        META_FORMAT_EPOCH,
        STORAGE_FORMAT_EPOCH.to_be_bytes(),
    );
    batch.insert(&keyspaces.meta, META_NEXT_NODE_ID, 0u64.to_be_bytes());
    batch.insert(&keyspaces.meta, META_NEXT_LABEL_ID, 1u64.to_be_bytes());
    batch.insert(&keyspaces.meta, META_NEXT_REL_TYPE_ID, 1u64.to_be_bytes());
    batch.commit()?;
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

pub(crate) fn key_u32(value: u32) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

pub(crate) fn key_u64(value: u64) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

pub(crate) fn decode_u32(bytes: &[u8]) -> Option<u32> {
    let raw: [u8; 4] = bytes.try_into().ok()?;
    Some(u32::from_be_bytes(raw))
}

pub(crate) fn parse_iid_key(bytes: &[u8]) -> Option<InternalNodeId> {
    decode_u32(bytes)
}

pub(crate) fn encode_node_value(external_id: ExternalId, flags: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.extend_from_slice(&external_id.to_be_bytes());
    out.push(flags);
    out
}

pub(crate) fn decode_node_value(bytes: &[u8]) -> Option<(ExternalId, u8)> {
    parse_node_value(bytes)
}

pub(crate) fn parse_node_value(bytes: &[u8]) -> Option<(ExternalId, u8)> {
    if bytes.len() < 9 {
        return None;
    }
    let raw: [u8; 8] = bytes[..8].try_into().ok()?;
    Some((u64::from_be_bytes(raw), bytes[8]))
}

fn node_label_key(node: InternalNodeId, label: LabelId) -> Vec<u8> {
    let mut key = Vec::with_capacity(8);
    key.extend_from_slice(&node.to_be_bytes());
    key.extend_from_slice(&label.to_be_bytes());
    key
}

fn label_node_key(label: LabelId, node: InternalNodeId) -> Vec<u8> {
    let mut key = Vec::with_capacity(8);
    key.extend_from_slice(&label.to_be_bytes());
    key.extend_from_slice(&node.to_be_bytes());
    key
}

pub(crate) fn parse_label_node_key(key: &[u8]) -> Option<InternalNodeId> {
    if key.len() != 8 {
        return None;
    }
    decode_u32(&key[4..8])
}

pub(crate) fn edge_prefix(edge: EdgeKey) -> Vec<u8> {
    let mut key = Vec::with_capacity(12);
    key.extend_from_slice(&edge.src.to_be_bytes());
    key.extend_from_slice(&edge.rel.to_be_bytes());
    key.extend_from_slice(&edge.dst.to_be_bytes());
    key
}

fn adj_out_key(edge: EdgeKey) -> Vec<u8> {
    edge_prefix(edge)
}

fn adj_in_key(edge: EdgeKey) -> Vec<u8> {
    let mut key = Vec::with_capacity(12);
    key.extend_from_slice(&edge.dst.to_be_bytes());
    key.extend_from_slice(&edge.rel.to_be_bytes());
    key.extend_from_slice(&edge.src.to_be_bytes());
    key
}

pub(crate) fn edge_key_from_adj_out(key: &[u8]) -> Option<EdgeKey> {
    if key.len() != 12 {
        return None;
    }
    Some(EdgeKey {
        src: decode_u32(&key[0..4])?,
        rel: decode_u32(&key[4..8])?,
        dst: decode_u32(&key[8..12])?,
    })
}

pub(crate) fn edge_key_from_adj_in(key: &[u8]) -> Option<EdgeKey> {
    if key.len() != 12 {
        return None;
    }
    Some(EdgeKey {
        dst: decode_u32(&key[0..4])?,
        rel: decode_u32(&key[4..8])?,
        src: decode_u32(&key[8..12])?,
    })
}

pub(crate) fn node_prop_prefix(node: InternalNodeId, key: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + key.len());
    out.extend_from_slice(&node.to_be_bytes());
    out.extend_from_slice(&(key.len() as u32).to_be_bytes());
    out.extend_from_slice(key.as_bytes());
    out
}

pub(crate) fn parse_node_prop_key(key: &[u8], node: InternalNodeId) -> Option<String> {
    if key.len() < 8 || decode_u32(&key[0..4])? != node {
        return None;
    }
    let raw_len: [u8; 4] = key[4..8].try_into().ok()?;
    let len = u32::from_be_bytes(raw_len) as usize;
    if key.len() != 8 + len {
        return None;
    }
    String::from_utf8(key[8..].to_vec()).ok()
}

pub(crate) fn parse_prop_value(bytes: &[u8]) -> Result<PropertyValue> {
    PropertyValue::decode(bytes).map_err(|e| Error::PropertyDecode(e.to_string()))
}

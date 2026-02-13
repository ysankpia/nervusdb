use crate::csr::{CsrSegment, EdgeRecord, SegmentId};
use crate::idmap::{ExternalId, I2eRecord, IdMap, InternalNodeId, LabelId};
use crate::index::btree::BTree;
use crate::index::catalog::IndexCatalog;
use crate::index::hnsw::HnswIndex;
use crate::index::hnsw::params::HnswParams;
use crate::index::hnsw::storage::{PersistentGraphStorage, PersistentVectorStorage};
use crate::index::ordered_key::encode_ordered_value;
use crate::label_interner::{LabelInterner, LabelSnapshot};
use crate::memtable::MemTable;
use crate::pager::{PageId, Pager};
use crate::read_path_engine_idmap::{
    lookup_internal_node_id, read_i2e_snapshot, read_i2l_snapshot,
};
use crate::read_path_engine_labels::{
    lookup_label_id, lookup_label_name, published_label_snapshot,
};
use crate::read_path_engine_view::{
    build_snapshot_from_published, load_properties_and_stats_roots,
};
use crate::snapshot::{L0Run, RelTypeId, Snapshot};
use crate::wal::{CommittedTx, SegmentPointer, Wal, WalRecord};
use crate::{Error, Result};
use nervusdb_v2_api::{GraphSnapshot, GraphStore};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

type NativeHnsw = HnswIndex<PersistentVectorStorage, PersistentGraphStorage>;

fn parse_hnsw_env_usize(name: &str, default_value: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default_value)
}

fn load_hnsw_params_from_env() -> HnswParams {
    HnswParams {
        m: parse_hnsw_env_usize("NERVUSDB_HNSW_M", 16),
        ef_construction: parse_hnsw_env_usize("NERVUSDB_HNSW_EF_CONSTRUCTION", 200),
        ef_search: parse_hnsw_env_usize("NERVUSDB_HNSW_EF_SEARCH", 200),
    }
}

#[derive(Debug)]
pub struct GraphEngine {
    ndb_path: PathBuf,
    wal_path: PathBuf,

    pager: Arc<RwLock<Pager>>,
    wal: Mutex<Wal>,
    idmap: Mutex<IdMap>,
    label_interner: Mutex<LabelInterner>,
    index_catalog: Arc<Mutex<IndexCatalog>>,

    // T203: Vector Search Index
    vector_index: Arc<Mutex<NativeHnsw>>,

    published_runs: RwLock<Arc<Vec<Arc<L0Run>>>>,
    published_segments: RwLock<Arc<Vec<Arc<CsrSegment>>>>,
    published_labels: RwLock<Arc<LabelSnapshot>>,
    published_node_labels: RwLock<Arc<Vec<Vec<LabelId>>>>,
    write_lock: Mutex<()>,
    next_txid: AtomicU64,
    next_segment_id: AtomicU64,
    manifest_epoch: AtomicU64,
    checkpoint_txid: AtomicU64,
    properties_root: AtomicU64,
    stats_root: AtomicU64,
}

impl GraphEngine {
    pub fn open(ndb_path: impl AsRef<Path>, wal_path: impl AsRef<Path>) -> Result<Self> {
        let ndb_path = ndb_path.as_ref().to_path_buf();
        let wal_path = wal_path.as_ref().to_path_buf();

        let mut pager = Pager::open(&ndb_path)?;
        let wal = Wal::open(&wal_path)?;

        let mut idmap = IdMap::load(&mut pager)?;
        let mut index_catalog = IndexCatalog::open_or_create(&mut pager)?;

        // Initialize HNSW Index (T203)
        // We use RESERVED names in IndexCatalog to store the roots for Vector and Graph BTrees.
        let vec_def = index_catalog.get_or_create(&mut pager, "__sys_hnsw_vec")?;
        let graph_def = index_catalog.get_or_create(&mut pager, "__sys_hnsw_graph")?;

        let v_store = PersistentVectorStorage::new(BTree::load(vec_def.root));
        let g_store = PersistentGraphStorage::new(BTree::load(graph_def.root));
        let params = load_hnsw_params_from_env();
        // HnswIndex::load needs generic Ctx = &mut Pager
        let vector_index = HnswIndex::load(params, v_store, g_store, &mut pager)?;

        let committed = wal.replay_committed()?;
        let state = scan_recovery_state(&committed);

        let mut segments: Vec<Arc<CsrSegment>> = Vec::new();
        let mut max_seg_id = 0u64;
        for ptr in &state.manifest_segments {
            let seg = Arc::new(CsrSegment::load(&mut pager, ptr.meta_page_id)?);
            if seg.id.0 != ptr.id {
                return Err(Error::WalProtocol("csr segment id mismatch"));
            }
            max_seg_id = max_seg_id.max(seg.id.0);
            segments.push(seg);
        }

        // Build label interner from recovered state (first, before graph transactions)
        let mut label_interner = LabelInterner::new();
        replay_label_transactions(&committed, &mut label_interner)?;

        let mut runs = Vec::new();
        replay_graph_transactions(
            &mut pager,
            &mut idmap,
            committed.clone(),
            state.checkpoint_txid,
            &mut runs,
        )?;

        runs.reverse(); // newest first for read path

        let label_snapshot = label_interner.snapshot();
        let node_labels_snapshot = idmap.get_i2l_snapshot();

        Ok(Self {
            ndb_path,
            wal_path,
            pager: Arc::new(RwLock::new(pager)),
            wal: Mutex::new(wal),
            idmap: Mutex::new(idmap),
            label_interner: Mutex::new(label_interner),
            index_catalog: Arc::new(Mutex::new(index_catalog)),
            vector_index: Arc::new(Mutex::new(vector_index)),
            published_runs: RwLock::new(Arc::new(runs)),
            published_segments: RwLock::new(Arc::new(segments)),
            published_labels: RwLock::new(Arc::new(label_snapshot)),
            published_node_labels: RwLock::new(Arc::new(node_labels_snapshot)),
            write_lock: Mutex::new(()),
            next_txid: AtomicU64::new(state.max_txid.saturating_add(1).max(1)),
            next_segment_id: AtomicU64::new(max_seg_id.saturating_add(1).max(1)),
            manifest_epoch: AtomicU64::new(state.manifest_epoch),
            checkpoint_txid: AtomicU64::new(state.checkpoint_txid),
            properties_root: AtomicU64::new(state.properties_root),
            stats_root: AtomicU64::new(state.stats_root),
        })
    }

    #[inline]
    pub fn ndb_path(&self) -> &Path {
        &self.ndb_path
    }

    #[inline]
    pub fn wal_path(&self) -> &Path {
        &self.wal_path
    }

    pub(crate) fn get_pager(&self) -> Arc<RwLock<Pager>> {
        self.pager.clone()
    }

    pub(crate) fn get_index_catalog(&self) -> Arc<Mutex<IndexCatalog>> {
        self.index_catalog.clone()
    }

    /// Creates a B-Tree index for the given label and property.
    ///
    /// If the index already exists, this is a no-op.
    /// Note: This MVP does not backfill existing data. The index will only track
    /// valid data inserted *after* index creation.
    pub fn create_index(&self, label: &str, field: &str) -> Result<()> {
        let mut catalog = self.index_catalog.lock().unwrap();
        let name = format!("{}.{}", label, field);
        if catalog.get(&name).is_some() {
            return Ok(());
        }

        let mut pager = self.pager.write().unwrap();
        catalog.get_or_create(&mut pager, &name)?;
        catalog.flush(&mut pager)?;
        Ok(())
    }

    pub fn begin_read(&self) -> Snapshot {
        let runs = self.published_runs.read().unwrap().clone();
        let segments = self.published_segments.read().unwrap().clone();
        let labels = self.published_labels.read().unwrap().clone();
        let node_labels = self.published_node_labels.read().unwrap().clone();
        let (properties_root, stats_root) =
            load_properties_and_stats_roots(&self.properties_root, &self.stats_root);
        build_snapshot_from_published(
            runs,
            segments,
            labels,
            node_labels,
            properties_root,
            stats_root,
        )
    }

    pub fn begin_write(&self) -> WriteTxn<'_> {
        let guard = self.write_lock.lock().unwrap();
        let txid = self.next_txid.fetch_add(1, Ordering::Relaxed);
        WriteTxn {
            engine: self,
            _guard: guard,
            txid,
            created_nodes: Vec::new(),
            pending_label_additions: Vec::new(),
            pending_label_removals: Vec::new(),
            created_external_ids: std::collections::HashSet::new(),
            memtable: MemTable::default(),
        }
    }

    pub fn lookup_internal_id(&self, external_id: ExternalId) -> Option<InternalNodeId> {
        lookup_internal_node_id(&self.idmap, external_id)
    }

    /// Get or create a label, returns the label ID.
    ///
    /// This is a write operation and must be called within a write transaction.
    pub fn get_or_create_label(&self, name: &str) -> Result<LabelId> {
        // Optimistic read
        {
            let interner = self.label_interner.lock().unwrap();
            if let Some(id) = interner.get_id(name) {
                return Ok(id);
            }
        }

        // Write path: serialize with a lock or just rely on interner lock?
        // We need to write to WAL, so let's handle it carefully.
        // We'll just lock interner, check again, then write WAL, then update interner.
        let mut interner = self.label_interner.lock().unwrap();
        if let Some(id) = interner.get_id(name) {
            return Ok(id);
        }

        // It's a new label.
        // We update memory first to get the authoritative ID.
        let returned_id = interner.get_or_create(name);

        // Durability: Log to WAL (post-facto, but before return)
        // We wrap this in a mini-transaction to ensure replayability.
        {
            let txid = self.next_txid.fetch_add(1, Ordering::Relaxed);
            let mut wal = self.wal.lock().unwrap();
            wal.append(&WalRecord::BeginTx { txid })?;
            wal.append(&WalRecord::CreateLabel {
                name: name.to_string(),
                label_id: returned_id,
            })?;
            wal.append(&WalRecord::CommitTx { txid })?;
            wal.fsync()?;
        }

        // Update Published Snapshot
        let snapshot = interner.snapshot();
        let mut published = self.published_labels.write().unwrap();
        *published = Arc::new(snapshot);

        Ok(returned_id)
    }

    /// Update published node labels from IdMap.
    /// Should be called after write transactions that create nodes.
    fn update_published_node_labels(&self) {
        let snapshot = read_i2l_snapshot(&self.idmap);
        let mut published = self.published_node_labels.write().unwrap();
        *published = Arc::new(snapshot);
    }

    /// Get a snapshot of the current label state for reading.
    pub fn label_snapshot(&self) -> Arc<LabelSnapshot> {
        published_label_snapshot(&self.published_labels)
    }

    /// Get label ID by name, returns None if not found.
    pub fn get_label_id(&self, name: &str) -> Option<LabelId> {
        lookup_label_id(&self.label_interner, name)
    }

    /// Get label name by ID, returns None if not found.
    pub fn get_label_name(&self, id: LabelId) -> Option<String> {
        lookup_label_name(&self.label_interner, id)
    }

    // T203: HNSW Public API
    pub fn insert_vector(&self, id: InternalNodeId, vector: Vec<f32>) -> Result<()> {
        let mut pager = self.pager.write().unwrap();
        let mut idx = self.vector_index.lock().unwrap();
        idx.insert(&mut *pager, id, vector)
    }

    pub fn search_vector(&self, query: &[f32], k: usize) -> Result<Vec<(InternalNodeId, f32)>> {
        let mut pager = self.pager.write().unwrap();
        let mut idx = self.vector_index.lock().unwrap();
        idx.search(&mut *pager, query, k)
    }

    pub fn scan_i2e_records(&self) -> Vec<I2eRecord> {
        read_i2e_snapshot(&self.idmap)
    }

    fn publish_run(&self, run: Arc<L0Run>) {
        let mut current = self.published_runs.write().unwrap();
        let mut next = Vec::with_capacity(current.len() + 1);
        next.push(run);
        next.extend(current.iter().cloned());
        *current = Arc::new(next);
    }

    /// M2/T45: Explicit compaction.
    ///
    /// Invariants:
    /// - Writes CSR segment pages to `.ndb` and fsyncs before publishing the manifest in WAL.
    /// - Writes `ManifestSwitch` + `Checkpoint` as a committed WAL tx to make the switch atomic.
    pub fn compact(&self) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap();

        let runs = self.published_runs.read().unwrap().clone();

        if runs.is_empty() {
            return Ok(());
        }

        let has_properties = runs.iter().any(|r| r.has_properties());

        let seg_id = SegmentId(self.next_segment_id.fetch_add(1, Ordering::Relaxed));
        let mut seg = build_segment_from_runs(seg_id, &runs);

        {
            let mut pager = self.pager.write().unwrap();
            seg.persist(&mut pager)?;
            pager.sync()?;
        }

        let up_to_txid = runs.iter().map(|r| r.txid()).max().unwrap_or(0);
        let epoch = self.manifest_epoch.load(Ordering::Relaxed) + 1;

        let new_segments = {
            let current = self.published_segments.read().unwrap().clone();
            let mut next = Vec::with_capacity(current.len() + 1);
            next.push(Arc::new(seg));
            next.extend(current.iter().cloned());
            Arc::new(next)
        };

        // Property Sinking: Persist properties from L0Runs into the B-Tree Property Store.
        let mut sink_node_props = BTreeMap::new();
        let mut sink_edge_props = BTreeMap::new();
        for run in runs.iter() {
            for (node, props) in &run.node_properties {
                for (key, val) in props {
                    sink_node_props
                        .entry((*node, key.clone()))
                        .or_insert(val.clone());
                }
            }
            for (edge, props) in &run.edge_properties {
                for (key, val) in props {
                    sink_edge_props
                        .entry((*edge, key.clone()))
                        .or_insert(val.clone());
                }
            }
        }

        let mut current_root = self.properties_root.load(Ordering::SeqCst);
        if !sink_node_props.is_empty() || !sink_edge_props.is_empty() {
            let mut pager = self.pager.write().unwrap();
            let mut tree = if current_root == 0 {
                BTree::create(&mut pager)?
            } else {
                BTree::load(PageId::new(current_root))
            };

            // Sink Node Properties (Tag 0)
            for ((node, key), value) in sink_node_props {
                let mut btree_key = Vec::with_capacity(1 + 4 + 4 + key.len());
                btree_key.push(0u8); // Tag 0: Node Property
                btree_key.extend_from_slice(&node.to_be_bytes());
                btree_key.extend_from_slice(&(key.len() as u32).to_be_bytes());
                btree_key.extend_from_slice(key.as_bytes());

                let encoded_val = value.encode();
                let blob_id = crate::blob_store::BlobStore::write(&mut pager, &encoded_val)?;
                tree.insert(&mut pager, &btree_key, blob_id)?;
            }

            // Sink Edge Properties (Tag 1)
            for ((edge, key), value) in sink_edge_props {
                let mut btree_key = Vec::with_capacity(1 + 4 + 4 + 4 + 4 + key.len());
                btree_key.push(1u8); // Tag 1: Edge Property
                btree_key.extend_from_slice(&edge.src.to_be_bytes());
                btree_key.extend_from_slice(&edge.rel.to_be_bytes());
                btree_key.extend_from_slice(&edge.dst.to_be_bytes());
                btree_key.extend_from_slice(&(key.len() as u32).to_be_bytes());
                btree_key.extend_from_slice(key.as_bytes());

                let encoded_val = value.encode();
                let blob_id = crate::blob_store::BlobStore::write(&mut pager, &encoded_val)?;
                tree.insert(&mut pager, &btree_key, blob_id)?;
            }

            current_root = tree.root().as_u64();
        }

        // Statistics Collection - read directly from IdMap for accuracy
        let mut stats = crate::stats::GraphStatistics::default();
        {
            let idmap = self.idmap.lock().unwrap();
            let node_labels = idmap.get_i2l_snapshot();

            // Count nodes per label (node_labels[iid] = vec of label_ids for that node)
            stats.total_nodes = node_labels.len() as u64;
            for labels in node_labels.iter() {
                for &label_id in labels.iter() {
                    *stats.node_counts_by_label.entry(label_id).or_default() += 1;
                }
            }
        }

        for seg in new_segments.iter() {
            stats.total_edges += seg.edges.len() as u64;
            for edge in &seg.edges {
                *stats.edge_counts_by_type.entry(edge.rel).or_default() += 1;
            }
        }

        let stats_root;
        {
            let mut pager = self.pager.write().unwrap();
            let encoded_stats = stats.encode();
            stats_root = crate::blob_store::BlobStore::write(&mut pager, &encoded_stats)?;
        }

        let pointers: Vec<SegmentPointer> = new_segments
            .iter()
            .map(|s| SegmentPointer {
                id: s.id.0,
                meta_page_id: s.meta_page_id,
            })
            .collect();

        let system_txid = self.next_txid.fetch_add(1, Ordering::Relaxed);
        {
            let mut wal = self.wal.lock().unwrap();
            wal.append(&WalRecord::BeginTx { txid: system_txid })?;
            wal.append(&WalRecord::ManifestSwitch {
                epoch,
                segments: pointers,
                properties_root: current_root,
                stats_root,
            })?;
            // After sinking properties, we can safely checkpoint up_to_txid
            wal.append(&WalRecord::Checkpoint {
                up_to_txid,
                epoch,
                properties_root: current_root,
                stats_root,
            })?;
            wal.append(&WalRecord::CommitTx { txid: system_txid })?;
            wal.fsync()?;
        }

        // 4. Update memory state
        self.checkpoint_txid.store(up_to_txid, Ordering::SeqCst);
        self.properties_root.store(current_root, Ordering::SeqCst);
        self.stats_root.store(stats_root, Ordering::SeqCst);
        {
            let mut cur_runs = self.published_runs.write().unwrap();
            *cur_runs = Arc::new(Vec::new());
        }
        {
            let mut cur_segs = self.published_segments.write().unwrap();
            *cur_segs = new_segments;
        }

        self.manifest_epoch.store(epoch, Ordering::Relaxed);
        if !has_properties {
            self.checkpoint_txid.store(up_to_txid, Ordering::Relaxed);
        }
        Ok(())
    }

    /// T106: Checkpoint-on-Close (WAL compaction).
    ///
    /// Safety rule:
    /// - Only allowed when there are no published L0 runs (otherwise we'd lose data that only exists in WAL).
    ///
    /// The resulting WAL contains a single committed tx that replays:
    /// - label mappings (`CreateLabel`) and
    /// - the current manifest (`ManifestSwitch`) plus
    /// - a `Checkpoint` that allows recovery to skip older graph tx.
    pub fn checkpoint_on_close(&self) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap();

        let runs = self.published_runs.read().unwrap().clone();
        if !runs.is_empty() {
            // Cannot compact WAL safely while L0 runs (esp. properties) are WAL-only.
            // Best-effort durability: flush NDB + WAL.
            {
                let mut pager = self.pager.write().unwrap();
                pager.sync()?;
            }
            {
                let mut wal = self.wal.lock().unwrap();
                wal.fsync()?;
            }
            return Ok(());
        }

        // Ensure idmap/pages are durable before allowing recovery to skip old WAL.
        {
            let mut pager = self.pager.write().unwrap();
            pager.sync()?;
        }

        let labels = {
            let interner = self.label_interner.lock().unwrap();
            interner.snapshot()
        };

        let segments = self.published_segments.read().unwrap().clone();
        let pointers: Vec<SegmentPointer> = segments
            .iter()
            .map(|s| SegmentPointer {
                id: s.id.0,
                meta_page_id: s.meta_page_id,
            })
            .collect();

        let up_to_txid = self.next_txid.load(Ordering::Relaxed).saturating_sub(1);
        let epoch = self.manifest_epoch.load(Ordering::Relaxed);
        let system_txid = self.next_txid.fetch_add(1, Ordering::Relaxed);

        let mut ops: Vec<WalRecord> = Vec::new();
        for id in labels.iter_ids() {
            if let Some(name) = labels.get_name(id) {
                ops.push(WalRecord::CreateLabel {
                    name: name.to_string(),
                    label_id: id,
                });
            }
        }

        let (properties_root, stats_root) =
            load_properties_and_stats_roots(&self.properties_root, &self.stats_root);
        ops.push(WalRecord::ManifestSwitch {
            epoch,
            segments: pointers,
            properties_root,
            stats_root,
        });
        ops.push(WalRecord::Checkpoint {
            up_to_txid,
            epoch,
            properties_root,
            stats_root,
        });

        {
            let mut wal = self.wal.lock().unwrap();
            wal.rewrite_as_snapshot(system_txid, ops)?;
            wal.fsync()?;
        }

        Ok(())
    }
}

fn build_segment_from_runs(seg_id: SegmentId, runs: &Arc<Vec<Arc<L0Run>>>) -> CsrSegment {
    // Apply the same semantics as snapshot merge: newest->oldest, key-based tombstones.
    use std::collections::{BTreeMap, HashSet};

    let mut blocked_nodes: HashSet<InternalNodeId> = HashSet::new();
    let mut blocked_edges: HashSet<crate::snapshot::EdgeKey> = HashSet::new();
    let mut edges: Vec<crate::snapshot::EdgeKey> = Vec::new();

    for run in runs.iter() {
        blocked_nodes.extend(run.iter_tombstoned_nodes());
        blocked_edges.extend(run.iter_tombstoned_edges());

        for e in run.iter_edges() {
            if blocked_nodes.contains(&e.src) || blocked_nodes.contains(&e.dst) {
                continue;
            }
            if blocked_edges.contains(&e) {
                continue;
            }
            edges.push(e);
        }
    }

    edges.sort();

    let (min_src, max_src) = edges.iter().fold((u32::MAX, 0u32), |(min_s, max_s), e| {
        (min_s.min(e.src), max_s.max(e.src))
    });

    if edges.is_empty() {
        return CsrSegment {
            id: seg_id,
            meta_page_id: 0,
            min_src: 0,
            max_src: 0,
            min_dst: 0,
            max_dst: 0,
            offsets: vec![0, 0],
            edges: Vec::new(),
            in_offsets: Vec::new(),
            in_edges: Vec::new(),
        };
    }

    let range = (max_src - min_src) as usize + 2;
    let mut offsets = vec![0u64; range];
    let mut edges_by_src: BTreeMap<InternalNodeId, Vec<EdgeRecord>> = BTreeMap::new();
    for e in edges {
        edges_by_src.entry(e.src).or_default().push(EdgeRecord {
            rel: e.rel,
            dst: e.dst,
        });
    }

    let mut edge_vec: Vec<EdgeRecord> = Vec::new();
    let mut cursor = 0u64;
    for src in min_src..=max_src {
        let idx = (src - min_src) as usize;
        offsets[idx] = cursor;
        if let Some(mut list) = edges_by_src.remove(&src) {
            list.sort_by_key(|r| (r.rel, r.dst));
            cursor += list.len() as u64;
            edge_vec.extend(list);
        }
    }
    offsets[(max_src - min_src) as usize + 1] = cursor;

    CsrSegment {
        id: seg_id,
        meta_page_id: 0,
        min_src,
        max_src,
        min_dst: 0,
        max_dst: 0,
        offsets,
        edges: edge_vec,
        in_offsets: Vec::new(),
        in_edges: Vec::new(),
    }
}

pub struct WriteTxn<'a> {
    engine: &'a GraphEngine,
    _guard: std::sync::MutexGuard<'a, ()>,
    txid: u64,
    created_nodes: Vec<(ExternalId, LabelId, InternalNodeId)>,
    pending_label_additions: Vec<(InternalNodeId, LabelId)>,
    pending_label_removals: Vec<(InternalNodeId, LabelId)>,
    created_external_ids: std::collections::HashSet<ExternalId>,
    memtable: MemTable,
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
        // Reuse label interner for relationship types for now
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

    // T203: HNSW Support
    pub fn set_vector(&mut self, id: InternalNodeId, vector: Vec<f32>) -> Result<()> {
        self.engine.insert_vector(id, vector)
    }

    pub fn commit(self) -> Result<()> {
        // Extract property data before freezing (since freeze consumes memtable)
        let node_properties = self.memtable.node_properties_for_wal();
        let edge_properties = self.memtable.edge_properties_for_wal();
        let removed_node_props = self.memtable.removed_node_properties_for_wal();
        let removed_edge_props = self.memtable.removed_edge_properties_for_wal();

        let run = self.memtable.freeze_into_run(self.txid);

        // 1) Append WAL and fsync (durability Full by default).
        {
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

            // Write property operations
            // Write property operations
            for (node, key, value) in &node_properties {
                wal.append(&WalRecord::SetNodeProperty {
                    node: *node,
                    key: key.clone(),
                    value: value.clone(),
                })?;
            }
            // Removed Node properties
            for (node, key) in &removed_node_props {
                wal.append(&WalRecord::RemoveNodeProperty {
                    node: *node,
                    key: key.clone(),
                })?;
            }

            for (src, rel, dst, key, value) in edge_properties {
                wal.append(&WalRecord::SetEdgeProperty {
                    src,
                    rel,
                    dst,
                    key,
                    value,
                })?;
            }
            // Removed Edge properties
            for (src, rel, dst, key) in &removed_edge_props {
                wal.append(&WalRecord::RemoveEdgeProperty {
                    src: *src,
                    rel: *rel,
                    dst: *dst,
                    key: key.clone(),
                })?;
            }

            // T107/T108: Update Indexes
            // We separate Read (Old Values) phase from Write (Index Update) phase to avoid deadlocks
            // caused by holding pager lock during property lookup.
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

            // Create a snapshot for reading current state (for old values/labels)
            let snapshot = self.engine.snapshot();

            // Helper to convert API PropertyValue to Storage PropertyValue
            use crate::read_path_convert::convert_property_to_storage as to_storage;

            for (node, key, value) in &node_properties {
                let is_new = self.created_nodes.iter().any(|(_, _, iid)| iid == node);
                let label_id = if is_new {
                    self.created_nodes
                        .iter()
                        .find(|(_, _, iid)| iid == node)
                        .map(|(_, l, _)| *l)
                } else {
                    snapshot.node_label(*node)
                };

                if let Some(lid) = label_id {
                    // Resolve Label Name
                    let label_name = self
                        .engine
                        .label_interner
                        .lock()
                        .unwrap()
                        .get_name(lid)
                        .map(|s| s.to_string());

                    if let Some(label_name) = label_name {
                        let index_name = format!("{}.{}", label_name, key);
                        // Check if index exists without holding the lock for long
                        let has_index = self
                            .engine
                            .index_catalog
                            .lock()
                            .unwrap()
                            .get(&index_name)
                            .is_some();

                        if has_index {
                            if is_new {
                                index_ops.push((IndexOp::Insert(index_name, value.clone()), *node));
                            } else {
                                // For existing nodes, we need the old value to remove it from index
                                let old_value = snapshot.node_property(*node, key).map(to_storage);
                                index_ops.push((
                                    IndexOp::Update(index_name, old_value, value.clone()),
                                    *node,
                                ));
                            }
                        }
                    }
                }
            }

            // Removed properties index updates
            for (node, key) in &removed_node_props {
                // If it was created in this tx, it won't be in index yet, so removing it is no-op for index
                // (except if we added then removed in same tx, MemTable handles that by removing from node_properties)
                // So we only care about existing nodes.
                let is_new = self.created_nodes.iter().any(|(_, _, iid)| iid == node);
                if is_new {
                    continue;
                }

                let label_id = snapshot.node_label(*node);
                if let Some(lid) = label_id {
                    let label_name = self
                        .engine
                        .label_interner
                        .lock()
                        .unwrap()
                        .get_name(lid)
                        .map(|s| s.to_string());
                    if let Some(label_name) = label_name {
                        let index_name = format!("{}.{}", label_name, key);
                        let has_index = self
                            .engine
                            .index_catalog
                            .lock()
                            .unwrap()
                            .get(&index_name)
                            .is_some();

                        if has_index {
                            let old_value = snapshot.node_property(*node, key).map(to_storage);
                            index_ops.push((IndexOp::Remove(index_name, old_value), *node));
                        }
                    }
                }
            }

            // Apply Index Updates
            if !index_ops.is_empty() {
                let mut catalog = self.engine.index_catalog.lock().unwrap();
                let mut pager = self.engine.pager.write().unwrap();

                for (op, node_id) in index_ops {
                    match op {
                        IndexOp::Insert(name, val) => {
                            if let Some(re) = catalog.entries.get_mut(&name) {
                                let mut tree = crate::index::btree::BTree::load(re.root);

                                let mut key = Vec::new();
                                key.extend_from_slice(&re.id.to_be_bytes());
                                key.extend_from_slice(&encode_ordered_value(&val));

                                let _ = tree.insert(&mut pager, &key, node_id as u64);
                                re.root = tree.root();
                            }
                        }
                        IndexOp::Update(name, old_val_opt, new_val) => {
                            if let Some(re) = catalog.entries.get_mut(&name) {
                                let mut tree = crate::index::btree::BTree::load(re.root);

                                // 1. Remove old value
                                if let Some(old_val) = old_val_opt {
                                    let mut old_key = Vec::new();
                                    old_key.extend_from_slice(&re.id.to_be_bytes());
                                    old_key.extend_from_slice(&encode_ordered_value(&old_val));

                                    let _ = tree.delete(&mut pager, &old_key, node_id as u64);
                                }

                                // 2. Insert new value
                                let mut new_key = Vec::new();
                                new_key.extend_from_slice(&re.id.to_be_bytes());
                                new_key.extend_from_slice(&encode_ordered_value(&new_val));

                                let _ = tree.insert(&mut pager, &new_key, node_id as u64);
                                re.root = tree.root();
                            }
                        }
                        IndexOp::Remove(name, old_val_opt) => {
                            if let Some(re) = catalog.entries.get_mut(&name) {
                                let mut tree = crate::index::btree::BTree::load(re.root);

                                if let Some(old_val) = old_val_opt {
                                    let mut old_key = Vec::new();
                                    old_key.extend_from_slice(&re.id.to_be_bytes());
                                    old_key.extend_from_slice(&encode_ordered_value(&old_val));
                                    let _ = tree.delete(&mut pager, &old_key, node_id as u64);
                                    re.root = tree.root();
                                }
                            }
                        }
                    }
                }
                catalog.flush(&mut pager)?;
            }

            // Flush WAL
            // wal.append calls flush internally, we just need fsync at end of commit
            wal.append(&WalRecord::CommitTx { txid: self.txid })?;
            wal.fsync()?;
        }

        let has_new_nodes = !self.created_nodes.is_empty();
        let has_label_additions = !self.pending_label_additions.is_empty();
        let has_label_removals = !self.pending_label_removals.is_empty();

        // 3. Apply created nodes to IdMap / Node Index
        {
            let mut idmap = self.engine.idmap.lock().unwrap();
            let mut pager = self.engine.pager.write().unwrap();
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

        let has_label_mutations = has_new_nodes || has_label_additions || has_label_removals;
        if has_label_mutations {
            self.engine.update_published_node_labels();
        }

        if !run.is_empty() {
            self.engine.publish_run(Arc::new(run));
        }

        self.engine.next_txid.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }
}

fn replay_graph_transactions(
    pager: &mut Pager,
    idmap: &mut IdMap,
    committed: Vec<CommittedTx>,
    checkpoint_txid: u64,
    out_runs: &mut Vec<Arc<L0Run>>,
) -> Result<()> {
    for tx in committed {
        if tx.txid <= checkpoint_txid {
            continue;
        }

        let mut memtable = MemTable::default();

        for op in tx.ops {
            match op {
                WalRecord::CreateNode {
                    external_id,
                    label_id,
                    internal_id,
                } => {
                    if let Some(existing) = idmap.lookup(external_id) {
                        if existing != internal_id {
                            return Err(Error::WalProtocol("external id remapped"));
                        }
                        continue;
                    }
                    idmap.apply_create_node(pager, external_id, label_id, internal_id)?;
                }
                WalRecord::AddNodeLabel { node, label_id } => {
                    idmap.apply_add_label(pager, node, label_id)?;
                }
                WalRecord::RemoveNodeLabel { node, label_id } => {
                    idmap.apply_remove_label(pager, node, label_id)?;
                }
                WalRecord::CreateEdge { src, rel, dst } => memtable.create_edge(src, rel, dst),
                WalRecord::TombstoneNode { node } => memtable.tombstone_node(node),
                WalRecord::TombstoneEdge { src, rel, dst } => {
                    memtable.tombstone_edge(src, rel, dst)
                }
                WalRecord::SetNodeProperty { node, key, value } => {
                    memtable.set_node_property(node, key, value);
                }
                WalRecord::SetEdgeProperty {
                    src,
                    rel,
                    dst,
                    key,
                    value,
                } => {
                    memtable.set_edge_property(src, rel, dst, key, value);
                }
                WalRecord::RemoveNodeProperty { node, key } => {
                    memtable.remove_node_property(node, &key);
                }
                WalRecord::RemoveEdgeProperty { src, rel, dst, key } => {
                    memtable.remove_edge_property(src, rel, dst, &key);
                }
                WalRecord::BeginTx { .. }
                | WalRecord::CommitTx { .. }
                | WalRecord::PageWrite { .. }
                | WalRecord::PageFree { .. }
                | WalRecord::CreateLabel { .. }
                | WalRecord::ManifestSwitch { .. }
                | WalRecord::Checkpoint { .. } => {}
            }
        }

        let run = Arc::new(memtable.freeze_into_run(tx.txid));
        if !run.is_empty() {
            out_runs.push(run);
        }
    }

    Ok(())
}

/// Replay label creation transactions from WAL.
fn replay_label_transactions(
    committed: &[CommittedTx],
    interner: &mut LabelInterner,
) -> Result<()> {
    for tx in committed {
        for op in &tx.ops {
            if let WalRecord::CreateLabel { name, label_id } = op {
                // Ensure the label exists at the expected ID
                let existing_id = interner.get_id(name);
                match existing_id {
                    Some(id) => {
                        if id != *label_id {
                            return Err(Error::WalProtocol("label id mismatch"));
                        }
                    }
                    None => {
                        // Label doesn't exist, create it with correct ID
                        // We need to insert at the correct position
                        while interner.next_id() < *label_id {
                            // Fill with dummy entries
                            interner
                                .get_or_create(&format!("__placeholder_{}", interner.next_id()));
                        }
                        interner.get_or_create(name);
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Default, Clone)]
struct RecoveryState {
    manifest_epoch: u64,
    manifest_segments: Vec<SegmentPointer>,
    checkpoint_txid: u64,
    max_txid: u64,
    properties_root: u64,
    stats_root: u64,
}

fn scan_recovery_state(committed: &[CommittedTx]) -> RecoveryState {
    let mut state = RecoveryState::default();
    for tx in committed {
        state.max_txid = state.max_txid.max(tx.txid);
        for op in &tx.ops {
            match op {
                WalRecord::ManifestSwitch {
                    epoch,
                    segments,
                    properties_root,
                    stats_root,
                } => {
                    if *epoch >= state.manifest_epoch {
                        state.manifest_epoch = *epoch;
                        state.manifest_segments = segments.clone();
                        state.checkpoint_txid = 0;
                        state.properties_root = *properties_root;
                        state.stats_root = *stats_root;
                    }
                }
                WalRecord::Checkpoint {
                    up_to_txid,
                    epoch,
                    properties_root,
                    stats_root,
                } => {
                    if *epoch == state.manifest_epoch {
                        state.checkpoint_txid = state.checkpoint_txid.max(*up_to_txid);
                        state.properties_root = *properties_root;
                        state.stats_root = *stats_root;
                    }
                }
                _ => {}
            }
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn set_env(name: &str, value: &str) {
        unsafe { std::env::set_var(name, value) }
    }

    fn remove_env(name: &str) {
        unsafe { std::env::remove_var(name) }
    }

    #[test]
    fn hnsw_params_use_defaults_when_env_missing() {
        let _guard = ENV_LOCK.lock().unwrap();

        let m_old = std::env::var("NERVUSDB_HNSW_M").ok();
        let ec_old = std::env::var("NERVUSDB_HNSW_EF_CONSTRUCTION").ok();
        let es_old = std::env::var("NERVUSDB_HNSW_EF_SEARCH").ok();

        remove_env("NERVUSDB_HNSW_M");
        remove_env("NERVUSDB_HNSW_EF_CONSTRUCTION");
        remove_env("NERVUSDB_HNSW_EF_SEARCH");

        let params = load_hnsw_params_from_env();
        assert_eq!(params.m, 16);
        assert_eq!(params.ef_construction, 200);
        assert_eq!(params.ef_search, 200);

        if let Some(v) = m_old {
            set_env("NERVUSDB_HNSW_M", &v);
        }
        if let Some(v) = ec_old {
            set_env("NERVUSDB_HNSW_EF_CONSTRUCTION", &v);
        }
        if let Some(v) = es_old {
            set_env("NERVUSDB_HNSW_EF_SEARCH", &v);
        }
    }

    #[test]
    fn hnsw_params_read_valid_env_and_ignore_invalid() {
        let _guard = ENV_LOCK.lock().unwrap();

        let m_old = std::env::var("NERVUSDB_HNSW_M").ok();
        let ec_old = std::env::var("NERVUSDB_HNSW_EF_CONSTRUCTION").ok();
        let es_old = std::env::var("NERVUSDB_HNSW_EF_SEARCH").ok();

        set_env("NERVUSDB_HNSW_M", "32");
        set_env("NERVUSDB_HNSW_EF_CONSTRUCTION", "0");
        set_env("NERVUSDB_HNSW_EF_SEARCH", "abc");

        let params = load_hnsw_params_from_env();
        assert_eq!(params.m, 32);
        assert_eq!(params.ef_construction, 200);
        assert_eq!(params.ef_search, 200);

        if let Some(v) = m_old {
            set_env("NERVUSDB_HNSW_M", &v);
        } else {
            remove_env("NERVUSDB_HNSW_M");
        }
        if let Some(v) = ec_old {
            set_env("NERVUSDB_HNSW_EF_CONSTRUCTION", &v);
        } else {
            remove_env("NERVUSDB_HNSW_EF_CONSTRUCTION");
        }
        if let Some(v) = es_old {
            set_env("NERVUSDB_HNSW_EF_SEARCH", &v);
        } else {
            remove_env("NERVUSDB_HNSW_EF_SEARCH");
        }
    }

    #[test]
    fn t45_compaction_persists_manifest_and_skips_replay_before_checkpoint() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("graph.ndb");
        let wal = dir.path().join("graph.wal");

        {
            let engine = GraphEngine::open(&ndb, &wal).unwrap();
            let (a, _b, c) = {
                let mut tx = engine.begin_write();
                let a = tx.create_node(10, 1).unwrap();
                let b = tx.create_node(20, 1).unwrap();
                let c = tx.create_node(30, 1).unwrap();
                tx.create_edge(a, 7, b);
                tx.commit().unwrap();
                (a, b, c)
            };

            {
                let mut tx = engine.begin_write();
                tx.create_edge(a, 7, c);
                tx.commit().unwrap();
            }

            engine.compact().unwrap();
            assert_eq!(engine.published_runs.read().unwrap().len(), 0);
            assert_eq!(engine.published_segments.read().unwrap().len(), 1);
        }

        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        assert_eq!(engine.published_runs.read().unwrap().len(), 0);
        assert_eq!(engine.published_segments.read().unwrap().len(), 1);

        let snap = engine.begin_read();
        let a = engine.lookup_internal_id(10).unwrap();
        assert_eq!(snap.neighbors(a, Some(7)).count(), 2);
    }

    #[test]
    fn t103_compaction_checkpoints_even_with_properties() {
        use crate::api::StorageSnapshot;
        use nervusdb_v2_api::GraphSnapshot;

        let dir = tempdir().unwrap();
        let ndb = dir.path().join("graph_props.ndb");
        let wal = dir.path().join("graph_props.wal");

        let internal_id;
        {
            let engine = GraphEngine::open(&ndb, &wal).unwrap();
            let mut tx = engine.begin_write();
            let node = tx.create_node(10, 1).unwrap();
            tx.set_node_property(
                node,
                "age".to_string(),
                crate::property::PropertyValue::Int(30),
            );
            tx.commit().unwrap();
            internal_id = node;

            engine.compact().unwrap();
            // Runs MUST BE cleared because properties are now persisted.
            assert!(engine.published_runs.read().unwrap().is_empty());
        }

        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        // Use API-level snapshot which supports reading from B-Tree
        let snap: StorageSnapshot = engine.snapshot();
        let age = snap.node_property(internal_id, "age").unwrap();
        assert_eq!(age, nervusdb_v2_api::PropertyValue::Int(30));
        // And we must have NO runs after restart, because they were checkpointed.
        assert!(engine.published_runs.read().unwrap().is_empty());
    }
}

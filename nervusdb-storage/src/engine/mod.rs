mod write_txn;
pub use write_txn::WriteTxn;

use crate::csr::{CsrSegment, EdgeRecord, SegmentId};
use crate::idmap::{ExternalId, I2eRecord, IdMap, InternalNodeId, LabelId};
use crate::index::btree::BTree;
use crate::index::catalog::{IndexCatalog, IndexDef};
use crate::label_interner::{LabelInterner, LabelSnapshot};
use crate::memtable::MemTable;
use crate::pager::{PageId, Pager};
use crate::published_state::PublishedState;
use crate::read_path_engine_idmap::{lookup_internal_node_id, read_i2e_arc, read_i2l_arc};
use crate::read_path_engine_view::{
    build_snapshot_from_published, load_properties_and_stats_roots,
};
use crate::read_path_nodes::collect_tombstoned_nodes;
use crate::snapshot::{L0Run, PublishedRuns, PublishedSegments, Snapshot};
use crate::wal::{CommittedTx, SegmentPointer, Wal, WalRecord};
use crate::{Error, Result};
use arc_swap::ArcSwap;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug)]
pub struct GraphEngine {
    ndb_path: PathBuf,
    wal_path: PathBuf,

    pub(super) pager: Arc<RwLock<Pager>>,
    pub(super) wal: Mutex<Wal>,
    pub(super) idmap: Mutex<IdMap>,
    label_interner: Mutex<LabelInterner>,
    index_catalog: Arc<Mutex<IndexCatalog>>,

    pub(super) published_state: ArcSwap<PublishedState>,
    write_lock: Mutex<()>,
    pub(super) next_txid: AtomicU64,
    next_segment_id: AtomicU64,
    manifest_epoch: AtomicU64,
    checkpoint_txid: AtomicU64,
    properties_root: AtomicU64,
    stats_root: AtomicU64,
}

impl GraphEngine {
    pub(super) fn with_catalog_pager<T>(
        &self,
        f: impl FnOnce(&mut IndexCatalog, &mut Pager) -> Result<T>,
    ) -> Result<T> {
        let mut catalog = self.index_catalog.lock().unwrap();
        let mut pager = self.pager.write().unwrap();
        f(&mut catalog, &mut pager)
    }

    pub(super) fn publish_index_entries_snapshot(&self, catalog: &IndexCatalog) {
        let current = self.published_state.load_full();
        let mut next = (*current).clone();
        next.index_entries = Arc::new(catalog.entries.clone());
        self.published_state.store(Arc::new(next));
    }

    pub fn open(ndb_path: impl AsRef<Path>, wal_path: impl AsRef<Path>) -> Result<Self> {
        let ndb_path = ndb_path.as_ref().to_path_buf();
        let wal_path = wal_path.as_ref().to_path_buf();

        let mut pager = Pager::open(&ndb_path)?;
        let wal = Wal::open(&wal_path)?;

        let mut idmap = IdMap::load(&mut pager)?;
        let index_catalog = IndexCatalog::open_or_create(&mut pager)?;

        let committed = wal.replay_committed()?;
        let state = scan_recovery_state(&committed);

        let mut segments = PublishedSegments::new();
        let mut max_seg_id = 0u64;
        for ptr in &state.manifest_segments {
            let seg = Arc::new(CsrSegment::load(&mut pager, ptr.meta_page_id)?);
            if seg.id.0 != ptr.id {
                return Err(Error::WalProtocol("csr segment id mismatch"));
            }
            max_seg_id = max_seg_id.max(seg.id.0);
            segments.push_back(seg);
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
        let node_labels_snapshot = idmap.get_i2l_arc();
        let i2e_snapshot = idmap.get_i2e_arc();
        let index_entries_snapshot = index_catalog.entries.clone();
        let tombstoned_nodes_snapshot =
            collect_tombstoned_nodes(&Arc::new(PublishedRuns::from(runs.clone())));

        Ok(Self {
            ndb_path,
            wal_path,
            pager: Arc::new(RwLock::new(pager)),
            wal: Mutex::new(wal),
            idmap: Mutex::new(idmap),
            label_interner: Mutex::new(label_interner),
            index_catalog: Arc::new(Mutex::new(index_catalog)),
            published_state: ArcSwap::from(Arc::new(PublishedState::new(
                Arc::new(PublishedRuns::from(runs)),
                Arc::new(segments),
                Arc::new(label_snapshot),
                node_labels_snapshot,
                i2e_snapshot,
                Arc::new(index_entries_snapshot),
                tombstoned_nodes_snapshot.into(),
            ))),
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

    pub(crate) fn get_published_index_entries(&self) -> Arc<BTreeMap<String, IndexDef>> {
        self.published_state.load_full().index_entries.clone()
    }

    pub(crate) fn get_published_tombstoned_nodes(
        &self,
    ) -> Arc<std::collections::HashSet<InternalNodeId>> {
        self.published_state.load_full().tombstoned_nodes.clone()
    }

    pub(crate) fn get_published_i2e(&self) -> Arc<Vec<I2eRecord>> {
        self.published_state.load_full().i2e.clone()
    }

    /// Creates a B-Tree index for the given label and property.
    ///
    /// If the index already exists, this is a no-op.
    /// Note: This MVP does not backfill existing data. The index will only track
    /// valid data inserted *after* index creation.
    pub fn create_index(&self, label: &str, field: &str) -> Result<()> {
        let name = format!("{}.{}", label, field);
        self.with_catalog_pager(|catalog, pager| {
            if catalog.get(&name).is_some() {
                return Ok(());
            }

            catalog.get_or_create(pager, &name)?;
            catalog.flush(pager)?;
            self.publish_index_entries_snapshot(catalog);
            Ok(())
        })
    }

    pub fn begin_read(&self) -> Snapshot {
        let state = self.published_state.load_full();
        let (properties_root, stats_root) =
            load_properties_and_stats_roots(&self.properties_root, &self.stats_root);
        build_snapshot_from_published(
            state.runs.clone(),
            state.segments.clone(),
            state.labels.clone(),
            state.node_labels.clone(),
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
        if let Some(id) = self.published_state.load_full().labels.get_id(name) {
            return Ok(id);
        }

        let mut interner = self.label_interner.lock().unwrap();
        if let Some(id) = interner.get_id(name) {
            return Ok(id);
        }

        let returned_id = interner.get_or_create(name);

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

        let snapshot = interner.snapshot();
        let current = self.published_state.load_full();
        let mut next = (*current).clone();
        next.labels = Arc::new(snapshot);
        self.published_state.store(Arc::new(next));

        Ok(returned_id)
    }

    /// Update published node labels from IdMap.
    /// Should be called after write transactions that create nodes.
    pub(super) fn update_published_idmap_snapshots(&self) {
        let labels = read_i2l_arc(&self.idmap);
        let i2e = read_i2e_arc(&self.idmap);
        let current = self.published_state.load_full();
        let mut next = (*current).clone();
        next.node_labels = labels;
        next.i2e = i2e;
        self.published_state.store(Arc::new(next));
    }

    /// Get a snapshot of the current label state for reading.
    pub fn label_snapshot(&self) -> Arc<LabelSnapshot> {
        self.published_state.load_full().labels.clone()
    }

    /// Get label ID by name, returns None if not found.
    pub fn get_label_id(&self, name: &str) -> Option<LabelId> {
        self.published_state.load_full().labels.get_id(name)
    }

    /// Get label name by ID, returns None if not found.
    pub fn get_label_name(&self, id: LabelId) -> Option<String> {
        self.published_state
            .load_full()
            .labels
            .get_name(id)
            .map(ToOwned::to_owned)
    }

    /// Compatibility helper for callers that explicitly need an owned snapshot copy.
    /// Hot read paths should prefer the published Arc snapshot instead.
    pub fn scan_i2e_records(&self) -> Vec<I2eRecord> {
        self.published_state.load_full().i2e.as_ref().clone()
    }

    pub(super) fn publish_run(&self, run: Arc<L0Run>) {
        let current = self.published_state.load_full();
        let mut next = (*current).clone();
        next.tombstoned_nodes = {
            let mut nodes = (*next.tombstoned_nodes).clone();
            nodes.extend(run.iter_tombstoned_nodes());
            Arc::new(nodes)
        };
        let mut next_runs = (*next.runs).clone();
        next_runs.push_front(run);
        next.runs = Arc::new(next_runs);
        self.published_state.store(Arc::new(next));
    }

    /// M2/T45: Explicit compaction.
    ///
    /// Invariants:
    /// - Writes CSR segment pages to `.ndb` and fsyncs before publishing the manifest in WAL.
    /// - Writes `ManifestSwitch` + `Checkpoint` as a committed WAL tx to make the switch atomic.
    pub fn compact(&self) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap();

        let state = self.published_state.load_full();
        let runs = state.runs.clone();

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
            let mut next = (*state.segments).clone();
            next.push_front(Arc::new(seg));
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

        // Statistics Collection - read from published label snapshots to avoid cloning IdMap state.
        let mut stats = crate::stats::GraphStatistics::default();
        {
            let node_labels = state.node_labels.clone();

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
            let current = self.published_state.load_full();
            let mut next = (*current).clone();
            next.runs = Arc::new(PublishedRuns::new());
            next.segments = new_segments;
            next.tombstoned_nodes = Arc::new(std::collections::HashSet::new());
            self.published_state.store(Arc::new(next));
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

        let state = self.published_state.load_full();
        let runs = state.runs.clone();
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

        let state = self.published_state.load_full();

        let pointers: Vec<SegmentPointer> = state
            .segments
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
        for id in state.labels.iter_ids() {
            if let Some(name) = state.labels.get_name(id) {
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

fn build_segment_from_runs(seg_id: SegmentId, runs: &Arc<PublishedRuns>) -> CsrSegment {
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
    use nervusdb_api::GraphStore;
    use tempfile::tempdir;

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
            assert_eq!(engine.published_state.load_full().runs.len(), 0);
            assert_eq!(engine.published_state.load_full().segments.len(), 1);
        }

        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        assert_eq!(engine.published_state.load_full().runs.len(), 0);
        assert_eq!(engine.published_state.load_full().segments.len(), 1);

        let snap = engine.begin_read();
        let a = engine.lookup_internal_id(10).unwrap();
        assert_eq!(snap.neighbors(a, Some(7)).count(), 2);
    }

    #[test]
    fn t103_compaction_checkpoints_even_with_properties() {
        use crate::api::StorageSnapshot;
        use nervusdb_api::GraphSnapshot;

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
            assert!(engine.published_state.load_full().runs.is_empty());
        }

        let engine = GraphEngine::open(&ndb, &wal).unwrap();
        // Use API-level snapshot which supports reading from B-Tree
        let snap: StorageSnapshot = engine.snapshot();
        let age = snap.node_property(internal_id, "age").unwrap();
        assert_eq!(age, nervusdb_api::PropertyValue::Int(30));
        // And we must have NO runs after restart, because they were checkpointed.
        assert!(engine.published_state.load_full().runs.is_empty());
    }
}

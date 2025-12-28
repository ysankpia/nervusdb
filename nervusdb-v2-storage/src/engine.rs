use crate::csr::{CsrSegment, EdgeRecord, SegmentId};
use crate::idmap::{ExternalId, I2eRecord, IdMap, InternalNodeId, LabelId};
use crate::label_interner::{LabelInterner, LabelSnapshot};
use crate::memtable::MemTable;
use crate::pager::Pager;
use crate::snapshot::{L0Run, RelTypeId, Snapshot};
use crate::wal::{CommittedTx, SegmentPointer, Wal, WalRecord};
use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug)]
pub struct GraphEngine {
    ndb_path: PathBuf,
    wal_path: PathBuf,

    pager: Mutex<Pager>,
    wal: Mutex<Wal>,
    idmap: Mutex<IdMap>,
    label_interner: Mutex<LabelInterner>,

    published_runs: RwLock<Arc<Vec<Arc<L0Run>>>>,
    published_segments: RwLock<Arc<Vec<Arc<CsrSegment>>>>,
    published_labels: RwLock<Arc<LabelSnapshot>>,
    write_lock: Mutex<()>,
    next_txid: AtomicU64,
    next_segment_id: AtomicU64,
    manifest_epoch: AtomicU64,
    checkpoint_txid: AtomicU64,
}

impl GraphEngine {
    pub fn open(ndb_path: impl AsRef<Path>, wal_path: impl AsRef<Path>) -> Result<Self> {
        let ndb_path = ndb_path.as_ref().to_path_buf();
        let wal_path = wal_path.as_ref().to_path_buf();

        let mut pager = Pager::open(&ndb_path)?;
        let wal = Wal::open(&wal_path)?;

        let mut idmap = IdMap::load(&mut pager)?;

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

        Ok(Self {
            ndb_path,
            wal_path,
            pager: Mutex::new(pager),
            wal: Mutex::new(wal),
            idmap: Mutex::new(idmap),
            label_interner: Mutex::new(label_interner),
            published_runs: RwLock::new(Arc::new(runs)),
            published_segments: RwLock::new(Arc::new(segments)),
            published_labels: RwLock::new(Arc::new(label_snapshot)),
            write_lock: Mutex::new(()),
            next_txid: AtomicU64::new(state.max_txid.saturating_add(1).max(1)),
            next_segment_id: AtomicU64::new(max_seg_id.saturating_add(1).max(1)),
            manifest_epoch: AtomicU64::new(state.manifest_epoch),
            checkpoint_txid: AtomicU64::new(state.checkpoint_txid),
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

    pub fn begin_read(&self) -> Snapshot {
        let runs = self.published_runs.read().unwrap().clone();
        let segments = self.published_segments.read().unwrap().clone();
        Snapshot::new(runs, segments)
    }

    pub fn begin_write(&self) -> WriteTxn<'_> {
        let guard = self.write_lock.lock().unwrap();
        let txid = self.next_txid.fetch_add(1, Ordering::Relaxed);
        WriteTxn {
            engine: self,
            _guard: guard,
            txid,
            created_nodes: Vec::new(),
            created_external_ids: std::collections::HashSet::new(),
            memtable: MemTable::default(),
        }
    }

    pub fn lookup_internal_id(&self, external_id: ExternalId) -> Option<InternalNodeId> {
        let idmap = self.idmap.lock().unwrap();
        idmap.lookup(external_id)
    }

    /// Get or create a label, returns the label ID.
    ///
    /// This is a write operation and must be called within a write transaction.
    pub fn get_or_create_label(&self, name: &str) -> Result<LabelId> {
        let mut interner = self.label_interner.lock().unwrap();
        let old_len = interner.len();
        let id = interner.get_or_create(name);

        // Always update published labels when a new label is created
        if interner.len() > old_len {
            let snapshot = interner.snapshot();
            let mut published = self.published_labels.write().unwrap();
            *published = Arc::new(snapshot);
        }

        Ok(id)
    }

    /// Get a snapshot of the current label state for reading.
    pub fn label_snapshot(&self) -> Arc<LabelSnapshot> {
        self.published_labels.read().unwrap().clone()
    }

    /// Get label ID by name, returns None if not found.
    pub fn get_label_id(&self, name: &str) -> Option<LabelId> {
        self.label_interner.lock().unwrap().get_id(name)
    }

    /// Get label name by ID, returns None if not found.
    pub fn get_label_name(&self, id: LabelId) -> Option<String> {
        self.label_interner
            .lock()
            .unwrap()
            .get_name(id)
            .map(String::from)
    }

    pub fn scan_i2e_records(&self) -> Result<Vec<I2eRecord>> {
        let mut pager = self.pager.lock().unwrap();
        let idmap = self.idmap.lock().unwrap();
        idmap.scan_i2e(&mut pager)
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

        let seg_id = SegmentId(self.next_segment_id.fetch_add(1, Ordering::Relaxed));
        let mut seg = build_segment_from_runs(seg_id, &runs);

        {
            let mut pager = self.pager.lock().unwrap();
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
            })?;
            wal.append(&WalRecord::Checkpoint { up_to_txid, epoch })?;
            wal.append(&WalRecord::CommitTx { txid: system_txid })?;
            wal.fsync()?;
        }

        {
            let mut cur_runs = self.published_runs.write().unwrap();
            *cur_runs = Arc::new(Vec::new());
        }
        {
            let mut cur_segs = self.published_segments.write().unwrap();
            *cur_segs = new_segments;
        }

        self.manifest_epoch.store(epoch, Ordering::Relaxed);
        self.checkpoint_txid.store(up_to_txid, Ordering::Relaxed);
        Ok(())
    }
}

fn build_segment_from_runs(seg_id: SegmentId, runs: &Arc<Vec<Arc<L0Run>>>) -> CsrSegment {
    // Apply the same semantics as snapshot merge: newest->oldest, key-based tombstones.
    use std::collections::{BTreeMap, HashSet};

    let mut blocked_nodes: HashSet<InternalNodeId> = HashSet::new();
    let mut blocked_edges: HashSet<crate::snapshot::EdgeKey> = HashSet::new();
    let mut seen_edges: HashSet<crate::snapshot::EdgeKey> = HashSet::new();
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
            if !seen_edges.insert(e) {
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
            offsets: vec![0, 0],
            edges: Vec::new(),
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
        offsets,
        edges: edge_vec,
    }
}

pub struct WriteTxn<'a> {
    engine: &'a GraphEngine,
    _guard: std::sync::MutexGuard<'a, ()>,
    txid: u64,
    created_nodes: Vec<(ExternalId, LabelId, InternalNodeId)>,
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

    pub fn create_edge(&mut self, src: InternalNodeId, rel: RelTypeId, dst: InternalNodeId) {
        self.memtable.create_edge(src, rel, dst);
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

    pub fn commit(self) -> Result<()> {
        // Extract property data before freezing (since freeze consumes memtable)
        let node_properties = self.memtable.node_properties_for_wal();
        let edge_properties = self.memtable.edge_properties_for_wal();

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
            for (node, key, value) in node_properties {
                wal.append(&WalRecord::SetNodeProperty { node, key, value })?;
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

            wal.append(&WalRecord::CommitTx { txid: self.txid })?;
            wal.fsync()?;
        }

        // 2) Apply to ndb/idmap and publish immutable run.
        let mut pager = self.engine.pager.lock().unwrap();
        let mut idmap = self.engine.idmap.lock().unwrap();
        for (external_id, label_id, internal_id) in self.created_nodes {
            idmap.apply_create_node(&mut pager, external_id, label_id, internal_id)?;
        }

        if !run.is_empty() {
            self.engine.publish_run(Arc::new(run));
        }

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
}

fn scan_recovery_state(committed: &[CommittedTx]) -> RecoveryState {
    let mut state = RecoveryState::default();
    for tx in committed {
        state.max_txid = state.max_txid.max(tx.txid);
        for op in &tx.ops {
            match op {
                WalRecord::ManifestSwitch { epoch, segments } => {
                    if *epoch >= state.manifest_epoch {
                        state.manifest_epoch = *epoch;
                        state.manifest_segments = segments.clone();
                        state.checkpoint_txid = 0;
                    }
                }
                WalRecord::Checkpoint { up_to_txid, epoch } => {
                    if *epoch == state.manifest_epoch {
                        state.checkpoint_txid = state.checkpoint_txid.max(*up_to_txid);
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
}

use crate::csr::{CsrSegment, EdgeRecord, SegmentId};
use crate::idmap::{ExternalId, IdMap, InternalNodeId, LabelId};
use crate::memtable::MemTable;
use crate::pager::Pager;
use crate::snapshot::{L0Run, RelTypeId, Snapshot};
use crate::wal::{CommittedTx, Wal, WalRecord};
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

    published_runs: RwLock<Arc<Vec<Arc<L0Run>>>>,
    published_segments: RwLock<Arc<Vec<Arc<CsrSegment>>>>,
    write_lock: Mutex<()>,
    next_txid: AtomicU64,
    next_segment_id: AtomicU64,
}

impl GraphEngine {
    pub fn open(ndb_path: impl AsRef<Path>, wal_path: impl AsRef<Path>) -> Result<Self> {
        let ndb_path = ndb_path.as_ref().to_path_buf();
        let wal_path = wal_path.as_ref().to_path_buf();

        let mut pager = Pager::open(&ndb_path)?;
        let wal = Wal::open(&wal_path)?;

        let mut idmap = IdMap::load(&mut pager)?;

        let committed = wal.replay_committed()?;
        let mut runs = Vec::new();
        replay_graph_transactions(&mut pager, &mut idmap, committed, &mut runs)?;

        runs.reverse(); // newest first for read path

        Ok(Self {
            ndb_path,
            wal_path,
            pager: Mutex::new(pager),
            wal: Mutex::new(wal),
            idmap: Mutex::new(idmap),
            published_runs: RwLock::new(Arc::new(runs)),
            published_segments: RwLock::new(Arc::new(Vec::new())),
            write_lock: Mutex::new(()),
            next_txid: AtomicU64::new(1),
            next_segment_id: AtomicU64::new(1),
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
            memtable: MemTable::default(),
        }
    }

    pub fn lookup_internal_id(&self, external_id: ExternalId) -> Option<InternalNodeId> {
        let idmap = self.idmap.lock().unwrap();
        idmap.lookup(external_id)
    }

    fn publish_run(&self, run: Arc<L0Run>) {
        let mut current = self.published_runs.write().unwrap();
        let mut next = Vec::with_capacity(current.len() + 1);
        next.push(run);
        next.extend(current.iter().cloned());
        *current = Arc::new(next);
    }

    fn publish_segment(&self, seg: Arc<CsrSegment>) {
        let mut current = self.published_segments.write().unwrap();
        let mut next = Vec::with_capacity(current.len() + 1);
        next.push(seg);
        next.extend(current.iter().cloned());
        *current = Arc::new(next);
    }

    /// M2: Explicit compaction.
    ///
    /// Current implementation is intentionally simple:
    /// - Compacts all currently published L0 runs into a single in-memory CSR segment.
    /// - Clears published runs for new snapshots.
    ///
    /// Durability/manifest is handled in T45.
    pub fn compact(&self) -> Result<()> {
        let _guard = self.write_lock.lock().unwrap();

        let runs = {
            let mut current = self.published_runs.write().unwrap();
            let taken = current.clone();
            *current = Arc::new(Vec::new());
            taken
        };

        if runs.is_empty() {
            return Ok(());
        }

        let seg_id = SegmentId(self.next_segment_id.fetch_add(1, Ordering::Relaxed));
        let seg = Arc::new(build_segment_from_runs(seg_id, &runs));
        self.publish_segment(seg);
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
    memtable: MemTable,
}

impl<'a> WriteTxn<'a> {
    pub fn create_node(
        &mut self,
        external_id: ExternalId,
        label_id: LabelId,
    ) -> Result<InternalNodeId> {
        if self.created_nodes.iter().any(|(e, _, _)| *e == external_id) {
            return Err(Error::WalProtocol("duplicate external id in same tx"));
        }

        if self.engine.lookup_internal_id(external_id).is_some() {
            return Err(Error::WalProtocol("external id already exists"));
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

    pub fn commit(self) -> Result<()> {
        let run = self.memtable.freeze_into_run();

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
    out_runs: &mut Vec<Arc<L0Run>>,
) -> Result<()> {
    for tx in committed {
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
                WalRecord::BeginTx { .. }
                | WalRecord::CommitTx { .. }
                | WalRecord::PageWrite { .. }
                | WalRecord::PageFree { .. } => {}
            }
        }

        let run = Arc::new(memtable.freeze_into_run());
        if !run.is_empty() {
            out_runs.push(run);
        }
    }

    Ok(())
}

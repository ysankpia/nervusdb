use crate::csr::CsrSegment;
use crate::idmap::InternalNodeId;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

pub type RelTypeId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeKey {
    pub src: InternalNodeId,
    pub rel: RelTypeId,
    pub dst: InternalNodeId,
}

#[derive(Debug)]
pub struct L0Run {
    txid: u64,
    edges_by_src: BTreeMap<InternalNodeId, Vec<EdgeKey>>,
    tombstoned_nodes: BTreeSet<InternalNodeId>,
    tombstoned_edges: BTreeSet<EdgeKey>,
}

impl L0Run {
    pub fn new(
        txid: u64,
        edges_by_src: BTreeMap<InternalNodeId, Vec<EdgeKey>>,
        tombstoned_nodes: BTreeSet<InternalNodeId>,
        tombstoned_edges: BTreeSet<EdgeKey>,
    ) -> Self {
        Self {
            txid,
            edges_by_src,
            tombstoned_nodes,
            tombstoned_edges,
        }
    }

    pub(crate) fn txid(&self) -> u64 {
        self.txid
    }

    fn edges_for_src(&self, src: InternalNodeId) -> &[EdgeKey] {
        self.edges_by_src
            .get(&src)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.edges_by_src.is_empty()
            && self.tombstoned_nodes.is_empty()
            && self.tombstoned_edges.is_empty()
    }

    pub(crate) fn iter_edges(&self) -> impl Iterator<Item = EdgeKey> + '_ {
        self.edges_by_src.values().flat_map(|v| v.iter().copied())
    }

    pub(crate) fn iter_tombstoned_nodes(&self) -> impl Iterator<Item = InternalNodeId> + '_ {
        self.tombstoned_nodes.iter().copied()
    }

    pub(crate) fn iter_tombstoned_edges(&self) -> impl Iterator<Item = EdgeKey> + '_ {
        self.tombstoned_edges.iter().copied()
    }
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    runs: Arc<Vec<Arc<L0Run>>>,
    segments: Arc<Vec<Arc<CsrSegment>>>,
}

impl Snapshot {
    pub fn new(runs: Arc<Vec<Arc<L0Run>>>, segments: Arc<Vec<Arc<CsrSegment>>>) -> Self {
        Self { runs, segments }
    }

    pub fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> NeighborsIter {
        NeighborsIter::new(self.runs.clone(), self.segments.clone(), src, rel)
    }
}

pub struct NeighborsIter {
    runs: Arc<Vec<Arc<L0Run>>>,
    segments: Arc<Vec<Arc<CsrSegment>>>,
    src: InternalNodeId,
    rel: Option<RelTypeId>,
    run_idx: usize,
    edge_idx: usize,
    current_edges: Vec<EdgeKey>,
    segment_idx: usize,
    segment_edge_idx: usize,
    current_segment_edges: Vec<EdgeKey>,
    blocked_nodes: HashSet<InternalNodeId>,
    blocked_edges: HashSet<EdgeKey>,
    seen_edges: HashSet<EdgeKey>,
    terminated: bool,
}

impl NeighborsIter {
    fn new(
        runs: Arc<Vec<Arc<L0Run>>>,
        segments: Arc<Vec<Arc<CsrSegment>>>,
        src: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Self {
        Self {
            runs,
            segments,
            src,
            rel,
            run_idx: 0,
            edge_idx: 0,
            current_edges: Vec::new(),
            segment_idx: 0,
            segment_edge_idx: 0,
            current_segment_edges: Vec::new(),
            blocked_nodes: HashSet::new(),
            blocked_edges: HashSet::new(),
            seen_edges: HashSet::new(),
            terminated: false,
        }
    }

    fn load_run(&mut self) {
        self.current_edges.clear();
        self.edge_idx = 0;

        let Some(run) = self.runs.get(self.run_idx) else {
            self.terminated = true;
            return;
        };

        self.blocked_nodes
            .extend(run.tombstoned_nodes.iter().copied());
        self.blocked_edges
            .extend(run.tombstoned_edges.iter().copied());

        if self.blocked_nodes.contains(&self.src) {
            self.terminated = true;
            return;
        }

        self.current_edges
            .extend_from_slice(run.edges_for_src(self.src));
    }

    fn load_segment(&mut self) {
        self.current_segment_edges.clear();
        self.segment_edge_idx = 0;

        let Some(seg) = self.segments.get(self.segment_idx) else {
            return;
        };

        self.current_segment_edges
            .extend(seg.neighbors(self.src, self.rel).collect::<Vec<_>>());
    }
}

impl Iterator for NeighborsIter {
    type Item = EdgeKey;

    fn next(&mut self) -> Option<Self::Item> {
        if self.terminated {
            return None;
        }

        loop {
            if self.edge_idx >= self.current_edges.len() {
                if self.run_idx < self.runs.len() {
                    self.load_run();
                    self.run_idx += 1;
                    continue;
                }

                // After exhausting runs, scan CSR segments (new->old).
                if self.segment_edge_idx >= self.current_segment_edges.len() {
                    if self.segment_idx >= self.segments.len() {
                        self.terminated = true;
                        return None;
                    }
                    self.load_segment();
                    self.segment_idx += 1;
                    continue;
                }

                let edge = self.current_segment_edges[self.segment_edge_idx];
                self.segment_edge_idx += 1;

                if self.blocked_nodes.contains(&edge.dst) {
                    continue;
                }

                if self.blocked_edges.contains(&edge) {
                    continue;
                }

                if !self.seen_edges.insert(edge) {
                    continue;
                }

                return Some(edge);
            }

            let edge = self.current_edges[self.edge_idx];
            self.edge_idx += 1;

            if let Some(rel) = self.rel
                && edge.rel != rel
            {
                continue;
            }

            if self.blocked_nodes.contains(&edge.dst) {
                continue;
            }

            if self.blocked_edges.contains(&edge) {
                continue;
            }

            if !self.seen_edges.insert(edge) {
                continue;
            }

            return Some(edge);
        }
    }
}

use crate::csr::CsrSegment;
use crate::idmap::InternalNodeId;
use crate::read_path_neighbors::{
    edge_blocked_incoming, edge_blocked_outgoing, load_incoming_run_edges,
    load_incoming_segment_edges, load_outgoing_run_edges, load_outgoing_segment_edges,
};
use crate::snapshot::{EdgeKey, L0Run, RelTypeId};
use std::collections::HashSet;
use std::sync::Arc;

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
    pending_tombstoned_nodes: HashSet<InternalNodeId>,
    pending_tombstoned_edges: HashSet<EdgeKey>,
    terminated: bool,
}

impl NeighborsIter {
    pub(crate) fn new(
        runs: Arc<Vec<Arc<L0Run>>>,
        segments: Arc<Vec<Arc<CsrSegment>>>,
        src: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Self {
        let base_cap = runs.len().saturating_mul(8).saturating_add(16);
        Self {
            runs,
            segments,
            src,
            rel,
            run_idx: 0,
            edge_idx: 0,
            current_edges: Vec::with_capacity(16),
            segment_idx: 0,
            segment_edge_idx: 0,
            current_segment_edges: Vec::with_capacity(16),
            blocked_nodes: HashSet::with_capacity(base_cap),
            blocked_edges: HashSet::with_capacity(base_cap),
            pending_tombstoned_nodes: HashSet::with_capacity(base_cap),
            pending_tombstoned_edges: HashSet::with_capacity(base_cap),
            terminated: false,
        }
    }

    fn apply_pending_tombstones(&mut self) {
        if self.pending_tombstoned_nodes.is_empty() && self.pending_tombstoned_edges.is_empty() {
            return;
        }
        self.blocked_nodes
            .extend(self.pending_tombstoned_nodes.drain());
        self.blocked_edges
            .extend(self.pending_tombstoned_edges.drain());
        if self.blocked_nodes.contains(&self.src) {
            self.terminated = true;
        }
    }

    fn load_run(&mut self) {
        self.current_edges.clear();
        self.edge_idx = 0;

        let Some(run) = self.runs.get(self.run_idx) else {
            self.terminated = true;
            return;
        };

        if self.blocked_nodes.contains(&self.src) {
            self.terminated = true;
            return;
        }

        self.pending_tombstoned_nodes.clear();
        self.pending_tombstoned_nodes
            .extend(run.iter_tombstoned_nodes());
        self.pending_tombstoned_edges.clear();
        self.pending_tombstoned_edges
            .extend(run.iter_tombstoned_edges());

        if self.pending_tombstoned_nodes.contains(&self.src) {
            return;
        }

        load_outgoing_run_edges(run, self.src, &mut self.current_edges);
    }

    fn load_segment(&mut self) {
        self.current_segment_edges.clear();
        self.segment_edge_idx = 0;

        let Some(seg) = self.segments.get(self.segment_idx) else {
            return;
        };

        load_outgoing_segment_edges(seg, self.src, self.rel, &mut self.current_segment_edges);
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
                    self.apply_pending_tombstones();
                    if self.terminated {
                        return None;
                    }
                    self.load_run();
                    self.run_idx += 1;
                    continue;
                }

                self.apply_pending_tombstones();
                if self.terminated {
                    return None;
                }

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

                if edge_blocked_outgoing(edge, &self.blocked_nodes, &self.blocked_edges) {
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

            if edge_blocked_outgoing(edge, &self.blocked_nodes, &self.blocked_edges) {
                continue;
            }

            return Some(edge);
        }
    }
}

pub struct IncomingNeighborsIter {
    runs: Arc<Vec<Arc<L0Run>>>,
    segments: Arc<Vec<Arc<CsrSegment>>>,
    dst_node: InternalNodeId,
    rel: Option<RelTypeId>,
    run_idx: usize,
    edge_idx: usize,
    current_edges: Vec<EdgeKey>,
    segment_idx: usize,
    segment_edge_idx: usize,
    current_segment_edges: Vec<EdgeKey>,
    blocked_nodes: HashSet<InternalNodeId>,
    blocked_edges: HashSet<EdgeKey>,
    pending_tombstoned_nodes: HashSet<InternalNodeId>,
    pending_tombstoned_edges: HashSet<EdgeKey>,
    terminated: bool,
}

impl IncomingNeighborsIter {
    pub(crate) fn new(
        runs: Arc<Vec<Arc<L0Run>>>,
        segments: Arc<Vec<Arc<CsrSegment>>>,
        dst_node: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Self {
        let base_cap = runs.len().saturating_mul(8).saturating_add(16);
        Self {
            runs,
            segments,
            dst_node,
            rel,
            run_idx: 0,
            edge_idx: 0,
            current_edges: Vec::with_capacity(16),
            segment_idx: 0,
            segment_edge_idx: 0,
            current_segment_edges: Vec::with_capacity(16),
            blocked_nodes: HashSet::with_capacity(base_cap),
            blocked_edges: HashSet::with_capacity(base_cap),
            pending_tombstoned_nodes: HashSet::with_capacity(base_cap),
            pending_tombstoned_edges: HashSet::with_capacity(base_cap),
            terminated: false,
        }
    }

    fn apply_pending_tombstones(&mut self) {
        if self.pending_tombstoned_nodes.is_empty() && self.pending_tombstoned_edges.is_empty() {
            return;
        }
        self.blocked_nodes
            .extend(self.pending_tombstoned_nodes.drain());
        self.blocked_edges
            .extend(self.pending_tombstoned_edges.drain());
        if self.blocked_nodes.contains(&self.dst_node) {
            self.terminated = true;
        }
    }

    fn load_run(&mut self) {
        self.current_edges.clear();
        self.edge_idx = 0;

        let Some(run) = self.runs.get(self.run_idx) else {
            self.terminated = true;
            return;
        };

        if self.blocked_nodes.contains(&self.dst_node) {
            self.terminated = true;
            return;
        }

        self.pending_tombstoned_nodes.clear();
        self.pending_tombstoned_nodes
            .extend(run.iter_tombstoned_nodes());
        self.pending_tombstoned_edges.clear();
        self.pending_tombstoned_edges
            .extend(run.iter_tombstoned_edges());

        if self.pending_tombstoned_nodes.contains(&self.dst_node) {
            return;
        }

        load_incoming_run_edges(run, self.dst_node, &mut self.current_edges);
    }

    fn load_segment(&mut self) {
        self.current_segment_edges.clear();
        self.segment_edge_idx = 0;

        let Some(seg) = self.segments.get(self.segment_idx) else {
            return;
        };

        load_incoming_segment_edges(
            seg,
            self.dst_node,
            self.rel,
            &mut self.current_segment_edges,
        );
    }
}

impl Iterator for IncomingNeighborsIter {
    type Item = EdgeKey;

    fn next(&mut self) -> Option<Self::Item> {
        if self.terminated {
            return None;
        }

        loop {
            if self.edge_idx >= self.current_edges.len() {
                if self.run_idx < self.runs.len() {
                    self.apply_pending_tombstones();
                    if self.terminated {
                        return None;
                    }
                    self.load_run();
                    self.run_idx += 1;
                    continue;
                }

                self.apply_pending_tombstones();
                if self.terminated {
                    return None;
                }

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

                if edge_blocked_incoming(edge, &self.blocked_nodes, &self.blocked_edges) {
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

            if edge_blocked_incoming(edge, &self.blocked_nodes, &self.blocked_edges) {
                continue;
            }

            return Some(edge);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{IncomingNeighborsIter, NeighborsIter};
    use crate::snapshot::{EdgeKey, L0Run};
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    fn run_with_edge(
        txid: u64,
        edge: EdgeKey,
        copies: usize,
        tombstone_same_edge: bool,
    ) -> Arc<L0Run> {
        let mut out_edges = Vec::new();
        let mut in_edges = Vec::new();
        for _ in 0..copies {
            out_edges.push(edge);
            in_edges.push(edge);
        }
        Arc::new(L0Run::new(
            txid,
            BTreeMap::from([(edge.src, out_edges)]),
            BTreeMap::from([(edge.dst, in_edges)]),
            BTreeSet::new(),
            if tombstone_same_edge {
                BTreeSet::from([edge])
            } else {
                BTreeSet::new()
            },
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
        ))
    }

    #[test]
    fn outgoing_neighbors_keep_current_run_edge_while_hiding_older_same_key() {
        let edge = EdgeKey {
            src: 1,
            rel: 7,
            dst: 2,
        };
        let runs = Arc::new(vec![
            run_with_edge(2, edge, 1, true),
            run_with_edge(1, edge, 2, false),
        ]);
        let segments = Arc::new(Vec::new());

        let got: Vec<EdgeKey> = NeighborsIter::new(runs, segments, 1, Some(7)).collect();
        assert_eq!(got, vec![edge]);
    }

    #[test]
    fn incoming_neighbors_keep_current_run_edge_while_hiding_older_same_key() {
        let edge = EdgeKey {
            src: 3,
            rel: 9,
            dst: 4,
        };
        let runs = Arc::new(vec![
            run_with_edge(2, edge, 1, true),
            run_with_edge(1, edge, 3, false),
        ]);
        let segments = Arc::new(Vec::new());

        let got: Vec<EdgeKey> = IncomingNeighborsIter::new(runs, segments, 4, Some(9)).collect();
        assert_eq!(got, vec![edge]);
    }
}

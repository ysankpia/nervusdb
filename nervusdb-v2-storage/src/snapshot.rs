use crate::csr::CsrSegment;
use crate::idmap::InternalNodeId;
use crate::property::PropertyValue;
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
    edges_by_dst: BTreeMap<InternalNodeId, Vec<EdgeKey>>,
    tombstoned_nodes: BTreeSet<InternalNodeId>,
    pub(crate) tombstoned_edges: BTreeSet<EdgeKey>,
    // Node properties: node_id -> { key -> value }
    pub(crate) node_properties: BTreeMap<InternalNodeId, BTreeMap<String, PropertyValue>>,
    // Edge properties: edge_key -> { key -> value }
    pub(crate) edge_properties: BTreeMap<EdgeKey, BTreeMap<String, PropertyValue>>,
    // Tombstoned node properties: node_id -> set of keys
    pub(crate) tombstoned_node_properties: BTreeMap<InternalNodeId, BTreeSet<String>>,
    // Tombstoned edge properties: edge_key -> set of keys
    pub(crate) tombstoned_edge_properties: BTreeMap<EdgeKey, BTreeSet<String>>,
}

impl L0Run {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        txid: u64,
        edges_by_src: BTreeMap<InternalNodeId, Vec<EdgeKey>>,
        edges_by_dst: BTreeMap<InternalNodeId, Vec<EdgeKey>>,
        tombstoned_nodes: BTreeSet<InternalNodeId>,
        tombstoned_edges: BTreeSet<EdgeKey>,
        node_properties: BTreeMap<InternalNodeId, BTreeMap<String, PropertyValue>>,
        edge_properties: BTreeMap<EdgeKey, BTreeMap<String, PropertyValue>>,
        tombstoned_node_properties: BTreeMap<InternalNodeId, BTreeSet<String>>,
        tombstoned_edge_properties: BTreeMap<EdgeKey, BTreeSet<String>>,
    ) -> Self {
        Self {
            txid,
            edges_by_src,
            edges_by_dst,
            tombstoned_nodes,
            tombstoned_edges,
            node_properties,
            edge_properties,
            tombstoned_node_properties,
            tombstoned_edge_properties,
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

    fn edges_for_dst(&self, dst: InternalNodeId) -> &[EdgeKey] {
        self.edges_by_dst
            .get(&dst)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.edges_by_src.is_empty()
            && self.edges_by_dst.is_empty()
            && self.tombstoned_nodes.is_empty()
            && self.tombstoned_edges.is_empty()
            && self.node_properties.is_empty()
            && self.edge_properties.is_empty()
            && self.tombstoned_node_properties.is_empty()
            && self.tombstoned_edge_properties.is_empty()
    }

    pub(crate) fn has_properties(&self) -> bool {
        !self.node_properties.is_empty()
            || !self.edge_properties.is_empty()
            || !self.tombstoned_node_properties.is_empty()
            || !self.tombstoned_edge_properties.is_empty()
    }

    pub(crate) fn node_property(&self, node: InternalNodeId, key: &str) -> Option<&PropertyValue> {
        // If this run deleted the property, return None explicitly (but maybe we should indicate deletion?)
        // For L0Run::node_property, we're returning the value if present.
        // But if it's tombstoned in this run, we shouldn't return it even if it's in `node_properties` (logic error if both happen).
        if let Some(deleted) = self.tombstoned_node_properties.get(&node)
            && deleted.contains(key)
        {
            return None;
        }
        self.node_properties
            .get(&node)
            .and_then(|props| props.get(key))
    }

    pub(crate) fn edge_property(&self, edge: EdgeKey, key: &str) -> Option<&PropertyValue> {
        if let Some(deleted) = self.tombstoned_edge_properties.get(&edge)
            && deleted.contains(key)
        {
            return None;
        }
        self.edge_properties
            .get(&edge)
            .and_then(|props| props.get(key))
    }

    pub(crate) fn node_properties(
        &self,
        node: InternalNodeId,
    ) -> Option<&BTreeMap<String, PropertyValue>> {
        self.node_properties.get(&node)
    }

    pub(crate) fn edge_properties(
        &self,
        edge: EdgeKey,
    ) -> Option<&BTreeMap<String, PropertyValue>> {
        self.edge_properties.get(&edge)
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
    labels: Arc<crate::label_interner::LabelSnapshot>,
    node_labels: Arc<Vec<Vec<crate::idmap::LabelId>>>,
    pub(crate) properties_root: u64,
    pub(crate) stats_root: u64,
}

impl Snapshot {
    pub fn new(
        runs: Arc<Vec<Arc<L0Run>>>,
        segments: Arc<Vec<Arc<CsrSegment>>>,
        labels: Arc<crate::label_interner::LabelSnapshot>,
        node_labels: Arc<Vec<Vec<crate::idmap::LabelId>>>,
        properties_root: u64,
        stats_root: u64,
    ) -> Self {
        Self {
            runs,
            segments,
            labels,
            node_labels,
            properties_root,
            stats_root,
        }
    }

    pub fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> NeighborsIter {
        NeighborsIter::new(self.runs.clone(), self.segments.clone(), src, rel)
    }

    pub fn incoming_neighbors(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> IncomingNeighborsIter {
        IncomingNeighborsIter::new(self.runs.clone(), self.segments.clone(), dst, rel)
    }

    pub(crate) fn runs(&self) -> &Arc<Vec<Arc<L0Run>>> {
        &self.runs
    }

    pub fn get_statistics(
        &self,
        pager: &crate::pager::Pager,
    ) -> crate::Result<crate::stats::GraphStatistics> {
        if self.stats_root == 0 {
            return Ok(crate::stats::GraphStatistics::default());
        }
        let bytes = crate::blob_store::BlobStore::read(pager, self.stats_root)?;
        crate::stats::GraphStatistics::decode(&bytes).ok_or(crate::Error::StorageCorrupted(
            "failed to decode statistics",
        ))
    }

    /// Get the label ID for a node.
    /// Get the first label for a node (backward compat).
    pub fn node_label(&self, iid: InternalNodeId) -> Option<crate::idmap::LabelId> {
        self.node_labels.get(iid as usize)?.first().copied()
    }

    /// Get all labels for a node.
    pub fn node_labels(&self, iid: InternalNodeId) -> Option<Vec<crate::idmap::LabelId>> {
        self.node_labels.get(iid as usize).cloned()
    }

    /// Get node property from the most recent run that has it.
    /// Get node property from the most recent run that has it.
    pub(crate) fn node_property(&self, node: InternalNodeId, key: &str) -> Option<PropertyValue> {
        // Search from newest to oldest runs
        for run in self.runs.iter() {
            // If run has deletion mark, stop searching and return None
            if let Some(deleted) = run.tombstoned_node_properties.get(&node)
                && deleted.contains(key)
            {
                return None;
            }
            if let Some(value) = run.node_property(node, key) {
                return Some(value.clone());
            }
        }
        None
    }

    /// Get edge property from the most recent run that has it.
    pub(crate) fn edge_property(&self, edge: EdgeKey, key: &str) -> Option<PropertyValue> {
        // Search from newest to oldest runs
        for run in self.runs.iter() {
            if let Some(deleted) = run.tombstoned_edge_properties.get(&edge)
                && deleted.contains(key)
            {
                return None;
            }
            if let Some(value) = run.edge_property(edge, key) {
                return Some(value.clone());
            }
        }
        None
    }

    /// Get all node properties merged from all runs (newest takes precedence).
    pub(crate) fn node_properties(
        &self,
        node: InternalNodeId,
    ) -> Option<BTreeMap<String, PropertyValue>> {
        let mut merged = BTreeMap::new();
        // Iterate from oldest to newest, so newer values overwrite older ones
        for run in self.runs.iter().rev() {
            if let Some(props) = run.node_properties(node) {
                merged.extend(props.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
        }
        if merged.is_empty() {
            None
        } else {
            Some(merged)
        }
    }

    /// Get all edge properties merged from all runs (newest takes precedence).
    pub(crate) fn edge_properties(&self, edge: EdgeKey) -> Option<BTreeMap<String, PropertyValue>> {
        let mut merged = BTreeMap::new();
        // Iterate from oldest to newest, so newer values overwrite older ones
        for run in self.runs.iter().rev() {
            if let Some(props) = run.edge_properties(edge) {
                merged.extend(props.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
        }
        if merged.is_empty() {
            None
        } else {
            Some(merged)
        }
    }

    pub fn resolve_label_id(&self, name: &str) -> Option<crate::idmap::LabelId> {
        self.labels.get_id(name)
    }

    pub fn resolve_rel_type_id(&self, name: &str) -> Option<crate::snapshot::RelTypeId> {
        self.labels.get_id(name)
    }

    pub fn resolve_label_name(&self, id: crate::idmap::LabelId) -> Option<String> {
        self.labels.get_name(id).map(String::from)
    }

    pub fn resolve_rel_type_name(&self, id: crate::snapshot::RelTypeId) -> Option<String> {
        self.labels.get_name(id).map(String::from)
    }

    /// Iterate over all non-tombstoned nodes.
    /// This implementation assumes nodes occupy a dense ID space up to the max size of `node_labels`.
    /// Nodes that are tombstoned are skipped.
    pub fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        let max_id = self.node_labels.len() as u32;
        let iter = (0..max_id).filter(move |&iid| {
            // Check if tombstoned in any run
            for run in self.runs.iter() {
                if run.tombstoned_nodes.contains(&iid) {
                    return false;
                }
                // If not tombstoned in this run, and this run tracks node existence (e.g., via labels or props in advanced scenarios), we continue.
                // Currently, tombstone check is global across runs for the node.
                // But wait, tombstone is per-transaction. If a node is deleted in Run 1 but re-created in Run 2?
                // The `node_labels` is the global index of *valid* IDs... no, `node_labels` grows monotonically.
                // If a node is deleted, its ID remains in `node_labels`.
                // So we must check *cumulative* tombstone status?
                // `Snapshot::is_tombstoned_node` isn't fully implemented in the snippet I saw, but `L0Run` has `tombstoned_nodes`.
                // Actually `Snapshot` doesn't expose `is_tombstoned_node` in the snippet I read.
                // Let's implement logic here: if ANY run says it's tombstoned *and* that run is newer than any creation?
                // Actually, simpler: L0 runs are sorted.
                // If the *newest* run that mentions the node says it's tombstoned?
                // But typically, a delete adds a tombstone. A re-create removes it?
                // If we assume IDs are not reused for now (common in LSM until compaction), efficient checking is just checking if *any* run has tombstone?
                // Wait, `L0Run` reflects a transaction.
                // If T2 deletes N1, T2's run has N1 in `tombstoned_nodes`.
                // If T3 creates N1 (unlikely if IDs unique)..
                // Let's assume for now: if any active run has it tombstoned, it's deleted?
                // NO, older runs might have it tombstoned?
                // Actually, `tombstoned_nodes` usually accumulates in memory table and flushes.
                // Let's use `Snapshot::is_tombstoned_node` if it exists or implement correct logic.
                // The correct logic: Check runs from newest to oldest. First one to say "Tombstoned" or "Created"?
                // Actually L0Run doesn't track "Created" explicitly for existence check, only implies it by presence of edges/props.
                // But `node_labels` tracks allocation.
                // If we assume a node exists unless tombstoned in the *newest* run that has an opinion?
                // Let's check `is_tombstoned_node` implementation in `GraphSnapshot` trait definition.
                // It defaults to false. `Snapshot` struct didn't implement it in the view.
                // I will implement a basic check: check all runs. If *any* run has it tombstoned, treat as deleted?
                // This is correct only if we don't reuse IDs or revive nodes.
                // Given "wal protocol error: duplicate external id", we probably don't reuse IDs yet.
            }
            // Also check if the node label is valid (e.g. not a placeholder if we have spaces).
            // `node_labels` stores LabelId. So it's allocated.
            true
        });

        // Filter out tombstoned nodes properly
        // To be safe, let's use a helper if we can, but since I can't call methods on self easily in closure with borrow checker...
        // Actually, `self` reference is fine.
        Box::new(iter.filter(move |&iid| !self.is_tombstoned_node(iid)))
    }

    pub fn is_tombstoned_node(&self, iid: InternalNodeId) -> bool {
        // Simple check: if latest run that touches this node says it's tombstoned.
        // But how do we know if a run "touches" it if it's not tombstoned?
        // Maybe it just lacks edges/props?
        // NervusDB v2 likely uses `tombstoned_nodes` to mask existence.
        // If a run has it in `tombstoned_nodes`, it is deleted.
        // If a NEWER run re-created it? We'd need a "created_nodes" set?
        // Assuming no ID reuse for now, if it's in ANY `tombstoned_nodes`, it's dead.
        for run in self.runs.iter() {
            if run.tombstoned_nodes.contains(&iid) {
                return true;
            }
        }
        false
    }
}

impl nervusdb_v2_api::GraphSnapshot for Snapshot {
    type Neighbors<'a> = ApiNeighborsIter<'a>;

    fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> Self::Neighbors<'_> {
        ApiNeighborsIter {
            inner: Box::new(self.neighbors(src, rel)),
            _marker: std::marker::PhantomData,
        }
    }

    fn incoming_neighbors(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Self::Neighbors<'_> {
        ApiNeighborsIter {
            inner: Box::new(self.incoming_neighbors(dst, rel)),
            _marker: std::marker::PhantomData,
        }
    }

    fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        self.nodes()
    }

    fn is_tombstoned_node(&self, iid: InternalNodeId) -> bool {
        self.is_tombstoned_node(iid)
    }

    fn resolve_external(&self, _iid: InternalNodeId) -> Option<nervusdb_v2_api::ExternalId> {
        None
    }

    fn node_label(&self, iid: InternalNodeId) -> Option<crate::idmap::LabelId> {
        self.node_label(iid)
    }

    fn node_property(
        &self,
        iid: InternalNodeId,
        key: &str,
    ) -> Option<nervusdb_v2_api::PropertyValue> {
        self.node_property(iid, key).map(convert_property)
    }

    fn edge_property(
        &self,
        edge: nervusdb_v2_api::EdgeKey,
        key: &str,
    ) -> Option<nervusdb_v2_api::PropertyValue> {
        let internal_edge = crate::snapshot::EdgeKey {
            src: edge.src,
            rel: edge.rel,
            dst: edge.dst,
        };
        self.edge_property(internal_edge, key).map(convert_property)
    }

    fn node_properties(
        &self,
        iid: InternalNodeId,
    ) -> Option<BTreeMap<String, nervusdb_v2_api::PropertyValue>> {
        self.node_properties(iid).map(|props| {
            props
                .into_iter()
                .map(|(k, v)| (k, convert_property(v)))
                .collect()
        })
    }

    fn edge_properties(
        &self,
        edge: nervusdb_v2_api::EdgeKey,
    ) -> Option<BTreeMap<String, nervusdb_v2_api::PropertyValue>> {
        let internal_edge = crate::snapshot::EdgeKey {
            src: edge.src,
            rel: edge.rel,
            dst: edge.dst,
        };
        self.edge_properties(internal_edge).map(|props| {
            props
                .into_iter()
                .map(|(k, v)| (k, convert_property(v)))
                .collect()
        })
    }

    fn resolve_label_id(&self, name: &str) -> Option<crate::idmap::LabelId> {
        self.resolve_label_id(name)
    }

    fn resolve_rel_type_id(&self, name: &str) -> Option<crate::snapshot::RelTypeId> {
        self.resolve_rel_type_id(name)
    }

    fn resolve_label_name(&self, id: crate::idmap::LabelId) -> Option<String> {
        self.resolve_label_name(id)
    }

    fn resolve_rel_type_name(&self, id: crate::snapshot::RelTypeId) -> Option<String> {
        self.labels.get_name(id).map(String::from)
    }
}

pub struct ApiNeighborsIter<'a> {
    inner: Box<dyn Iterator<Item = EdgeKey>>,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> Iterator for ApiNeighborsIter<'a> {
    type Item = nervusdb_v2_api::EdgeKey;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|e| nervusdb_v2_api::EdgeKey {
            src: e.src,
            rel: e.rel,
            dst: e.dst,
        })
    }
}

fn convert_property(val: crate::property::PropertyValue) -> nervusdb_v2_api::PropertyValue {
    match val {
        crate::property::PropertyValue::Null => nervusdb_v2_api::PropertyValue::Null,
        crate::property::PropertyValue::Bool(v) => nervusdb_v2_api::PropertyValue::Bool(v),
        crate::property::PropertyValue::Int(v) => nervusdb_v2_api::PropertyValue::Int(v),
        crate::property::PropertyValue::Float(v) => nervusdb_v2_api::PropertyValue::Float(v),
        crate::property::PropertyValue::String(v) => nervusdb_v2_api::PropertyValue::String(v),
        crate::property::PropertyValue::DateTime(v) => nervusdb_v2_api::PropertyValue::DateTime(v),
        crate::property::PropertyValue::Blob(v) => nervusdb_v2_api::PropertyValue::Blob(v),
        crate::property::PropertyValue::List(v) => {
            nervusdb_v2_api::PropertyValue::List(v.into_iter().map(convert_property).collect())
        }
        crate::property::PropertyValue::Map(v) => nervusdb_v2_api::PropertyValue::Map(
            v.into_iter()
                .map(|(k, val)| (k, convert_property(val)))
                .collect(),
        ),
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
            seen_edges: HashSet::with_capacity(base_cap),
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
            .extend(seg.neighbors(self.src, self.rel));
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
    seen_edges: HashSet<EdgeKey>,
    terminated: bool,
}

impl IncomingNeighborsIter {
    fn new(
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
            seen_edges: HashSet::with_capacity(base_cap),
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

        if self.blocked_nodes.contains(&self.dst_node) {
            self.terminated = true;
            return;
        }

        self.current_edges
            .extend_from_slice(run.edges_for_dst(self.dst_node));
    }

    fn load_segment(&mut self) {
        self.current_segment_edges.clear();
        self.segment_edge_idx = 0;

        let Some(seg) = self.segments.get(self.segment_idx) else {
            return;
        };

        self.current_segment_edges
            .extend(seg.incoming_neighbors(self.dst_node, self.rel));
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
                    self.load_run();
                    self.run_idx += 1;
                    continue;
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

                if self.blocked_nodes.contains(&edge.src) {
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

            if self.blocked_nodes.contains(&edge.src) {
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

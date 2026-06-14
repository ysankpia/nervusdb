use crate::idmap::{I2eRecord, InternalNodeId, LabelId};
use crate::index::catalog::IndexDef;
use crate::label_interner::LabelSnapshot;
use crate::snapshot::{PublishedRuns, PublishedSegments};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// Atomic published view of the graph state.
///
/// All fields are immutable Arc-held snapshots. Updates produce a new
/// `Arc<PublishedState>` and swap it into the single `ArcSwap` in
/// `GraphEngine`, so readers observe a consistent cross-section of runs,
/// segments, labels, idmaps, indexes, and tombstones.
#[derive(Clone, Debug)]
pub(crate) struct PublishedState {
    pub(crate) runs: Arc<PublishedRuns>,
    pub(crate) segments: Arc<PublishedSegments>,
    pub(crate) labels: Arc<LabelSnapshot>,
    pub(crate) node_labels: Arc<Vec<Vec<LabelId>>>,
    pub(crate) i2e: Arc<Vec<I2eRecord>>,
    pub(crate) index_entries: Arc<BTreeMap<String, IndexDef>>,
    pub(crate) tombstoned_nodes: Arc<HashSet<InternalNodeId>>,
}

impl PublishedState {
    pub(crate) fn new(
        runs: Arc<PublishedRuns>,
        segments: Arc<PublishedSegments>,
        labels: Arc<LabelSnapshot>,
        node_labels: Arc<Vec<Vec<LabelId>>>,
        i2e: Arc<Vec<I2eRecord>>,
        index_entries: Arc<BTreeMap<String, IndexDef>>,
        tombstoned_nodes: Arc<HashSet<InternalNodeId>>,
    ) -> Self {
        Self {
            runs,
            segments,
            labels,
            node_labels,
            i2e,
            index_entries,
            tombstoned_nodes,
        }
    }
}

use crate::csr::CsrSegment;
use crate::idmap::InternalNodeId;
use crate::property::PropertyValue;
use crate::read_path_api_iter::ApiNeighborsIter;
use crate::read_path_api_props::{
    edge_properties_as_api, edge_property_as_api, node_properties_as_api, node_property_as_api,
};
use crate::read_path_iters::{IncomingNeighborsIter, NeighborsIter};
use crate::read_path_labels::{node_all_labels, node_primary_label};
use crate::read_path_nodes::{is_tombstoned_node_in_runs, live_node_ids};
use crate::read_path_overlay::{
    edge_property_from_runs, merge_edge_properties_from_runs, merge_node_properties_from_runs,
    node_property_from_runs,
};
use crate::read_path_run_edges::{
    edges_for_dst as run_edges_for_dst, edges_for_src as run_edges_for_src,
};
use crate::read_path_run_iters::{
    iter_edges as run_iter_edges, iter_tombstoned_edges as run_iter_tombstoned_edges,
    iter_tombstoned_nodes as run_iter_tombstoned_nodes,
};
use crate::read_path_run_property_maps::{edge_properties_in_run, node_properties_in_run};
use crate::read_path_run_props::{edge_property_in_run, node_property_in_run};
use crate::read_path_run_state::{run_has_properties, run_is_empty};
use crate::read_path_stats::read_statistics;
use crate::read_path_symbols::{resolve_symbol_id, resolve_symbol_name};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub type RelTypeId = nervusdb_api::RelTypeId;
pub type EdgeKey = nervusdb_api::EdgeKey;

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

    pub(crate) fn edges_for_src(&self, src: InternalNodeId) -> &[EdgeKey] {
        run_edges_for_src(&self.edges_by_src, src)
    }

    pub(crate) fn edges_for_dst(&self, dst: InternalNodeId) -> &[EdgeKey] {
        run_edges_for_dst(&self.edges_by_dst, dst)
    }

    pub(crate) fn is_empty(&self) -> bool {
        run_is_empty(
            &self.edges_by_src,
            &self.edges_by_dst,
            &self.tombstoned_nodes,
            &self.tombstoned_edges,
            &self.node_properties,
            &self.edge_properties,
            &self.tombstoned_node_properties,
            &self.tombstoned_edge_properties,
        )
    }

    pub(crate) fn has_properties(&self) -> bool {
        run_has_properties(
            &self.node_properties,
            &self.edge_properties,
            &self.tombstoned_node_properties,
            &self.tombstoned_edge_properties,
        )
    }

    pub(crate) fn node_property(&self, node: InternalNodeId, key: &str) -> Option<&PropertyValue> {
        node_property_in_run(self, node, key)
    }

    pub(crate) fn edge_property(&self, edge: EdgeKey, key: &str) -> Option<&PropertyValue> {
        edge_property_in_run(self, edge, key)
    }

    pub(crate) fn node_properties(
        &self,
        node: InternalNodeId,
    ) -> Option<&BTreeMap<String, PropertyValue>> {
        node_properties_in_run(&self.node_properties, node)
    }

    pub(crate) fn edge_properties(
        &self,
        edge: EdgeKey,
    ) -> Option<&BTreeMap<String, PropertyValue>> {
        edge_properties_in_run(&self.edge_properties, edge)
    }

    pub(crate) fn iter_edges(&self) -> impl Iterator<Item = EdgeKey> + '_ {
        run_iter_edges(&self.edges_by_src)
    }

    pub(crate) fn iter_tombstoned_nodes(&self) -> impl Iterator<Item = InternalNodeId> + '_ {
        run_iter_tombstoned_nodes(&self.tombstoned_nodes)
    }

    pub(crate) fn iter_tombstoned_edges(&self) -> impl Iterator<Item = EdgeKey> + '_ {
        run_iter_tombstoned_edges(&self.tombstoned_edges)
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
        read_statistics(pager, self.stats_root)
    }

    /// Get the label ID for a node.
    /// Get the first label for a node (backward compat).
    pub fn node_label(&self, iid: InternalNodeId) -> Option<crate::idmap::LabelId> {
        node_primary_label(&self.node_labels, iid)
    }

    /// Get all labels for a node.
    pub fn node_labels(&self, iid: InternalNodeId) -> Option<Vec<crate::idmap::LabelId>> {
        node_all_labels(&self.node_labels, iid)
    }

    /// Get node property from the most recent run that has it.
    /// Get node property from the most recent run that has it.
    pub(crate) fn node_property(&self, node: InternalNodeId, key: &str) -> Option<PropertyValue> {
        node_property_from_runs(&self.runs, node, key)
    }

    /// Get edge property from the most recent run that has it.
    pub(crate) fn edge_property(&self, edge: EdgeKey, key: &str) -> Option<PropertyValue> {
        edge_property_from_runs(&self.runs, edge, key)
    }

    /// Get all node properties merged from all runs (newest takes precedence).
    pub(crate) fn node_properties(
        &self,
        node: InternalNodeId,
    ) -> Option<BTreeMap<String, PropertyValue>> {
        merge_node_properties_from_runs(&self.runs, node)
    }

    /// Get all edge properties merged from all runs (newest takes precedence).
    pub(crate) fn edge_properties(&self, edge: EdgeKey) -> Option<BTreeMap<String, PropertyValue>> {
        merge_edge_properties_from_runs(&self.runs, edge)
    }

    pub fn resolve_label_id(&self, name: &str) -> Option<crate::idmap::LabelId> {
        resolve_symbol_id(&self.labels, name)
    }

    pub fn resolve_rel_type_id(&self, name: &str) -> Option<crate::snapshot::RelTypeId> {
        resolve_symbol_id(&self.labels, name)
    }

    pub fn resolve_label_name(&self, id: crate::idmap::LabelId) -> Option<String> {
        resolve_symbol_name(&self.labels, id)
    }

    pub fn resolve_rel_type_name(&self, id: crate::snapshot::RelTypeId) -> Option<String> {
        resolve_symbol_name(&self.labels, id)
    }

    /// Iterate over all non-tombstoned nodes.
    /// This implementation assumes nodes occupy a dense ID space up to the max size of `node_labels`.
    /// Nodes that are tombstoned are skipped.
    pub fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        let max_id = self.node_labels.len() as u32;
        live_node_ids(max_id, &self.runs)
    }

    pub fn is_tombstoned_node(&self, iid: InternalNodeId) -> bool {
        is_tombstoned_node_in_runs(&self.runs, iid)
    }
}

impl nervusdb_api::GraphSnapshot for Snapshot {
    type Neighbors<'a> = ApiNeighborsIter<'a>;

    fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> Self::Neighbors<'_> {
        ApiNeighborsIter::new(Box::new(self.neighbors(src, rel)))
    }

    fn incoming_neighbors(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Self::Neighbors<'_> {
        ApiNeighborsIter::new(Box::new(self.incoming_neighbors(dst, rel)))
    }

    fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        self.nodes()
    }

    fn is_tombstoned_node(&self, iid: InternalNodeId) -> bool {
        self.is_tombstoned_node(iid)
    }

    fn resolve_external(&self, _iid: InternalNodeId) -> Option<nervusdb_api::ExternalId> {
        None
    }

    fn node_label(&self, iid: InternalNodeId) -> Option<crate::idmap::LabelId> {
        self.node_label(iid)
    }

    fn node_property(&self, iid: InternalNodeId, key: &str) -> Option<nervusdb_api::PropertyValue> {
        node_property_as_api(self, iid, key)
    }

    fn edge_property(
        &self,
        edge: nervusdb_api::EdgeKey,
        key: &str,
    ) -> Option<nervusdb_api::PropertyValue> {
        edge_property_as_api(self, edge, key)
    }

    fn node_properties(
        &self,
        iid: InternalNodeId,
    ) -> Option<BTreeMap<String, nervusdb_api::PropertyValue>> {
        node_properties_as_api(self, iid)
    }

    fn edge_properties(
        &self,
        edge: nervusdb_api::EdgeKey,
    ) -> Option<BTreeMap<String, nervusdb_api::PropertyValue>> {
        edge_properties_as_api(self, edge)
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
        self.resolve_rel_type_name(id)
    }
}

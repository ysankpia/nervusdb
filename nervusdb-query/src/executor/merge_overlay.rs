use super::PropertyValue;
use nervusdb_api::{EdgeKey, InternalNodeId};

#[derive(Clone)]
pub(super) struct MergeOverlayNode {
    pub(super) iid: InternalNodeId,
    pub(super) labels: Vec<String>,
    pub(super) props: std::collections::BTreeMap<String, PropertyValue>,
}

#[derive(Clone)]
pub(super) struct MergeOverlayEdge {
    pub(super) key: EdgeKey,
    pub(super) props: std::collections::BTreeMap<String, PropertyValue>,
}

#[derive(Default)]
pub(super) struct MergeOverlayState {
    pub(super) nodes: Vec<MergeOverlayNode>,
    pub(super) edges: Vec<MergeOverlayEdge>,
    pub(super) deleted_nodes: std::collections::BTreeSet<InternalNodeId>,
    pub(super) deleted_edges: std::collections::BTreeSet<EdgeKey>,
    pub(super) anonymous_nodes: Vec<(
        Vec<String>,
        std::collections::BTreeMap<String, PropertyValue>,
    )>,
}

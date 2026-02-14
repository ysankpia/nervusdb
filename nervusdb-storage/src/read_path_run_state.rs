use crate::idmap::InternalNodeId;
use crate::property::PropertyValue;
use crate::snapshot::EdgeKey;
use std::collections::{BTreeMap, BTreeSet};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_is_empty(
    edges_by_src: &BTreeMap<InternalNodeId, Vec<EdgeKey>>,
    edges_by_dst: &BTreeMap<InternalNodeId, Vec<EdgeKey>>,
    tombstoned_nodes: &BTreeSet<InternalNodeId>,
    tombstoned_edges: &BTreeSet<EdgeKey>,
    node_properties: &BTreeMap<InternalNodeId, BTreeMap<String, PropertyValue>>,
    edge_properties: &BTreeMap<EdgeKey, BTreeMap<String, PropertyValue>>,
    tombstoned_node_properties: &BTreeMap<InternalNodeId, BTreeSet<String>>,
    tombstoned_edge_properties: &BTreeMap<EdgeKey, BTreeSet<String>>,
) -> bool {
    edges_by_src.is_empty()
        && edges_by_dst.is_empty()
        && tombstoned_nodes.is_empty()
        && tombstoned_edges.is_empty()
        && node_properties.is_empty()
        && edge_properties.is_empty()
        && tombstoned_node_properties.is_empty()
        && tombstoned_edge_properties.is_empty()
}

pub(crate) fn run_has_properties(
    node_properties: &BTreeMap<InternalNodeId, BTreeMap<String, PropertyValue>>,
    edge_properties: &BTreeMap<EdgeKey, BTreeMap<String, PropertyValue>>,
    tombstoned_node_properties: &BTreeMap<InternalNodeId, BTreeSet<String>>,
    tombstoned_edge_properties: &BTreeMap<EdgeKey, BTreeSet<String>>,
) -> bool {
    !node_properties.is_empty()
        || !edge_properties.is_empty()
        || !tombstoned_node_properties.is_empty()
        || !tombstoned_edge_properties.is_empty()
}

#[cfg(test)]
mod tests {
    use super::{run_has_properties, run_is_empty};
    use crate::property::PropertyValue;
    use crate::snapshot::EdgeKey;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn run_is_empty_only_when_all_internal_sets_are_empty() {
        let empty_edges_by_src = BTreeMap::new();
        let empty_edges_by_dst = BTreeMap::new();
        let empty_tombstoned_nodes = BTreeSet::new();
        let empty_tombstoned_edges = BTreeSet::new();
        let empty_node_props = BTreeMap::new();
        let empty_edge_props = BTreeMap::new();
        let empty_tombstoned_node_props = BTreeMap::new();
        let empty_tombstoned_edge_props = BTreeMap::new();

        assert!(run_is_empty(
            &empty_edges_by_src,
            &empty_edges_by_dst,
            &empty_tombstoned_nodes,
            &empty_tombstoned_edges,
            &empty_node_props,
            &empty_edge_props,
            &empty_tombstoned_node_props,
            &empty_tombstoned_edge_props,
        ));

        let mut non_empty_edges_by_src = BTreeMap::new();
        non_empty_edges_by_src.insert(
            1,
            vec![EdgeKey {
                src: 1,
                rel: 2,
                dst: 3,
            }],
        );
        assert!(!run_is_empty(
            &non_empty_edges_by_src,
            &empty_edges_by_dst,
            &empty_tombstoned_nodes,
            &empty_tombstoned_edges,
            &empty_node_props,
            &empty_edge_props,
            &empty_tombstoned_node_props,
            &empty_tombstoned_edge_props,
        ));
    }

    #[test]
    fn run_has_properties_detects_any_property_bucket() {
        let empty_node_props = BTreeMap::new();
        let empty_edge_props = BTreeMap::new();
        let empty_tombstoned_node_props = BTreeMap::new();
        let empty_tombstoned_edge_props = BTreeMap::new();

        assert!(!run_has_properties(
            &empty_node_props,
            &empty_edge_props,
            &empty_tombstoned_node_props,
            &empty_tombstoned_edge_props,
        ));

        let non_empty_node_props = BTreeMap::from([(
            1,
            BTreeMap::from([(
                "name".to_string(),
                PropertyValue::String("alice".to_string()),
            )]),
        )]);
        assert!(run_has_properties(
            &non_empty_node_props,
            &empty_edge_props,
            &empty_tombstoned_node_props,
            &empty_tombstoned_edge_props,
        ));
    }
}

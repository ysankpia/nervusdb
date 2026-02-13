use super::{
    MergeOverlayEdge, MergeOverlayNode, MergeOverlayState, NodeValue, PropertyValue,
    UNLABELED_LABEL_ID, Value, WriteableGraph, convert_api_property_to_value,
    merge_props_to_values, merge_storage_property_to_api,
};
use crate::ast::{NodePattern, RelationshipDirection};
use crate::error::Result;
use nervusdb_api::{EdgeKey, ExternalId, GraphSnapshot, InternalNodeId, RelTypeId};

pub(super) fn merge_node_matches_snapshot<S: GraphSnapshot>(
    snapshot: &S,
    iid: InternalNodeId,
    labels: &[String],
    props: &std::collections::BTreeMap<String, PropertyValue>,
) -> bool {
    if snapshot.is_tombstoned_node(iid) {
        return false;
    }

    if !labels.is_empty() {
        let mut node_labels = Vec::new();
        if let Some(ids) = snapshot.resolve_node_labels(iid) {
            for id in ids {
                if let Some(name) = snapshot.resolve_label_name(id) {
                    node_labels.push(name);
                }
            }
        } else if let Some(id) = snapshot.node_label(iid)
            && let Some(name) = snapshot.resolve_label_name(id)
        {
            node_labels.push(name);
        }

        for required in labels {
            if !node_labels.iter().any(|actual| actual == required) {
                return false;
            }
        }
    }

    for (k, v) in props {
        if snapshot.node_property(iid, k) != Some(merge_storage_property_to_api(v)) {
            return false;
        }
    }

    true
}

pub(super) fn merge_node_matches_overlay(
    node: &MergeOverlayNode,
    labels: &[String],
    props: &std::collections::BTreeMap<String, PropertyValue>,
) -> bool {
    for required in labels {
        if !node.labels.iter().any(|actual| actual == required) {
            return false;
        }
    }
    for (k, v) in props {
        if node.props.get(k) != Some(v) {
            return false;
        }
    }
    true
}

pub(super) fn merge_find_node_candidates<S: GraphSnapshot>(
    snapshot: &S,
    overlay: &MergeOverlayState,
    labels: &[String],
    props: &std::collections::BTreeMap<String, PropertyValue>,
) -> Vec<InternalNodeId> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for n in &overlay.nodes {
        if overlay.deleted_nodes.contains(&n.iid) {
            continue;
        }
        if merge_node_matches_overlay(n, labels, props) && seen.insert(n.iid) {
            out.push(n.iid);
        }
    }

    for iid in snapshot.nodes() {
        if overlay.deleted_nodes.contains(&iid) {
            continue;
        }
        if merge_node_matches_snapshot(snapshot, iid, labels, props) && seen.insert(iid) {
            out.push(iid);
        }
    }

    out
}

pub(super) fn merge_materialize_node_value<S: GraphSnapshot>(
    snapshot: &S,
    overlay: &MergeOverlayState,
    iid: InternalNodeId,
) -> Value {
    if let Some(node) = overlay.nodes.iter().find(|n| n.iid == iid) {
        return Value::Node(NodeValue {
            id: iid,
            labels: node.labels.clone(),
            properties: merge_props_to_values(&node.props),
        });
    }

    let labels = snapshot
        .resolve_node_labels(iid)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|lid| snapshot.resolve_label_name(lid))
        .collect::<Vec<_>>();

    let properties = snapshot
        .node_properties(iid)
        .unwrap_or_default()
        .iter()
        .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
        .collect::<std::collections::BTreeMap<_, _>>();

    Value::Node(NodeValue {
        id: iid,
        labels,
        properties,
    })
}

pub(super) fn merge_edge_matches_snapshot<S: GraphSnapshot>(
    snapshot: &S,
    edge: EdgeKey,
    props: &std::collections::BTreeMap<String, PropertyValue>,
) -> bool {
    for (k, v) in props {
        if snapshot.edge_property(edge, k) != Some(merge_storage_property_to_api(v)) {
            return false;
        }
    }
    true
}

pub(super) fn merge_edge_matches_overlay(
    edge: &MergeOverlayEdge,
    props: &std::collections::BTreeMap<String, PropertyValue>,
) -> bool {
    for (k, v) in props {
        if edge.props.get(k) != Some(v) {
            return false;
        }
    }
    true
}

pub(super) fn merge_collect_edges_between<S: GraphSnapshot>(
    snapshot: &S,
    overlay: &MergeOverlayState,
    left: InternalNodeId,
    right: InternalNodeId,
    rel_type: RelTypeId,
    direction: &RelationshipDirection,
    rel_props: &std::collections::BTreeMap<String, PropertyValue>,
) -> Vec<EdgeKey> {
    if overlay.deleted_nodes.contains(&left) || overlay.deleted_nodes.contains(&right) {
        return Vec::new();
    }

    let mut out = Vec::new();
    let dedup_by_key = !rel_props.is_empty();
    let mut seen = std::collections::HashSet::new();

    let mut collect_dir = |src: InternalNodeId, dst: InternalNodeId| {
        for edge in snapshot.neighbors(src, Some(rel_type)) {
            if overlay.deleted_edges.contains(&edge) {
                continue;
            }
            if edge.dst == dst
                && merge_edge_matches_snapshot(snapshot, edge, rel_props)
                && (!dedup_by_key || seen.insert(edge))
            {
                out.push(edge);
            }
        }
        for edge in &overlay.edges {
            if overlay.deleted_edges.contains(&edge.key) {
                continue;
            }
            if edge.key.src == src
                && edge.key.dst == dst
                && edge.key.rel == rel_type
                && merge_edge_matches_overlay(edge, rel_props)
                && (!dedup_by_key || seen.insert(edge.key))
            {
                out.push(edge.key);
            }
        }
    };

    match direction {
        RelationshipDirection::LeftToRight => collect_dir(left, right),
        RelationshipDirection::RightToLeft => collect_dir(right, left),
        RelationshipDirection::Undirected => {
            collect_dir(left, right);
            collect_dir(right, left);
        }
    }

    out
}

pub(super) fn merge_create_node(
    txn: &mut dyn WriteableGraph,
    node_pat: &NodePattern,
    props: &std::collections::BTreeMap<String, PropertyValue>,
    created_count: &mut u32,
) -> Result<InternalNodeId> {
    let external_id = ExternalId::from(
        *created_count as u64 + chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64,
    );
    let label_id = if let Some(label) = node_pat.labels.first() {
        txn.get_or_create_label_id(label)?
    } else {
        UNLABELED_LABEL_ID
    };

    let iid = txn.create_node(external_id, label_id)?;
    for extra_label in node_pat.labels.iter().skip(1) {
        let extra_label_id = txn.get_or_create_label_id(extra_label)?;
        txn.add_node_label(iid, extra_label_id)?;
    }
    for (k, v) in props {
        txn.set_node_property(iid, k.clone(), v.clone())?;
    }
    *created_count += 1;
    Ok(iid)
}

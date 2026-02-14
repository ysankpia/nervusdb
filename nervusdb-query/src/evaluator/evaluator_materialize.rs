use crate::executor::{NodeValue, Row, Value, convert_api_property_to_value};
use nervusdb_api::{GraphSnapshot, InternalNodeId};
use std::collections::BTreeMap;

pub(super) fn materialize_node_from_row_or_snapshot<S: GraphSnapshot>(
    row: &Row,
    snapshot: &S,
    node_id: InternalNodeId,
) -> Value {
    for (_, v) in row.columns() {
        match v {
            Value::Node(node) if node.id == node_id => return Value::Node(node.clone()),
            Value::NodeId(id) if *id == node_id => return Value::NodeId(*id),
            _ => {}
        }
    }

    let labels = snapshot
        .resolve_node_labels(node_id)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|lid| snapshot.resolve_label_name(lid))
        .collect::<Vec<_>>();
    let properties = snapshot
        .node_properties(node_id)
        .unwrap_or_default()
        .iter()
        .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
        .collect::<BTreeMap<_, _>>();

    if labels.is_empty() && properties.is_empty() {
        Value::NodeId(node_id)
    } else {
        Value::Node(NodeValue {
            id: node_id,
            labels,
            properties,
        })
    }
}

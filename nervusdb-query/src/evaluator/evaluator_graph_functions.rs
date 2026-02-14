use super::evaluator_materialize::materialize_node_from_row_or_snapshot;
use super::{Row, Value};
use nervusdb_api::GraphSnapshot;

pub(super) fn evaluate_graph_function<S: GraphSnapshot>(
    name: &str,
    args: &[Value],
    row: &Row,
    snapshot: &S,
) -> Option<Value> {
    match name {
        "startnode" => Some(evaluate_start_node(args, row, snapshot)),
        "endnode" => Some(evaluate_end_node(args, row, snapshot)),
        "labels" => Some(evaluate_labels(args, snapshot)),
        "type" => Some(evaluate_type(args, snapshot)),
        "id" => Some(evaluate_id(args)),
        _ => None,
    }
}

fn evaluate_start_node<S: GraphSnapshot>(args: &[Value], row: &Row, snapshot: &S) -> Value {
    match args.first() {
        Some(Value::EdgeKey(edge_key)) => {
            materialize_node_from_row_or_snapshot(row, snapshot, edge_key.src)
        }
        Some(Value::Relationship(rel)) => {
            materialize_node_from_row_or_snapshot(row, snapshot, rel.key.src)
        }
        _ => Value::Null,
    }
}

fn evaluate_end_node<S: GraphSnapshot>(args: &[Value], row: &Row, snapshot: &S) -> Value {
    match args.first() {
        Some(Value::EdgeKey(edge_key)) => {
            materialize_node_from_row_or_snapshot(row, snapshot, edge_key.dst)
        }
        Some(Value::Relationship(rel)) => {
            materialize_node_from_row_or_snapshot(row, snapshot, rel.key.dst)
        }
        _ => Value::Null,
    }
}

fn evaluate_labels<S: GraphSnapshot>(args: &[Value], snapshot: &S) -> Value {
    match args.first() {
        Some(Value::NodeId(id)) => snapshot
            .resolve_node_labels(*id)
            .map(|labels| {
                Value::List(
                    labels
                        .into_iter()
                        .filter_map(|label_id| snapshot.resolve_label_name(label_id))
                        .map(Value::String)
                        .collect(),
                )
            })
            .unwrap_or(Value::Null),
        Some(Value::Node(node)) => {
            Value::List(node.labels.iter().cloned().map(Value::String).collect())
        }
        Some(Value::Null) => Value::Null,
        _ => Value::Null,
    }
}

fn evaluate_type<S: GraphSnapshot>(args: &[Value], snapshot: &S) -> Value {
    if let Some(Value::EdgeKey(edge_key)) = args.first() {
        if let Some(name) = snapshot.resolve_rel_type_name(edge_key.rel) {
            Value::String(name)
        } else {
            Value::String(format!("<{}>", edge_key.rel))
        }
    } else {
        Value::Null
    }
}

fn evaluate_id(args: &[Value]) -> Value {
    match args.first() {
        Some(Value::NodeId(id)) => Value::Int(*id as i64),
        Some(Value::EdgeKey(edge_key)) => Value::Int(edge_key.src as i64),
        _ => Value::Null,
    }
}

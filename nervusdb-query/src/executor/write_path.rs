use super::{
    NodeValue, Plan, PropertyValue, RelationshipValue, Result, Row, Value, WriteableGraph,
    api_property_map_to_storage, convert_api_property_to_value, execute_plan,
};
use crate::ast::{Expression, Literal};
use crate::error::Error;
use crate::evaluator::evaluate_expression_value;
use nervusdb_api::GraphSnapshot;

pub(super) fn evaluate_property_value(
    expr: &Expression,
    params: &crate::query_api::Params,
) -> Result<PropertyValue> {
    match expr {
        Expression::Literal(lit) => match lit {
            Literal::Null => Ok(PropertyValue::Null),
            Literal::Boolean(b) => Ok(PropertyValue::Bool(*b)),
            Literal::Integer(n) => Ok(PropertyValue::Int(*n)),
            Literal::Float(n) => Ok(PropertyValue::Float(*n)),
            Literal::String(s) => Ok(PropertyValue::String(s.clone())),
        },
        Expression::Parameter(name) => {
            if let Some(value) = params.get(name) {
                convert_executor_value_to_property(value)
            } else {
                Ok(PropertyValue::Null)
            }
        }
        _ => Err(Error::NotImplemented(
            "complex expressions in property values not supported in v2 M3",
        )),
    }
}

pub(super) fn execute_set<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    items: &[(String, String, Expression)],
    params: &crate::query_api::Params,
) -> Result<u32> {
    let mut count = 0;
    // Iterate over input rows
    for row in execute_plan(snapshot, input, params) {
        let row = row?;
        for (var, key, expr) in items {
            super::plan_mid::ensure_runtime_expression_compatible(expr, &row, snapshot, params)?;
            // Evaluate expression
            let val = evaluate_expression_value(expr, &row, snapshot, params);

            // Convert value to PropertyValue
            let prop_val = convert_executor_value_to_property(&val)?;
            let is_remove = matches!(prop_val, PropertyValue::Null);

            if let Some(node_id) = row.get_node(var) {
                if is_remove {
                    let existed = match row.get(var) {
                        Some(Value::Node(node)) => node.properties.contains_key(key),
                        _ => snapshot.node_property(node_id, key).is_some(),
                    };
                    txn.remove_node_property(node_id, key)?;
                    if existed {
                        count += 1;
                    }
                } else {
                    txn.set_node_property(node_id, key.clone(), prop_val)?;
                    count += 1;
                }
            } else if let Some(edge) = row.get_edge(var) {
                if is_remove {
                    let existed = match row.get(var) {
                        Some(Value::Relationship(rel)) => rel.properties.contains_key(key),
                        _ => snapshot.edge_property(edge, key).is_some(),
                    };
                    txn.remove_edge_property(edge.src, edge.rel, edge.dst, key)?;
                    if existed {
                        count += 1;
                    }
                } else {
                    txn.set_edge_property(edge.src, edge.rel, edge.dst, key.clone(), prop_val)?;
                    count += 1;
                }
            } else if matches!(row.get(var), Some(Value::Null)) {
                continue;
            } else {
                return Err(Error::Other(format!("Variable {} not found in row", var)));
            }
        }
    }
    Ok(count)
}

fn value_map_to_property_map(
    map: &std::collections::BTreeMap<String, Value>,
) -> Result<std::collections::BTreeMap<String, PropertyValue>> {
    let mut out = std::collections::BTreeMap::new();
    for (k, v) in map {
        out.insert(k.clone(), convert_executor_value_to_property(v)?);
    }
    Ok(out)
}

pub(super) fn execute_set_from_maps<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    items: &[(String, Expression, bool)],
    params: &crate::query_api::Params,
) -> Result<u32> {
    let mut count = 0;

    for row in execute_plan(snapshot, input, params) {
        let row = row?;
        for (var, expr, append) in items {
            super::plan_mid::ensure_runtime_expression_compatible(expr, &row, snapshot, params)?;
            let evaluated = evaluate_expression_value(expr, &row, snapshot, params);
            if matches!(evaluated, Value::Null) {
                continue;
            }
            let map_values = match evaluated {
                Value::Map(map_values) => map_values,
                Value::Node(node) => node.properties,
                Value::Relationship(rel) => rel.properties,
                Value::NodeId(node_id) => snapshot
                    .node_properties(node_id)
                    .unwrap_or_default()
                    .iter()
                    .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
                    .collect(),
                Value::EdgeKey(edge) => snapshot
                    .edge_properties(edge)
                    .unwrap_or_default()
                    .iter()
                    .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
                    .collect(),
                _ => {
                    return Err(Error::Other(
                        "SET map operation expects a map expression".to_string(),
                    ));
                }
            };

            if let Some(node_id) = row.get_node(var) {
                let existing: std::collections::BTreeMap<String, PropertyValue> = match row.get(var)
                {
                    Some(Value::Node(node)) => value_map_to_property_map(&node.properties)?,
                    _ => snapshot
                        .node_properties(node_id)
                        .map(|props| api_property_map_to_storage(&props))
                        .unwrap_or_default(),
                };
                let mut target = if *append {
                    existing.clone()
                } else {
                    std::collections::BTreeMap::new()
                };

                for (key, value) in &map_values {
                    if matches!(value, Value::Null) {
                        target.remove(key);
                    } else {
                        target.insert(key.clone(), convert_executor_value_to_property(value)?);
                    }
                }

                for key in existing.keys() {
                    if !target.contains_key(key) {
                        txn.remove_node_property(node_id, key)?;
                        count += 1;
                    }
                }
                for (key, value) in target {
                    if existing.get(&key) != Some(&value) {
                        txn.set_node_property(node_id, key, value)?;
                        count += 1;
                    }
                }
                continue;
            }

            if let Some(edge) = row.get_edge(var) {
                let existing: std::collections::BTreeMap<String, PropertyValue> = match row.get(var)
                {
                    Some(Value::Relationship(rel)) => value_map_to_property_map(&rel.properties)?,
                    _ => snapshot
                        .edge_properties(edge)
                        .map(|props| api_property_map_to_storage(&props))
                        .unwrap_or_default(),
                };
                let mut target = if *append {
                    existing.clone()
                } else {
                    std::collections::BTreeMap::new()
                };

                for (key, value) in &map_values {
                    if matches!(value, Value::Null) {
                        target.remove(key);
                    } else {
                        target.insert(key.clone(), convert_executor_value_to_property(value)?);
                    }
                }

                for key in existing.keys() {
                    if !target.contains_key(key) {
                        txn.remove_edge_property(edge.src, edge.rel, edge.dst, key)?;
                        count += 1;
                    }
                }
                for (key, value) in target {
                    if existing.get(&key) != Some(&value) {
                        txn.set_edge_property(edge.src, edge.rel, edge.dst, key, value)?;
                        count += 1;
                    }
                }
                continue;
            }

            if matches!(row.get(var), Some(Value::Null)) {
                continue;
            }

            return Err(Error::Other(format!("Variable {} not found in row", var)));
        }
    }

    Ok(count)
}

pub(super) fn execute_remove<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    items: &[(String, String)],
    params: &crate::query_api::Params,
) -> Result<u32> {
    let mut count = 0;
    for row in execute_plan(snapshot, input, params) {
        let row = row?;
        for (var, key) in items {
            match row.get(var) {
                Some(Value::Node(node)) => {
                    let existed = node.properties.contains_key(key);
                    txn.remove_node_property(node.id, key)?;
                    if existed {
                        count += 1;
                    }
                }
                Some(Value::NodeId(node_id)) => {
                    let existed = snapshot.node_property(*node_id, key).is_some();
                    txn.remove_node_property(*node_id, key)?;
                    if existed {
                        count += 1;
                    }
                }
                Some(Value::Relationship(rel)) => {
                    let existed = rel.properties.contains_key(key);
                    txn.remove_edge_property(rel.key.src, rel.key.rel, rel.key.dst, key)?;
                    if existed {
                        count += 1;
                    }
                }
                Some(Value::EdgeKey(edge)) => {
                    let existed = snapshot.edge_property(*edge, key).is_some();
                    txn.remove_edge_property(edge.src, edge.rel, edge.dst, key)?;
                    if existed {
                        count += 1;
                    }
                }
                Some(Value::Null) => continue,
                Some(_) | None => {
                    return Err(Error::Other(format!("Variable {} not found in row", var)));
                }
            }
        }
    }
    Ok(count)
}

pub(super) fn execute_set_labels<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    items: &[(String, Vec<String>)],
    params: &crate::query_api::Params,
) -> Result<u32> {
    let mut count = 0;
    for row in execute_plan(snapshot, input, params) {
        let row = row?;
        for (var, labels) in items {
            if let Some(node_id) = row.get_node(var) {
                for label in labels {
                    let label_id = txn.get_or_create_label_id(label)?;
                    txn.add_node_label(node_id, label_id)?;
                    count += 1;
                }
                continue;
            }

            if matches!(row.get(var), Some(Value::Null)) {
                continue;
            }

            return Err(Error::Other(format!("Variable {} not found in row", var)));
        }
    }
    Ok(count)
}

pub(super) fn execute_remove_labels<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    items: &[(String, Vec<String>)],
    params: &crate::query_api::Params,
) -> Result<u32> {
    let mut count = 0;
    for row in execute_plan(snapshot, input, params) {
        let row = row?;
        for (var, labels) in items {
            if let Some(node_id) = row.get_node(var) {
                for label in labels {
                    if let Some(label_id) = snapshot.resolve_label_id(label) {
                        txn.remove_node_label(node_id, label_id)?;
                        count += 1;
                    }
                }
                continue;
            }

            if matches!(row.get(var), Some(Value::Null)) {
                continue;
            }

            return Err(Error::Other(format!("Variable {} not found in row", var)));
        }
    }
    Ok(count)
}

pub(super) fn apply_set_property_overlay_to_rows<S: GraphSnapshot>(
    snapshot: &S,
    rows: Vec<Row>,
    items: &[(String, String, Expression)],
    params: &crate::query_api::Params,
) -> Vec<Row> {
    rows.into_iter()
        .map(|mut row| {
            // Keep expression evaluation semantics aligned with execute_set:
            // expressions are evaluated against the pre-SET row values.
            let source_row = row.clone();

            for (var, key, expr) in items {
                let Some(current) = row.get(var).cloned() else {
                    continue;
                };

                let val = evaluate_expression_value(expr, &source_row, snapshot, params);
                let is_remove = matches!(val, Value::Null);

                match current {
                    Value::Node(mut node) => {
                        if is_remove {
                            node.properties.remove(key);
                        } else {
                            node.properties.insert(key.clone(), val);
                        }
                        row = row.with(var.clone(), Value::Node(node));
                    }
                    Value::NodeId(node_id) => {
                        let labels = snapshot
                            .resolve_node_labels(node_id)
                            .unwrap_or_default()
                            .into_iter()
                            .filter_map(|id| snapshot.resolve_label_name(id))
                            .collect();
                        let mut properties: std::collections::BTreeMap<String, Value> = snapshot
                            .node_properties(node_id)
                            .unwrap_or_default()
                            .iter()
                            .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
                            .collect();
                        if is_remove {
                            properties.remove(key);
                        } else {
                            properties.insert(key.clone(), val);
                        }
                        row = row.with(
                            var.clone(),
                            Value::Node(NodeValue {
                                id: node_id,
                                labels,
                                properties,
                            }),
                        );
                    }
                    Value::Relationship(mut rel) => {
                        if is_remove {
                            rel.properties.remove(key);
                        } else {
                            rel.properties.insert(key.clone(), val);
                        }
                        row = row.with(var.clone(), Value::Relationship(rel));
                    }
                    Value::EdgeKey(edge) => {
                        let rel_type = snapshot
                            .resolve_rel_type_name(edge.rel)
                            .unwrap_or_else(|| format!("<{}>", edge.rel));
                        let mut properties: std::collections::BTreeMap<String, Value> = snapshot
                            .edge_properties(edge)
                            .unwrap_or_default()
                            .iter()
                            .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
                            .collect();
                        if is_remove {
                            properties.remove(key);
                        } else {
                            properties.insert(key.clone(), val);
                        }
                        row = row.with(
                            var.clone(),
                            Value::Relationship(RelationshipValue {
                                key: edge,
                                rel_type,
                                properties,
                            }),
                        );
                    }
                    _ => {}
                }
            }

            row
        })
        .collect()
}

pub(super) fn apply_set_map_overlay_to_rows<S: GraphSnapshot>(
    snapshot: &S,
    rows: Vec<Row>,
    items: &[(String, Expression, bool)],
    params: &crate::query_api::Params,
) -> Vec<Row> {
    rows.into_iter()
        .map(|mut row| {
            // Keep evaluation semantics aligned with execute_set_from_maps.
            let source_row = row.clone();

            for (var, expr, append) in items {
                let Some(current) = row.get(var).cloned() else {
                    continue;
                };

                let evaluated = evaluate_expression_value(expr, &source_row, snapshot, params);
                if matches!(evaluated, Value::Null) {
                    continue;
                }
                let Value::Map(map_values) = evaluated else {
                    continue;
                };

                match current {
                    Value::Node(mut node) => {
                        if !append {
                            node.properties.clear();
                        }
                        for (key, value) in map_values {
                            if matches!(value, Value::Null) {
                                node.properties.remove(&key);
                            } else {
                                node.properties.insert(key, value);
                            }
                        }
                        row = row.with(var.clone(), Value::Node(node));
                    }
                    Value::NodeId(node_id) => {
                        let labels = snapshot
                            .resolve_node_labels(node_id)
                            .unwrap_or_default()
                            .into_iter()
                            .filter_map(|id| snapshot.resolve_label_name(id))
                            .collect();
                        let mut properties: std::collections::BTreeMap<String, Value> = if *append {
                            snapshot
                                .node_properties(node_id)
                                .unwrap_or_default()
                                .iter()
                                .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
                                .collect()
                        } else {
                            std::collections::BTreeMap::new()
                        };
                        for (key, value) in map_values {
                            if matches!(value, Value::Null) {
                                properties.remove(&key);
                            } else {
                                properties.insert(key, value);
                            }
                        }
                        row = row.with(
                            var.clone(),
                            Value::Node(NodeValue {
                                id: node_id,
                                labels,
                                properties,
                            }),
                        );
                    }
                    Value::Relationship(mut rel) => {
                        if !append {
                            rel.properties.clear();
                        }
                        for (key, value) in map_values {
                            if matches!(value, Value::Null) {
                                rel.properties.remove(&key);
                            } else {
                                rel.properties.insert(key, value);
                            }
                        }
                        row = row.with(var.clone(), Value::Relationship(rel));
                    }
                    Value::EdgeKey(edge) => {
                        let rel_type = snapshot
                            .resolve_rel_type_name(edge.rel)
                            .unwrap_or_else(|| format!("<{}>", edge.rel));
                        let mut properties: std::collections::BTreeMap<String, Value> = if *append {
                            snapshot
                                .edge_properties(edge)
                                .unwrap_or_default()
                                .iter()
                                .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
                                .collect()
                        } else {
                            std::collections::BTreeMap::new()
                        };
                        for (key, value) in map_values {
                            if matches!(value, Value::Null) {
                                properties.remove(&key);
                            } else {
                                properties.insert(key, value);
                            }
                        }
                        row = row.with(
                            var.clone(),
                            Value::Relationship(RelationshipValue {
                                key: edge,
                                rel_type,
                                properties,
                            }),
                        );
                    }
                    _ => {}
                }
            }

            row
        })
        .collect()
}

pub(super) fn apply_label_overlay_to_rows<S: GraphSnapshot>(
    snapshot: &S,
    rows: Vec<Row>,
    items: &[(String, Vec<String>)],
    is_add: bool,
) -> Vec<Row> {
    rows.into_iter()
        .map(|mut row| {
            for (var, labels) in items {
                let Some(node_id) = row.get_node(var) else {
                    continue;
                };

                let mut current_labels: Vec<String> = match row.get(var) {
                    Some(Value::Node(node)) => node.labels.clone(),
                    _ => snapshot
                        .resolve_node_labels(node_id)
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(|id| snapshot.resolve_label_name(id))
                        .collect(),
                };

                if is_add {
                    for label in labels {
                        if !current_labels.iter().any(|existing| existing == label) {
                            current_labels.push(label.clone());
                        }
                    }
                } else {
                    current_labels.retain(|existing| !labels.iter().any(|label| label == existing));
                }

                let properties = snapshot
                    .node_properties(node_id)
                    .unwrap_or_default()
                    .iter()
                    .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
                    .collect();

                row = row.with(
                    var.clone(),
                    Value::Node(NodeValue {
                        id: node_id,
                        labels: current_labels,
                        properties,
                    }),
                );
            }
            row
        })
        .collect()
}

pub(super) fn apply_removed_property_overlay_to_rows<S: GraphSnapshot>(
    snapshot: &S,
    rows: Vec<Row>,
    items: &[(String, String)],
) -> Vec<Row> {
    rows.into_iter()
        .map(|mut row| {
            for (var, key) in items {
                let Some(current) = row.get(var).cloned() else {
                    continue;
                };

                match current {
                    Value::Node(mut node) => {
                        node.properties.remove(key);
                        row = row.with(var.clone(), Value::Node(node));
                    }
                    Value::NodeId(node_id) => {
                        let labels = snapshot
                            .resolve_node_labels(node_id)
                            .unwrap_or_default()
                            .into_iter()
                            .filter_map(|id| snapshot.resolve_label_name(id))
                            .collect();
                        let mut properties: std::collections::BTreeMap<String, Value> = snapshot
                            .node_properties(node_id)
                            .unwrap_or_default()
                            .iter()
                            .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
                            .collect();
                        properties.remove(key);
                        row = row.with(
                            var.clone(),
                            Value::Node(NodeValue {
                                id: node_id,
                                labels,
                                properties,
                            }),
                        );
                    }
                    Value::Relationship(mut rel) => {
                        rel.properties.remove(key);
                        row = row.with(var.clone(), Value::Relationship(rel));
                    }
                    Value::EdgeKey(edge) => {
                        let rel_type = snapshot
                            .resolve_rel_type_name(edge.rel)
                            .unwrap_or_else(|| format!("<{}>", edge.rel));
                        let mut properties: std::collections::BTreeMap<String, Value> = snapshot
                            .edge_properties(edge)
                            .unwrap_or_default()
                            .iter()
                            .map(|(k, v)| (k.clone(), convert_api_property_to_value(v)))
                            .collect();
                        properties.remove(key);
                        row = row.with(
                            var.clone(),
                            Value::Relationship(RelationshipValue {
                                key: edge,
                                rel_type,
                                properties,
                            }),
                        );
                    }
                    _ => {}
                }
            }
            row
        })
        .collect()
}

pub(super) fn convert_executor_value_to_property(value: &Value) -> Result<PropertyValue> {
    match value {
        Value::Null => Ok(PropertyValue::Null),
        Value::Bool(b) => Ok(PropertyValue::Bool(*b)),
        Value::Int(i) => Ok(PropertyValue::Int(*i)),
        Value::String(s) => Ok(PropertyValue::String(s.clone())),
        Value::Float(f) => Ok(PropertyValue::Float(*f)),
        Value::DateTime(i) => Ok(PropertyValue::DateTime(*i)),
        Value::Blob(b) => Ok(PropertyValue::Blob(b.clone())),
        Value::Path(_) => Err(Error::Other(
            "Path value cannot be stored as property".to_string(),
        )),
        Value::List(l) => {
            let mut list = Vec::with_capacity(l.len());
            for v in l {
                if matches!(
                    v,
                    Value::Node(_)
                        | Value::Relationship(_)
                        | Value::Path(_)
                        | Value::ReifiedPath(_)
                        | Value::NodeId(_)
                        | Value::ExternalId(_)
                        | Value::EdgeKey(_)
                ) || matches!(v, Value::Map(_)) && !is_duration_map_value(v)
                {
                    return Err(Error::Other(
                        "runtime error: InvalidPropertyType".to_string(),
                    ));
                }
                list.push(convert_executor_value_to_property(v)?);
            }
            Ok(PropertyValue::List(list))
        }
        Value::Map(m) => {
            let mut map = std::collections::BTreeMap::new();
            for (k, v) in m {
                map.insert(k.clone(), convert_executor_value_to_property(v)?);
            }
            Ok(PropertyValue::Map(map))
        }
        Value::Node(_) | Value::Relationship(_) | Value::ReifiedPath(_) => {
            Err(Error::NotImplemented(
                "node/relationship/path objects as property values are not supported",
            ))
        }
        Value::NodeId(_) | Value::ExternalId(_) | Value::EdgeKey(_) => Err(Error::NotImplemented(
            "node/edge identifiers as property values are not supported",
        )),
    }
}

fn is_duration_map_value(value: &Value) -> bool {
    matches!(
        value,
        Value::Map(map)
            if matches!(map.get("__kind"), Some(Value::String(kind)) if kind == "duration")
    )
}

#[cfg(test)]
mod tests {
    use super::convert_executor_value_to_property;
    use crate::error::Error;
    use crate::executor::{PropertyValue, Value};
    use std::collections::BTreeMap;

    fn duration_value(display: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("__kind".to_string(), Value::String("duration".to_string())),
            ("months".to_string(), Value::Int(0)),
            ("days".to_string(), Value::Int(0)),
            ("nanos".to_string(), Value::Int(13_000_000_000)),
            ("seconds".to_string(), Value::Int(13)),
            ("nanosecondsOfSecond".to_string(), Value::Int(0)),
            ("__display".to_string(), Value::String(display.to_string())),
        ]))
    }

    #[test]
    fn allows_duration_maps_inside_list_properties() {
        let value = Value::List(vec![
            duration_value("PT13S"),
            duration_value("PT14S"),
            duration_value("PT15S"),
        ]);

        let converted =
            convert_executor_value_to_property(&value).expect("duration list should be accepted");

        match converted {
            PropertyValue::List(items) => {
                assert_eq!(items.len(), 3);
                assert!(
                    items
                        .iter()
                        .all(|item| matches!(item, PropertyValue::Map(_)))
                );
            }
            other => panic!("expected list property, got {other:?}"),
        }
    }

    #[test]
    fn rejects_regular_map_inside_list_properties() {
        let value = Value::List(vec![Value::Map(BTreeMap::from([(
            "num".to_string(),
            Value::Int(1),
        )]))]);

        let err = convert_executor_value_to_property(&value).expect_err("map list must fail");
        match err {
            Error::Other(msg) => assert_eq!(msg, "runtime error: InvalidPropertyType"),
            other => panic!("expected InvalidPropertyType error, got {other:?}"),
        }
    }
}

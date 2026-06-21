use super::{Plan, PropertyValue, Result, Value, WriteableGraph, execute_plan};
use crate::ast::Expression;
use crate::error::Error;
use crate::evaluator::evaluate_expression_value;
use nervusdb_api::GraphSnapshot;

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
            let val = evaluate_expression_value(expr, &row, snapshot, params);

            let prop_val = convert_executor_value_to_property(&val)?;
            if let Some(node_id) = row.get_node(var) {
                if matches!(prop_val, PropertyValue::Null) {
                    txn.remove_node_property(node_id, key)?;
                } else {
                    txn.set_node_property(node_id, key.clone(), prop_val)?;
                }
                count += 1;
            } else if let Some(edge) = row.get_edge(var) {
                if matches!(prop_val, PropertyValue::Null) {
                    txn.remove_edge_property(edge.src, edge.rel, edge.dst, key)?;
                } else {
                    txn.set_edge_property(edge.src, edge.rel, edge.dst, key.clone(), prop_val)?;
                }
                count += 1;
            } else if matches!(row.get(var), Some(Value::Null)) {
                continue;
            } else {
                return Err(Error::Other(format!("Variable {} not found in row", var)));
            }
        }
    }
    Ok(count)
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

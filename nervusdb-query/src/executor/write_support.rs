use super::{
    Plan, PropertyValue, Row, Value, WriteableGraph, apply_set_map_overlay_to_rows,
    convert_executor_value_to_property, execute_set_from_maps,
};
use crate::ast::Expression;
use crate::error::{Error, Result};
use crate::evaluator::evaluate_expression_value;
use nervusdb_api::GraphSnapshot;

pub(super) fn merge_apply_set_items<S: GraphSnapshot>(
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    row: &mut Row,
    items: &[(String, String, Expression)],
    params: &crate::query_api::Params,
) -> Result<()> {
    for (var, key, expr) in items {
        super::plan_mid::ensure_runtime_expression_compatible(expr, row, snapshot, params)?;
        let val = evaluate_expression_value(expr, row, snapshot, params);
        let prop_val = convert_executor_value_to_property(&val)?;
        let is_remove = matches!(prop_val, PropertyValue::Null);
        if let Some(node_id) = row.get_node(var) {
            if is_remove {
                txn.remove_node_property(node_id, key)?;
            } else {
                txn.set_node_property(node_id, key.clone(), prop_val)?;
            }
            overlay_set_property_value(row, var, key, &val, is_remove);
        } else if let Some(edge) = row.get_edge(var) {
            if is_remove {
                txn.remove_edge_property(edge.src, edge.rel, edge.dst, key)?;
            } else {
                txn.set_edge_property(edge.src, edge.rel, edge.dst, key.clone(), prop_val)?;
            }
            overlay_set_property_value(row, var, key, &val, is_remove);
        } else {
            return Err(Error::Other(format!("Variable {} not found in row", var)));
        }
    }
    Ok(())
}

pub(super) fn merge_apply_label_items(
    txn: &mut dyn WriteableGraph,
    row: &mut Row,
    items: &[(String, Vec<String>)],
) -> Result<()> {
    for (var, labels) in items {
        let node_id = row
            .get_node(var)
            .ok_or_else(|| Error::Other(format!("Variable {} not found in row", var)))?;
        for label in labels {
            let label_id = txn.get_or_create_label_id(label)?;
            txn.add_node_label(node_id, label_id)?;
            overlay_add_label_value(row, var, label);
        }
    }
    Ok(())
}

pub(super) fn merge_apply_map_items<S: GraphSnapshot>(
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    row: &mut Row,
    items: &[(String, Expression, bool)],
    params: &crate::query_api::Params,
) -> Result<()> {
    for item in items {
        let single_item = vec![item.clone()];
        let input = Plan::Values {
            rows: vec![row.clone()],
        };
        execute_set_from_maps(snapshot, &input, txn, &single_item, params)?;
        if let Some(updated_row) =
            apply_set_map_overlay_to_rows(snapshot, vec![row.clone()], &single_item, params)
                .into_iter()
                .next()
        {
            *row = updated_row;
        }
    }
    Ok(())
}

fn overlay_set_property_value(row: &mut Row, var: &str, key: &str, value: &Value, is_remove: bool) {
    let Some(current) = row.get(var).cloned() else {
        return;
    };

    let updated = match current {
        Value::Node(mut node) => {
            if is_remove {
                node.properties.remove(key);
            } else {
                node.properties.insert(key.to_string(), value.clone());
            }
            Value::Node(node)
        }
        Value::Relationship(mut rel) => {
            if is_remove {
                rel.properties.remove(key);
            } else {
                rel.properties.insert(key.to_string(), value.clone());
            }
            Value::Relationship(rel)
        }
        other => other,
    };

    *row = row.clone().with(var.to_string(), updated);
}

fn overlay_add_label_value(row: &mut Row, var: &str, label: &str) {
    let Some(current) = row.get(var).cloned() else {
        return;
    };

    let updated = match current {
        Value::Node(mut node) => {
            if !node.labels.iter().any(|existing| existing == label) {
                node.labels.push(label.to_string());
            }
            Value::Node(node)
        }
        other => other,
    };

    *row = row.clone().with(var.to_string(), updated);
}

pub(super) fn merge_eval_props_on_row<S: GraphSnapshot>(
    snapshot: &S,
    row: &Row,
    props: &Option<crate::ast::PropertyMap>,
    params: &crate::query_api::Params,
) -> Result<std::collections::BTreeMap<String, PropertyValue>> {
    let mut out = std::collections::BTreeMap::new();
    if let Some(props) = props {
        for pair in &props.properties {
            super::plan_mid::ensure_runtime_expression_compatible(
                &pair.value,
                row,
                snapshot,
                params,
            )?;
            let v = evaluate_expression_value(&pair.value, row, snapshot, params);
            out.insert(pair.key.clone(), convert_executor_value_to_property(&v)?);
        }
    }
    Ok(out)
}

use super::{
    EdgeKey, Error, ExternalId, GraphSnapshot, InternalNodeId, NodeValue, PathElement, Pattern,
    Plan, RelationshipValue, Result, Row, UNLABELED_LABEL_ID, Value, WriteableGraph,
    convert_executor_value_to_property, evaluate_expression_value, execute_plan,
};
use crate::ast::Expression;

pub(super) fn execute_create_from_rows<S: GraphSnapshot>(
    snapshot: &S,
    input_rows: Vec<Row>,
    txn: &mut dyn WriteableGraph,
    pattern: &Pattern,
    params: &crate::query_api::Params,
) -> Result<(u32, Vec<Row>)> {
    let mut created_count = 0u32;
    let mut output_rows = Vec::with_capacity(input_rows.len());

    let mut node_patterns: Vec<(usize, &crate::ast::NodePattern)> = Vec::new();
    let mut rel_patterns: Vec<(usize, &crate::ast::RelationshipPattern)> = Vec::new();

    for (idx, element) in pattern.elements.iter().enumerate() {
        match element {
            PathElement::Node(n) => node_patterns.push((idx, n)),
            PathElement::Relationship(r) => rel_patterns.push((idx, r)),
        }
    }

    for mut row in input_rows {
        let mut row_node_ids: std::collections::HashMap<usize, InternalNodeId> =
            std::collections::HashMap::new();

        for (idx, node_pat) in &node_patterns {
            if let Some(var) = &node_pat.variable
                && let Some(existing_iid) = row.get_node(var)
            {
                row_node_ids.insert(*idx, existing_iid);
                continue;
            }

            let external_id = ExternalId::from(
                created_count as u64 + chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64,
            );

            let label_id = if let Some(label) = node_pat.labels.first() {
                txn.get_or_create_label_id(label)?
            } else {
                UNLABELED_LABEL_ID
            };

            let node_id = txn.create_node(external_id, label_id)?;
            for extra_label in node_pat.labels.iter().skip(1) {
                let extra_label_id = txn.get_or_create_label_id(extra_label)?;
                txn.add_node_label(node_id, extra_label_id)?;
            }
            created_count += 1;
            row_node_ids.insert(*idx, node_id);

            if let Some(var) = &node_pat.variable {
                row = row.with(var.clone(), Value::NodeId(node_id));
            }

            let mut node_props = std::collections::BTreeMap::new();
            if let Some(props) = &node_pat.properties {
                for prop in &props.properties {
                    super::plan_mid::ensure_runtime_expression_compatible(
                        &prop.value,
                        &row,
                        snapshot,
                        params,
                    )?;
                    let val = evaluate_expression_value(&prop.value, &row, snapshot, params);
                    if matches!(val, Value::Null) {
                        continue;
                    }
                    let prop_val = convert_executor_value_to_property(&val)?;
                    txn.set_node_property(node_id, prop.key.clone(), prop_val)?;
                    node_props.insert(prop.key.clone(), val);
                }
            }

            if let Some(var) = &node_pat.variable {
                row = row.with(
                    var.clone(),
                    Value::Node(NodeValue {
                        id: node_id,
                        labels: node_pat.labels.clone(),
                        properties: node_props,
                    }),
                );
            }
        }

        for (idx, rel_pat) in &rel_patterns {
            let rel_type_name = rel_pat
                .types
                .first()
                .ok_or_else(|| Error::Other("CREATE relationship requires a type".into()))?;
            let rel_type = txn.get_or_create_rel_type_id(rel_type_name)?;

            let left_node_id = if let Some(src) = row_node_ids.get(&(idx - 1)).copied() {
                src
            } else if let Some(src_var) = pattern.elements.get(idx - 1).and_then(|el| match el {
                PathElement::Node(n) => n.variable.as_ref(),
                _ => None,
            }) {
                row.get_node(src_var)
                    .ok_or(Error::Other("CREATE relationship src node missing".into()))?
            } else {
                return Err(Error::Other("CREATE relationship src node missing".into()));
            };

            let right_node_id = if let Some(dst) = row_node_ids.get(&(idx + 1)).copied() {
                dst
            } else if let Some(dst_var) = pattern.elements.get(idx + 1).and_then(|el| match el {
                PathElement::Node(n) => n.variable.as_ref(),
                _ => None,
            }) {
                row.get_node(dst_var)
                    .ok_or(Error::Other("CREATE relationship dst node missing".into()))?
            } else {
                return Err(Error::Other("CREATE relationship dst node missing".into()));
            };

            let (src_id, dst_id) = match rel_pat.direction {
                crate::ast::RelationshipDirection::LeftToRight
                | crate::ast::RelationshipDirection::Undirected => (left_node_id, right_node_id),
                crate::ast::RelationshipDirection::RightToLeft => (right_node_id, left_node_id),
            };

            txn.create_edge(src_id, rel_type, dst_id)?;
            created_count += 1;

            let edge_key = EdgeKey {
                src: src_id,
                rel: rel_type,
                dst: dst_id,
            };

            if let Some(var) = &rel_pat.variable {
                row = row.with(var.clone(), Value::EdgeKey(edge_key));
            }

            let mut rel_props = std::collections::BTreeMap::new();
            if let Some(props) = &rel_pat.properties {
                for prop in &props.properties {
                    super::plan_mid::ensure_runtime_expression_compatible(
                        &prop.value,
                        &row,
                        snapshot,
                        params,
                    )?;
                    let val = evaluate_expression_value(&prop.value, &row, snapshot, params);
                    if matches!(val, Value::Null) {
                        continue;
                    }
                    let prop_val = convert_executor_value_to_property(&val)?;
                    txn.set_edge_property(src_id, rel_type, dst_id, prop.key.clone(), prop_val)?;
                    rel_props.insert(prop.key.clone(), val);
                }
            }

            if let Some(var) = &rel_pat.variable {
                row = row.with(
                    var.clone(),
                    Value::Relationship(RelationshipValue {
                        key: edge_key,
                        rel_type: rel_type_name.clone(),
                        properties: rel_props,
                    }),
                );
            }
        }

        output_rows.push(row);
    }

    Ok((created_count, output_rows))
}

fn push_delete_node_target(
    node_id: InternalNodeId,
    nodes_to_delete: &mut Vec<InternalNodeId>,
    seen_nodes: &mut std::collections::HashSet<InternalNodeId>,
    edges_to_delete: &[EdgeKey],
    max_delete_targets: usize,
) -> Result<()> {
    if seen_nodes.insert(node_id) {
        nodes_to_delete.push(node_id);
    }

    if nodes_to_delete.len() + edges_to_delete.len() > max_delete_targets {
        return Err(Error::Other(format!(
            "DELETE target limit exceeded ({max_delete_targets}); batch your deletes"
        )));
    }

    Ok(())
}

fn push_delete_edge_target(
    edge: EdgeKey,
    nodes_to_delete: &[InternalNodeId],
    edges_to_delete: &mut Vec<EdgeKey>,
    seen_edges: &mut std::collections::HashSet<EdgeKey>,
    max_delete_targets: usize,
) -> Result<()> {
    if seen_edges.insert(edge) {
        edges_to_delete.push(edge);
    }

    if nodes_to_delete.len() + edges_to_delete.len() > max_delete_targets {
        return Err(Error::Other(format!(
            "DELETE target limit exceeded ({max_delete_targets}); batch your deletes"
        )));
    }

    Ok(())
}

fn collect_delete_targets_from_value(
    value: &Value,
    nodes_to_delete: &mut Vec<InternalNodeId>,
    seen_nodes: &mut std::collections::HashSet<InternalNodeId>,
    edges_to_delete: &mut Vec<EdgeKey>,
    seen_edges: &mut std::collections::HashSet<EdgeKey>,
    max_delete_targets: usize,
) -> Result<()> {
    match value {
        Value::Null => {}
        Value::NodeId(node_id) => {
            push_delete_node_target(
                *node_id,
                nodes_to_delete,
                seen_nodes,
                edges_to_delete,
                max_delete_targets,
            )?;
        }
        Value::Node(node) => {
            push_delete_node_target(
                node.id,
                nodes_to_delete,
                seen_nodes,
                edges_to_delete,
                max_delete_targets,
            )?;
        }
        Value::EdgeKey(edge) => {
            push_delete_edge_target(
                *edge,
                nodes_to_delete,
                edges_to_delete,
                seen_edges,
                max_delete_targets,
            )?;
        }
        Value::Relationship(rel) => {
            push_delete_edge_target(
                rel.key,
                nodes_to_delete,
                edges_to_delete,
                seen_edges,
                max_delete_targets,
            )?;
        }
        Value::Path(path) => {
            for edge in &path.edges {
                push_delete_edge_target(
                    *edge,
                    nodes_to_delete,
                    edges_to_delete,
                    seen_edges,
                    max_delete_targets,
                )?;
            }
            for node_id in &path.nodes {
                push_delete_node_target(
                    *node_id,
                    nodes_to_delete,
                    seen_nodes,
                    edges_to_delete,
                    max_delete_targets,
                )?;
            }
        }
        Value::ReifiedPath(path) => {
            for rel in &path.relationships {
                push_delete_edge_target(
                    rel.key,
                    nodes_to_delete,
                    edges_to_delete,
                    seen_edges,
                    max_delete_targets,
                )?;
            }
            for node in &path.nodes {
                push_delete_node_target(
                    node.id,
                    nodes_to_delete,
                    seen_nodes,
                    edges_to_delete,
                    max_delete_targets,
                )?;
            }
        }
        Value::List(list) => {
            for item in list {
                collect_delete_targets_from_value(
                    item,
                    nodes_to_delete,
                    seen_nodes,
                    edges_to_delete,
                    seen_edges,
                    max_delete_targets,
                )?;
            }
        }
        Value::Map(map) => {
            for item in map.values() {
                collect_delete_targets_from_value(
                    item,
                    nodes_to_delete,
                    seen_nodes,
                    edges_to_delete,
                    seen_edges,
                    max_delete_targets,
                )?;
            }
        }
        _ => {
            return Err(Error::Other(
                "DELETE only supports node, relationship, or path values".to_string(),
            ));
        }
    }

    Ok(())
}

pub(super) fn execute_delete_on_rows<S: GraphSnapshot>(
    snapshot: &S,
    rows: &[Row],
    txn: &mut dyn WriteableGraph,
    detach: bool,
    expressions: &[Expression],
    params: &crate::query_api::Params,
) -> Result<u32> {
    const MAX_DELETE_TARGETS: usize = 100_000;

    let mut deleted_count = 0u32;
    let mut nodes_to_delete: Vec<InternalNodeId> = Vec::new();
    let mut seen_nodes: std::collections::HashSet<InternalNodeId> =
        std::collections::HashSet::new();
    let mut edges_to_delete: Vec<EdgeKey> = Vec::new();
    let mut seen_edges: std::collections::HashSet<EdgeKey> = std::collections::HashSet::new();

    for row in rows {
        for expr in expressions {
            super::plan_mid::ensure_runtime_expression_compatible(expr, row, snapshot, params)?;
            let value = evaluate_expression_value(expr, row, snapshot, params);
            collect_delete_targets_from_value(
                &value,
                &mut nodes_to_delete,
                &mut seen_nodes,
                &mut edges_to_delete,
                &mut seen_edges,
                MAX_DELETE_TARGETS,
            )?;
        }
    }

    if detach {
        for &node_id in &nodes_to_delete {
            for edge in snapshot.neighbors(node_id, None) {
                txn.tombstone_edge(edge.src, edge.rel, edge.dst)?;
                deleted_count += 1;
            }
        }
    }

    for edge in edges_to_delete {
        txn.tombstone_edge(edge.src, edge.rel, edge.dst)?;
        deleted_count += 1;
    }

    for node_id in nodes_to_delete {
        txn.tombstone_node(node_id)?;
        deleted_count += 1;
    }

    Ok(deleted_count)
}

pub(super) fn execute_create_write_rows<S: GraphSnapshot>(
    plan: &Plan,
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    params: &crate::query_api::Params,
) -> Result<(u32, Vec<Row>)> {
    match plan {
        Plan::Create { input, pattern, .. } => {
            let (prefix_mods, input_rows) =
                execute_create_write_rows(input, snapshot, txn, params)?;
            let (created, out_rows) =
                execute_create_from_rows(snapshot, input_rows, txn, pattern, params)?;
            Ok((prefix_mods + created, out_rows))
        }
        Plan::Delete {
            input,
            detach,
            expressions,
        } => {
            let (prefix_mods, rows) = execute_create_write_rows(input, snapshot, txn, params)?;
            let deleted =
                execute_delete_on_rows(snapshot, &rows, txn, *detach, expressions, params)?;
            Ok((prefix_mods + deleted, rows))
        }
        Plan::Values { rows } => Ok((0, rows.clone())),
        Plan::ReturnOne => Ok((0, vec![Row::default()])),
        _ => {
            // Non-write wrappers (WITH/UNWIND/PROJECT/...) may still contain nested writes.
            // Delegate back to write orchestration so CREATE/DELETE children are executed
            // through write-capable paths instead of read-only `execute_plan`.
            super::execute_write_with_rows(plan, snapshot, txn, params)
        }
    }
}

pub(super) fn execute_create<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    pattern: &Pattern,
    params: &crate::query_api::Params,
) -> Result<u32> {
    let mut prefix_mod_count = 0u32;
    let mut input_rows = Vec::new();

    let mut input_iter = execute_plan(snapshot, input, params);
    match input_iter.next() {
        Some(Ok(row)) => {
            input_rows.push(row);
            for row in input_iter {
                input_rows.push(row?);
            }
        }
        Some(Err(err)) => {
            let msg = err.to_string();
            if msg.contains("must be executed via execute_write") {
                let (mods, rows) = execute_create_write_rows(input, snapshot, txn, params)?;
                prefix_mod_count = mods;
                input_rows = rows;
            } else {
                return Err(err);
            }
        }
        None => {}
    }

    let (created_count, _output_rows) =
        execute_create_from_rows(snapshot, input_rows, txn, pattern, params)?;
    Ok(created_count + prefix_mod_count)
}

pub(super) fn execute_delete<S: GraphSnapshot>(
    snapshot: &S,
    input: &Plan,
    txn: &mut dyn WriteableGraph,
    detach: bool,
    expressions: &[Expression],
    params: &crate::query_api::Params,
) -> Result<u32> {
    const MAX_DELETE_TARGETS: usize = 100_000;

    let mut deleted_count = 0u32;
    let mut nodes_to_delete: Vec<InternalNodeId> = Vec::new();
    let mut seen_nodes: std::collections::HashSet<InternalNodeId> =
        std::collections::HashSet::new();
    let mut edges_to_delete: Vec<EdgeKey> = Vec::new();
    let mut seen_edges: std::collections::HashSet<EdgeKey> = std::collections::HashSet::new();

    // Stream input rows and collect delete targets without materializing all rows.
    for row in execute_plan(snapshot, input, params) {
        let row = row?;
        for expr in expressions {
            super::plan_mid::ensure_runtime_expression_compatible(expr, &row, snapshot, params)?;
            let value = evaluate_expression_value(expr, &row, snapshot, params);
            collect_delete_targets_from_value(
                &value,
                &mut nodes_to_delete,
                &mut seen_nodes,
                &mut edges_to_delete,
                &mut seen_edges,
                MAX_DELETE_TARGETS,
            )?;
        }
    }

    // If detach=true, delete all edges connected to nodes being deleted
    if detach {
        for &node_id in &nodes_to_delete {
            // Get all edges connected to this node and delete them
            for edge in snapshot.neighbors(node_id, None) {
                txn.tombstone_edge(edge.src, edge.rel, edge.dst)?;
                deleted_count += 1;
            }
        }
    }

    // Delete explicitly targeted edges.
    for edge in edges_to_delete {
        txn.tombstone_edge(edge.src, edge.rel, edge.dst)?;
        deleted_count += 1;
    }

    // Delete the nodes
    for node_id in nodes_to_delete {
        txn.tombstone_node(node_id)?;
        deleted_count += 1;
    }

    Ok(deleted_count)
}

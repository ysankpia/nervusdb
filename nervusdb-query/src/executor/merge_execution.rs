use super::merge_execute_support::{ExecMergeOverlayNode, exec_merge_find_or_create_node};
use super::merge_helpers::{
    merge_collect_edges_between, merge_create_node, merge_edge_matches_overlay,
    merge_find_node_candidates, merge_materialize_node_value,
};
use super::merge_overlay::{MergeOverlayEdge, MergeOverlayNode, MergeOverlayState};
use super::property_bridge::merge_props_to_values;
use super::write_support::{
    merge_apply_label_items, merge_apply_set_items, merge_eval_props_on_row,
};
use super::{
    EdgeKey, Error, GraphSnapshot, NodeValue, PathValue, Plan, RelationshipValue, Result, Row,
    Value, WriteableGraph,
};
use crate::ast::{Expression, PathElement, Pattern};

pub(super) fn execute_merge_create_from_rows<S: GraphSnapshot>(
    snapshot: &S,
    input_rows: Vec<Row>,
    txn: &mut dyn WriteableGraph,
    pattern: &Pattern,
    params: &crate::query_api::Params,
    on_create_items: &[(String, String, Expression)],
    on_match_items: &[(String, String, Expression)],
    on_create_labels: &[(String, Vec<String>)],
    on_match_labels: &[(String, Vec<String>)],
    overlay: &mut MergeOverlayState,
) -> Result<(u32, Vec<Row>)> {
    let mut created_count = 0u32;
    let mut output_rows = Vec::new();

    if pattern.elements.is_empty() {
        return Err(Error::Other("MERGE pattern cannot be empty".into()));
    }

    if pattern.elements.len() == 1 {
        let node_pat = match &pattern.elements[0] {
            PathElement::Node(n) => n,
            _ => return Err(Error::Other("MERGE pattern must start with a node".into())),
        };

        for row in input_rows {
            let node_props = merge_eval_props_on_row(snapshot, &row, &node_pat.properties, params)?;
            let mut was_created = false;
            let mut candidates = if let Some(var) = &node_pat.variable {
                row.get_node(var).map(|iid| vec![iid]).unwrap_or_default()
            } else {
                Vec::new()
            };

            if candidates.is_empty() {
                candidates =
                    merge_find_node_candidates(snapshot, overlay, &node_pat.labels, &node_props);
            }

            if candidates.is_empty()
                && node_pat.variable.is_none()
                && overlay.anonymous_nodes.iter().any(|(labels, props)| {
                    anonymous_node_matches(labels, props, node_pat, &node_props)
                })
            {
                output_rows.push(row.clone());
                continue;
            }

            if candidates.is_empty() {
                let iid = merge_create_node(txn, node_pat, &node_props, &mut created_count)?;
                overlay.nodes.push(MergeOverlayNode {
                    iid,
                    labels: node_pat.labels.clone(),
                    props: node_props.clone(),
                });
                candidates.push(iid);
                was_created = true;
            }

            for iid in candidates {
                let mut out_row = row.clone();
                if let Some(var) = &node_pat.variable {
                    out_row = out_row.with(
                        var.clone(),
                        merge_materialize_node_value(snapshot, overlay, iid),
                    );
                }
                if let Some(path_var) = &pattern.variable {
                    out_row = out_row.with(
                        path_var.clone(),
                        Value::Path(PathValue {
                            nodes: vec![iid],
                            edges: vec![],
                        }),
                    );
                }
                if was_created {
                    if !on_create_items.is_empty() {
                        merge_apply_set_items(
                            snapshot,
                            txn,
                            &mut out_row,
                            on_create_items,
                            params,
                        )?;
                    }
                    if !on_create_labels.is_empty() {
                        merge_apply_label_items(txn, &mut out_row, on_create_labels)?;
                    }
                } else {
                    if !on_match_items.is_empty() {
                        merge_apply_set_items(snapshot, txn, &mut out_row, on_match_items, params)?;
                    }
                    if !on_match_labels.is_empty() {
                        merge_apply_label_items(txn, &mut out_row, on_match_labels)?;
                    }
                }
                output_rows.push(out_row);
            }
        }

        return Ok((created_count, output_rows));
    }

    if pattern.elements.len() != 3 {
        return Err(Error::NotImplemented(
            "MERGE patterns with more than one relationship are not supported in execute_mixed",
        ));
    }

    let src_node = match &pattern.elements[0] {
        PathElement::Node(n) => n,
        _ => return Err(Error::Other("MERGE pattern must start with a node".into())),
    };
    let rel_pat = match &pattern.elements[1] {
        PathElement::Relationship(r) => r,
        _ => {
            return Err(Error::Other(
                "MERGE pattern middle element must be a relationship".into(),
            ));
        }
    };
    let dst_node = match &pattern.elements[2] {
        PathElement::Node(n) => n,
        _ => return Err(Error::Other("MERGE pattern must end with a node".into())),
    };

    let rel_type_name = rel_pat
        .types
        .first()
        .ok_or_else(|| Error::Other("MERGE relationship requires a type".into()))?
        .clone();

    for row in input_rows {
        let src_props = merge_eval_props_on_row(snapshot, &row, &src_node.properties, params)?;
        let dst_props = merge_eval_props_on_row(snapshot, &row, &dst_node.properties, params)?;
        let rel_props = merge_eval_props_on_row(snapshot, &row, &rel_pat.properties, params)?;

        let mut src_candidates = if let Some(var) = &src_node.variable {
            row.get_node(var).map(|iid| vec![iid]).unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut dst_candidates = if let Some(var) = &dst_node.variable {
            row.get_node(var).map(|iid| vec![iid]).unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut created_src = None;
        let mut created_dst = None;

        if src_candidates.is_empty() {
            src_candidates =
                merge_find_node_candidates(snapshot, overlay, &src_node.labels, &src_props);
        }
        if src_candidates.is_empty() {
            let iid = merge_create_node(txn, src_node, &src_props, &mut created_count)?;
            overlay.nodes.push(MergeOverlayNode {
                iid,
                labels: src_node.labels.clone(),
                props: src_props.clone(),
            });
            src_candidates.push(iid);
            created_src = Some(iid);
        }

        if dst_candidates.is_empty() {
            dst_candidates =
                merge_find_node_candidates(snapshot, overlay, &dst_node.labels, &dst_props);
        }
        if dst_candidates.is_empty() {
            let iid = merge_create_node(txn, dst_node, &dst_props, &mut created_count)?;
            overlay.nodes.push(MergeOverlayNode {
                iid,
                labels: dst_node.labels.clone(),
                props: dst_props.clone(),
            });
            dst_candidates.push(iid);
            created_dst = Some(iid);
        }

        let rel_type = txn.get_or_create_rel_type_id(&rel_type_name)?;
        let mut matched_rows = Vec::new();

        for &src_iid in &src_candidates {
            for &dst_iid in &dst_candidates {
                let edges = merge_collect_edges_between(
                    snapshot,
                    overlay,
                    src_iid,
                    dst_iid,
                    rel_type,
                    &rel_pat.direction,
                    &rel_props,
                );

                for edge in edges {
                    let mut out_row = row.clone();
                    if let Some(var) = &src_node.variable {
                        if out_row.get(var).is_none() {
                            out_row = out_row.with(
                                var.clone(),
                                merge_materialize_node_value(snapshot, overlay, src_iid),
                            );
                        }
                    }
                    if let Some(var) = &dst_node.variable {
                        if out_row.get(var).is_none() {
                            out_row = out_row.with(
                                var.clone(),
                                merge_materialize_node_value(snapshot, overlay, dst_iid),
                            );
                        }
                    }
                    if let Some(var) = &rel_pat.variable {
                        if let Some(overlay_edge) =
                            overlay.edges.iter().rev().find(|e| {
                                e.key == edge && merge_edge_matches_overlay(e, &rel_props)
                            })
                        {
                            out_row = out_row.with(
                                var.clone(),
                                Value::Relationship(RelationshipValue {
                                    key: edge,
                                    rel_type: rel_type_name.clone(),
                                    properties: merge_props_to_values(&overlay_edge.props),
                                }),
                            );
                        } else {
                            out_row = out_row.with(var.clone(), Value::EdgeKey(edge));
                        }
                    }
                    if let Some(path_var) = &pattern.variable {
                        out_row.join_path(path_var, edge.src, edge, edge.dst);
                    }
                    if !on_match_items.is_empty() {
                        merge_apply_set_items(snapshot, txn, &mut out_row, on_match_items, params)?;
                    }
                    if !on_match_labels.is_empty() {
                        merge_apply_label_items(txn, &mut out_row, on_match_labels)?;
                    }
                    matched_rows.push(out_row);
                }
            }
        }

        if !matched_rows.is_empty() {
            output_rows.extend(matched_rows);
            continue;
        }

        let src_iid = *src_candidates
            .first()
            .ok_or_else(|| Error::Other("missing source node for MERGE relationship".into()))?;
        let dst_iid = *dst_candidates.first().ok_or_else(|| {
            Error::Other("missing destination node for MERGE relationship".into())
        })?;

        let (edge_src, edge_dst) = match rel_pat.direction {
            crate::ast::RelationshipDirection::LeftToRight
            | crate::ast::RelationshipDirection::Undirected => (src_iid, dst_iid),
            crate::ast::RelationshipDirection::RightToLeft => (dst_iid, src_iid),
        };

        txn.create_edge(edge_src, rel_type, edge_dst)?;
        created_count += 1;
        let edge_key = EdgeKey {
            src: edge_src,
            rel: rel_type,
            dst: edge_dst,
        };

        for (k, v) in &rel_props {
            txn.set_edge_property(edge_src, rel_type, edge_dst, k.clone(), v.clone())?;
        }
        overlay.edges.push(MergeOverlayEdge {
            key: edge_key,
            props: rel_props.clone(),
        });

        let mut out_row = row.clone();
        if let Some(var) = &src_node.variable {
            if out_row.get(var).is_none() {
                let value = if Some(src_iid) == created_src {
                    Value::Node(NodeValue {
                        id: src_iid,
                        labels: src_node.labels.clone(),
                        properties: merge_props_to_values(&src_props),
                    })
                } else {
                    merge_materialize_node_value(snapshot, overlay, src_iid)
                };
                out_row = out_row.with(var.clone(), value);
            }
        }
        if let Some(var) = &dst_node.variable {
            if out_row.get(var).is_none() {
                let value = if Some(dst_iid) == created_dst {
                    Value::Node(NodeValue {
                        id: dst_iid,
                        labels: dst_node.labels.clone(),
                        properties: merge_props_to_values(&dst_props),
                    })
                } else {
                    merge_materialize_node_value(snapshot, overlay, dst_iid)
                };
                out_row = out_row.with(var.clone(), value);
            }
        }
        if let Some(var) = &rel_pat.variable {
            out_row = out_row.with(
                var.clone(),
                Value::Relationship(RelationshipValue {
                    key: edge_key,
                    rel_type: rel_type_name.clone(),
                    properties: merge_props_to_values(&rel_props),
                }),
            );
        }
        if let Some(path_var) = &pattern.variable {
            out_row.join_path(path_var, edge_src, edge_key, edge_dst);
        }
        if !on_create_items.is_empty() {
            merge_apply_set_items(snapshot, txn, &mut out_row, on_create_items, params)?;
        }
        if !on_create_labels.is_empty() {
            merge_apply_label_items(txn, &mut out_row, on_create_labels)?;
        }
        output_rows.push(out_row);
    }

    Ok((created_count, output_rows))
}

fn anonymous_node_matches(
    labels: &[String],
    props: &std::collections::BTreeMap<String, super::PropertyValue>,
    node_pat: &crate::ast::NodePattern,
    expected_props: &std::collections::BTreeMap<String, super::PropertyValue>,
) -> bool {
    for required in &node_pat.labels {
        if !labels.iter().any(|actual| actual == required) {
            return false;
        }
    }
    props == expected_props
}

fn find_create_plan(plan: &Plan) -> Option<&Plan> {
    match plan {
        Plan::Create { merge: true, .. } => Some(plan),
        Plan::Create { input, .. } => find_create_plan(input),
        Plan::Filter { input, .. }
        | Plan::Project { input, .. }
        | Plan::Limit { input, .. }
        | Plan::Skip { input, .. }
        | Plan::OrderBy { input, .. }
        | Plan::Distinct { input }
        | Plan::Unwind { input, .. }
        | Plan::Aggregate { input, .. }
        | Plan::Delete { input, .. }
        | Plan::SetProperty { input, .. }
        | Plan::SetPropertiesFromMap { input, .. }
        | Plan::SetLabels { input, .. }
        | Plan::RemoveProperty { input, .. }
        | Plan::RemoveLabels { input, .. }
        | Plan::Foreach { input, .. } => find_create_plan(input),
        Plan::OptionalWhereFixup {
            outer, filtered, ..
        } => find_create_plan(filtered).or(find_create_plan(outer)),
        Plan::CartesianProduct { left, right } => {
            find_create_plan(left).or(find_create_plan(right))
        }
        Plan::Union { left, right, .. } => find_create_plan(left).or(find_create_plan(right)),
        Plan::Apply {
            input, subquery, ..
        } => find_create_plan(input).or(find_create_plan(subquery)),
        Plan::ProcedureCall { input, .. } => find_create_plan(input),
        Plan::MatchOut { input, .. }
        | Plan::MatchIn { input, .. }
        | Plan::MatchUndirected { input, .. }
        | Plan::MatchOutVarLen { input, .. } => input.as_deref().and_then(find_create_plan),
        Plan::MatchBoundRel { input, .. } => find_create_plan(input),
        Plan::IndexSeek { fallback, .. } => find_create_plan(fallback),
        Plan::NodeScan { .. } | Plan::Values { .. } | Plan::ReturnOne => None,
    }
}

pub(super) fn execute_merge<S: GraphSnapshot>(
    plan: &Plan,
    snapshot: &S,
    txn: &mut dyn WriteableGraph,
    params: &crate::query_api::Params,
    on_create_items: &[(String, String, Expression)],
    on_match_items: &[(String, String, Expression)],
    on_create_labels: &[(String, Vec<String>)],
    on_match_labels: &[(String, Vec<String>)],
) -> Result<u32> {
    let create_plan = find_create_plan(plan)
        .ok_or_else(|| Error::Other("MERGE plan must contain a CREATE stage".into()))?;
    let Plan::Create { pattern, merge, .. } = create_plan else {
        return Err(Error::Other(
            "MERGE CREATE stage is not available in compiled plan".into(),
        ));
    };
    if !merge {
        return Err(Error::Other(
            "MERGE CREATE stage is not available in compiled plan".into(),
        ));
    }

    let mut created_count = 0u32;
    let mut overlay: Vec<ExecMergeOverlayNode> = Vec::new();
    let mut current_iids = Vec::new();
    let mut created_any = false;
    let mut merge_row = Row::default();

    let mut i = 0;
    while i < pattern.elements.len() {
        match &pattern.elements[i] {
            PathElement::Node(node_pat) => {
                let (iid, node_created) = exec_merge_find_or_create_node(
                    snapshot,
                    txn,
                    node_pat,
                    &mut overlay,
                    params,
                    &mut created_count,
                )?;
                if node_created {
                    created_any = true;
                }
                if current_iids.len() <= i {
                    current_iids.resize(i + 1, None);
                }
                current_iids[i] = Some(iid);
                if let Some(var) = &node_pat.variable {
                    merge_row = merge_row.with(var.clone(), Value::NodeId(iid));
                }
                i += 1;
            }
            PathElement::Relationship(rel_pat) => {
                if i + 1 >= pattern.elements.len() {
                    return Err(Error::Other("relationship must be followed by node".into()));
                }
                if let PathElement::Node(dst_node) = &pattern.elements[i + 1] {
                    if i == 0 {
                        return Err(Error::Other(
                            "relationship cannot be the first element in MERGE pattern".into(),
                        ));
                    }
                    let src_iid = current_iids.get(i - 1).and_then(|x| *x).ok_or_else(|| {
                        Error::Other("missing source node for relationship".into())
                    })?;

                    if rel_pat.variable_length.is_some() {
                        return Err(Error::NotImplemented(
                            "MERGE variable-length relationships need multi-hop expansion",
                        ));
                    }

                    let rel_type_name = rel_pat.types.first().cloned().ok_or_else(|| {
                        Error::Other("MERGE relationship requires a type for creation".into())
                    })?;
                    let rel_type = txn.get_or_create_rel_type_id(&rel_type_name)?;

                    let (dst_iid, dst_created) = exec_merge_find_or_create_node(
                        snapshot,
                        txn,
                        dst_node,
                        &mut overlay,
                        params,
                        &mut created_count,
                    )?;
                    if dst_created {
                        created_any = true;
                    }

                    let mut exists = false;
                    for edge in snapshot.neighbors(src_iid, Some(rel_type)) {
                        if edge.dst == dst_iid {
                            exists = true;
                            break;
                        }
                    }

                    if !exists {
                        txn.create_edge(src_iid, rel_type, dst_iid)?;
                        created_count += 1;
                        created_any = true;
                    }

                    if let Some(var) = &rel_pat.variable {
                        merge_row = merge_row.with(
                            var.clone(),
                            Value::EdgeKey(EdgeKey {
                                src: src_iid,
                                rel: rel_type,
                                dst: dst_iid,
                            }),
                        );
                    }
                    if let PathElement::Node(src_node) = &pattern.elements[i - 1]
                        && let Some(src_var) = &src_node.variable
                    {
                        merge_row = merge_row.with(src_var.clone(), Value::NodeId(src_iid));
                    }
                    if let Some(dst_var) = &dst_node.variable {
                        merge_row = merge_row.with(dst_var.clone(), Value::NodeId(dst_iid));
                    }

                    if current_iids.len() <= i + 1 {
                        current_iids.resize(i + 2, None);
                    }
                    current_iids[i + 1] = Some(dst_iid);

                    i += 2;
                } else {
                    return Err(Error::Other("relationship must be followed by node".into()));
                }
            }
        }
    }

    let set_items = if created_any {
        on_create_items
    } else {
        on_match_items
    };
    let label_items = if created_any {
        on_create_labels
    } else {
        on_match_labels
    };
    if !set_items.is_empty() {
        merge_apply_set_items(snapshot, txn, &mut merge_row, set_items, params)?;
    }
    if !label_items.is_empty() {
        merge_apply_label_items(txn, &mut merge_row, label_items)?;
    }

    Ok(created_count)
}

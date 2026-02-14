use super::{Params, Row, Value, cypher_equals, evaluate_expression_value};
use crate::ast::{
    Expression, NodePattern, PathElement, Pattern, PatternComprehension, RelationshipDirection,
    RelationshipPattern,
};
use crate::executor::{PathValue, convert_api_property_to_value};
use nervusdb_api::{EdgeKey, GraphSnapshot, InternalNodeId, RelTypeId};

const PATTERN_PREDICATE_MAX_VARLEN_HOPS: u32 = 16;

pub(super) fn evaluate_has_label<S: GraphSnapshot>(
    left: &Value,
    right: &Value,
    snapshot: &S,
) -> Value {
    let Value::String(label) = right else {
        return if matches!(left, Value::Null) || matches!(right, Value::Null) {
            Value::Null
        } else {
            Value::Bool(false)
        };
    };

    match left {
        Value::NodeId(node_id) => {
            if let Some(label_id) = snapshot.resolve_label_id(label) {
                let labels = snapshot.resolve_node_labels(*node_id).unwrap_or_default();
                Value::Bool(labels.contains(&label_id))
            } else {
                Value::Bool(false)
            }
        }
        Value::Node(node) => Value::Bool(node.labels.iter().any(|node_label| node_label == label)),
        Value::EdgeKey(edge_key) => {
            Value::Bool(snapshot.resolve_rel_type_name(edge_key.rel).as_deref() == Some(label))
        }
        Value::Relationship(rel) => Value::Bool(rel.rel_type == *label),
        Value::Null => Value::Null,
        _ => Value::Bool(false),
    }
}

pub(super) fn evaluate_pattern_exists<S: GraphSnapshot>(
    pattern: &Pattern,
    row: &Row,
    snapshot: &S,
    params: &Params,
) -> Value {
    if pattern.elements.len() < 3 {
        return Value::Null;
    }

    let PathElement::Node(start_node_pattern) = &pattern.elements[0] else {
        return Value::Null;
    };

    let Some(start_node) = resolve_node_binding(start_node_pattern, row) else {
        return Value::Null;
    };

    if !node_pattern_matches(start_node_pattern, start_node, row, snapshot, params) {
        return Value::Bool(false);
    }

    let mut used_edges: Vec<EdgeKey> = Vec::new();
    Value::Bool(match_pattern_from(
        pattern,
        1,
        start_node,
        row,
        snapshot,
        params,
        &mut used_edges,
    ))
}

pub(super) fn evaluate_pattern_comprehension<S: GraphSnapshot>(
    pattern_comp: &PatternComprehension,
    row: &Row,
    snapshot: &S,
    params: &Params,
) -> Value {
    if pattern_comp.pattern.elements.is_empty() {
        return Value::List(vec![]);
    }
    let PathElement::Node(start_node_pattern) = &pattern_comp.pattern.elements[0] else {
        return Value::Null;
    };

    let start_nodes: Vec<InternalNodeId> =
        if let Some(bound) = resolve_node_binding(start_node_pattern, row) {
            vec![bound]
        } else {
            snapshot.nodes().collect()
        };

    let mut out = Vec::new();
    for start_node in start_nodes {
        if !node_pattern_matches(start_node_pattern, start_node, row, snapshot, params) {
            continue;
        }

        let mut local_row = row.clone();
        if let Some(var) = &start_node_pattern.variable {
            local_row = local_row.with(var.clone(), Value::NodeId(start_node));
        }

        collect_pattern_comprehension_matches_from(
            &pattern_comp.pattern,
            1,
            start_node,
            &local_row,
            vec![start_node],
            Vec::new(),
            &pattern_comp.where_expression,
            &pattern_comp.projection,
            snapshot,
            params,
            &mut out,
        );
    }

    Value::List(out)
}

fn collect_pattern_comprehension_matches_from<S: GraphSnapshot>(
    pattern: &Pattern,
    rel_index: usize,
    current_node: InternalNodeId,
    row: &Row,
    path_nodes: Vec<InternalNodeId>,
    path_edges: Vec<EdgeKey>,
    where_expression: &Option<Expression>,
    projection: &Expression,
    snapshot: &S,
    params: &Params,
    out: &mut Vec<Value>,
) {
    if rel_index >= pattern.elements.len() {
        let mut eval_row = row.clone();
        if let Some(path_var) = &pattern.variable {
            eval_row = eval_row.with(
                path_var.clone(),
                Value::Path(PathValue {
                    nodes: path_nodes,
                    edges: path_edges,
                }),
            );
        }

        if let Some(where_expr) = where_expression {
            match evaluate_expression_value(where_expr, &eval_row, snapshot, params) {
                Value::Bool(true) => {}
                Value::Bool(false) | Value::Null => return,
                _ => return,
            }
        }

        out.push(evaluate_expression_value(
            projection, &eval_row, snapshot, params,
        ));
        return;
    }

    let PathElement::Relationship(rel_pattern) = &pattern.elements[rel_index] else {
        return;
    };
    let PathElement::Node(dst_node_pattern) = &pattern.elements[rel_index + 1] else {
        return;
    };

    let rel_type_ids = resolve_rel_type_ids(rel_pattern, snapshot);
    if rel_pattern.variable_length.is_some() {
        collect_variable_length_pattern_comprehension_matches(
            pattern,
            rel_index + 2,
            rel_pattern,
            dst_node_pattern,
            rel_type_ids.as_deref(),
            current_node,
            row,
            path_nodes,
            path_edges,
            where_expression,
            projection,
            snapshot,
            params,
            out,
        );
        return;
    }

    for (edge, next_node) in candidate_edges(
        current_node,
        rel_pattern.direction.clone(),
        rel_type_ids.as_deref(),
        snapshot,
    ) {
        if path_edges.contains(&edge) {
            continue;
        }
        if !relationship_pattern_matches(rel_pattern, edge, row, snapshot, params) {
            continue;
        }
        if !node_pattern_matches(dst_node_pattern, next_node, row, snapshot, params) {
            continue;
        }

        let mut next_row = row.clone();
        if let Some(var) = &rel_pattern.variable {
            next_row = next_row.with(var.clone(), Value::EdgeKey(edge));
        }
        if let Some(var) = &dst_node_pattern.variable {
            next_row = next_row.with(var.clone(), Value::NodeId(next_node));
        }

        let mut next_path_nodes = path_nodes.clone();
        next_path_nodes.push(next_node);
        let mut next_path_edges = path_edges.clone();
        next_path_edges.push(edge);

        collect_pattern_comprehension_matches_from(
            pattern,
            rel_index + 2,
            next_node,
            &next_row,
            next_path_nodes,
            next_path_edges,
            where_expression,
            projection,
            snapshot,
            params,
            out,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_variable_length_pattern_comprehension_matches<S: GraphSnapshot>(
    pattern: &Pattern,
    next_rel_index: usize,
    rel_pattern: &RelationshipPattern,
    dst_node_pattern: &NodePattern,
    rel_type_ids: Option<&[RelTypeId]>,
    start_node: InternalNodeId,
    row: &Row,
    path_nodes: Vec<InternalNodeId>,
    path_edges: Vec<EdgeKey>,
    where_expression: &Option<Expression>,
    projection: &Expression,
    snapshot: &S,
    params: &Params,
    out: &mut Vec<Value>,
) {
    let var_len = rel_pattern
        .variable_length
        .as_ref()
        .expect("checked by caller");
    let min_hops = var_len.min.unwrap_or(1);
    let max_hops = var_len.max.unwrap_or(PATTERN_PREDICATE_MAX_VARLEN_HOPS);
    if max_hops < min_hops {
        return;
    }

    struct PatternComprehensionCtx<'a, S: GraphSnapshot> {
        pattern: &'a Pattern,
        next_rel_index: usize,
        rel_pattern: &'a RelationshipPattern,
        dst_node_pattern: &'a NodePattern,
        rel_type_ids: Option<&'a [RelTypeId]>,
        where_expression: &'a Option<Expression>,
        projection: &'a Expression,
        snapshot: &'a S,
        params: &'a Params,
    }

    #[allow(clippy::too_many_arguments)]
    fn dfs<S: GraphSnapshot>(
        ctx: &PatternComprehensionCtx<'_, S>,
        node: InternalNodeId,
        depth: u32,
        min_hops: u32,
        max_hops: u32,
        row: &Row,
        path_nodes: Vec<InternalNodeId>,
        path_edges: Vec<EdgeKey>,
        out: &mut Vec<Value>,
    ) {
        if depth >= min_hops
            && node_pattern_matches(ctx.dst_node_pattern, node, row, ctx.snapshot, ctx.params)
        {
            let mut matched_row = row.clone();
            if let Some(var) = &ctx.dst_node_pattern.variable {
                matched_row = matched_row.with(var.clone(), Value::NodeId(node));
            }
            collect_pattern_comprehension_matches_from(
                ctx.pattern,
                ctx.next_rel_index,
                node,
                &matched_row,
                path_nodes.clone(),
                path_edges.clone(),
                ctx.where_expression,
                ctx.projection,
                ctx.snapshot,
                ctx.params,
                out,
            );
        }

        if depth >= max_hops {
            return;
        }

        for (edge, next_node) in candidate_edges(
            node,
            ctx.rel_pattern.direction.clone(),
            ctx.rel_type_ids,
            ctx.snapshot,
        ) {
            if path_edges.contains(&edge) {
                continue;
            }
            if !relationship_pattern_matches(ctx.rel_pattern, edge, row, ctx.snapshot, ctx.params) {
                continue;
            }

            let mut next_row = row.clone();
            if let Some(var) = &ctx.rel_pattern.variable {
                next_row = next_row.with(var.clone(), Value::EdgeKey(edge));
            }

            let mut next_path_nodes = path_nodes.clone();
            next_path_nodes.push(next_node);
            let mut next_path_edges = path_edges.clone();
            next_path_edges.push(edge);

            dfs(
                ctx,
                next_node,
                depth + 1,
                min_hops,
                max_hops,
                &next_row,
                next_path_nodes,
                next_path_edges,
                out,
            );
        }
    }

    let ctx = PatternComprehensionCtx {
        pattern,
        next_rel_index,
        rel_pattern,
        dst_node_pattern,
        rel_type_ids,
        where_expression,
        projection,
        snapshot,
        params,
    };

    dfs(
        &ctx, start_node, 0, min_hops, max_hops, row, path_nodes, path_edges, out,
    );
}

fn resolve_node_binding(node_pattern: &NodePattern, row: &Row) -> Option<InternalNodeId> {
    node_pattern
        .variable
        .as_ref()
        .and_then(|name| row.get_node(name))
}

fn match_pattern_from<S: GraphSnapshot>(
    pattern: &Pattern,
    rel_index: usize,
    current_node: InternalNodeId,
    row: &Row,
    snapshot: &S,
    params: &Params,
    used_edges: &mut Vec<EdgeKey>,
) -> bool {
    if rel_index >= pattern.elements.len() {
        return true;
    }

    let PathElement::Relationship(rel_pattern) = &pattern.elements[rel_index] else {
        return false;
    };
    let PathElement::Node(dst_node_pattern) = &pattern.elements[rel_index + 1] else {
        return false;
    };

    let rel_type_ids = resolve_rel_type_ids(rel_pattern, snapshot);
    if rel_pattern.variable_length.is_some() {
        return match_variable_length_pattern(
            pattern,
            rel_index + 2,
            rel_pattern,
            dst_node_pattern,
            rel_type_ids.as_deref(),
            current_node,
            row,
            snapshot,
            params,
            used_edges,
        );
    }

    for (edge, next_node) in candidate_edges(
        current_node,
        rel_pattern.direction.clone(),
        rel_type_ids.as_deref(),
        snapshot,
    ) {
        if used_edges.contains(&edge) {
            continue;
        }
        if !relationship_pattern_matches(rel_pattern, edge, row, snapshot, params) {
            continue;
        }
        if !node_pattern_matches(dst_node_pattern, next_node, row, snapshot, params) {
            continue;
        }

        used_edges.push(edge);
        if match_pattern_from(
            pattern,
            rel_index + 2,
            next_node,
            row,
            snapshot,
            params,
            used_edges,
        ) {
            return true;
        }
        used_edges.pop();
    }

    false
}

#[allow(clippy::too_many_arguments)]
fn match_variable_length_pattern<S: GraphSnapshot>(
    pattern: &Pattern,
    next_rel_index: usize,
    rel_pattern: &RelationshipPattern,
    dst_node_pattern: &NodePattern,
    rel_type_ids: Option<&[RelTypeId]>,
    start_node: InternalNodeId,
    row: &Row,
    snapshot: &S,
    params: &Params,
    used_edges: &mut Vec<EdgeKey>,
) -> bool {
    let var_len = rel_pattern
        .variable_length
        .as_ref()
        .expect("checked by caller");
    let min_hops = var_len.min.unwrap_or(1);
    let max_hops = var_len.max.unwrap_or(PATTERN_PREDICATE_MAX_VARLEN_HOPS);
    if max_hops < min_hops {
        return false;
    }

    #[allow(clippy::too_many_arguments)]
    fn dfs<S: GraphSnapshot>(
        pattern: &Pattern,
        next_rel_index: usize,
        rel_pattern: &RelationshipPattern,
        dst_node_pattern: &NodePattern,
        rel_type_ids: Option<&[RelTypeId]>,
        node: InternalNodeId,
        depth: u32,
        min_hops: u32,
        max_hops: u32,
        row: &Row,
        snapshot: &S,
        params: &Params,
        used_edges: &mut Vec<EdgeKey>,
    ) -> bool {
        if depth >= min_hops && node_pattern_matches(dst_node_pattern, node, row, snapshot, params)
        {
            if match_pattern_from(
                pattern,
                next_rel_index,
                node,
                row,
                snapshot,
                params,
                used_edges,
            ) {
                return true;
            }
        }

        if depth >= max_hops {
            return false;
        }

        for (edge, next_node) in
            candidate_edges(node, rel_pattern.direction.clone(), rel_type_ids, snapshot)
        {
            if used_edges.contains(&edge) {
                continue;
            }
            if !relationship_pattern_matches(rel_pattern, edge, row, snapshot, params) {
                continue;
            }

            used_edges.push(edge);
            if dfs(
                pattern,
                next_rel_index,
                rel_pattern,
                dst_node_pattern,
                rel_type_ids,
                next_node,
                depth + 1,
                min_hops,
                max_hops,
                row,
                snapshot,
                params,
                used_edges,
            ) {
                return true;
            }
            used_edges.pop();
        }

        false
    }

    dfs(
        pattern,
        next_rel_index,
        rel_pattern,
        dst_node_pattern,
        rel_type_ids,
        start_node,
        0,
        min_hops,
        max_hops,
        row,
        snapshot,
        params,
        used_edges,
    )
}

fn resolve_rel_type_ids<S: GraphSnapshot>(
    rel_pattern: &RelationshipPattern,
    snapshot: &S,
) -> Option<Vec<RelTypeId>> {
    if rel_pattern.types.is_empty() {
        return None;
    }
    Some(
        rel_pattern
            .types
            .iter()
            .filter_map(|name| snapshot.resolve_rel_type_id(name))
            .collect(),
    )
}

fn candidate_edges<S: GraphSnapshot>(
    src: InternalNodeId,
    direction: RelationshipDirection,
    rel_type_ids: Option<&[RelTypeId]>,
    snapshot: &S,
) -> Vec<(EdgeKey, InternalNodeId)> {
    let mut out = Vec::new();

    match direction {
        RelationshipDirection::LeftToRight => match rel_type_ids {
            Some(ids) if ids.is_empty() => {}
            Some(ids) => {
                for rel in ids {
                    for edge in snapshot.neighbors(src, Some(*rel)) {
                        out.push((edge, edge.dst));
                    }
                }
            }
            None => {
                for edge in snapshot.neighbors(src, None) {
                    out.push((edge, edge.dst));
                }
            }
        },
        RelationshipDirection::RightToLeft => match rel_type_ids {
            Some(ids) if ids.is_empty() => {}
            Some(ids) => {
                for rel in ids {
                    for edge in snapshot.incoming_neighbors(src, Some(*rel)) {
                        out.push((edge, edge.src));
                    }
                }
            }
            None => {
                for edge in snapshot.incoming_neighbors(src, None) {
                    out.push((edge, edge.src));
                }
            }
        },
        RelationshipDirection::Undirected => match rel_type_ids {
            Some(ids) if ids.is_empty() => {}
            Some(ids) => {
                for rel in ids {
                    for edge in snapshot.neighbors(src, Some(*rel)) {
                        out.push((edge, edge.dst));
                    }
                    for edge in snapshot.incoming_neighbors(src, Some(*rel)) {
                        out.push((edge, edge.src));
                    }
                }
            }
            None => {
                for edge in snapshot.neighbors(src, None) {
                    out.push((edge, edge.dst));
                }
                for edge in snapshot.incoming_neighbors(src, None) {
                    out.push((edge, edge.src));
                }
            }
        },
    }

    out
}

fn relationship_pattern_matches<S: GraphSnapshot>(
    rel_pattern: &RelationshipPattern,
    edge: EdgeKey,
    row: &Row,
    snapshot: &S,
    params: &Params,
) -> bool {
    if let Some(var) = &rel_pattern.variable
        && let Some(bound_edge) = row.get_edge(var)
        && bound_edge != edge
    {
        return false;
    }

    if let Some(props) = &rel_pattern.properties {
        for pair in &props.properties {
            let expected = evaluate_expression_value(&pair.value, row, snapshot, params);
            let actual = snapshot
                .edge_property(edge, &pair.key)
                .as_ref()
                .map(convert_api_property_to_value)
                .unwrap_or(Value::Null);
            if !matches!(cypher_equals(&actual, &expected), Value::Bool(true)) {
                return false;
            }
        }
    }

    true
}

fn node_pattern_matches<S: GraphSnapshot>(
    node_pattern: &NodePattern,
    node_id: InternalNodeId,
    row: &Row,
    snapshot: &S,
    params: &Params,
) -> bool {
    if let Some(var) = &node_pattern.variable
        && let Some(bound) = row.get_node(var)
        && bound != node_id
    {
        return false;
    }

    if !node_pattern.labels.is_empty() {
        let labels = snapshot.resolve_node_labels(node_id).unwrap_or_default();
        for label in &node_pattern.labels {
            let Some(label_id) = snapshot.resolve_label_id(label) else {
                return false;
            };
            if !labels.contains(&label_id) {
                return false;
            }
        }
    }

    if let Some(props) = &node_pattern.properties {
        for pair in &props.properties {
            let expected = evaluate_expression_value(&pair.value, row, snapshot, params);
            let actual = snapshot
                .node_property(node_id, &pair.key)
                .as_ref()
                .map(convert_api_property_to_value)
                .unwrap_or(Value::Null);
            if !matches!(cypher_equals(&actual, &expected), Value::Bool(true)) {
                return false;
            }
        }
    }

    true
}

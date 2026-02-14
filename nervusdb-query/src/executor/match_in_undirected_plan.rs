use super::{
    EdgeKey, ErasedSnapshot, GraphSnapshot, Plan, PlanIterator, RelTypeId, Result, Row, Value,
    apply_optional_unbinds_row, execute_plan, node_matches_label_constraint,
    path_alias_contains_edge, resolve_label_constraint, row_matches_node_binding,
};

fn resolve_rel_ids<S: GraphSnapshot>(snapshot: &S, rels: &[String]) -> Option<Vec<RelTypeId>> {
    if rels.is_empty() {
        return None;
    }
    let mut ids = Vec::new();
    for rel in rels {
        if let Some(id) = snapshot.resolve_rel_type_id(rel) {
            ids.push(id);
        }
    }
    Some(ids)
}

fn chain_incoming_candidates<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    target_iid: super::InternalNodeId,
    rel_ids: &Option<Vec<RelTypeId>>,
) -> Box<dyn Iterator<Item = EdgeKey> + 'a> {
    if let Some(rids) = rel_ids {
        let mut iter: Box<dyn Iterator<Item = EdgeKey>> = Box::new(std::iter::empty());
        for rid in rids {
            iter = Box::new(iter.chain(snapshot.incoming_neighbors_erased(target_iid, Some(*rid))));
        }
        iter
    } else {
        snapshot.incoming_neighbors_erased(target_iid, None)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_match_in<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Option<Box<Plan>>,
    src_alias: &str,
    rels: &[String],
    edge_alias: &Option<String>,
    dst_alias: &str,
    dst_labels: &[String],
    src_prebound: bool,
    optional: bool,
    optional_unbind: &[String],
    path_alias: &Option<String>,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let rel_ids = resolve_rel_ids(snapshot, rels);
    let dst_label_constraint = resolve_label_constraint(snapshot, dst_labels);

    let input_iter: Box<dyn Iterator<Item = Result<Row>>> = if let Some(input_plan) = input {
        Box::new(execute_plan(snapshot, input_plan, params))
    } else {
        Box::new(std::iter::once(Ok(Row::default())))
    };

    let src_alias = src_alias.to_string();
    let dst_alias = dst_alias.to_string();
    let edge_alias = edge_alias.clone();
    let optional_unbind = optional_unbind.to_vec();
    let path_alias = path_alias.clone();

    PlanIterator::Dynamic(Box::new(input_iter.flat_map(move |result| match result {
        Ok(row) => {
            let target_iid = match row.get(&src_alias).cloned() {
                Some(Value::NodeId(id)) => id,
                Some(Value::Null) | None if optional => {
                    let new_row = apply_optional_unbinds_row(row.clone(), &optional_unbind);
                    return Box::new(std::iter::once(Ok(new_row)))
                        as Box<dyn Iterator<Item = Result<Row>>>;
                }
                _ => return Box::new(std::iter::empty()) as Box<dyn Iterator<Item = Result<Row>>>,
            };

            let candidates = chain_incoming_candidates(snapshot, target_iid, &rel_ids);
            let dst_alias_binding = dst_alias.clone();
            let edge_alias_binding = edge_alias.clone();
            let row_for_map = row.clone();
            let path_alias = path_alias.clone();
            let dst_label_constraint = dst_label_constraint.clone();

            let mapped = candidates.filter_map(move |edge| {
                if path_alias_contains_edge(snapshot, &row_for_map, path_alias.as_deref(), edge) {
                    return None;
                }
                if !row_matches_node_binding(&row_for_map, &dst_alias_binding, edge.src) {
                    return None;
                }
                if !node_matches_label_constraint(snapshot, edge.src, &dst_label_constraint) {
                    return None;
                }

                let mut new_row = row_for_map.clone();
                new_row = new_row.with(dst_alias_binding.clone(), Value::NodeId(edge.src));
                if let Some(ea) = &edge_alias_binding {
                    new_row = new_row.with(ea.clone(), Value::EdgeKey(edge));
                }
                if let Some(pa) = &path_alias {
                    new_row.join_path(pa, edge.dst, edge, edge.src);
                }
                Some(Ok(new_row))
            });

            if optional {
                let results: Vec<_> = mapped.collect();
                if results.is_empty() {
                    if !src_prebound {
                        return Box::new(std::iter::empty())
                            as Box<dyn Iterator<Item = Result<Row>>>;
                    }
                    let new_row = apply_optional_unbinds_row(row.clone(), &optional_unbind);
                    Box::new(std::iter::once(Ok(new_row))) as Box<dyn Iterator<Item = Result<Row>>>
                } else {
                    Box::new(results.into_iter()) as Box<dyn Iterator<Item = Result<Row>>>
                }
            } else {
                Box::new(mapped) as Box<dyn Iterator<Item = Result<Row>>>
            }
        }
        Err(e) => Box::new(std::iter::once(Err(e))) as Box<dyn Iterator<Item = Result<Row>>>,
    })))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_match_undirected<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Option<Box<Plan>>,
    src_alias: &str,
    rels: &[String],
    edge_alias: &Option<String>,
    dst_alias: &str,
    dst_labels: &[String],
    src_prebound: bool,
    limit: Option<usize>,
    optional: bool,
    optional_unbind: &[String],
    path_alias: &Option<String>,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let rel_ids = resolve_rel_ids(snapshot, rels);
    let dst_label_constraint = resolve_label_constraint(snapshot, dst_labels);

    let input_iter: Box<dyn Iterator<Item = Result<Row>>> = if let Some(input_plan) = input {
        Box::new(execute_plan(snapshot, input_plan, params))
    } else {
        Box::new(std::iter::once(Ok(Row::default())))
    };

    let src_alias = src_alias.to_string();
    let dst_alias = dst_alias.to_string();
    let edge_alias = edge_alias.clone();
    let optional_unbind = optional_unbind.to_vec();
    let path_alias = path_alias.clone();

    let expanded = input_iter.flat_map(move |res| match res {
        Ok(row) => {
            let src_iid = match row.get(&src_alias).cloned() {
                Some(Value::NodeId(id)) => id,
                Some(Value::Null) | None if optional => {
                    let null_row = apply_optional_unbinds_row(row.clone(), &optional_unbind);
                    return Box::new(std::iter::once(Ok(null_row)))
                        as Box<dyn Iterator<Item = Result<Row>>>;
                }
                _ => return Box::new(std::iter::empty()) as Box<dyn Iterator<Item = Result<Row>>>,
            };

            let mut rows: Vec<Result<Row>> = Vec::new();

            if let Some(rids) = &rel_ids {
                for rid in rids {
                    for edge in snapshot.neighbors(src_iid, Some(*rid)) {
                        if path_alias_contains_edge(snapshot, &row, path_alias.as_deref(), edge) {
                            continue;
                        }
                        if !row_matches_node_binding(&row, &dst_alias, edge.dst) {
                            continue;
                        }
                        if !node_matches_label_constraint(snapshot, edge.dst, &dst_label_constraint)
                        {
                            continue;
                        }
                        let mut new_row = row.clone();
                        new_row = new_row.with(dst_alias.clone(), Value::NodeId(edge.dst));
                        if let Some(ea) = &edge_alias {
                            new_row = new_row.with(ea.clone(), Value::EdgeKey(edge));
                        }
                        if let Some(pa) = &path_alias {
                            new_row.join_path(pa, edge.src, edge, edge.dst);
                        }
                        rows.push(Ok(new_row));
                    }
                    for edge in snapshot.incoming_neighbors_erased(src_iid, Some(*rid)) {
                        if path_alias_contains_edge(snapshot, &row, path_alias.as_deref(), edge) {
                            continue;
                        }
                        if edge.src == edge.dst {
                            continue;
                        }
                        if !row_matches_node_binding(&row, &dst_alias, edge.src) {
                            continue;
                        }
                        if !node_matches_label_constraint(snapshot, edge.src, &dst_label_constraint)
                        {
                            continue;
                        }
                        let mut new_row = row.clone();
                        new_row = new_row.with(dst_alias.clone(), Value::NodeId(edge.src));
                        if let Some(ea) = &edge_alias {
                            new_row = new_row.with(ea.clone(), Value::EdgeKey(edge));
                        }
                        if let Some(pa) = &path_alias {
                            new_row.join_path(pa, edge.dst, edge, edge.src);
                        }
                        rows.push(Ok(new_row));
                    }
                }
            } else {
                for edge in snapshot.neighbors(src_iid, None) {
                    if path_alias_contains_edge(snapshot, &row, path_alias.as_deref(), edge) {
                        continue;
                    }
                    if !row_matches_node_binding(&row, &dst_alias, edge.dst) {
                        continue;
                    }
                    if !node_matches_label_constraint(snapshot, edge.dst, &dst_label_constraint) {
                        continue;
                    }
                    let mut new_row = row.clone();
                    new_row = new_row.with(dst_alias.clone(), Value::NodeId(edge.dst));
                    if let Some(ea) = &edge_alias {
                        new_row = new_row.with(ea.clone(), Value::EdgeKey(edge));
                    }
                    if let Some(pa) = &path_alias {
                        new_row.join_path(pa, edge.src, edge, edge.dst);
                    }
                    rows.push(Ok(new_row));
                }
                for edge in snapshot.incoming_neighbors_erased(src_iid, None) {
                    if path_alias_contains_edge(snapshot, &row, path_alias.as_deref(), edge) {
                        continue;
                    }
                    if edge.src == edge.dst {
                        continue;
                    }
                    if !row_matches_node_binding(&row, &dst_alias, edge.src) {
                        continue;
                    }
                    if !node_matches_label_constraint(snapshot, edge.src, &dst_label_constraint) {
                        continue;
                    }
                    let mut new_row = row.clone();
                    new_row = new_row.with(dst_alias.clone(), Value::NodeId(edge.src));
                    if let Some(ea) = &edge_alias {
                        new_row = new_row.with(ea.clone(), Value::EdgeKey(edge));
                    }
                    if let Some(pa) = &path_alias {
                        new_row.join_path(pa, edge.dst, edge, edge.src);
                    }
                    rows.push(Ok(new_row));
                }
            }

            if optional && rows.is_empty() {
                if !src_prebound {
                    return Box::new(std::iter::empty()) as Box<dyn Iterator<Item = Result<Row>>>;
                }
                let null_row = apply_optional_unbinds_row(row.clone(), &optional_unbind);
                Box::new(std::iter::once(Ok(null_row))) as Box<dyn Iterator<Item = Result<Row>>>
            } else {
                Box::new(rows.into_iter()) as Box<dyn Iterator<Item = Result<Row>>>
            }
        }
        Err(e) => Box::new(std::iter::once(Err(e))) as Box<dyn Iterator<Item = Result<Row>>>,
    });

    if let Some(limit) = limit {
        PlanIterator::Dynamic(Box::new(expanded.take(limit)))
    } else {
        PlanIterator::Dynamic(Box::new(expanded))
    }
}

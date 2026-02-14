use super::{
    GraphSnapshot, InternalNodeId, Plan, PlanIterator, RelationshipDirection, Result, Row, Value,
    apply_optional_unbinds_row, execute_plan, node_matches_label_constraint,
    path_alias_contains_edge, resolve_label_constraint,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_match_bound_rel<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: &'a Plan,
    rel_alias: &str,
    src_alias: &str,
    dst_alias: &str,
    dst_labels: &[String],
    src_prebound: bool,
    rels: &[String],
    direction: &RelationshipDirection,
    optional: bool,
    optional_unbind: &[String],
    path_alias: &Option<String>,
    params: &'a crate::query_api::Params,
) -> PlanIterator<'a, S> {
    let rel_ids = if rels.is_empty() {
        None
    } else {
        let mut ids = Vec::new();
        for r in rels {
            if let Some(id) = snapshot.resolve_rel_type_id(r) {
                ids.push(id);
            }
        }
        Some(ids)
    };
    let dst_label_constraint = resolve_label_constraint(snapshot, dst_labels);

    let input_iter = execute_plan(snapshot, input, params);
    let rel_alias = rel_alias.to_string();
    let src_alias = src_alias.to_string();
    let dst_alias = dst_alias.to_string();
    let direction = direction.clone();
    let optional_unbind = optional_unbind.to_vec();
    let path_alias = path_alias.clone();

    PlanIterator::Dynamic(Box::new(input_iter.flat_map(move |res| match res {
        Ok(row) => {
            let bound_edge = row.get_edge(&rel_alias);
            let mut out: Vec<Result<Row>> = Vec::new();

            if let Some(edge) = bound_edge
                && rel_ids
                    .as_ref()
                    .is_none_or(|ids| ids.iter().any(|id| *id == edge.rel))
            {
                let orientations: Vec<(InternalNodeId, InternalNodeId)> = match direction {
                    RelationshipDirection::LeftToRight => vec![(edge.src, edge.dst)],
                    RelationshipDirection::RightToLeft => vec![(edge.dst, edge.src)],
                    RelationshipDirection::Undirected => {
                        if edge.src == edge.dst {
                            vec![(edge.src, edge.dst)]
                        } else {
                            vec![(edge.src, edge.dst), (edge.dst, edge.src)]
                        }
                    }
                };

                for (src_id, dst_id) in orientations {
                    if path_alias_contains_edge(snapshot, &row, path_alias.as_deref(), edge) {
                        continue;
                    }

                    let src_ok = match row.get(&src_alias) {
                        Some(Value::NodeId(id)) => *id == src_id,
                        Some(Value::Null) => false,
                        Some(_) => false,
                        None => true,
                    };
                    if !src_ok {
                        continue;
                    }

                    let dst_ok = match row.get(&dst_alias) {
                        Some(Value::NodeId(id)) => *id == dst_id,
                        Some(Value::Null) => false,
                        Some(_) => false,
                        None => true,
                    };
                    if !dst_ok {
                        continue;
                    }
                    if !node_matches_label_constraint(snapshot, dst_id, &dst_label_constraint) {
                        continue;
                    }

                    let mut new_row = row.clone();
                    new_row = new_row.with(src_alias.clone(), Value::NodeId(src_id));
                    new_row = new_row.with(dst_alias.clone(), Value::NodeId(dst_id));
                    if let Some(pa) = &path_alias {
                        new_row.join_path(pa, src_id, edge, dst_id);
                    }
                    out.push(Ok(new_row));
                }
            }

            if out.is_empty() && optional {
                if !src_prebound {
                    return Box::new(std::iter::empty()) as Box<dyn Iterator<Item = Result<Row>>>;
                }
                let null_row = apply_optional_unbinds_row(row.clone(), &optional_unbind);
                out.push(Ok(null_row));
            }

            Box::new(out.into_iter()) as Box<dyn Iterator<Item = Result<Row>>>
        }
        Err(e) => Box::new(std::iter::once(Err(e))) as Box<dyn Iterator<Item = Result<Row>>>,
    })))
}

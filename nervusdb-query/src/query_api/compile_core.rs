use super::{
    BTreeMap, BTreeSet, BindingKind, CallClause, Clause, Error, Expression, Plan, Query, Result,
    VecDeque, WriteSemantics, compile_create_plan, compile_delete_plan_v2, compile_foreach_plan,
    compile_match_plan, compile_merge_plan, compile_merge_set_items, compile_remove_plan_v2,
    compile_return_plan, compile_set_plan_v2, compile_unwind_plan, compile_with_plan,
    extract_merge_pattern_vars, extract_output_var_kinds, extract_predicates,
    validate_expression_types, validate_where_expression_bindings,
};

pub(crate) struct CompiledQuery {
    pub(crate) plan: Plan,
    pub(crate) write: WriteSemantics,
    pub(crate) merge_on_create_items: Vec<(String, String, Expression)>,
    pub(crate) merge_on_match_items: Vec<(String, String, Expression)>,
    pub(crate) merge_on_create_labels: Vec<(String, Vec<String>)>,
    pub(crate) merge_on_match_labels: Vec<(String, Vec<String>)>,
}

pub(crate) fn compile_m3_plan(
    query: Query,
    merge_subclauses: &mut VecDeque<crate::parser::MergeSubclauses>,
    initial_input: Option<Plan>,
) -> Result<CompiledQuery> {
    let mut plan: Option<Plan> = initial_input;
    let mut clauses = query.clauses.iter().peekable();
    let mut write_semantics = WriteSemantics::Default;
    let mut merge_on_create_items: Vec<(String, String, Expression)> = Vec::new();
    let mut merge_on_match_items: Vec<(String, String, Expression)> = Vec::new();
    let mut merge_on_create_labels: Vec<(String, Vec<String>)> = Vec::new();
    let mut merge_on_match_labels: Vec<(String, Vec<String>)> = Vec::new();
    let mut next_anon_id = 0u32;
    let mut pending_optional_where_fixup: Option<(Plan, Vec<String>)> = None;

    while let Some(clause) = clauses.next() {
        if !matches!(clause, Clause::Match(_) | Clause::Where(_)) {
            pending_optional_where_fixup = None;
        }

        match clause {
            Clause::Match(m) => {
                // Check ahead for WHERE to optimize immediately
                let mut predicates = BTreeMap::new();
                if let Some(Clause::Where(w)) = clauses.peek() {
                    extract_predicates(&w.expression, &mut predicates);
                }

                let previous_plan = plan.clone().unwrap_or(Plan::ReturnOne);
                let mut before_kinds: BTreeMap<String, BindingKind> = BTreeMap::new();
                if let Some(existing_plan) = &plan {
                    extract_output_var_kinds(existing_plan, &mut before_kinds);
                }

                let mut compiled_match = m.clone();
                if compiled_match.optional {
                    // OPTIONAL 语义由 OptionalWhereFixup 在子句边界统一处理，
                    // 避免多跳链路逐 hop 产出多余 null 行。
                    compiled_match.optional = false;
                }

                plan = Some(compile_match_plan(
                    plan,
                    compiled_match,
                    &predicates,
                    &mut next_anon_id,
                )?);

                if m.optional {
                    let mut after_kinds: BTreeMap<String, BindingKind> = BTreeMap::new();
                    if let Some(compiled_plan) = &plan {
                        extract_output_var_kinds(compiled_plan, &mut after_kinds);
                    }
                    let mut aliases: BTreeSet<String> =
                        collect_optional_match_aliases(m, &before_kinds);
                    aliases.extend(
                        after_kinds
                            .keys()
                            .filter(|name| !before_kinds.contains_key(*name))
                            .cloned(),
                    );
                    let aliases = aliases.into_iter().collect::<Vec<_>>();

                    if matches!(clauses.peek(), Some(Clause::Where(_))) {
                        pending_optional_where_fixup = Some((previous_plan, aliases));
                    } else {
                        plan = Some(Plan::OptionalWhereFixup {
                            outer: Box::new(previous_plan),
                            filtered: Box::new(plan.unwrap()),
                            null_aliases: aliases,
                        });
                        pending_optional_where_fixup = None;
                    }
                } else {
                    pending_optional_where_fixup = None;
                }
            }
            Clause::Where(w) => {
                if plan.is_none() {
                    return Err(Error::Other("WHERE cannot be the first clause".into()));
                }

                let mut where_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();
                if let Some(current_plan) = &plan {
                    extract_output_var_kinds(current_plan, &mut where_bindings);
                }
                if let Some((_, pending_aliases)) = &pending_optional_where_fixup {
                    for alias in pending_aliases {
                        where_bindings
                            .entry(alias.clone())
                            .or_insert(BindingKind::Unknown);
                    }
                }

                validate_expression_types(&w.expression)?;
                validate_where_expression_bindings(&w.expression, &where_bindings)?;

                let filtered = Plan::Filter {
                    input: Box::new(plan.unwrap()),
                    predicate: w.expression.clone(),
                };

                if let Some((outer_plan, null_aliases)) = pending_optional_where_fixup.take() {
                    plan = Some(Plan::OptionalWhereFixup {
                        outer: Box::new(outer_plan),
                        filtered: Box::new(filtered),
                        null_aliases,
                    });
                } else {
                    plan = Some(filtered);
                }
            }
            Clause::Call(c) => match c {
                CallClause::Subquery(sub_query) => {
                    let input = plan.unwrap_or(Plan::ReturnOne);
                    let sub_query_compiled =
                        compile_m3_plan(sub_query.clone(), merge_subclauses, None)?;
                    plan = Some(Plan::Apply {
                        input: Box::new(input),
                        subquery: Box::new(sub_query_compiled.plan),
                        alias: None,
                    });
                }
                CallClause::Procedure(proc_call) => {
                    let input = plan.unwrap_or(Plan::ReturnOne);
                    let mut yields = Vec::new();
                    if let Some(items) = &proc_call.yields {
                        for item in items {
                            yields.push((item.name.clone(), item.alias.clone()));
                        }
                    }
                    plan = Some(Plan::ProcedureCall {
                        input: Box::new(input),
                        name: proc_call.name.clone(),
                        args: proc_call.arguments.clone(),
                        yields,
                    });
                }
            },
            Clause::With(w) => {
                let input = plan.unwrap_or(Plan::ReturnOne);
                plan = Some(compile_with_plan(input, w)?);
            }
            Clause::Return(r) => {
                let input = plan.unwrap_or(Plan::ReturnOne);
                let (p, _) = compile_return_plan(input, r)?;
                plan = Some(p);
                // If there are more clauses after RETURN, it might be an error or valid?
                // In standard Cypher, RETURN is terminal UNLESS followed by UNION.
                // Check if any clauses left?
                if let Some(next_clause) = clauses.peek() {
                    // Allow UNION to follow RETURN
                    if !matches!(next_clause, Clause::Union(_)) {
                        return Err(Error::NotImplemented(
                            "Clauses after RETURN are not supported",
                        ));
                    }
                    // Continue loop to process UNION
                } else {
                    // No more clauses, return successfully
                    return Ok(CompiledQuery {
                        plan: plan.unwrap(),
                        write: write_semantics,
                        merge_on_create_items,
                        merge_on_match_items,
                        merge_on_create_labels,
                        merge_on_match_labels,
                    });
                }
            }
            Clause::Create(c) => {
                let input = plan.unwrap_or(Plan::ReturnOne);
                plan = Some(compile_create_plan(input, c.clone())?);
            }
            Clause::Merge(m) => {
                write_semantics = WriteSemantics::Merge;
                // For chained MERGE, each MERGE can follow previous plan
                let input = plan.unwrap_or(Plan::ReturnOne);
                let sub = merge_subclauses.pop_front().ok_or_else(|| {
                    Error::Other("internal error: missing MERGE subclauses".into())
                })?;
                let merge_vars = extract_merge_pattern_vars(&m.pattern);
                let compiled_on_create = compile_merge_set_items(&merge_vars, sub.on_create)?;
                merge_on_create_items = compiled_on_create.property_items;
                merge_on_create_labels = compiled_on_create.label_items;
                let compiled_on_match = compile_merge_set_items(&merge_vars, sub.on_match)?;
                merge_on_match_items = compiled_on_match.property_items;
                merge_on_match_labels = compiled_on_match.label_items;
                plan = Some(compile_merge_plan(input, m.clone())?);
            }
            Clause::Set(s) => {
                let input = plan.ok_or_else(|| Error::Other("SET need input".into()))?;
                // We need to associate WHERE?
                // SET doesn't have its own WHERE. It operates on rows.
                plan = Some(compile_set_plan_v2(input, s.clone())?);
            }
            Clause::Remove(r) => {
                let input = plan.ok_or_else(|| Error::Other("REMOVE need input".into()))?;
                plan = Some(compile_remove_plan_v2(input, r.clone())?);
            }
            Clause::Delete(d) => {
                let input = plan.ok_or_else(|| Error::Other("DELETE need input".into()))?;
                plan = Some(compile_delete_plan_v2(input, d.clone())?);

                // If DELETE is not terminal, we might have issues if we detach/delete nodes used later?
                // But for now, let's allow it.
            }
            Clause::Unwind(u) => {
                let input = plan.unwrap_or(Plan::ReturnOne);
                plan = Some(compile_unwind_plan(input, u.clone()));
            }
            Clause::Union(u) => {
                // UNION logic: current plan is the "left" side; the clause's nested query is the "right" side
                let left_plan =
                    plan.ok_or_else(|| Error::Other("UNION requires left query part".into()))?;
                let right_compiled = compile_m3_plan(u.query.clone(), merge_subclauses, None)?;
                let left_columns = extract_union_output_columns(&left_plan);
                let right_columns = extract_union_output_columns(&right_compiled.plan);
                if left_columns != right_columns {
                    return Err(Error::Other(
                        "syntax error: DifferentColumnsInUnion".to_string(),
                    ));
                }
                plan = Some(Plan::Union {
                    left: Box::new(left_plan),
                    right: Box::new(right_compiled.plan),
                    all: u.all,
                });
            }
            Clause::Foreach(f) => {
                let input = plan.unwrap_or(Plan::ReturnOne);
                plan = Some(compile_foreach_plan(input, f.clone(), merge_subclauses)?);
            }
        }
    }

    // If we exit loop without RETURN
    // For update queries (CREATE/DELETE/SET), this is valid if we return count?
    // M3 requires RETURN usually for read.
    // Spec says: "query without RETURN" is error for read queries.
    // Write queries might return stats?
    // Existing code returned "query without RETURN" error.
    // We'll stick to that unless it's a write-only query?
    // Let's enforce RETURN for now as per previous logic, unless we tracked we did logical writes?
    // But previous `prepare` returns `Result<CompiledQuery>`.

    // If plan exists here, but no RETURN hit.
    // For queries ending in update clauses (CREATE, DELETE, etc.), this is valid.
    if let Some(plan) = plan {
        return Ok(CompiledQuery {
            plan,
            write: write_semantics,
            merge_on_create_items,
            merge_on_match_items,
            merge_on_create_labels,
            merge_on_match_labels,
        });
    }

    Err(Error::NotImplemented("Empty query"))
}

fn extract_union_output_columns(plan: &Plan) -> Vec<String> {
    match plan {
        Plan::Project { projections, .. } => {
            projections.iter().map(|(alias, _)| alias.clone()).collect()
        }
        Plan::Aggregate {
            group_by,
            aggregates,
            ..
        } => {
            let mut cols = group_by.clone();
            cols.extend(aggregates.iter().map(|(_, alias)| alias.clone()));
            cols
        }
        Plan::Filter { input, .. }
        | Plan::OrderBy { input, .. }
        | Plan::Skip { input, .. }
        | Plan::Limit { input, .. }
        | Plan::Distinct { input } => extract_union_output_columns(input),
        Plan::OptionalWhereFixup { filtered, .. } => extract_union_output_columns(filtered),
        Plan::Union { left, .. } => extract_union_output_columns(left),
        _ => {
            let mut vars: BTreeMap<String, BindingKind> = BTreeMap::new();
            extract_output_var_kinds(plan, &mut vars);
            vars.keys().cloned().collect()
        }
    }
}

fn collect_optional_match_aliases(
    match_clause: &crate::ast::MatchClause,
    known_before: &BTreeMap<String, BindingKind>,
) -> BTreeSet<String> {
    let mut aliases = BTreeSet::new();
    for pattern in &match_clause.patterns {
        if let Some(path_alias) = &pattern.variable
            && !known_before.contains_key(path_alias)
        {
            aliases.insert(path_alias.clone());
        }
        for element in &pattern.elements {
            match element {
                crate::ast::PathElement::Node(node) => {
                    if let Some(alias) = &node.variable
                        && !known_before.contains_key(alias)
                    {
                        aliases.insert(alias.clone());
                    }
                }
                crate::ast::PathElement::Relationship(rel) => {
                    if let Some(alias) = &rel.variable
                        && !known_before.contains_key(alias)
                    {
                        aliases.insert(alias.clone());
                    }
                }
            }
        }
    }
    aliases
}

#[cfg(test)]
mod tests {
    use super::compile_m3_plan;
    use std::collections::VecDeque;

    fn compile_query(cypher: &str) -> crate::error::Result<()> {
        let (query, merge_subclauses) = crate::parser::Parser::parse_with_merge_subclauses(cypher)?;
        let mut merge_subclauses = VecDeque::from(merge_subclauses);
        compile_m3_plan(query, &mut merge_subclauses, None).map(|_| ())
    }

    #[test]
    fn union_rejects_different_projection_columns() {
        let err = compile_query("RETURN 1 AS a UNION RETURN 2 AS b")
            .expect_err("UNION with different columns should fail");
        assert_eq!(err.to_string(), "syntax error: DifferentColumnsInUnion");
    }

    #[test]
    fn union_all_rejects_different_projection_columns() {
        let err = compile_query("RETURN 1 AS a UNION ALL RETURN 2 AS b")
            .expect_err("UNION ALL with different columns should fail");
        assert_eq!(err.to_string(), "syntax error: DifferentColumnsInUnion");
    }
}

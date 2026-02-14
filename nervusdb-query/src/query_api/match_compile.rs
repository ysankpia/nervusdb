use super::{
    BTreeMap, BTreeSet, BindingKind, Error, Expression, Plan, Result, alloc_internal_path_alias,
    build_optional_unbind_aliases, extract_output_var_kinds, first_relationship_is_bound,
    maybe_reanchor_pattern, pattern_has_bound_relationship, validate_match_pattern_bindings,
};
use crate::query_api::ast_walk::extract_variables_from_expr;

pub(super) fn compile_match_plan(
    input: Option<Plan>,
    m: crate::ast::MatchClause,
    predicates: &BTreeMap<String, BTreeMap<String, Expression>>,
    next_anon_id: &mut u32,
) -> Result<Plan> {
    let mut plan = input;
    let mut known_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();
    if let Some(p) = &plan {
        extract_output_var_kinds(p, &mut known_bindings);
    }

    for raw_pattern in m.patterns {
        let pattern = maybe_reanchor_pattern(raw_pattern, &known_bindings);
        if pattern.elements.is_empty() {
            return Err(Error::Other("pattern cannot be empty".into()));
        }
        validate_match_pattern_bindings(&pattern, &known_bindings)?;

        let first_node_alias = match &pattern.elements[0] {
            crate::ast::PathElement::Node(n) => {
                if let Some(v) = &n.variable {
                    v.clone()
                } else {
                    // Generate anonymous variable name
                    let name = format!("_gen_{}", next_anon_id);
                    *next_anon_id += 1;
                    name
                }
            }
            _ => return Err(Error::Other("pattern must start with a node".into())),
        };

        let join_via_bound_node = matches!(
            known_bindings.get(&first_node_alias),
            Some(BindingKind::Node | BindingKind::Unknown)
        );
        let join_via_bound_relationship = pattern_has_bound_relationship(&pattern, &known_bindings);
        let correlated_with_outer =
            pattern_uses_outer_bindings(&pattern, &known_bindings, predicates);

        if join_via_bound_node || join_via_bound_relationship || correlated_with_outer {
            // Join via expansion (bound start node) or via already-bound relationship variable.
            plan = Some(compile_pattern_chain(
                plan,
                &pattern,
                predicates,
                m.optional,
                &known_bindings,
                next_anon_id,
            )?);
        } else {
            // Start a new component
            let sub_plan = compile_pattern_chain(
                None,
                &pattern,
                predicates,
                m.optional,
                &known_bindings,
                next_anon_id,
            )?;
            if let Some(existing) = plan {
                plan = Some(Plan::CartesianProduct {
                    left: Box::new(existing),
                    right: Box::new(sub_plan),
                });
            } else {
                plan = Some(sub_plan);
            }
        }

        // Update known bindings after each pattern.
        if let Some(p) = &plan {
            known_bindings.clear();
            extract_output_var_kinds(p, &mut known_bindings);
        }
    }

    plan.ok_or_else(|| Error::Other("No patterns in MATCH".into()))
}

fn compile_pattern_chain(
    input: Option<Plan>,
    pattern: &crate::ast::Pattern,
    predicates: &BTreeMap<String, BTreeMap<String, Expression>>,
    optional: bool,
    known_bindings: &BTreeMap<String, BindingKind>,
    next_anon_id: &mut u32,
) -> Result<Plan> {
    if pattern.elements.is_empty() {
        return Err(Error::Other("pattern cannot be empty".into()));
    }

    let src_node_el = match &pattern.elements[0] {
        crate::ast::PathElement::Node(n) => n,
        _ => return Err(Error::Other("pattern must start with a node".into())),
    };

    let src_alias = if let Some(v) = &src_node_el.variable {
        v.clone()
    } else {
        // Generate anonymous variable name
        let name = format!("_gen_{}", next_anon_id);
        *next_anon_id += 1;
        name
    };
    let src_labels = src_node_el.labels.clone();
    let src_label = src_labels.first().cloned();

    let mut local_predicates = predicates.clone();
    let mut plan = if let Some(existing_plan) = input {
        let src_is_bound = matches!(
            known_bindings.get(&src_alias),
            Some(BindingKind::Node | BindingKind::Unknown)
        );
        let first_rel_is_bound = first_relationship_is_bound(pattern, known_bindings);

        if src_is_bound {
            // Apply filters for inline properties of this node only when source alias is already bound.
            extend_predicates_from_properties(
                &src_alias,
                &src_node_el.properties,
                &mut local_predicates,
            );
            let plan = apply_filters_for_alias(existing_plan, &src_alias, &local_predicates);
            apply_label_filters_for_alias(plan, &src_alias, &src_labels)
        } else if first_rel_is_bound {
            extend_predicates_from_properties(
                &src_alias,
                &src_node_el.properties,
                &mut local_predicates,
            );
            let plan = apply_filters_for_alias(existing_plan, &src_alias, &local_predicates);
            apply_label_filters_for_alias(plan, &src_alias, &src_labels)
        } else {
            // No direct anchor on the first hop. Keep correlated bindings from `existing_plan`
            // and start this pattern from a fresh source scan.
            // NOTE: filters may reference outer variables (e.g. `event.year` after UNWIND),
            // so they must be applied after the Cartesian product.
            extend_predicates_from_properties(
                &src_alias,
                &src_node_el.properties,
                &mut local_predicates,
            );

            let start_plan = Plan::NodeScan {
                alias: src_alias.clone(),
                label: src_label.clone(),
                optional,
            };

            let joined = Plan::CartesianProduct {
                left: Box::new(existing_plan),
                right: Box::new(start_plan),
            };
            let joined = apply_filters_for_alias(joined, &src_alias, &local_predicates);
            apply_label_filters_for_alias(joined, &src_alias, &src_labels)
        }
    } else {
        // Build Initial Plan (Scan or IndexSeek)
        extend_predicates_from_properties(
            &src_alias,
            &src_node_el.properties,
            &mut local_predicates,
        );

        let mut start_plan = Plan::NodeScan {
            alias: src_alias.clone(),
            label: src_label.clone(),
            optional,
        };

        // Try IndexSeek optimization
        if let Some(label_name) = &src_label
            && let Some(var_preds) = local_predicates.get(&src_alias)
            && let Some((field, val_expr)) = var_preds.iter().next()
        {
            start_plan = Plan::IndexSeek {
                alias: src_alias.clone(),
                label: label_name.clone(),
                field: field.clone(),
                value_expr: val_expr.clone(),
                fallback: Box::new(start_plan),
            };
        }

        let plan = apply_filters_for_alias(start_plan, &src_alias, &local_predicates);
        apply_label_filters_for_alias(plan, &src_alias, &src_labels)
    };

    // Subsequent hops
    let mut i = 1;
    let mut curr_src_alias = src_alias.clone();
    let mut local_bound_aliases: BTreeSet<String> = BTreeSet::new();
    let chain_path_alias = pattern
        .variable
        .clone()
        .or_else(|| Some(alloc_internal_path_alias(next_anon_id)));

    while i < pattern.elements.len() {
        if i + 1 >= pattern.elements.len() {
            return Err(Error::Other("pattern must end with a node".into()));
        }

        let rel_el = match &pattern.elements[i] {
            crate::ast::PathElement::Relationship(r) => r,
            _ => return Err(Error::Other("expected relationship at odd index".into())),
        };
        let dst_node_el = match &pattern.elements[i + 1] {
            crate::ast::PathElement::Node(n) => n,
            _ => return Err(Error::Other("expected node at even index".into())),
        };

        let dst_alias = if let Some(v) = &dst_node_el.variable {
            v.clone()
        } else {
            // Generate anonymous variable name
            let name = format!("_gen_{}", next_anon_id);
            *next_anon_id += 1;
            name
        };

        let edge_alias = rel_el.variable.clone();
        let rel_types = rel_el.types.clone();
        let dst_labels = dst_node_el.labels.clone();
        let is_var_len = rel_el.variable_length.is_some();
        let src_prebound =
            is_bound_before_local(known_bindings, &local_bound_aliases, &curr_src_alias);

        let path_alias = chain_path_alias.clone();
        let optional_unbind = build_optional_unbind_aliases(
            known_bindings,
            &curr_src_alias,
            &dst_alias,
            edge_alias.as_deref(),
            path_alias.as_deref(),
        );

        if let Some(var_len) = &rel_el.variable_length {
            plan = Plan::MatchOutVarLen {
                input: Some(Box::new(plan)),
                src_alias: curr_src_alias.clone(),
                dst_alias: dst_alias.clone(),
                dst_labels: dst_labels.clone(),
                src_prebound,
                edge_alias: edge_alias.clone(),
                rels: rel_types,
                direction: rel_el.direction.clone(),
                min_hops: var_len.min.unwrap_or(1),
                max_hops: var_len.max,
                limit: None,
                project: Vec::new(),
                project_external: false,
                optional,
                optional_unbind: optional_unbind.clone(),
                path_alias: path_alias.clone(),
            };
        } else if let Some(rel_alias) = &edge_alias
            && matches!(
                known_bindings.get(rel_alias),
                Some(BindingKind::Relationship | BindingKind::Unknown)
            )
        {
            plan = Plan::MatchBoundRel {
                input: Box::new(plan),
                rel_alias: rel_alias.clone(),
                src_alias: curr_src_alias.clone(),
                dst_alias: dst_alias.clone(),
                dst_labels: dst_labels.clone(),
                src_prebound,
                rels: rel_types,
                direction: rel_el.direction.clone(),
                optional,
                optional_unbind: optional_unbind.clone(),
                path_alias: path_alias.clone(),
            };
        } else {
            match rel_el.direction {
                crate::ast::RelationshipDirection::LeftToRight => {
                    plan = Plan::MatchOut {
                        input: Some(Box::new(plan)),
                        src_alias: curr_src_alias.clone(),
                        dst_alias: dst_alias.clone(),
                        dst_labels: dst_labels.clone(),
                        src_prebound,
                        edge_alias: edge_alias.clone(),
                        rels: rel_types,
                        limit: None,
                        project: Vec::new(),
                        project_external: false,
                        optional,
                        optional_unbind: optional_unbind.clone(),
                        path_alias: path_alias.clone(),
                    };
                }
                crate::ast::RelationshipDirection::RightToLeft => {
                    plan = Plan::MatchIn {
                        input: Some(Box::new(plan)),
                        src_alias: curr_src_alias.clone(),
                        dst_alias: dst_alias.clone(),
                        dst_labels: dst_labels.clone(),
                        src_prebound,
                        edge_alias: edge_alias.clone(),
                        rels: rel_types,
                        limit: None,
                        optional,
                        optional_unbind: optional_unbind.clone(),
                        path_alias: path_alias.clone(),
                    };
                }
                crate::ast::RelationshipDirection::Undirected => {
                    plan = Plan::MatchUndirected {
                        input: Some(Box::new(plan)),
                        src_alias: curr_src_alias.clone(),
                        dst_alias: dst_alias.clone(),
                        dst_labels: dst_labels.clone(),
                        src_prebound,
                        edge_alias: edge_alias.clone(),
                        rels: rel_types,
                        limit: None,
                        optional,
                        optional_unbind: optional_unbind.clone(),
                        path_alias: path_alias.clone(),
                    };
                }
            }
        }

        if is_var_len
            && let (Some(path_alias_name), Some(rel_props)) =
                (path_alias.as_deref(), rel_el.properties.as_ref())
            && let Some(predicate) =
                build_var_len_rel_properties_predicate(path_alias_name, rel_props)
        {
            plan = Plan::Filter {
                input: Box::new(plan),
                predicate,
            };
        }

        // Extract properties from dst node and relationship
        extend_predicates_from_properties(
            &dst_alias,
            &dst_node_el.properties,
            &mut local_predicates,
        );
        if !is_var_len && let Some(ea) = &edge_alias {
            extend_predicates_from_properties(ea, &rel_el.properties, &mut local_predicates);
        }

        // Apply filters
        plan = apply_filters_for_alias(plan, &dst_alias, &local_predicates);
        if !is_var_len && let Some(ea) = &edge_alias {
            plan = apply_filters_for_alias(plan, ea, &local_predicates);
        }

        local_bound_aliases.insert(dst_alias.clone());
        if let Some(ea) = &edge_alias {
            local_bound_aliases.insert(ea.clone());
        }
        if let Some(pa) = &pattern.variable {
            local_bound_aliases.insert(pa.clone());
        }

        curr_src_alias = dst_alias;
        i += 2;
    }

    if pattern.elements.len() == 1
        && let Some(path_alias) = &pattern.variable
    {
        let mut vars = BTreeMap::new();
        extract_output_var_kinds(&plan, &mut vars);
        let mut projections: Vec<(String, Expression)> = vars
            .keys()
            .map(|name| (name.clone(), Expression::Variable(name.clone())))
            .collect();
        projections.push((
            path_alias.clone(),
            Expression::FunctionCall(crate::ast::FunctionCall {
                name: "__nervus_singleton_path".to_string(),
                args: vec![Expression::Variable(src_alias.clone())],
            }),
        ));
        plan = Plan::Project {
            input: Box::new(plan),
            projections,
        };
    }

    Ok(plan)
}

fn build_var_len_rel_properties_predicate(
    path_alias: &str,
    rel_props: &crate::ast::PropertyMap,
) -> Option<Expression> {
    let mut per_relationship_predicate: Option<Expression> = None;
    for prop in &rel_props.properties {
        let prop_access = Expression::PropertyAccess(crate::ast::PropertyAccess {
            variable: "__nervus_rel".to_string(),
            property: prop.key.clone(),
        });
        let eq_expr = Expression::Binary(Box::new(crate::ast::BinaryExpression {
            operator: crate::ast::BinaryOperator::Equals,
            left: prop_access,
            right: prop.value.clone(),
        }));
        per_relationship_predicate = Some(match per_relationship_predicate {
            Some(prev) => Expression::Binary(Box::new(crate::ast::BinaryExpression {
                operator: crate::ast::BinaryOperator::And,
                left: prev,
                right: eq_expr,
            })),
            None => eq_expr,
        });
    }

    per_relationship_predicate.map(|predicate| {
        Expression::FunctionCall(crate::ast::FunctionCall {
            name: "__quant_all".to_string(),
            args: vec![
                Expression::Variable("__nervus_rel".to_string()),
                Expression::FunctionCall(crate::ast::FunctionCall {
                    name: "relationships".to_string(),
                    args: vec![Expression::Variable(path_alias.to_string())],
                }),
                predicate,
            ],
        })
    })
}

fn is_bound_before_local(
    known_bindings: &BTreeMap<String, BindingKind>,
    local_bound_aliases: &BTreeSet<String>,
    alias: &str,
) -> bool {
    if local_bound_aliases.contains(alias) {
        return true;
    }
    if !known_bindings.contains_key(alias) {
        return false;
    }
    matches!(
        known_bindings.get(alias),
        Some(BindingKind::Node | BindingKind::Unknown)
    )
}

fn apply_filters_for_alias(
    plan: Plan,
    alias: &str,
    local_predicates: &BTreeMap<String, BTreeMap<String, Expression>>,
) -> Plan {
    if let Some(var_preds) = local_predicates.get(alias) {
        let mut combined_pred: Option<Expression> = None;
        for (field, val_expr) in var_preds {
            let prop_access = Expression::PropertyAccess(crate::ast::PropertyAccess {
                variable: alias.to_string(),
                property: field.clone(),
            });
            let eq_expr = Expression::Binary(Box::new(crate::ast::BinaryExpression {
                operator: crate::ast::BinaryOperator::Equals,
                left: prop_access,
                right: val_expr.clone(),
            }));

            combined_pred = match combined_pred {
                Some(prev) => Some(Expression::Binary(Box::new(crate::ast::BinaryExpression {
                    operator: crate::ast::BinaryOperator::And,
                    left: prev,
                    right: eq_expr,
                }))),
                None => Some(eq_expr),
            };
        }

        if let Some(predicate) = combined_pred {
            return Plan::Filter {
                input: Box::new(plan),
                predicate,
            };
        }
    }
    plan
}

fn apply_label_filters_for_alias(plan: Plan, alias: &str, labels: &[String]) -> Plan {
    let mut combined_pred: Option<Expression> = None;

    for label in labels {
        let has_label = Expression::Binary(Box::new(crate::ast::BinaryExpression {
            operator: crate::ast::BinaryOperator::HasLabel,
            left: Expression::Variable(alias.to_string()),
            right: Expression::Literal(crate::ast::Literal::String(label.clone())),
        }));

        // Keep OPTIONAL fallback rows where the alias is null.
        let label_or_null = Expression::Binary(Box::new(crate::ast::BinaryExpression {
            operator: crate::ast::BinaryOperator::Or,
            left: Expression::Binary(Box::new(crate::ast::BinaryExpression {
                operator: crate::ast::BinaryOperator::IsNull,
                left: Expression::Variable(alias.to_string()),
                right: Expression::Literal(crate::ast::Literal::Null),
            })),
            right: has_label,
        }));

        combined_pred = match combined_pred {
            Some(prev) => Some(Expression::Binary(Box::new(crate::ast::BinaryExpression {
                operator: crate::ast::BinaryOperator::And,
                left: prev,
                right: label_or_null,
            }))),
            None => Some(label_or_null),
        };
    }

    if let Some(predicate) = combined_pred {
        Plan::Filter {
            input: Box::new(plan),
            predicate,
        }
    } else {
        plan
    }
}

/// Helper to convert inline map properties to predicates
fn extend_predicates_from_properties(
    variable: &str,
    properties: &Option<crate::ast::PropertyMap>,
    predicates: &mut BTreeMap<String, BTreeMap<String, Expression>>,
) {
    if let Some(props) = properties {
        for prop in &props.properties {
            predicates
                .entry(variable.to_string())
                .or_default()
                .insert(prop.key.clone(), prop.value.clone());
        }
    }
}

fn pattern_uses_outer_bindings(
    pattern: &crate::ast::Pattern,
    known_bindings: &BTreeMap<String, BindingKind>,
    predicates: &BTreeMap<String, BTreeMap<String, Expression>>,
) -> bool {
    if known_bindings.is_empty() {
        return false;
    }

    let mut local_aliases = BTreeSet::new();
    if let Some(path_alias) = &pattern.variable {
        local_aliases.insert(path_alias.clone());
    }
    for element in &pattern.elements {
        match element {
            crate::ast::PathElement::Node(node) => {
                if let Some(alias) = &node.variable {
                    local_aliases.insert(alias.clone());
                }
                if property_map_uses_outer_bindings(
                    &node.properties,
                    known_bindings,
                    &local_aliases,
                ) {
                    return true;
                }
            }
            crate::ast::PathElement::Relationship(rel) => {
                if let Some(alias) = &rel.variable {
                    local_aliases.insert(alias.clone());
                }
                if property_map_uses_outer_bindings(&rel.properties, known_bindings, &local_aliases)
                {
                    return true;
                }
            }
        }
    }

    for (alias, fields) in predicates {
        if !local_aliases.contains(alias) {
            continue;
        }
        for expr in fields.values() {
            if expression_uses_outer_bindings(expr, known_bindings, &local_aliases) {
                return true;
            }
        }
    }

    false
}

fn property_map_uses_outer_bindings(
    properties: &Option<crate::ast::PropertyMap>,
    known_bindings: &BTreeMap<String, BindingKind>,
    local_aliases: &BTreeSet<String>,
) -> bool {
    properties.as_ref().is_some_and(|props| {
        props
            .properties
            .iter()
            .any(|pair| expression_uses_outer_bindings(&pair.value, known_bindings, local_aliases))
    })
}

fn expression_uses_outer_bindings(
    expr: &Expression,
    known_bindings: &BTreeMap<String, BindingKind>,
    local_aliases: &BTreeSet<String>,
) -> bool {
    let mut refs = std::collections::HashSet::new();
    extract_variables_from_expr(expr, &mut refs);
    refs.into_iter()
        .any(|name| known_bindings.contains_key(&name) && !local_aliases.contains(&name))
}

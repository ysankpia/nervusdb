use super::{
    BTreeMap, BindingKind, Error, Expression, HashSet, Literal, Plan, Result,
    compile_order_by_items, compile_projection_aggregation, contains_aggregate_expression,
    extract_output_var_kinds, extract_variables_from_expr, rewrite_order_expression,
    validate_expression_types, validate_order_by_aggregate_semantics, validate_order_by_scope,
    validate_where_expression_bindings,
};

fn validate_with_projection_aliases(with: &crate::ast::WithClause) -> Result<()> {
    for item in &with.items {
        if item.alias.is_some() {
            continue;
        }
        if matches!(&item.expression, Expression::Variable(_)) {
            continue;
        }
        if matches!(&item.expression, Expression::Literal(Literal::String(s)) if s == "*") {
            continue;
        }
        return Err(Error::Other("syntax error: NoExpressionAlias".to_string()));
    }
    Ok(())
}

fn validate_skip_or_limit_expression(expr: &Expression) -> Result<()> {
    let mut used = HashSet::new();
    extract_variables_from_expr(expr, &mut used);
    if !used.is_empty() {
        return Err(Error::Other(
            "syntax error: NonConstantExpression".to_string(),
        ));
    }

    validate_expression_types(expr)?;

    match expr {
        Expression::Unary(unary)
            if matches!(unary.operator, crate::ast::UnaryOperator::Negate)
                && matches!(unary.operand, Expression::Literal(Literal::Integer(_))) =>
        {
            return Err(Error::Other(
                "syntax error: NegativeIntegerArgument".to_string(),
            ));
        }
        Expression::Literal(Literal::Integer(v)) if *v < 0 => {
            return Err(Error::Other(
                "syntax error: NegativeIntegerArgument".to_string(),
            ));
        }
        Expression::Literal(Literal::Float(_))
        | Expression::Unary(_)
        | Expression::Literal(Literal::Boolean(_) | Literal::String(_) | Literal::Null)
        | Expression::Map(_)
        | Expression::List(_) => {
            return Err(Error::Other(
                "syntax error: InvalidArgumentType".to_string(),
            ));
        }
        _ => {}
    }

    Ok(())
}

pub(super) fn compile_with_plan(input: Plan, with: &crate::ast::WithClause) -> Result<Plan> {
    validate_with_projection_aliases(with)?;

    let has_aggregation = with
        .items
        .iter()
        .any(|item| contains_aggregate_expression(&item.expression));
    let mut input_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();
    extract_output_var_kinds(&input, &mut input_bindings);

    let (mut plan, project_cols) = compile_projection_aggregation(input, &with.items, true)?;

    if let Some(w) = &with.where_clause {
        validate_expression_types(&w.expression)?;
        let mut where_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();
        extract_output_var_kinds(&plan, &mut where_bindings);
        if !has_aggregation {
            for (name, kind) in &input_bindings {
                where_bindings.entry(name.clone()).or_insert(*kind);
            }
        }
        validate_where_expression_bindings(&w.expression, &where_bindings)?;

        let mut passthrough: Vec<String> = Vec::new();
        if !has_aggregation {
            let projected: HashSet<String> = project_cols.iter().cloned().collect();
            let mut used = HashSet::new();
            extract_variables_from_expr(&w.expression, &mut used);
            passthrough = used
                .into_iter()
                .filter(|name| !projected.contains(name) && input_bindings.contains_key(name))
                .collect();
            passthrough.sort();
            passthrough.dedup();

            if !passthrough.is_empty()
                && let Plan::Project {
                    input,
                    mut projections,
                } = plan
            {
                for name in &passthrough {
                    projections.push((name.clone(), Expression::Variable(name.clone())));
                }
                plan = Plan::Project { input, projections };
            }
        }

        plan = Plan::Filter {
            input: Box::new(plan),
            predicate: w.expression.clone(),
        };

        if !has_aggregation && !passthrough.is_empty() {
            plan = Plan::Project {
                input: Box::new(plan),
                projections: project_cols
                    .iter()
                    .cloned()
                    .map(|name| (name.clone(), Expression::Variable(name)))
                    .collect(),
            };
        }
    }

    if let Some(order_by) = &with.order_by {
        let rewrite_bindings: Vec<(Expression, String)> = with
            .items
            .iter()
            .zip(project_cols.iter())
            .map(|(item, alias)| {
                let alias = item.alias.clone().unwrap_or_else(|| alias.clone());
                (item.expression.clone(), alias)
            })
            .collect();

        let mut normalized = order_by.clone();
        for item in &mut normalized.items {
            item.expression = rewrite_order_expression(&item.expression, &rewrite_bindings);
        }

        let mut order_scope_cols = project_cols.clone();
        let mut order_passthrough: Vec<String> = Vec::new();
        if !has_aggregation && !with.distinct {
            let projected: HashSet<String> = project_cols.iter().cloned().collect();
            let mut used = HashSet::new();
            for item in &normalized.items {
                extract_variables_from_expr(&item.expression, &mut used);
            }

            order_passthrough = used
                .into_iter()
                .filter(|name| !projected.contains(name) && input_bindings.contains_key(name))
                .collect();
            order_passthrough.sort();
            order_passthrough.dedup();

            if !order_passthrough.is_empty()
                && let Plan::Project {
                    input,
                    mut projections,
                } = plan
            {
                for name in &order_passthrough {
                    projections.push((name.clone(), Expression::Variable(name.clone())));
                }
                plan = Plan::Project { input, projections };
            }

            order_scope_cols.extend(order_passthrough.iter().cloned());
        }

        validate_order_by_scope(&normalized, &order_scope_cols, &with.items, with.distinct)?;
        validate_order_by_aggregate_semantics(order_by, &with.items)?;
        let items = compile_order_by_items(&normalized)?;
        plan = Plan::OrderBy {
            input: Box::new(plan),
            items,
        };

        if !order_passthrough.is_empty() {
            plan = Plan::Project {
                input: Box::new(plan),
                projections: project_cols
                    .iter()
                    .cloned()
                    .map(|name| (name.clone(), Expression::Variable(name)))
                    .collect(),
            };
        }
    }

    if let Some(skip) = &with.skip {
        validate_skip_or_limit_expression(skip)?;
        plan = Plan::Skip {
            input: Box::new(plan),
            skip: skip.clone(),
        };
    }

    if let Some(limit) = &with.limit {
        validate_skip_or_limit_expression(limit)?;
        plan = Plan::Limit {
            input: Box::new(plan),
            limit: limit.clone(),
        };
    }

    if with.distinct {
        plan = Plan::Distinct {
            input: Box::new(plan),
        };
    }

    Ok(plan)
}

pub(super) fn compile_return_plan(
    input: Plan,
    ret: &crate::ast::ReturnClause,
) -> Result<(Plan, Vec<String>)> {
    let has_aggregation = ret
        .items
        .iter()
        .any(|item| contains_aggregate_expression(&item.expression));
    let mut input_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();
    extract_output_var_kinds(&input, &mut input_bindings);

    let (mut plan, project_cols) = compile_projection_aggregation(input, &ret.items, false)?;

    if let Some(order_by) = &ret.order_by {
        let rewrite_bindings: Vec<(Expression, String)> = ret
            .items
            .iter()
            .zip(project_cols.iter())
            .map(|(item, alias)| {
                let alias = item.alias.clone().unwrap_or_else(|| alias.clone());
                (item.expression.clone(), alias)
            })
            .collect();

        let mut normalized = order_by.clone();
        for item in &mut normalized.items {
            item.expression = rewrite_order_expression(&item.expression, &rewrite_bindings);
        }

        let mut order_scope_cols = project_cols.clone();
        let mut order_passthrough: Vec<String> = Vec::new();
        if !has_aggregation && !ret.distinct {
            let projected: HashSet<String> = project_cols.iter().cloned().collect();
            let mut used = HashSet::new();
            for item in &normalized.items {
                extract_variables_from_expr(&item.expression, &mut used);
            }

            order_passthrough = used
                .into_iter()
                .filter(|name| !projected.contains(name) && input_bindings.contains_key(name))
                .collect();
            order_passthrough.sort();
            order_passthrough.dedup();

            if !order_passthrough.is_empty()
                && let Plan::Project {
                    input,
                    mut projections,
                } = plan
            {
                for name in &order_passthrough {
                    projections.push((name.clone(), Expression::Variable(name.clone())));
                }
                plan = Plan::Project { input, projections };
            }

            order_scope_cols.extend(order_passthrough.iter().cloned());
        }

        validate_order_by_scope(&normalized, &order_scope_cols, &ret.items, ret.distinct)?;
        validate_order_by_aggregate_semantics(order_by, &ret.items)?;
        let items = compile_order_by_items(&normalized)?;
        plan = Plan::OrderBy {
            input: Box::new(plan),
            items,
        };

        if !order_passthrough.is_empty() {
            plan = Plan::Project {
                input: Box::new(plan),
                projections: project_cols
                    .iter()
                    .cloned()
                    .map(|name| (name.clone(), Expression::Variable(name)))
                    .collect(),
            };
        }
    }

    if let Some(skip) = &ret.skip {
        validate_skip_or_limit_expression(skip)?;
        plan = Plan::Skip {
            input: Box::new(plan),
            skip: skip.clone(),
        };
    }

    if let Some(limit) = &ret.limit {
        validate_skip_or_limit_expression(limit)?;
        plan = Plan::Limit {
            input: Box::new(plan),
            limit: limit.clone(),
        };
    }

    if ret.distinct {
        plan = Plan::Distinct {
            input: Box::new(plan),
        };
    }

    Ok((plan, project_cols))
}

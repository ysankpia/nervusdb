use super::{
    BTreeMap, BindingKind, Error, Expression, Literal, Plan, Result, default_projection_alias,
    ensure_no_pattern_predicate, extract_output_var_kinds, infer_expression_binding_kind,
    is_internal_path_alias, validate_expression_types,
};

fn is_quantifier_call(call: &crate::query::ast::FunctionCall) -> bool {
    matches!(
        call.name.as_str(),
        "__quant_any" | "__quant_all" | "__quant_none" | "__quant_single"
    )
}

fn is_reduce_call(call: &crate::query::ast::FunctionCall) -> bool {
    call.name.eq_ignore_ascii_case("__reduce")
}

fn resolve_projection_source_expr<'a>(plan: &'a Plan, variable: &str) -> Option<&'a Expression> {
    match plan {
        Plan::Project { input, projections } => {
            if let Some((_, expr)) = projections
                .iter()
                .rev()
                .find(|(alias, _)| alias == variable)
            {
                Some(expr)
            } else {
                resolve_projection_source_expr(input, variable)
            }
        }
        Plan::Filter { input, .. }
        | Plan::Limit { input, .. }
        | Plan::Delete { input, .. }
        | Plan::SetProperty { input, .. }
        | Plan::Create { input, .. } => resolve_projection_source_expr(input, variable),
        Plan::CartesianProduct { left, right } => resolve_projection_source_expr(right, variable)
            .or_else(|| resolve_projection_source_expr(left, variable)),
        Plan::MatchBoundRel { input, .. } => resolve_projection_source_expr(input, variable),
        Plan::Values { .. } => None,
        Plan::MatchOut { input, .. } => input
            .as_deref()
            .and_then(|inner| resolve_projection_source_expr(inner, variable)),
        Plan::NodeScan { .. } | Plan::ReturnOne => None,
    }
}

fn is_definitely_non_map_source(expr: &Expression, input_plan: &Plan, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    match expr {
        Expression::Map(_) => false,
        Expression::Literal(crate::query::ast::Literal::Null) => false,
        Expression::Literal(_) | Expression::List(_) => true,
        Expression::Variable(name) => resolve_projection_source_expr(input_plan, name)
            .is_some_and(|source| is_definitely_non_map_source(source, input_plan, depth + 1)),
        _ => false,
    }
}

fn validate_projection_expression_bindings(
    expr: &Expression,
    known_bindings: &BTreeMap<String, BindingKind>,
    local_scopes: &mut Vec<std::collections::HashSet<String>>,
    input_plan: &Plan,
) -> Result<()> {
    match expr {
        Expression::Variable(var) => {
            if !is_locally_bound(local_scopes, var) && !known_bindings.contains_key(var) {
                return Err(Error::Other(format!(
                    "syntax error: UndefinedVariable ({})",
                    var
                )));
            }
        }
        Expression::PropertyAccess(pa) => {
            if !is_locally_bound(local_scopes, &pa.variable)
                && matches!(
                    known_bindings.get(&pa.variable),
                    Some(BindingKind::Path | BindingKind::RelationshipList)
                )
            {
                return Err(Error::Other(
                    "syntax error: InvalidArgumentType".to_string(),
                ));
            }
            if !is_locally_bound(local_scopes, &pa.variable)
                && !known_bindings.contains_key(&pa.variable)
            {
                return Err(Error::Other(format!(
                    "syntax error: UndefinedVariable ({})",
                    pa.variable
                )));
            }
            if !is_locally_bound(local_scopes, &pa.variable)
                && matches!(known_bindings.get(&pa.variable), Some(BindingKind::Scalar))
                && resolve_projection_source_expr(input_plan, &pa.variable)
                    .is_some_and(|source| is_definitely_non_map_source(source, input_plan, 0))
            {
                return Err(Error::Other(
                    "syntax error: InvalidArgumentType".to_string(),
                ));
            }
        }
        Expression::Unary(u) => validate_projection_expression_bindings(
            &u.operand,
            known_bindings,
            local_scopes,
            input_plan,
        )?,
        Expression::Binary(b) => {
            validate_projection_expression_bindings(
                &b.left,
                known_bindings,
                local_scopes,
                input_plan,
            )?;
            validate_projection_expression_bindings(
                &b.right,
                known_bindings,
                local_scopes,
                input_plan,
            )?;
        }
        Expression::FunctionCall(call) => {
            if is_quantifier_call(call) && call.args.len() == 3 {
                validate_projection_expression_bindings(
                    &call.args[1],
                    known_bindings,
                    local_scopes,
                    input_plan,
                )?;
                if let Expression::Variable(var) = &call.args[0] {
                    let mut scope = std::collections::HashSet::new();
                    scope.insert(var.clone());
                    local_scopes.push(scope);
                    validate_projection_expression_bindings(
                        &call.args[2],
                        known_bindings,
                        local_scopes,
                        input_plan,
                    )?;
                    local_scopes.pop();
                } else {
                    validate_projection_expression_bindings(
                        &call.args[2],
                        known_bindings,
                        local_scopes,
                        input_plan,
                    )?;
                }
            } else if is_reduce_call(call) && call.args.len() == 5 {
                validate_projection_expression_bindings(
                    &call.args[1],
                    known_bindings,
                    local_scopes,
                    input_plan,
                )?;
                validate_projection_expression_bindings(
                    &call.args[3],
                    known_bindings,
                    local_scopes,
                    input_plan,
                )?;

                let mut scope = std::collections::HashSet::new();
                if let Expression::Variable(acc) = &call.args[0] {
                    scope.insert(acc.clone());
                }
                if let Expression::Variable(item) = &call.args[2] {
                    scope.insert(item.clone());
                }

                if scope.is_empty() {
                    validate_projection_expression_bindings(
                        &call.args[4],
                        known_bindings,
                        local_scopes,
                        input_plan,
                    )?;
                } else {
                    local_scopes.push(scope);
                    validate_projection_expression_bindings(
                        &call.args[4],
                        known_bindings,
                        local_scopes,
                        input_plan,
                    )?;
                    local_scopes.pop();
                }
            } else {
                for arg in &call.args {
                    validate_projection_expression_bindings(
                        arg,
                        known_bindings,
                        local_scopes,
                        input_plan,
                    )?;
                }
            }
        }
        Expression::List(items) => {
            for item in items {
                validate_projection_expression_bindings(
                    item,
                    known_bindings,
                    local_scopes,
                    input_plan,
                )?;
            }
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                validate_projection_expression_bindings(
                    &pair.value,
                    known_bindings,
                    local_scopes,
                    input_plan,
                )?;
            }
        }
        Expression::Case(case_expr) => {
            if let Some(test_expr) = &case_expr.expression {
                validate_projection_expression_bindings(
                    test_expr,
                    known_bindings,
                    local_scopes,
                    input_plan,
                )?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                validate_projection_expression_bindings(
                    when_expr,
                    known_bindings,
                    local_scopes,
                    input_plan,
                )?;
                validate_projection_expression_bindings(
                    then_expr,
                    known_bindings,
                    local_scopes,
                    input_plan,
                )?;
            }
            if let Some(else_expr) = &case_expr.else_expression {
                validate_projection_expression_bindings(
                    else_expr,
                    known_bindings,
                    local_scopes,
                    input_plan,
                )?;
            }
        }
        Expression::ListComprehension(list_comp) => {
            validate_projection_expression_bindings(
                &list_comp.list,
                known_bindings,
                local_scopes,
                input_plan,
            )?;
            let mut scope = std::collections::HashSet::new();
            scope.insert(list_comp.variable.clone());
            local_scopes.push(scope);
            if let Some(where_expr) = &list_comp.where_expression {
                validate_projection_expression_bindings(
                    where_expr,
                    known_bindings,
                    local_scopes,
                    input_plan,
                )?;
            }
            if let Some(map_expr) = &list_comp.map_expression {
                validate_projection_expression_bindings(
                    map_expr,
                    known_bindings,
                    local_scopes,
                    input_plan,
                )?;
            }
            local_scopes.pop();
        }
        Expression::PatternComprehension(pattern_comp) => {
            let scope = collect_pattern_local_variables(&pattern_comp.pattern);
            local_scopes.push(scope);

            for element in &pattern_comp.pattern.elements {
                match element {
                    crate::query::ast::PathElement::Node(node) => {
                        if let Some(props) = &node.properties {
                            for pair in &props.properties {
                                validate_projection_expression_bindings(
                                    &pair.value,
                                    known_bindings,
                                    local_scopes,
                                    input_plan,
                                )?;
                            }
                        }
                    }
                    crate::query::ast::PathElement::Relationship(rel) => {
                        if let Some(props) = &rel.properties {
                            for pair in &props.properties {
                                validate_projection_expression_bindings(
                                    &pair.value,
                                    known_bindings,
                                    local_scopes,
                                    input_plan,
                                )?;
                            }
                        }
                    }
                }
            }

            if let Some(where_expr) = &pattern_comp.where_expression {
                validate_projection_expression_bindings(
                    where_expr,
                    known_bindings,
                    local_scopes,
                    input_plan,
                )?;
            }
            validate_projection_expression_bindings(
                &pattern_comp.projection,
                known_bindings,
                local_scopes,
                input_plan,
            )?;

            local_scopes.pop();
        }
        Expression::Exists(exists_expr) => match exists_expr.as_ref() {
            crate::query::ast::ExistsExpression::Pattern(pattern) => {
                let scope = collect_pattern_local_variables(pattern);
                local_scopes.push(scope);

                for element in &pattern.elements {
                    match element {
                        crate::query::ast::PathElement::Node(node) => {
                            if let Some(props) = &node.properties {
                                for pair in &props.properties {
                                    validate_projection_expression_bindings(
                                        &pair.value,
                                        known_bindings,
                                        local_scopes,
                                        input_plan,
                                    )?;
                                }
                            }
                        }
                        crate::query::ast::PathElement::Relationship(rel) => {
                            if let Some(props) = &rel.properties {
                                for pair in &props.properties {
                                    validate_projection_expression_bindings(
                                        &pair.value,
                                        known_bindings,
                                        local_scopes,
                                        input_plan,
                                    )?;
                                }
                            }
                        }
                    }
                }

                local_scopes.pop();
            }
            crate::query::ast::ExistsExpression::Subquery(_) => {
                // Subquery variables are validated in nested query compilation.
            }
        },
        Expression::Parameter(_) | Expression::Literal(_) => {}
    }
    Ok(())
}

fn validate_projection_bindings_root(
    expr: &Expression,
    known_bindings: &BTreeMap<String, BindingKind>,
    input_plan: &Plan,
) -> Result<()> {
    let mut local_scopes: Vec<std::collections::HashSet<String>> = Vec::new();
    validate_projection_expression_bindings(expr, known_bindings, &mut local_scopes, input_plan)
}

fn validate_projection_expression_semantics(
    expr: &Expression,
    vars: &BTreeMap<String, BindingKind>,
) -> Result<()> {
    match expr {
        Expression::FunctionCall(call) => {
            for arg in &call.args {
                validate_projection_expression_semantics(arg, vars)?;
            }
            if call.name.eq_ignore_ascii_case("labels") && call.args.len() == 1 {
                match infer_expression_binding_kind(&call.args[0], vars) {
                    BindingKind::Relationship
                    | BindingKind::RelationshipList
                    | BindingKind::Path => {
                        return Err(Error::Other(
                            "syntax error: InvalidArgumentType".to_string(),
                        ));
                    }
                    _ => {}
                }
            }
            if call.name.eq_ignore_ascii_case("type") && call.args.len() == 1 {
                match infer_expression_binding_kind(&call.args[0], vars) {
                    BindingKind::Node | BindingKind::RelationshipList | BindingKind::Path => {
                        return Err(Error::Other(
                            "syntax error: InvalidArgumentType".to_string(),
                        ));
                    }
                    _ => {}
                }
            }
            if call.name.eq_ignore_ascii_case("size")
                && call.args.len() == 1
                && infer_expression_binding_kind(&call.args[0], vars) == BindingKind::Path
            {
                return Err(Error::Other(
                    "syntax error: InvalidArgumentType".to_string(),
                ));
            }
            if call.name.eq_ignore_ascii_case("length") && call.args.len() == 1 {
                match infer_expression_binding_kind(&call.args[0], vars) {
                    BindingKind::Node
                    | BindingKind::Relationship
                    | BindingKind::RelationshipList => {
                        return Err(Error::Other(
                            "syntax error: InvalidArgumentType".to_string(),
                        ));
                    }
                    _ => {}
                }
            }
        }
        Expression::Unary(u) => validate_projection_expression_semantics(&u.operand, vars)?,
        Expression::Binary(b) => {
            validate_projection_expression_semantics(&b.left, vars)?;
            validate_projection_expression_semantics(&b.right, vars)?;
        }
        Expression::List(items) => {
            for item in items {
                validate_projection_expression_semantics(item, vars)?;
            }
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                validate_projection_expression_semantics(&pair.value, vars)?;
            }
        }
        Expression::Case(case_expr) => {
            if let Some(test_expr) = &case_expr.expression {
                validate_projection_expression_semantics(test_expr, vars)?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                validate_projection_expression_semantics(when_expr, vars)?;
                validate_projection_expression_semantics(then_expr, vars)?;
            }
            if let Some(else_expr) = &case_expr.else_expression {
                validate_projection_expression_semantics(else_expr, vars)?;
            }
        }
        Expression::ListComprehension(list_comp) => {
            validate_projection_expression_semantics(&list_comp.list, vars)?;
            if let Some(where_expr) = &list_comp.where_expression {
                if contains_aggregate_expression(where_expr) {
                    return Err(Error::Other("syntax error: InvalidAggregation".to_string()));
                }
                validate_projection_expression_semantics(where_expr, vars)?;
            }
            if let Some(map_expr) = &list_comp.map_expression {
                if contains_aggregate_expression(map_expr) {
                    return Err(Error::Other("syntax error: InvalidAggregation".to_string()));
                }
                validate_projection_expression_semantics(map_expr, vars)?;
            }
        }
        Expression::PatternComprehension(pattern_comp) => {
            for element in &pattern_comp.pattern.elements {
                match element {
                    crate::query::ast::PathElement::Node(node) => {
                        if let Some(props) = &node.properties {
                            for pair in &props.properties {
                                validate_projection_expression_semantics(&pair.value, vars)?;
                            }
                        }
                    }
                    crate::query::ast::PathElement::Relationship(rel) => {
                        if let Some(props) = &rel.properties {
                            for pair in &props.properties {
                                validate_projection_expression_semantics(&pair.value, vars)?;
                            }
                        }
                    }
                }
            }
            if let Some(where_expr) = &pattern_comp.where_expression {
                validate_projection_expression_semantics(where_expr, vars)?;
            }
            validate_projection_expression_semantics(&pattern_comp.projection, vars)?;
        }
        Expression::Exists(exists) => match exists.as_ref() {
            crate::query::ast::ExistsExpression::Pattern(pattern) => {
                for element in &pattern.elements {
                    match element {
                        crate::query::ast::PathElement::Node(node) => {
                            if let Some(props) = &node.properties {
                                for pair in &props.properties {
                                    validate_projection_expression_semantics(&pair.value, vars)?;
                                }
                            }
                        }
                        crate::query::ast::PathElement::Relationship(rel) => {
                            if let Some(props) = &rel.properties {
                                for pair in &props.properties {
                                    validate_projection_expression_semantics(&pair.value, vars)?;
                                }
                            }
                        }
                    }
                }
            }
            crate::query::ast::ExistsExpression::Subquery(subquery) => {
                for clause in &subquery.clauses {
                    match clause {
                        crate::query::ast::Clause::Where(w) => {
                            validate_projection_expression_semantics(&w.expression, vars)?
                        }
                        crate::query::ast::Clause::With(w) => {
                            for item in &w.items {
                                validate_projection_expression_semantics(&item.expression, vars)?;
                            }
                            if let Some(where_clause) = &w.where_clause {
                                validate_projection_expression_semantics(
                                    &where_clause.expression,
                                    vars,
                                )?;
                            }
                        }
                        crate::query::ast::Clause::Return(r) => {
                            for item in &r.items {
                                validate_projection_expression_semantics(&item.expression, vars)?;
                            }
                        }
                        _ => {}
                    }
                }
            }
        },
        Expression::Literal(_)
        | Expression::Variable(_)
        | Expression::PropertyAccess(_)
        | Expression::Parameter(_) => {}
    }

    Ok(())
}

fn is_locally_bound(local_scopes: &[std::collections::HashSet<String>], var: &str) -> bool {
    local_scopes.iter().rev().any(|scope| scope.contains(var))
}

fn collect_pattern_local_variables(
    pattern: &crate::query::ast::Pattern,
) -> std::collections::HashSet<String> {
    let mut vars = std::collections::HashSet::new();
    if let Some(path_var) = &pattern.variable {
        vars.insert(path_var.clone());
    }
    for element in &pattern.elements {
        match element {
            crate::query::ast::PathElement::Node(node) => {
                if let Some(var) = &node.variable {
                    vars.insert(var.clone());
                }
            }
            crate::query::ast::PathElement::Relationship(rel) => {
                if let Some(var) = &rel.variable {
                    vars.insert(var.clone());
                }
            }
        }
    }
    vars
}

pub(super) fn compile_projection_aggregation(
    input: Plan,
    items: &[crate::query::ast::ReturnItem],
    allow_empty_scope_with_star: bool,
) -> Result<(Plan, Vec<String>)> {
    let mut input_bindings = BTreeMap::new();
    extract_output_var_kinds(&input, &mut input_bindings);
    for item in items {
        ensure_no_pattern_predicate(&item.expression)?;
        validate_projection_bindings_root(&item.expression, &input_bindings, &input)?;
        validate_expression_types(&item.expression)?;
        validate_projection_expression_semantics(&item.expression, &input_bindings)?;
    }

    // RETURN * / WITH * expansion.
    if items.len() == 1
        && items[0].alias.is_none()
        && matches!(&items[0].expression, Expression::Literal(Literal::String(s)) if s == "*")
    {
        let mut vars = BTreeMap::new();
        extract_output_var_kinds(&input, &mut vars);
        let cols: Vec<String> = vars
            .keys()
            .filter(|name| !is_internal_path_alias(name) && !name.starts_with("_gen_"))
            .cloned()
            .collect();
        if cols.is_empty() {
            if allow_empty_scope_with_star {
                return Ok((
                    Plan::Project {
                        input: Box::new(input),
                        projections: Vec::new(),
                    },
                    Vec::new(),
                ));
            }
            return Err(Error::Other("syntax error: NoVariablesInScope".to_string()));
        }
        let projections: Vec<(String, Expression)> = cols
            .iter()
            .map(|name| (name.clone(), Expression::Variable(name.clone())))
            .collect();

        return Ok((
            Plan::Project {
                input: Box::new(input),
                projections,
            },
            cols,
        ));
    }

    let mut resolved_items: Vec<(Expression, String)> = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        if contains_aggregate_expression(&item.expression) {
            return Err(Error::Other(
                "syntax error: aggregation is outside Mini-Cypher 0.1".to_string(),
            ));
        }

        let alias = if let Some(alias) = &item.alias {
            alias.clone()
        } else {
            default_projection_alias(&item.expression, i)
        };

        resolved_items.push((item.expression.clone(), alias));
    }

    let mut seen_aliases = std::collections::HashSet::new();
    for (_, alias) in &resolved_items {
        if !seen_aliases.insert(alias.clone()) {
            return Err(Error::Other("syntax error: ColumnNameConflict".to_string()));
        }
    }

    let projections: Vec<(String, Expression)> = resolved_items
        .iter()
        .map(|(expr, alias)| (alias.clone(), expr.clone()))
        .collect();
    let project_cols: Vec<String> = resolved_items
        .iter()
        .map(|(_, alias)| alias.clone())
        .collect();

    Ok((
        Plan::Project {
            input: Box::new(input),
            projections,
        },
        project_cols,
    ))
}

pub(super) fn contains_aggregate_expression(expr: &Expression) -> bool {
    match expr {
        Expression::FunctionCall(call) => {
            let name = call.name.to_lowercase();
            if matches!(
                name.as_str(),
                "count"
                    | "sum"
                    | "avg"
                    | "min"
                    | "max"
                    | "collect"
                    | "percentiledisc"
                    | "percentilecont"
            ) {
                return true;
            }
            call.args.iter().any(contains_aggregate_expression)
        }
        Expression::Binary(b) => {
            contains_aggregate_expression(&b.left) || contains_aggregate_expression(&b.right)
        }
        Expression::Unary(u) => contains_aggregate_expression(&u.operand),
        Expression::List(items) => items.iter().any(contains_aggregate_expression),
        Expression::ListComprehension(list_comp) => {
            contains_aggregate_expression(&list_comp.list)
                || list_comp
                    .where_expression
                    .as_ref()
                    .is_some_and(contains_aggregate_expression)
                || list_comp
                    .map_expression
                    .as_ref()
                    .is_some_and(contains_aggregate_expression)
        }
        Expression::PatternComprehension(pattern_comp) => {
            pattern_comp
                .where_expression
                .as_ref()
                .is_some_and(contains_aggregate_expression)
                || contains_aggregate_expression(&pattern_comp.projection)
                || pattern_comp
                    .pattern
                    .elements
                    .iter()
                    .any(|element| match element {
                        crate::query::ast::PathElement::Node(node) => {
                            node.properties.as_ref().is_some_and(|props| {
                                props
                                    .properties
                                    .iter()
                                    .any(|pair| contains_aggregate_expression(&pair.value))
                            })
                        }
                        crate::query::ast::PathElement::Relationship(rel) => {
                            rel.properties.as_ref().is_some_and(|props| {
                                props
                                    .properties
                                    .iter()
                                    .any(|pair| contains_aggregate_expression(&pair.value))
                            })
                        }
                    })
        }
        Expression::Map(map) => map
            .properties
            .iter()
            .any(|pair| contains_aggregate_expression(&pair.value)),
        _ => false,
    }
}

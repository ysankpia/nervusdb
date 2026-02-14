use super::{
    BTreeMap, BindingKind, Error, Expression, Literal, Plan, Result, default_aggregate_alias,
    default_projection_alias, ensure_no_pattern_predicate, extract_output_var_kinds,
    extract_variables_from_expr, infer_expression_binding_kind, is_internal_path_alias,
    parse_aggregate_function, validate_expression_types,
};

fn is_simple_group_expression(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::Variable(_) | Expression::PropertyAccess(_)
    )
}

fn contains_function_call_named(expr: &Expression, target: &str) -> bool {
    match expr {
        Expression::FunctionCall(call) => {
            if call.name.eq_ignore_ascii_case(target) {
                return true;
            }
            call.args
                .iter()
                .any(|arg| contains_function_call_named(arg, target))
        }
        Expression::Binary(b) => {
            contains_function_call_named(&b.left, target)
                || contains_function_call_named(&b.right, target)
        }
        Expression::Unary(u) => contains_function_call_named(&u.operand, target),
        Expression::List(items) => items
            .iter()
            .any(|item| contains_function_call_named(item, target)),
        Expression::ListComprehension(list_comp) => {
            contains_function_call_named(&list_comp.list, target)
                || list_comp
                    .where_expression
                    .as_ref()
                    .is_some_and(|expr| contains_function_call_named(expr, target))
                || list_comp
                    .map_expression
                    .as_ref()
                    .is_some_and(|expr| contains_function_call_named(expr, target))
        }
        Expression::PatternComprehension(pattern_comp) => {
            pattern_comp
                .where_expression
                .as_ref()
                .is_some_and(|expr| contains_function_call_named(expr, target))
                || contains_function_call_named(&pattern_comp.projection, target)
        }
        Expression::Map(map) => map
            .properties
            .iter()
            .any(|pair| contains_function_call_named(&pair.value, target)),
        Expression::Case(case_expr) => {
            case_expr
                .expression
                .as_ref()
                .is_some_and(|expr| contains_function_call_named(expr, target))
                || case_expr.when_clauses.iter().any(|(w, t)| {
                    contains_function_call_named(w, target)
                        || contains_function_call_named(t, target)
                })
                || case_expr
                    .else_expression
                    .as_ref()
                    .is_some_and(|expr| contains_function_call_named(expr, target))
        }
        _ => false,
    }
}

fn is_quantifier_call(call: &crate::ast::FunctionCall) -> bool {
    matches!(
        call.name.as_str(),
        "__quant_any" | "__quant_all" | "__quant_none" | "__quant_single"
    )
}

fn validate_projection_expression_bindings(
    expr: &Expression,
    known_bindings: &BTreeMap<String, BindingKind>,
    local_scopes: &mut Vec<std::collections::HashSet<String>>,
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
        }
        Expression::Unary(u) => {
            validate_projection_expression_bindings(&u.operand, known_bindings, local_scopes)?
        }
        Expression::Binary(b) => {
            validate_projection_expression_bindings(&b.left, known_bindings, local_scopes)?;
            validate_projection_expression_bindings(&b.right, known_bindings, local_scopes)?;
        }
        Expression::FunctionCall(call) => {
            if is_quantifier_call(call) && call.args.len() == 3 {
                validate_projection_expression_bindings(
                    &call.args[1],
                    known_bindings,
                    local_scopes,
                )?;
                if let Expression::Variable(var) = &call.args[0] {
                    let mut scope = std::collections::HashSet::new();
                    scope.insert(var.clone());
                    local_scopes.push(scope);
                    validate_projection_expression_bindings(
                        &call.args[2],
                        known_bindings,
                        local_scopes,
                    )?;
                    local_scopes.pop();
                } else {
                    validate_projection_expression_bindings(
                        &call.args[2],
                        known_bindings,
                        local_scopes,
                    )?;
                }
            } else {
                for arg in &call.args {
                    validate_projection_expression_bindings(arg, known_bindings, local_scopes)?;
                }
            }
        }
        Expression::List(items) => {
            for item in items {
                validate_projection_expression_bindings(item, known_bindings, local_scopes)?;
            }
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                validate_projection_expression_bindings(&pair.value, known_bindings, local_scopes)?;
            }
        }
        Expression::Case(case_expr) => {
            if let Some(test_expr) = &case_expr.expression {
                validate_projection_expression_bindings(test_expr, known_bindings, local_scopes)?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                validate_projection_expression_bindings(when_expr, known_bindings, local_scopes)?;
                validate_projection_expression_bindings(then_expr, known_bindings, local_scopes)?;
            }
            if let Some(else_expr) = &case_expr.else_expression {
                validate_projection_expression_bindings(else_expr, known_bindings, local_scopes)?;
            }
        }
        Expression::ListComprehension(list_comp) => {
            validate_projection_expression_bindings(&list_comp.list, known_bindings, local_scopes)?;
            let mut scope = std::collections::HashSet::new();
            scope.insert(list_comp.variable.clone());
            local_scopes.push(scope);
            if let Some(where_expr) = &list_comp.where_expression {
                validate_projection_expression_bindings(where_expr, known_bindings, local_scopes)?;
            }
            if let Some(map_expr) = &list_comp.map_expression {
                validate_projection_expression_bindings(map_expr, known_bindings, local_scopes)?;
            }
            local_scopes.pop();
        }
        Expression::PatternComprehension(pattern_comp) => {
            let scope = collect_pattern_local_variables(&pattern_comp.pattern);
            local_scopes.push(scope);

            for element in &pattern_comp.pattern.elements {
                match element {
                    crate::ast::PathElement::Node(node) => {
                        if let Some(props) = &node.properties {
                            for pair in &props.properties {
                                validate_projection_expression_bindings(
                                    &pair.value,
                                    known_bindings,
                                    local_scopes,
                                )?;
                            }
                        }
                    }
                    crate::ast::PathElement::Relationship(rel) => {
                        if let Some(props) = &rel.properties {
                            for pair in &props.properties {
                                validate_projection_expression_bindings(
                                    &pair.value,
                                    known_bindings,
                                    local_scopes,
                                )?;
                            }
                        }
                    }
                }
            }

            if let Some(where_expr) = &pattern_comp.where_expression {
                validate_projection_expression_bindings(where_expr, known_bindings, local_scopes)?;
            }
            validate_projection_expression_bindings(
                &pattern_comp.projection,
                known_bindings,
                local_scopes,
            )?;

            local_scopes.pop();
        }
        Expression::Exists(exists_expr) => match exists_expr.as_ref() {
            crate::ast::ExistsExpression::Pattern(pattern) => {
                let scope = collect_pattern_local_variables(pattern);
                local_scopes.push(scope);

                for element in &pattern.elements {
                    match element {
                        crate::ast::PathElement::Node(node) => {
                            if let Some(props) = &node.properties {
                                for pair in &props.properties {
                                    validate_projection_expression_bindings(
                                        &pair.value,
                                        known_bindings,
                                        local_scopes,
                                    )?;
                                }
                            }
                        }
                        crate::ast::PathElement::Relationship(rel) => {
                            if let Some(props) = &rel.properties {
                                for pair in &props.properties {
                                    validate_projection_expression_bindings(
                                        &pair.value,
                                        known_bindings,
                                        local_scopes,
                                    )?;
                                }
                            }
                        }
                    }
                }

                local_scopes.pop();
            }
            crate::ast::ExistsExpression::Subquery(_) => {
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
) -> Result<()> {
    let mut local_scopes: Vec<std::collections::HashSet<String>> = Vec::new();
    validate_projection_expression_bindings(expr, known_bindings, &mut local_scopes)
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
                    crate::ast::PathElement::Node(node) => {
                        if let Some(props) = &node.properties {
                            for pair in &props.properties {
                                validate_projection_expression_semantics(&pair.value, vars)?;
                            }
                        }
                    }
                    crate::ast::PathElement::Relationship(rel) => {
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
            crate::ast::ExistsExpression::Pattern(pattern) => {
                for element in &pattern.elements {
                    match element {
                        crate::ast::PathElement::Node(node) => {
                            if let Some(props) = &node.properties {
                                for pair in &props.properties {
                                    validate_projection_expression_semantics(&pair.value, vars)?;
                                }
                            }
                        }
                        crate::ast::PathElement::Relationship(rel) => {
                            if let Some(props) = &rel.properties {
                                for pair in &props.properties {
                                    validate_projection_expression_semantics(&pair.value, vars)?;
                                }
                            }
                        }
                    }
                }
            }
            crate::ast::ExistsExpression::Subquery(subquery) => {
                for clause in &subquery.clauses {
                    match clause {
                        crate::ast::Clause::Where(w) => {
                            validate_projection_expression_semantics(&w.expression, vars)?
                        }
                        crate::ast::Clause::With(w) => {
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
                        crate::ast::Clause::Return(r) => {
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

fn expression_uses_allowed_group_refs(
    expr: &Expression,
    grouping_keys: &[(Expression, String)],
    grouping_aliases: &std::collections::HashSet<String>,
    local_scopes: &mut Vec<std::collections::HashSet<String>>,
) -> bool {
    match expr {
        Expression::Literal(_) | Expression::Parameter(_) => true,
        Expression::Variable(v) => {
            if is_locally_bound(local_scopes, v) {
                return true;
            }
            grouping_aliases.contains(v)
                || grouping_keys.iter().any(|(group_expr, alias)| {
                    alias == v || matches!(group_expr, Expression::Variable(name) if name == v)
                })
        }
        Expression::PropertyAccess(pa) => {
            if is_locally_bound(local_scopes, &pa.variable) {
                return true;
            }
            let dotted = format!("{}.{}", pa.variable, pa.property);
            grouping_aliases.contains(&dotted) || grouping_keys.iter().any(|(group_expr, alias)| {
                alias == &dotted
                    || matches!(group_expr, Expression::PropertyAccess(group_pa) if group_pa == pa)
                    || matches!(group_expr, Expression::Variable(var) if var == &pa.variable)
            })
        }
        Expression::Unary(u) => expression_uses_allowed_group_refs(
            &u.operand,
            grouping_keys,
            grouping_aliases,
            local_scopes,
        ),
        Expression::Binary(b) => {
            expression_uses_allowed_group_refs(
                &b.left,
                grouping_keys,
                grouping_aliases,
                local_scopes,
            ) && expression_uses_allowed_group_refs(
                &b.right,
                grouping_keys,
                grouping_aliases,
                local_scopes,
            )
        }
        Expression::FunctionCall(call) => {
            if call.name.starts_with("__quant_") && call.args.len() == 3 {
                let Expression::Variable(variable) = &call.args[0] else {
                    return false;
                };
                if !expression_uses_allowed_group_refs(
                    &call.args[1],
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                ) {
                    return false;
                }
                let mut scope = std::collections::HashSet::new();
                scope.insert(variable.clone());
                local_scopes.push(scope);
                let ok = expression_uses_allowed_group_refs(
                    &call.args[2],
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                );
                local_scopes.pop();
                return ok;
            }
            call.args.iter().all(|arg| {
                expression_uses_allowed_group_refs(
                    arg,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )
            })
        }
        Expression::List(items) => items.iter().all(|item| {
            expression_uses_allowed_group_refs(item, grouping_keys, grouping_aliases, local_scopes)
        }),
        Expression::Map(map) => map.properties.iter().all(|pair| {
            expression_uses_allowed_group_refs(
                &pair.value,
                grouping_keys,
                grouping_aliases,
                local_scopes,
            )
        }),
        Expression::Case(case_expr) => {
            case_expr.expression.as_ref().map_or(true, |expr| {
                expression_uses_allowed_group_refs(
                    expr,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )
            }) && case_expr.when_clauses.iter().all(|(when_expr, then_expr)| {
                expression_uses_allowed_group_refs(
                    when_expr,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                ) && expression_uses_allowed_group_refs(
                    then_expr,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )
            }) && case_expr.else_expression.as_ref().map_or(true, |expr| {
                expression_uses_allowed_group_refs(
                    expr,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )
            })
        }
        Expression::ListComprehension(list_comp) => {
            if !expression_uses_allowed_group_refs(
                &list_comp.list,
                grouping_keys,
                grouping_aliases,
                local_scopes,
            ) {
                return false;
            }

            let mut scope = std::collections::HashSet::new();
            scope.insert(list_comp.variable.clone());
            local_scopes.push(scope);

            let ok = list_comp.where_expression.as_ref().map_or(true, |expr| {
                expression_uses_allowed_group_refs(
                    expr,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )
            }) && list_comp.map_expression.as_ref().map_or(true, |expr| {
                expression_uses_allowed_group_refs(
                    expr,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )
            });

            local_scopes.pop();
            ok
        }
        Expression::PatternComprehension(pattern_comp) => {
            let scope = collect_pattern_local_variables(&pattern_comp.pattern);
            local_scopes.push(scope);

            let ok = pattern_comp.where_expression.as_ref().map_or(true, |expr| {
                expression_uses_allowed_group_refs(
                    expr,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )
            }) && expression_uses_allowed_group_refs(
                &pattern_comp.projection,
                grouping_keys,
                grouping_aliases,
                local_scopes,
            );

            local_scopes.pop();
            ok
        }
        _ => false,
    }
}

fn validate_aggregate_mixed_expression(
    expr: &Expression,
    grouping_keys: &[(Expression, String)],
    grouping_aliases: &std::collections::HashSet<String>,
) -> Result<()> {
    if !contains_aggregate_expression(expr) {
        return Ok(());
    }

    let mut local_scopes: Vec<std::collections::HashSet<String>> = Vec::new();
    let valid = validate_aggregate_mixed_expression_impl(
        expr,
        grouping_keys,
        grouping_aliases,
        &mut local_scopes,
    )?;
    if valid {
        Ok(())
    } else {
        Err(Error::Other(
            "syntax error: AmbiguousAggregationExpression".to_string(),
        ))
    }
}

fn validate_aggregate_mixed_expression_impl(
    expr: &Expression,
    grouping_keys: &[(Expression, String)],
    grouping_aliases: &std::collections::HashSet<String>,
    local_scopes: &mut Vec<std::collections::HashSet<String>>,
) -> Result<bool> {
    if !contains_aggregate_expression(expr) {
        return Ok(expression_uses_allowed_group_refs(
            expr,
            grouping_keys,
            grouping_aliases,
            local_scopes,
        ));
    }

    match expr {
        Expression::FunctionCall(call) => {
            if parse_aggregate_function(call)?.is_some() {
                return Ok(true);
            }

            if call.name.starts_with("__quant_") && call.args.len() == 3 {
                let Expression::Variable(variable) = &call.args[0] else {
                    return Ok(false);
                };
                if !validate_aggregate_mixed_expression_impl(
                    &call.args[1],
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )? {
                    return Ok(false);
                }
                let mut scope = std::collections::HashSet::new();
                scope.insert(variable.clone());
                local_scopes.push(scope);
                let ok = validate_aggregate_mixed_expression_impl(
                    &call.args[2],
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )?;
                local_scopes.pop();
                return Ok(ok);
            }

            for arg in &call.args {
                if !validate_aggregate_mixed_expression_impl(
                    arg,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Expression::Binary(b) => Ok(validate_aggregate_mixed_expression_impl(
            &b.left,
            grouping_keys,
            grouping_aliases,
            local_scopes,
        )? && validate_aggregate_mixed_expression_impl(
            &b.right,
            grouping_keys,
            grouping_aliases,
            local_scopes,
        )?),
        Expression::Unary(u) => validate_aggregate_mixed_expression_impl(
            &u.operand,
            grouping_keys,
            grouping_aliases,
            local_scopes,
        ),
        Expression::List(items) => {
            for item in items {
                if !validate_aggregate_mixed_expression_impl(
                    item,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                if !validate_aggregate_mixed_expression_impl(
                    &pair.value,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Expression::Case(case_expr) => {
            if let Some(test_expr) = &case_expr.expression
                && !validate_aggregate_mixed_expression_impl(
                    test_expr,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )?
            {
                return Ok(false);
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                if !validate_aggregate_mixed_expression_impl(
                    when_expr,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )? {
                    return Ok(false);
                }
                if !validate_aggregate_mixed_expression_impl(
                    then_expr,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )? {
                    return Ok(false);
                }
            }
            if let Some(else_expr) = &case_expr.else_expression
                && !validate_aggregate_mixed_expression_impl(
                    else_expr,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )?
            {
                return Ok(false);
            }
            Ok(true)
        }
        Expression::ListComprehension(list_comp) => {
            if !validate_aggregate_mixed_expression_impl(
                &list_comp.list,
                grouping_keys,
                grouping_aliases,
                local_scopes,
            )? {
                return Ok(false);
            }

            let mut scope = std::collections::HashSet::new();
            scope.insert(list_comp.variable.clone());
            local_scopes.push(scope);

            let ok = list_comp
                .where_expression
                .as_ref()
                .map_or(Ok(true), |expr| {
                    validate_aggregate_mixed_expression_impl(
                        expr,
                        grouping_keys,
                        grouping_aliases,
                        local_scopes,
                    )
                })?
                && list_comp.map_expression.as_ref().map_or(Ok(true), |expr| {
                    validate_aggregate_mixed_expression_impl(
                        expr,
                        grouping_keys,
                        grouping_aliases,
                        local_scopes,
                    )
                })?;

            local_scopes.pop();
            Ok(ok)
        }
        Expression::PatternComprehension(pattern_comp) => {
            let scope = collect_pattern_local_variables(&pattern_comp.pattern);
            local_scopes.push(scope);

            let ok = pattern_comp
                .where_expression
                .as_ref()
                .map_or(Ok(true), |expr| {
                    validate_aggregate_mixed_expression_impl(
                        expr,
                        grouping_keys,
                        grouping_aliases,
                        local_scopes,
                    )
                })?
                && validate_aggregate_mixed_expression_impl(
                    &pattern_comp.projection,
                    grouping_keys,
                    grouping_aliases,
                    local_scopes,
                )?;

            local_scopes.pop();
            Ok(ok)
        }
        _ => Ok(expression_uses_allowed_group_refs(
            expr,
            grouping_keys,
            grouping_aliases,
            local_scopes,
        )),
    }
}

fn is_locally_bound(local_scopes: &[std::collections::HashSet<String>], var: &str) -> bool {
    local_scopes.iter().rev().any(|scope| scope.contains(var))
}

fn collect_pattern_local_variables(
    pattern: &crate::ast::Pattern,
) -> std::collections::HashSet<String> {
    let mut vars = std::collections::HashSet::new();
    if let Some(path_var) = &pattern.variable {
        vars.insert(path_var.clone());
    }
    for element in &pattern.elements {
        match element {
            crate::ast::PathElement::Node(node) => {
                if let Some(var) = &node.variable {
                    vars.insert(var.clone());
                }
            }
            crate::ast::PathElement::Relationship(rel) => {
                if let Some(var) = &rel.variable {
                    vars.insert(var.clone());
                }
            }
        }
    }
    vars
}

fn collect_aggregate_calls(
    expr: &Expression,
    out: &mut Vec<(Expression, crate::ast::AggregateFunction, String)>,
) -> Result<()> {
    match expr {
        Expression::FunctionCall(call) => {
            if let Some(agg) = parse_aggregate_function(call)? {
                for arg in &call.args {
                    if contains_aggregate_expression(arg) {
                        return Err(Error::Other("syntax error: NestedAggregation".to_string()));
                    }
                    if contains_function_call_named(arg, "rand") {
                        return Err(Error::Other(
                            "syntax error: NonConstantExpression".to_string(),
                        ));
                    }
                }

                if !out.iter().any(|(existing, _, _)| existing == expr) {
                    let alias = format!("__agg_{}", out.len());
                    out.push((expr.clone(), agg, alias));
                }
                return Ok(());
            }

            for arg in &call.args {
                collect_aggregate_calls(arg, out)?;
            }
            Ok(())
        }
        Expression::Binary(b) => {
            collect_aggregate_calls(&b.left, out)?;
            collect_aggregate_calls(&b.right, out)
        }
        Expression::Unary(u) => collect_aggregate_calls(&u.operand, out),
        Expression::List(items) => {
            for item in items {
                collect_aggregate_calls(item, out)?;
            }
            Ok(())
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                collect_aggregate_calls(&pair.value, out)?;
            }
            Ok(())
        }
        Expression::Case(case_expr) => {
            if let Some(test_expr) = &case_expr.expression {
                collect_aggregate_calls(test_expr, out)?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                collect_aggregate_calls(when_expr, out)?;
                collect_aggregate_calls(then_expr, out)?;
            }
            if let Some(else_expr) = &case_expr.else_expression {
                collect_aggregate_calls(else_expr, out)?;
            }
            Ok(())
        }
        Expression::ListComprehension(list_comp) => {
            collect_aggregate_calls(&list_comp.list, out)?;
            if let Some(where_expr) = &list_comp.where_expression {
                collect_aggregate_calls(where_expr, out)?;
            }
            if let Some(map_expr) = &list_comp.map_expression {
                collect_aggregate_calls(map_expr, out)?;
            }
            Ok(())
        }
        Expression::PatternComprehension(pattern_comp) => {
            for element in &pattern_comp.pattern.elements {
                match element {
                    crate::ast::PathElement::Node(node) => {
                        if let Some(props) = &node.properties {
                            for pair in &props.properties {
                                collect_aggregate_calls(&pair.value, out)?;
                            }
                        }
                    }
                    crate::ast::PathElement::Relationship(rel) => {
                        if let Some(props) = &rel.properties {
                            for pair in &props.properties {
                                collect_aggregate_calls(&pair.value, out)?;
                            }
                        }
                    }
                }
            }
            if let Some(where_expr) = &pattern_comp.where_expression {
                collect_aggregate_calls(where_expr, out)?;
            }
            collect_aggregate_calls(&pattern_comp.projection, out)?;
            Ok(())
        }
        _ => Ok(()),
    }
}

fn resolve_aggregate_alias(expr: &Expression, mappings: &[(Expression, String)]) -> Option<String> {
    mappings
        .iter()
        .find_map(|(aggregate_expr, alias)| (aggregate_expr == expr).then(|| alias.clone()))
}

fn rewrite_aggregate_references(
    expr: &Expression,
    mappings: &[(Expression, String)],
) -> Expression {
    if let Some(alias) = resolve_aggregate_alias(expr, mappings) {
        return Expression::Variable(alias);
    }

    match expr {
        Expression::Binary(b) => Expression::Binary(Box::new(crate::ast::BinaryExpression {
            left: rewrite_aggregate_references(&b.left, mappings),
            operator: b.operator.clone(),
            right: rewrite_aggregate_references(&b.right, mappings),
        })),
        Expression::Unary(u) => Expression::Unary(Box::new(crate::ast::UnaryExpression {
            operator: u.operator.clone(),
            operand: rewrite_aggregate_references(&u.operand, mappings),
        })),
        Expression::FunctionCall(call) => Expression::FunctionCall(crate::ast::FunctionCall {
            name: call.name.clone(),
            args: call
                .args
                .iter()
                .map(|arg| rewrite_aggregate_references(arg, mappings))
                .collect(),
        }),
        Expression::List(items) => Expression::List(
            items
                .iter()
                .map(|item| rewrite_aggregate_references(item, mappings))
                .collect(),
        ),
        Expression::ListComprehension(list_comp) => {
            Expression::ListComprehension(Box::new(crate::ast::ListComprehension {
                variable: list_comp.variable.clone(),
                list: rewrite_aggregate_references(&list_comp.list, mappings),
                where_expression: list_comp
                    .where_expression
                    .as_ref()
                    .map(|expr| rewrite_aggregate_references(expr, mappings)),
                map_expression: list_comp
                    .map_expression
                    .as_ref()
                    .map(|expr| rewrite_aggregate_references(expr, mappings)),
            }))
        }
        Expression::PatternComprehension(pattern_comp) => {
            Expression::PatternComprehension(Box::new(crate::ast::PatternComprehension {
                pattern: pattern_comp.pattern.clone(),
                where_expression: pattern_comp
                    .where_expression
                    .as_ref()
                    .map(|expr| rewrite_aggregate_references(expr, mappings)),
                projection: rewrite_aggregate_references(&pattern_comp.projection, mappings),
            }))
        }
        Expression::Map(map) => Expression::Map(crate::ast::PropertyMap {
            properties: map
                .properties
                .iter()
                .map(|pair| crate::ast::PropertyPair {
                    key: pair.key.clone(),
                    value: rewrite_aggregate_references(&pair.value, mappings),
                })
                .collect(),
        }),
        Expression::Case(case_expr) => Expression::Case(Box::new(crate::ast::CaseExpression {
            expression: case_expr
                .expression
                .as_ref()
                .map(|expr| rewrite_aggregate_references(expr, mappings)),
            when_clauses: case_expr
                .when_clauses
                .iter()
                .map(|(when_expr, then_expr)| {
                    (
                        rewrite_aggregate_references(when_expr, mappings),
                        rewrite_aggregate_references(then_expr, mappings),
                    )
                })
                .collect(),
            else_expression: case_expr
                .else_expression
                .as_ref()
                .map(|expr| rewrite_aggregate_references(expr, mappings)),
        })),
        _ => expr.clone(),
    }
}

fn rewrite_group_key_references(
    expr: &Expression,
    grouping_keys: &[(Expression, String)],
) -> Expression {
    if let Some(alias) = grouping_keys
        .iter()
        .find_map(|(group_expr, alias)| (group_expr == expr).then(|| alias.clone()))
    {
        return Expression::Variable(alias);
    }

    match expr {
        Expression::Binary(b) => Expression::Binary(Box::new(crate::ast::BinaryExpression {
            left: rewrite_group_key_references(&b.left, grouping_keys),
            operator: b.operator.clone(),
            right: rewrite_group_key_references(&b.right, grouping_keys),
        })),
        Expression::Unary(u) => Expression::Unary(Box::new(crate::ast::UnaryExpression {
            operator: u.operator.clone(),
            operand: rewrite_group_key_references(&u.operand, grouping_keys),
        })),
        Expression::FunctionCall(call) => Expression::FunctionCall(crate::ast::FunctionCall {
            name: call.name.clone(),
            args: call
                .args
                .iter()
                .map(|arg| rewrite_group_key_references(arg, grouping_keys))
                .collect(),
        }),
        Expression::List(items) => Expression::List(
            items
                .iter()
                .map(|item| rewrite_group_key_references(item, grouping_keys))
                .collect(),
        ),
        Expression::ListComprehension(list_comp) => {
            Expression::ListComprehension(Box::new(crate::ast::ListComprehension {
                variable: list_comp.variable.clone(),
                list: rewrite_group_key_references(&list_comp.list, grouping_keys),
                where_expression: list_comp
                    .where_expression
                    .as_ref()
                    .map(|expr| rewrite_group_key_references(expr, grouping_keys)),
                map_expression: list_comp
                    .map_expression
                    .as_ref()
                    .map(|expr| rewrite_group_key_references(expr, grouping_keys)),
            }))
        }
        Expression::PatternComprehension(pattern_comp) => {
            Expression::PatternComprehension(Box::new(crate::ast::PatternComprehension {
                pattern: pattern_comp.pattern.clone(),
                where_expression: pattern_comp
                    .where_expression
                    .as_ref()
                    .map(|expr| rewrite_group_key_references(expr, grouping_keys)),
                projection: rewrite_group_key_references(&pattern_comp.projection, grouping_keys),
            }))
        }
        Expression::Map(map) => Expression::Map(crate::ast::PropertyMap {
            properties: map
                .properties
                .iter()
                .map(|pair| crate::ast::PropertyPair {
                    key: pair.key.clone(),
                    value: rewrite_group_key_references(&pair.value, grouping_keys),
                })
                .collect(),
        }),
        Expression::Case(case_expr) => Expression::Case(Box::new(crate::ast::CaseExpression {
            expression: case_expr
                .expression
                .as_ref()
                .map(|expr| rewrite_group_key_references(expr, grouping_keys)),
            when_clauses: case_expr
                .when_clauses
                .iter()
                .map(|(when_expr, then_expr)| {
                    (
                        rewrite_group_key_references(when_expr, grouping_keys),
                        rewrite_group_key_references(then_expr, grouping_keys),
                    )
                })
                .collect(),
            else_expression: case_expr
                .else_expression
                .as_ref()
                .map(|expr| rewrite_group_key_references(expr, grouping_keys)),
        })),
        _ => expr.clone(),
    }
}

pub(super) fn compile_projection_aggregation(
    input: Plan,
    items: &[crate::ast::ReturnItem],
    allow_empty_scope_with_star: bool,
) -> Result<(Plan, Vec<String>)> {
    let mut input_bindings = BTreeMap::new();
    extract_output_var_kinds(&input, &mut input_bindings);
    for item in items {
        ensure_no_pattern_predicate(&item.expression)?;
        validate_projection_bindings_root(&item.expression, &input_bindings)?;
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

    let mut resolved_items: Vec<(Expression, String, bool)> = Vec::with_capacity(items.len());
    let mut has_aggregation = false;
    for (i, item) in items.iter().enumerate() {
        let contains_agg = contains_aggregate_expression(&item.expression);
        if contains_agg {
            has_aggregation = true;
        }

        let alias = if let Some(alias) = &item.alias {
            alias.clone()
        } else if let Expression::FunctionCall(call) = &item.expression {
            if parse_aggregate_function(call)?.is_some() {
                default_aggregate_alias(call, i)
            } else {
                default_projection_alias(&item.expression, i)
            }
        } else {
            default_projection_alias(&item.expression, i)
        };

        resolved_items.push((item.expression.clone(), alias, contains_agg));
    }

    let mut seen_aliases = std::collections::HashSet::new();
    for (_, alias, _) in &resolved_items {
        if !seen_aliases.insert(alias.clone()) {
            return Err(Error::Other("syntax error: ColumnNameConflict".to_string()));
        }
    }

    if !has_aggregation {
        let projections: Vec<(String, Expression)> = resolved_items
            .iter()
            .map(|(expr, alias, _)| (alias.clone(), expr.clone()))
            .collect();
        let project_cols: Vec<String> = resolved_items
            .iter()
            .map(|(_, alias, _)| alias.clone())
            .collect();

        return Ok((
            Plan::Project {
                input: Box::new(input),
                projections,
            },
            project_cols,
        ));
    }

    let mut pre_projections: Vec<(String, Expression)> = Vec::new();
    let mut projected_aliases = std::collections::HashSet::new();
    let mut grouping_keys: Vec<(Expression, String)> = Vec::new();
    let mut grouping_aliases = std::collections::HashSet::new();
    let mut group_by: Vec<String> = Vec::new();

    for (expr, alias, contains_agg) in &resolved_items {
        if *contains_agg {
            continue;
        }

        if !projected_aliases.contains(alias) {
            pre_projections.push((alias.clone(), expr.clone()));
            projected_aliases.insert(alias.clone());
        }

        group_by.push(alias.clone());
        grouping_aliases.insert(alias.clone());
        if is_simple_group_expression(expr) {
            grouping_keys.push((expr.clone(), alias.clone()));
        }
    }

    let mut aggregate_exprs: Vec<(Expression, crate::ast::AggregateFunction, String)> = Vec::new();
    for (expr, _alias, contains_agg) in &resolved_items {
        if !*contains_agg {
            continue;
        }

        validate_aggregate_mixed_expression(expr, &grouping_keys, &grouping_aliases)?;
        collect_aggregate_calls(expr, &mut aggregate_exprs)?;
    }

    if aggregate_exprs.is_empty() {
        return Err(Error::Other("syntax error: InvalidAggregation".to_string()));
    }

    for (_, agg, _) in &aggregate_exprs {
        match agg {
            crate::ast::AggregateFunction::Count(None) => {}
            crate::ast::AggregateFunction::Count(Some(expr))
            | crate::ast::AggregateFunction::CountDistinct(expr)
            | crate::ast::AggregateFunction::Sum(expr)
            | crate::ast::AggregateFunction::SumDistinct(expr)
            | crate::ast::AggregateFunction::Avg(expr)
            | crate::ast::AggregateFunction::AvgDistinct(expr)
            | crate::ast::AggregateFunction::Min(expr)
            | crate::ast::AggregateFunction::MinDistinct(expr)
            | crate::ast::AggregateFunction::Max(expr)
            | crate::ast::AggregateFunction::MaxDistinct(expr)
            | crate::ast::AggregateFunction::Collect(expr)
            | crate::ast::AggregateFunction::CollectDistinct(expr) => {
                let mut deps = std::collections::HashSet::new();
                extract_variables_from_expr(expr, &mut deps);
                for dep in deps {
                    if !projected_aliases.contains(&dep) {
                        pre_projections.push((dep.clone(), Expression::Variable(dep.clone())));
                        projected_aliases.insert(dep);
                    }
                }
            }
            crate::ast::AggregateFunction::PercentileDisc(value_expr, percentile_expr)
            | crate::ast::AggregateFunction::PercentileCont(value_expr, percentile_expr) => {
                let mut deps = std::collections::HashSet::new();
                extract_variables_from_expr(value_expr, &mut deps);
                extract_variables_from_expr(percentile_expr, &mut deps);
                for dep in deps {
                    if !projected_aliases.contains(&dep) {
                        pre_projections.push((dep.clone(), Expression::Variable(dep.clone())));
                        projected_aliases.insert(dep);
                    }
                }
            }
        }
    }

    let aggregate_aliases: Vec<(Expression, String)> = aggregate_exprs
        .iter()
        .map(|(expr, _, alias)| (expr.clone(), alias.clone()))
        .collect();

    let mut final_projections: Vec<(String, Expression)> = Vec::new();
    let mut project_cols: Vec<String> = Vec::new();
    for (expr, alias, contains_agg) in resolved_items {
        let final_expr = if contains_agg {
            let rewritten = rewrite_aggregate_references(&expr, &aggregate_aliases);
            rewrite_group_key_references(&rewritten, &grouping_keys)
        } else {
            Expression::Variable(alias.clone())
        };

        project_cols.push(alias.clone());
        final_projections.push((alias, final_expr));
    }

    let plan = if !pre_projections.is_empty() {
        Plan::Project {
            input: Box::new(input),
            projections: pre_projections,
        }
    } else {
        input
    };

    let plan = Plan::Aggregate {
        input: Box::new(plan),
        group_by,
        aggregates: aggregate_exprs
            .into_iter()
            .map(|(_, aggregate, alias)| (aggregate, alias))
            .collect(),
    };

    let plan = Plan::Project {
        input: Box::new(plan),
        projections: final_projections,
    };

    Ok((plan, project_cols))
}

pub(super) fn validate_order_by_scope(
    order_by: &crate::ast::OrderByClause,
    project_cols: &[String],
    projection_items: &[crate::ast::ReturnItem],
    strict_project_scope: bool,
) -> Result<()> {
    let mut scope: std::collections::HashSet<String> = project_cols.iter().cloned().collect();
    if !strict_project_scope {
        for item in projection_items {
            extract_variables_from_expr(&item.expression, &mut scope);
        }
    }

    for item in &order_by.items {
        if contains_aggregate_expression(&item.expression) {
            let aggregate_is_projected = projection_items.iter().any(|projection| {
                contains_aggregate_expression(&projection.expression)
                    && projection.expression == item.expression
            });
            if !aggregate_is_projected {
                return Err(Error::Other("syntax error: InvalidAggregation".to_string()));
            }
            // When sorting by an already projected aggregate expression, variable scope checks
            // should not run against aggregate internals (e.g. max(n.age)).
            continue;
        }

        let mut used = std::collections::HashSet::new();
        extract_variables_from_expr(&item.expression, &mut used);
        for var in used {
            if !scope.contains(&var) {
                return Err(Error::Other(format!(
                    "syntax error: UndefinedVariable ({})",
                    var
                )));
            }
        }
    }

    Ok(())
}

pub(super) fn validate_order_by_aggregate_semantics(
    order_by: &crate::ast::OrderByClause,
    projection_items: &[crate::ast::ReturnItem],
) -> Result<()> {
    let mut grouping_keys: Vec<(Expression, String)> = Vec::new();
    let mut grouping_aliases = std::collections::HashSet::new();

    for (idx, item) in projection_items.iter().enumerate() {
        if contains_aggregate_expression(&item.expression) {
            continue;
        }

        let alias = item
            .alias
            .clone()
            .unwrap_or_else(|| default_projection_alias(&item.expression, idx));
        grouping_aliases.insert(alias.clone());
        if is_simple_group_expression(&item.expression) {
            grouping_keys.push((item.expression.clone(), alias));
        }
    }

    for item in &order_by.items {
        if contains_aggregate_expression(&item.expression) {
            validate_aggregate_mixed_expression(
                &item.expression,
                &grouping_keys,
                &grouping_aliases,
            )?;
        }
    }

    Ok(())
}

pub(super) fn rewrite_order_expression(
    expr: &Expression,
    bindings: &[(Expression, String)],
) -> Expression {
    for (pattern, alias) in bindings {
        if expr == pattern {
            return Expression::Variable(alias.clone());
        }
    }

    match expr {
        Expression::Binary(b) => Expression::Binary(Box::new(crate::ast::BinaryExpression {
            left: rewrite_order_expression(&b.left, bindings),
            operator: b.operator.clone(),
            right: rewrite_order_expression(&b.right, bindings),
        })),
        Expression::Unary(u) => Expression::Unary(Box::new(crate::ast::UnaryExpression {
            operator: u.operator.clone(),
            operand: rewrite_order_expression(&u.operand, bindings),
        })),
        Expression::FunctionCall(f) => Expression::FunctionCall(crate::ast::FunctionCall {
            name: f.name.clone(),
            args: f
                .args
                .iter()
                .map(|arg| rewrite_order_expression(arg, bindings))
                .collect(),
        }),
        Expression::List(items) => Expression::List(
            items
                .iter()
                .map(|item| rewrite_order_expression(item, bindings))
                .collect(),
        ),
        Expression::Map(map) => Expression::Map(crate::ast::PropertyMap {
            properties: map
                .properties
                .iter()
                .map(|pair| crate::ast::PropertyPair {
                    key: pair.key.clone(),
                    value: rewrite_order_expression(&pair.value, bindings),
                })
                .collect(),
        }),
        _ => expr.clone(),
    }
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
                        crate::ast::PathElement::Node(node) => {
                            node.properties.as_ref().is_some_and(|props| {
                                props
                                    .properties
                                    .iter()
                                    .any(|pair| contains_aggregate_expression(&pair.value))
                            })
                        }
                        crate::ast::PathElement::Relationship(rel) => {
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

pub(super) fn compile_order_by_items(
    order_by: &crate::ast::OrderByClause,
) -> Result<Vec<(Expression, crate::ast::Direction)>> {
    Ok(order_by
        .items
        .iter()
        .map(|item| (item.expression.clone(), item.direction.clone()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::validate_aggregate_mixed_expression;
    use crate::ast::{BinaryExpression, BinaryOperator, Expression, FunctionCall, Literal};

    #[test]
    fn quantifier_predicate_variable_is_treated_as_local_in_aggregation_validation() {
        let collect_expr = Expression::FunctionCall(FunctionCall {
            name: "collect".to_string(),
            args: vec![Expression::Literal(Literal::Boolean(true))],
        });
        let quantifier = Expression::FunctionCall(FunctionCall {
            name: "__quant_all".to_string(),
            args: vec![
                Expression::Variable("ok".to_string()),
                collect_expr,
                Expression::Binary(Box::new(BinaryExpression {
                    left: Expression::Variable("ok".to_string()),
                    operator: BinaryOperator::Equals,
                    right: Expression::Literal(Literal::Boolean(true)),
                })),
            ],
        });

        validate_aggregate_mixed_expression(&quantifier, &[], &std::collections::HashSet::new())
            .expect("quantifier-local variable should not trigger ambiguous aggregation");
    }
}

use super::{
    BTreeMap, BindingKind, Error, Expression, HashSet, Result, infer_expression_binding_kind,
};

fn is_quantifier_call(call: &crate::ast::FunctionCall) -> bool {
    matches!(
        call.name.as_str(),
        "__quant_any" | "__quant_all" | "__quant_none" | "__quant_single"
    )
}

pub(super) fn validate_where_expression_bindings(
    expr: &Expression,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> Result<()> {
    ensure_no_aggregation_functions(expr)?;
    let mut local_scopes: Vec<HashSet<String>> = Vec::new();
    validate_where_expression_variables(expr, known_bindings, &mut local_scopes)?;

    validate_pattern_predicate_bindings(expr, known_bindings)?;
    if matches!(
        infer_expression_binding_kind(expr, known_bindings),
        BindingKind::Node
            | BindingKind::Relationship
            | BindingKind::RelationshipList
            | BindingKind::Path
    ) {
        return Err(Error::Other(
            "syntax error: InvalidArgumentType".to_string(),
        ));
    }
    Ok(())
}

fn ensure_no_aggregation_functions(expr: &Expression) -> Result<()> {
    match expr {
        Expression::FunctionCall(call) => {
            if super::parse_aggregate_function(call)?.is_some() {
                return Err(Error::Other("syntax error: InvalidAggregation".to_string()));
            }
            for arg in &call.args {
                ensure_no_aggregation_functions(arg)?;
            }
        }
        Expression::Unary(u) => ensure_no_aggregation_functions(&u.operand)?,
        Expression::Binary(b) => {
            ensure_no_aggregation_functions(&b.left)?;
            ensure_no_aggregation_functions(&b.right)?;
        }
        Expression::List(items) => {
            for item in items {
                ensure_no_aggregation_functions(item)?;
            }
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                ensure_no_aggregation_functions(&pair.value)?;
            }
        }
        Expression::Case(case_expr) => {
            if let Some(test_expr) = &case_expr.expression {
                ensure_no_aggregation_functions(test_expr)?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                ensure_no_aggregation_functions(when_expr)?;
                ensure_no_aggregation_functions(then_expr)?;
            }
            if let Some(else_expr) = &case_expr.else_expression {
                ensure_no_aggregation_functions(else_expr)?;
            }
        }
        Expression::ListComprehension(list_comp) => {
            ensure_no_aggregation_functions(&list_comp.list)?;
            if let Some(where_expr) = &list_comp.where_expression {
                ensure_no_aggregation_functions(where_expr)?;
            }
            if let Some(map_expr) = &list_comp.map_expression {
                ensure_no_aggregation_functions(map_expr)?;
            }
        }
        Expression::PatternComprehension(pattern_comp) => {
            for element in &pattern_comp.pattern.elements {
                match element {
                    crate::ast::PathElement::Node(node) => {
                        if let Some(props) = &node.properties {
                            for pair in &props.properties {
                                ensure_no_aggregation_functions(&pair.value)?;
                            }
                        }
                    }
                    crate::ast::PathElement::Relationship(rel) => {
                        if let Some(props) = &rel.properties {
                            for pair in &props.properties {
                                ensure_no_aggregation_functions(&pair.value)?;
                            }
                        }
                    }
                }
            }
            if let Some(where_expr) = &pattern_comp.where_expression {
                ensure_no_aggregation_functions(where_expr)?;
            }
            ensure_no_aggregation_functions(&pattern_comp.projection)?;
        }
        Expression::Exists(exists_expr) => match exists_expr.as_ref() {
            crate::ast::ExistsExpression::Pattern(pattern) => {
                for element in &pattern.elements {
                    match element {
                        crate::ast::PathElement::Node(node) => {
                            if let Some(props) = &node.properties {
                                for pair in &props.properties {
                                    ensure_no_aggregation_functions(&pair.value)?;
                                }
                            }
                        }
                        crate::ast::PathElement::Relationship(rel) => {
                            if let Some(props) = &rel.properties {
                                for pair in &props.properties {
                                    ensure_no_aggregation_functions(&pair.value)?;
                                }
                            }
                        }
                    }
                }
            }
            crate::ast::ExistsExpression::Subquery(subquery) => {
                let _ = subquery;
                // Subquery internals are validated when compiling the nested query itself.
                // Rejecting aggregates here would incorrectly ban legal constructs like
                // `EXISTS { ... WITH n, count(*) AS c WHERE c > 0 ... }`.
            }
        },
        Expression::Variable(_)
        | Expression::PropertyAccess(_)
        | Expression::Parameter(_)
        | Expression::Literal(_) => {}
    }

    Ok(())
}

fn validate_where_expression_variables(
    expr: &Expression,
    known_bindings: &BTreeMap<String, BindingKind>,
    local_scopes: &mut Vec<HashSet<String>>,
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
            validate_where_expression_variables(&u.operand, known_bindings, local_scopes)?;
        }
        Expression::Binary(b) => {
            validate_where_expression_variables(&b.left, known_bindings, local_scopes)?;
            validate_where_expression_variables(&b.right, known_bindings, local_scopes)?;
        }
        Expression::FunctionCall(call) => {
            if is_quantifier_call(call) && call.args.len() == 3 {
                validate_where_expression_variables(&call.args[1], known_bindings, local_scopes)?;
                if let Expression::Variable(var) = &call.args[0] {
                    let mut scope = HashSet::new();
                    scope.insert(var.clone());
                    local_scopes.push(scope);
                    validate_where_expression_variables(
                        &call.args[2],
                        known_bindings,
                        local_scopes,
                    )?;
                    local_scopes.pop();
                } else {
                    validate_where_expression_variables(
                        &call.args[2],
                        known_bindings,
                        local_scopes,
                    )?;
                }
            } else {
                for arg in &call.args {
                    validate_where_expression_variables(arg, known_bindings, local_scopes)?;
                }
            }
        }
        Expression::List(items) => {
            for item in items {
                validate_where_expression_variables(item, known_bindings, local_scopes)?;
            }
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                validate_where_expression_variables(&pair.value, known_bindings, local_scopes)?;
            }
        }
        Expression::Case(case_expr) => {
            if let Some(test_expr) = &case_expr.expression {
                validate_where_expression_variables(test_expr, known_bindings, local_scopes)?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                validate_where_expression_variables(when_expr, known_bindings, local_scopes)?;
                validate_where_expression_variables(then_expr, known_bindings, local_scopes)?;
            }
            if let Some(else_expr) = &case_expr.else_expression {
                validate_where_expression_variables(else_expr, known_bindings, local_scopes)?;
            }
        }
        Expression::ListComprehension(list_comp) => {
            validate_where_expression_variables(&list_comp.list, known_bindings, local_scopes)?;
            let mut scope = HashSet::new();
            scope.insert(list_comp.variable.clone());
            local_scopes.push(scope);
            if let Some(where_expr) = &list_comp.where_expression {
                validate_where_expression_variables(where_expr, known_bindings, local_scopes)?;
            }
            if let Some(map_expr) = &list_comp.map_expression {
                validate_where_expression_variables(map_expr, known_bindings, local_scopes)?;
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
                                validate_where_expression_variables(
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
                                validate_where_expression_variables(
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
                validate_where_expression_variables(where_expr, known_bindings, local_scopes)?;
            }
            validate_where_expression_variables(
                &pattern_comp.projection,
                known_bindings,
                local_scopes,
            )?;

            local_scopes.pop();
        }
        Expression::Exists(_) | Expression::Parameter(_) | Expression::Literal(_) => {}
    }
    Ok(())
}

fn is_locally_bound(local_scopes: &[HashSet<String>], var: &str) -> bool {
    local_scopes.iter().rev().any(|scope| scope.contains(var))
}

fn collect_pattern_local_variables(pattern: &crate::ast::Pattern) -> HashSet<String> {
    let mut vars = HashSet::new();
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

fn validate_pattern_predicate_bindings(
    expr: &Expression,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> Result<()> {
    match expr {
        Expression::Exists(exists_expr) => match exists_expr.as_ref() {
            crate::ast::ExistsExpression::Pattern(pattern) => {
                if pattern.elements.len() < 3 {
                    return Err(Error::Other(
                        "syntax error: InvalidArgumentType".to_string(),
                    ));
                }
                if let Some(path_var) = &pattern.variable
                    && !known_bindings.contains_key(path_var)
                {
                    return Err(Error::Other(format!(
                        "syntax error: UndefinedVariable ({})",
                        path_var
                    )));
                }
                for element in &pattern.elements {
                    match element {
                        crate::ast::PathElement::Node(node) => {
                            if let Some(var) = &node.variable
                                && !known_bindings.contains_key(var)
                            {
                                return Err(Error::Other(format!(
                                    "syntax error: UndefinedVariable ({})",
                                    var
                                )));
                            }
                            if let Some(props) = &node.properties {
                                for pair in &props.properties {
                                    validate_pattern_predicate_bindings(
                                        &pair.value,
                                        known_bindings,
                                    )?;
                                }
                            }
                        }
                        crate::ast::PathElement::Relationship(rel) => {
                            if let Some(var) = &rel.variable
                                && !known_bindings.contains_key(var)
                            {
                                return Err(Error::Other(format!(
                                    "syntax error: UndefinedVariable ({})",
                                    var
                                )));
                            }
                            if let Some(props) = &rel.properties {
                                for pair in &props.properties {
                                    validate_pattern_predicate_bindings(
                                        &pair.value,
                                        known_bindings,
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
            crate::ast::ExistsExpression::Subquery(subquery) => {
                let _ = subquery;
                // Variables introduced inside EXISTS subqueries are scoped to that subquery.
                // Recursing with the outer `known_bindings` causes false UndefinedVariable errors.
            }
        },
        Expression::Unary(u) => validate_pattern_predicate_bindings(&u.operand, known_bindings)?,
        Expression::Binary(b) => {
            validate_pattern_predicate_bindings(&b.left, known_bindings)?;
            validate_pattern_predicate_bindings(&b.right, known_bindings)?;
        }
        Expression::FunctionCall(call) => {
            if is_quantifier_call(call) && call.args.len() == 3 {
                validate_pattern_predicate_bindings(&call.args[1], known_bindings)?;
                if let Expression::Variable(var) = &call.args[0] {
                    let mut scoped = known_bindings.clone();
                    scoped.entry(var.clone()).or_insert(BindingKind::Unknown);
                    validate_pattern_predicate_bindings(&call.args[2], &scoped)?;
                } else {
                    validate_pattern_predicate_bindings(&call.args[2], known_bindings)?;
                }
            } else {
                for arg in &call.args {
                    validate_pattern_predicate_bindings(arg, known_bindings)?;
                }
            }
        }
        Expression::List(items) => {
            for item in items {
                validate_pattern_predicate_bindings(item, known_bindings)?;
            }
        }
        Expression::ListComprehension(list_comp) => {
            validate_pattern_predicate_bindings(&list_comp.list, known_bindings)?;
            if let Some(where_expr) = &list_comp.where_expression {
                validate_pattern_predicate_bindings(where_expr, known_bindings)?;
            }
            if let Some(map_expr) = &list_comp.map_expression {
                validate_pattern_predicate_bindings(map_expr, known_bindings)?;
            }
        }
        Expression::PatternComprehension(pattern_comp) => {
            let mut scoped = known_bindings.clone();
            for var in collect_pattern_local_variables(&pattern_comp.pattern) {
                scoped.entry(var).or_insert(BindingKind::Unknown);
            }

            for element in &pattern_comp.pattern.elements {
                match element {
                    crate::ast::PathElement::Node(node) => {
                        if let Some(props) = &node.properties {
                            for pair in &props.properties {
                                validate_pattern_predicate_bindings(&pair.value, &scoped)?;
                            }
                        }
                    }
                    crate::ast::PathElement::Relationship(rel) => {
                        if let Some(props) = &rel.properties {
                            for pair in &props.properties {
                                validate_pattern_predicate_bindings(&pair.value, &scoped)?;
                            }
                        }
                    }
                }
            }

            if let Some(where_expr) = &pattern_comp.where_expression {
                validate_pattern_predicate_bindings(where_expr, &scoped)?;
            }
            validate_pattern_predicate_bindings(&pattern_comp.projection, &scoped)?;
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                validate_pattern_predicate_bindings(&pair.value, known_bindings)?;
            }
        }
        Expression::Case(case_expr) => {
            if let Some(test_expr) = &case_expr.expression {
                validate_pattern_predicate_bindings(test_expr, known_bindings)?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                validate_pattern_predicate_bindings(when_expr, known_bindings)?;
                validate_pattern_predicate_bindings(then_expr, known_bindings)?;
            }
            if let Some(else_expr) = &case_expr.else_expression {
                validate_pattern_predicate_bindings(else_expr, known_bindings)?;
            }
        }
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_where_expression_bindings;
    use crate::ast::{
        BinaryExpression, BinaryOperator, Expression, FunctionCall, Literal, PropertyAccess,
    };
    use std::collections::BTreeMap;

    #[test]
    fn quantifier_variable_is_scoped_inside_where_validation() {
        let expr = Expression::FunctionCall(FunctionCall {
            name: "__quant_any".to_string(),
            args: vec![
                Expression::Variable("x".to_string()),
                Expression::Variable("list".to_string()),
                Expression::Binary(Box::new(BinaryExpression {
                    left: Expression::Variable("x".to_string()),
                    operator: BinaryOperator::Equals,
                    right: Expression::Literal(Literal::Integer(2)),
                })),
            ],
        });
        let mut known = BTreeMap::new();
        known.insert("list".to_string(), super::BindingKind::Unknown);
        validate_where_expression_bindings(&expr, &known)
            .expect("quantifier variable should be treated as local scope");
    }

    #[test]
    fn quantifier_predicate_still_rejects_unknown_outer_variable() {
        let expr = Expression::FunctionCall(FunctionCall {
            name: "__quant_any".to_string(),
            args: vec![
                Expression::Variable("x".to_string()),
                Expression::Variable("list".to_string()),
                Expression::Binary(Box::new(BinaryExpression {
                    left: Expression::Variable("y".to_string()),
                    operator: BinaryOperator::Equals,
                    right: Expression::Literal(Literal::Integer(2)),
                })),
            ],
        });
        let mut known = BTreeMap::new();
        known.insert("list".to_string(), super::BindingKind::Unknown);
        let err = validate_where_expression_bindings(&expr, &known)
            .expect_err("unknown variable should still be rejected");
        assert_eq!(err.to_string(), "syntax error: UndefinedVariable (y)");
    }

    #[test]
    fn rejects_path_property_access_in_where() {
        let expr = Expression::PropertyAccess(PropertyAccess {
            variable: "p".to_string(),
            property: "name".to_string(),
        });
        let mut known = BTreeMap::new();
        known.insert("p".to_string(), super::BindingKind::Path);
        let err = validate_where_expression_bindings(&expr, &known)
            .expect_err("path property access should be rejected");
        assert_eq!(err.to_string(), "syntax error: InvalidArgumentType");
    }

    #[test]
    fn rejects_aggregation_function_in_where() {
        let expr = Expression::Binary(Box::new(BinaryExpression {
            left: Expression::FunctionCall(FunctionCall {
                name: "count".to_string(),
                args: vec![Expression::Variable("a".to_string())],
            }),
            operator: BinaryOperator::GreaterThan,
            right: Expression::Literal(Literal::Integer(10)),
        }));
        let mut known = BTreeMap::new();
        known.insert("a".to_string(), super::BindingKind::Node);
        let err = validate_where_expression_bindings(&expr, &known)
            .expect_err("aggregation in WHERE should be rejected");
        assert_eq!(err.to_string(), "syntax error: InvalidAggregation");
    }
}

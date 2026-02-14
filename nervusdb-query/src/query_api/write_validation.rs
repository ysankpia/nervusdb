use super::{
    BTreeMap, BinaryOperator, BindingKind, Error, Expression, Result, extract_variables_from_expr,
};

pub(super) fn validate_delete_expression(
    expr: &Expression,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> Result<()> {
    let mut refs = std::collections::HashSet::new();
    extract_variables_from_expr(expr, &mut refs);
    let mut refs: Vec<_> = refs.into_iter().collect();
    refs.sort();

    for var in refs {
        if !known_bindings.contains_key(&var) {
            return Err(Error::Other(format!(
                "syntax error: UndefinedVariable ({})",
                var
            )));
        }
    }

    if contains_delete_label_predicate(expr) {
        return Err(Error::Other("syntax error: InvalidDelete".to_string()));
    }

    if !delete_expression_may_yield_entity(expr, known_bindings) {
        return Err(Error::Other(
            "syntax error: InvalidArgumentType".to_string(),
        ));
    }

    Ok(())
}

pub(super) fn validate_create_property_vars(
    props: &Option<crate::ast::PropertyMap>,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> Result<()> {
    if let Some(properties) = props {
        for prop in &properties.properties {
            let mut refs = std::collections::HashSet::new();
            extract_variables_from_expr(&prop.value, &mut refs);
            for var in refs {
                if !known_bindings.contains_key(&var) {
                    return Err(Error::Other(format!(
                        "syntax error: UndefinedVariable ({})",
                        var
                    )));
                }
            }
        }
    }
    Ok(())
}

fn contains_delete_label_predicate(expr: &Expression) -> bool {
    match expr {
        Expression::Binary(bin) => {
            matches!(bin.operator, BinaryOperator::HasLabel)
                || contains_delete_label_predicate(&bin.left)
                || contains_delete_label_predicate(&bin.right)
        }
        Expression::Unary(unary) => contains_delete_label_predicate(&unary.operand),
        Expression::FunctionCall(call) => call.args.iter().any(contains_delete_label_predicate),
        Expression::Case(case_expr) => {
            case_expr
                .expression
                .as_ref()
                .is_some_and(contains_delete_label_predicate)
                || case_expr.when_clauses.iter().any(|(when_expr, then_expr)| {
                    contains_delete_label_predicate(when_expr)
                        || contains_delete_label_predicate(then_expr)
                })
                || case_expr
                    .else_expression
                    .as_ref()
                    .is_some_and(contains_delete_label_predicate)
        }
        Expression::List(items) => items.iter().any(contains_delete_label_predicate),
        Expression::ListComprehension(comp) => {
            contains_delete_label_predicate(&comp.list)
                || comp
                    .where_expression
                    .as_ref()
                    .is_some_and(contains_delete_label_predicate)
                || comp
                    .map_expression
                    .as_ref()
                    .is_some_and(contains_delete_label_predicate)
        }
        Expression::PatternComprehension(comp) => {
            comp.where_expression
                .as_ref()
                .is_some_and(contains_delete_label_predicate)
                || contains_delete_label_predicate(&comp.projection)
        }
        Expression::Map(map) => map
            .properties
            .iter()
            .any(|pair| contains_delete_label_predicate(&pair.value)),
        _ => false,
    }
}

fn delete_expression_may_yield_entity(
    expr: &Expression,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> bool {
    match expr {
        Expression::Variable(name) => matches!(
            known_bindings.get(name),
            Some(BindingKind::Node)
                | Some(BindingKind::Relationship)
                | Some(BindingKind::RelationshipList)
                | Some(BindingKind::Path)
                | Some(BindingKind::Unknown)
        ),
        Expression::PropertyAccess(pa) => match known_bindings.get(&pa.variable) {
            Some(BindingKind::Node)
            | Some(BindingKind::Relationship)
            | Some(BindingKind::RelationshipList) => false,
            Some(_) => true,
            None => false,
        },
        Expression::FunctionCall(call) => {
            let allows_entity_passthrough = call.name.eq_ignore_ascii_case("coalesce")
                || call.name.eq_ignore_ascii_case("head")
                || call.name.eq_ignore_ascii_case("last")
                || call.name == "__getprop"
                || call.name == "__index"
                || call.name == "__slice";

            allows_entity_passthrough
                && call
                    .args
                    .iter()
                    .any(|arg| delete_expression_may_yield_entity(arg, known_bindings))
        }
        Expression::Case(case_expr) => {
            case_expr
                .when_clauses
                .iter()
                .any(|(_, then_expr)| delete_expression_may_yield_entity(then_expr, known_bindings))
                || case_expr
                    .else_expression
                    .as_ref()
                    .is_some_and(|expr| delete_expression_may_yield_entity(expr, known_bindings))
        }
        Expression::List(items) => items
            .iter()
            .any(|item| delete_expression_may_yield_entity(item, known_bindings)),
        Expression::ListComprehension(comp) => {
            comp.map_expression
                .as_ref()
                .is_some_and(|expr| delete_expression_may_yield_entity(expr, known_bindings))
                || delete_expression_may_yield_entity(&comp.list, known_bindings)
        }
        Expression::PatternComprehension(comp) => {
            delete_expression_may_yield_entity(&comp.projection, known_bindings)
        }
        Expression::Map(map) => map
            .properties
            .iter()
            .any(|pair| delete_expression_may_yield_entity(&pair.value, known_bindings)),
        _ => false,
    }
}

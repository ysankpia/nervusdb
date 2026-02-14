use super::{Error, Expression, Result};

pub(super) fn ensure_no_pattern_predicate(expr: &Expression) -> Result<()> {
    if contains_pattern_predicate(expr) {
        return Err(Error::Other("syntax error: UnexpectedSyntax".to_string()));
    }
    Ok(())
}

fn contains_pattern_predicate(expr: &Expression) -> bool {
    match expr {
        Expression::Exists(exists_expr) => {
            matches!(
                exists_expr.as_ref(),
                crate::ast::ExistsExpression::Pattern(_)
            )
        }
        Expression::Unary(u) => contains_pattern_predicate(&u.operand),
        Expression::Binary(b) => {
            contains_pattern_predicate(&b.left) || contains_pattern_predicate(&b.right)
        }
        Expression::FunctionCall(call) => call.args.iter().any(contains_pattern_predicate),
        Expression::List(items) => items.iter().any(contains_pattern_predicate),
        Expression::ListComprehension(list_comp) => {
            contains_pattern_predicate(&list_comp.list)
                || list_comp
                    .where_expression
                    .as_ref()
                    .is_some_and(contains_pattern_predicate)
                || list_comp
                    .map_expression
                    .as_ref()
                    .is_some_and(contains_pattern_predicate)
        }
        Expression::PatternComprehension(pattern_comp) => {
            pattern_comp
                .where_expression
                .as_ref()
                .is_some_and(contains_pattern_predicate)
                || contains_pattern_predicate(&pattern_comp.projection)
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
                                    .any(|pair| contains_pattern_predicate(&pair.value))
                            })
                        }
                        crate::ast::PathElement::Relationship(rel) => {
                            rel.properties.as_ref().is_some_and(|props| {
                                props
                                    .properties
                                    .iter()
                                    .any(|pair| contains_pattern_predicate(&pair.value))
                            })
                        }
                    })
        }
        Expression::Map(map) => map
            .properties
            .iter()
            .any(|pair| contains_pattern_predicate(&pair.value)),
        Expression::Case(case_expr) => {
            case_expr
                .expression
                .as_ref()
                .is_some_and(contains_pattern_predicate)
                || case_expr.when_clauses.iter().any(|(when_expr, then_expr)| {
                    contains_pattern_predicate(when_expr) || contains_pattern_predicate(then_expr)
                })
                || case_expr
                    .else_expression
                    .as_ref()
                    .is_some_and(contains_pattern_predicate)
        }
        _ => false,
    }
}

use super::{BinaryOperator, Clause, Error, Expression, Literal, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StaticScalarKind {
    Numeric,
    Boolean,
    String,
    Other,
}

fn is_definitely_non_boolean(expr: &Expression) -> bool {
    match expr {
        Expression::Literal(Literal::Boolean(_) | Literal::Null) => false,
        Expression::Literal(_) | Expression::List(_) | Expression::Map(_) => true,
        Expression::Unary(u) => match u.operator {
            crate::ast::UnaryOperator::Not => is_definitely_non_boolean(&u.operand),
            crate::ast::UnaryOperator::Negate => true,
        },
        Expression::Binary(b) => match b.operator {
            BinaryOperator::Equals
            | BinaryOperator::NotEquals
            | BinaryOperator::LessThan
            | BinaryOperator::LessEqual
            | BinaryOperator::GreaterThan
            | BinaryOperator::GreaterEqual
            | BinaryOperator::And
            | BinaryOperator::Or
            | BinaryOperator::Xor
            | BinaryOperator::In
            | BinaryOperator::StartsWith
            | BinaryOperator::EndsWith
            | BinaryOperator::Contains
            | BinaryOperator::HasLabel
            | BinaryOperator::IsNull
            | BinaryOperator::IsNotNull => false,
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Modulo
            | BinaryOperator::Power => true,
        },
        Expression::Parameter(_)
        | Expression::Variable(_)
        | Expression::PropertyAccess(_)
        | Expression::FunctionCall(_)
        | Expression::Case(_)
        | Expression::Exists(_)
        | Expression::ListComprehension(_)
        | Expression::PatternComprehension(_) => false,
    }
}

fn is_definitely_non_list_literal(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::Literal(
            Literal::Boolean(_) | Literal::Integer(_) | Literal::Float(_) | Literal::String(_)
        ) | Expression::Map(_)
    )
}

fn infer_static_scalar_kind(expr: &Expression) -> Option<StaticScalarKind> {
    match expr {
        Expression::Literal(Literal::Integer(_) | Literal::Float(_)) => {
            Some(StaticScalarKind::Numeric)
        }
        Expression::Literal(Literal::Boolean(_)) => Some(StaticScalarKind::Boolean),
        Expression::Literal(Literal::String(_)) => Some(StaticScalarKind::String),
        Expression::Literal(Literal::Null) => None,
        Expression::List(_) | Expression::Map(_) => Some(StaticScalarKind::Other),
        _ => None,
    }
}

fn infer_list_element_kind(list_expr: &Expression) -> Option<StaticScalarKind> {
    let items = match list_expr {
        Expression::List(items) => items,
        _ => return None,
    };

    let mut inferred: Option<StaticScalarKind> = None;
    for item in items {
        let Some(kind) = infer_static_scalar_kind(item) else {
            continue;
        };
        match inferred {
            None => inferred = Some(kind),
            Some(existing) if existing == kind => {}
            Some(_) => return None,
        }
    }
    inferred
}

fn expression_references_variable(expr: &Expression, variable: &str) -> bool {
    match expr {
        Expression::Variable(name) => name == variable,
        Expression::PropertyAccess(access) => access.variable == variable,
        Expression::Unary(unary) => expression_references_variable(&unary.operand, variable),
        Expression::Binary(binary) => {
            expression_references_variable(&binary.left, variable)
                || expression_references_variable(&binary.right, variable)
        }
        Expression::FunctionCall(call) => call
            .args
            .iter()
            .any(|arg| expression_references_variable(arg, variable)),
        Expression::List(items) => items
            .iter()
            .any(|item| expression_references_variable(item, variable)),
        Expression::ListComprehension(comp) => {
            expression_references_variable(&comp.list, variable)
                || comp
                    .where_expression
                    .as_ref()
                    .is_some_and(|expr| expression_references_variable(expr, variable))
                || comp
                    .map_expression
                    .as_ref()
                    .is_some_and(|expr| expression_references_variable(expr, variable))
        }
        Expression::PatternComprehension(comp) => {
            comp.where_expression
                .as_ref()
                .is_some_and(|expr| expression_references_variable(expr, variable))
                || expression_references_variable(&comp.projection, variable)
        }
        Expression::Map(map) => map
            .properties
            .iter()
            .any(|pair| expression_references_variable(&pair.value, variable)),
        Expression::Case(case_expr) => {
            case_expr
                .expression
                .as_ref()
                .is_some_and(|expr| expression_references_variable(expr, variable))
                || case_expr.when_clauses.iter().any(|(when_expr, then_expr)| {
                    expression_references_variable(when_expr, variable)
                        || expression_references_variable(then_expr, variable)
                })
                || case_expr
                    .else_expression
                    .as_ref()
                    .is_some_and(|expr| expression_references_variable(expr, variable))
        }
        Expression::Exists(_) | Expression::Parameter(_) | Expression::Literal(_) => false,
    }
}

fn expression_uses_variable_in_numeric_context(expr: &Expression, variable: &str) -> bool {
    match expr {
        Expression::Unary(unary) => {
            (matches!(unary.operator, crate::ast::UnaryOperator::Negate)
                && expression_references_variable(&unary.operand, variable))
                || expression_uses_variable_in_numeric_context(&unary.operand, variable)
        }
        Expression::Binary(binary) => {
            let numeric_op = matches!(
                binary.operator,
                BinaryOperator::Add
                    | BinaryOperator::Subtract
                    | BinaryOperator::Multiply
                    | BinaryOperator::Divide
                    | BinaryOperator::Modulo
                    | BinaryOperator::Power
            );
            (numeric_op
                && (expression_references_variable(&binary.left, variable)
                    || expression_references_variable(&binary.right, variable)))
                || expression_uses_variable_in_numeric_context(&binary.left, variable)
                || expression_uses_variable_in_numeric_context(&binary.right, variable)
        }
        Expression::FunctionCall(call) => call
            .args
            .iter()
            .any(|arg| expression_uses_variable_in_numeric_context(arg, variable)),
        Expression::List(items) => items
            .iter()
            .any(|item| expression_uses_variable_in_numeric_context(item, variable)),
        Expression::ListComprehension(comp) => {
            expression_uses_variable_in_numeric_context(&comp.list, variable)
                || comp
                    .where_expression
                    .as_ref()
                    .is_some_and(|expr| expression_uses_variable_in_numeric_context(expr, variable))
                || comp
                    .map_expression
                    .as_ref()
                    .is_some_and(|expr| expression_uses_variable_in_numeric_context(expr, variable))
        }
        Expression::PatternComprehension(comp) => {
            comp.where_expression
                .as_ref()
                .is_some_and(|expr| expression_uses_variable_in_numeric_context(expr, variable))
                || expression_uses_variable_in_numeric_context(&comp.projection, variable)
        }
        Expression::Map(map) => map
            .properties
            .iter()
            .any(|pair| expression_uses_variable_in_numeric_context(&pair.value, variable)),
        Expression::Case(case_expr) => {
            case_expr
                .expression
                .as_ref()
                .is_some_and(|expr| expression_uses_variable_in_numeric_context(expr, variable))
                || case_expr.when_clauses.iter().any(|(when_expr, then_expr)| {
                    expression_uses_variable_in_numeric_context(when_expr, variable)
                        || expression_uses_variable_in_numeric_context(then_expr, variable)
                })
                || case_expr
                    .else_expression
                    .as_ref()
                    .is_some_and(|expr| expression_uses_variable_in_numeric_context(expr, variable))
        }
        Expression::Exists(_)
        | Expression::Parameter(_)
        | Expression::Variable(_)
        | Expression::PropertyAccess(_)
        | Expression::Literal(_) => false,
    }
}

fn validate_quantifier_argument_types(call: &crate::ast::FunctionCall) -> Result<()> {
    if !call.name.starts_with("__quant_") || call.args.len() != 3 {
        return Ok(());
    }

    let variable = match &call.args[0] {
        Expression::Variable(name) => name,
        _ => return Ok(()),
    };
    let Some(element_kind) = infer_list_element_kind(&call.args[1]) else {
        return Ok(());
    };

    if element_kind != StaticScalarKind::Numeric
        && expression_uses_variable_in_numeric_context(&call.args[2], variable)
    {
        return Err(Error::Other(
            "syntax error: InvalidArgumentType".to_string(),
        ));
    }

    Ok(())
}

fn is_supported_function_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("__quant_") {
        return true;
    }

    matches!(
        lower.as_str(),
        // Aggregates
        "count"
            | "sum"
            | "avg"
            | "min"
            | "max"
            | "collect"
            | "percentiledisc"
            | "percentilecont"
            // Collections / list / map helpers
            | "size"
            | "head"
            | "tail"
            | "last"
            | "keys"
            | "length"
            | "nodes"
            | "relationships"
            | "range"
            | "properties"
            // Scalar helpers
            | "rand"
            | "abs"
            | "tolower"
            | "toupper"
            | "reverse"
            | "tostring"
            | "trim"
            | "ltrim"
            | "rtrim"
            | "substring"
            | "replace"
            | "split"
            | "coalesce"
            | "sqrt"
            | "sign"
            | "ceil"
            | "tointeger"
            | "tofloat"
            | "toboolean"
            // Graph helpers
            | "startnode"
            | "endnode"
            | "labels"
            | "type"
            | "id"
            // Temporal + duration
            | "date"
            | "date.transaction"
            | "date.statement"
            | "date.realtime"
            | "time"
            | "time.transaction"
            | "time.statement"
            | "time.realtime"
            | "localtime"
            | "localtime.transaction"
            | "localtime.statement"
            | "localtime.realtime"
            | "datetime"
            | "datetime.transaction"
            | "datetime.statement"
            | "datetime.realtime"
            | "localdatetime"
            | "localdatetime.transaction"
            | "localdatetime.statement"
            | "localdatetime.realtime"
            | "duration"
            | "date.truncate"
            | "time.truncate"
            | "localtime.truncate"
            | "datetime.truncate"
            | "localdatetime.truncate"
            | "datetime.fromepoch"
            | "datetime.fromepochmillis"
            | "duration.between"
            | "duration.inmonths"
            | "duration.indays"
            | "duration.inseconds"
            // Internal planner/parser helpers
            | "__index"
            | "__slice"
            | "__getprop"
            | "__distinct"
            | "__nervus_singleton_path"
    )
}

pub(super) fn validate_expression_types(expr: &Expression) -> Result<()> {
    match expr {
        Expression::Unary(u) => {
            validate_expression_types(&u.operand)?;
            if matches!(u.operator, crate::ast::UnaryOperator::Not)
                && is_definitely_non_boolean(&u.operand)
            {
                return Err(Error::Other(
                    "syntax error: InvalidArgumentType".to_string(),
                ));
            }
            Ok(())
        }
        Expression::Binary(b) => {
            validate_expression_types(&b.left)?;
            validate_expression_types(&b.right)?;
            if matches!(b.operator, BinaryOperator::In) && is_definitely_non_list_literal(&b.right)
            {
                return Err(Error::Other(
                    "syntax error: InvalidArgumentType".to_string(),
                ));
            }
            if matches!(
                b.operator,
                BinaryOperator::And | BinaryOperator::Or | BinaryOperator::Xor
            ) && (is_definitely_non_boolean(&b.left) || is_definitely_non_boolean(&b.right))
            {
                return Err(Error::Other(
                    "syntax error: InvalidArgumentType".to_string(),
                ));
            }
            Ok(())
        }
        Expression::FunctionCall(call) => {
            for arg in &call.args {
                validate_expression_types(arg)?;
            }
            if !is_supported_function_name(&call.name) {
                return Err(Error::Other("syntax error: UnknownFunction".to_string()));
            }
            validate_quantifier_argument_types(call)?;
            if call.name.eq_ignore_ascii_case("properties") {
                if call.args.len() != 1 {
                    return Err(Error::Other(
                        "syntax error: InvalidArgumentType".to_string(),
                    ));
                }
                if matches!(
                    call.args[0],
                    Expression::Literal(Literal::Integer(_) | Literal::Float(_))
                        | Expression::Literal(Literal::String(_))
                        | Expression::Literal(Literal::Boolean(_))
                        | Expression::List(_)
                ) {
                    return Err(Error::Other(
                        "syntax error: InvalidArgumentType".to_string(),
                    ));
                }
            }
            Ok(())
        }
        Expression::List(items) => {
            for item in items {
                validate_expression_types(item)?;
            }
            Ok(())
        }
        Expression::ListComprehension(list_comp) => {
            validate_expression_types(&list_comp.list)?;
            if let Some(where_expr) = &list_comp.where_expression {
                validate_expression_types(where_expr)?;
            }
            if let Some(map_expr) = &list_comp.map_expression {
                validate_expression_types(map_expr)?;
            }
            Ok(())
        }
        Expression::PatternComprehension(pattern_comp) => {
            for element in &pattern_comp.pattern.elements {
                match element {
                    crate::ast::PathElement::Node(node) => {
                        if let Some(props) = &node.properties {
                            for pair in &props.properties {
                                validate_expression_types(&pair.value)?;
                            }
                        }
                    }
                    crate::ast::PathElement::Relationship(rel) => {
                        if let Some(props) = &rel.properties {
                            for pair in &props.properties {
                                validate_expression_types(&pair.value)?;
                            }
                        }
                    }
                }
            }
            if let Some(where_expr) = &pattern_comp.where_expression {
                validate_expression_types(where_expr)?;
            }
            validate_expression_types(&pattern_comp.projection)?;
            Ok(())
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                validate_expression_types(&pair.value)?;
            }
            Ok(())
        }
        Expression::Case(case_expr) => {
            if let Some(test) = &case_expr.expression {
                validate_expression_types(test)?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                validate_expression_types(when_expr)?;
                validate_expression_types(then_expr)?;
            }
            if let Some(otherwise) = &case_expr.else_expression {
                validate_expression_types(otherwise)?;
            }
            Ok(())
        }
        Expression::Exists(exists) => {
            match exists.as_ref() {
                crate::ast::ExistsExpression::Pattern(pattern) => {
                    for element in &pattern.elements {
                        match element {
                            crate::ast::PathElement::Node(node) => {
                                if let Some(props) = &node.properties {
                                    for pair in &props.properties {
                                        validate_expression_types(&pair.value)?;
                                    }
                                }
                            }
                            crate::ast::PathElement::Relationship(rel) => {
                                if let Some(props) = &rel.properties {
                                    for pair in &props.properties {
                                        validate_expression_types(&pair.value)?;
                                    }
                                }
                            }
                        }
                    }
                }
                crate::ast::ExistsExpression::Subquery(subquery) => {
                    for clause in &subquery.clauses {
                        match clause {
                            Clause::Where(w) => validate_expression_types(&w.expression)?,
                            Clause::With(w) => {
                                for item in &w.items {
                                    validate_expression_types(&item.expression)?;
                                }
                                if let Some(where_clause) = &w.where_clause {
                                    validate_expression_types(&where_clause.expression)?;
                                }
                            }
                            Clause::Return(r) => {
                                for item in &r.items {
                                    validate_expression_types(&item.expression)?;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_expression_types;
    use crate::ast::{
        BinaryExpression, BinaryOperator, Expression, FunctionCall, Literal, PropertyAccess,
    };

    fn quantifier_expr(list: Expression, predicate: Expression) -> Expression {
        Expression::FunctionCall(FunctionCall {
            name: "__quant_single".to_string(),
            args: vec![Expression::Variable("x".to_string()), list, predicate],
        })
    }

    #[test]
    fn quantifier_rejects_non_numeric_list_when_predicate_uses_modulo() {
        let expr = quantifier_expr(
            Expression::List(vec![Expression::Literal(Literal::String(
                "Clara".to_string(),
            ))]),
            Expression::Binary(Box::new(BinaryExpression {
                left: Expression::Binary(Box::new(BinaryExpression {
                    left: Expression::Variable("x".to_string()),
                    operator: BinaryOperator::Modulo,
                    right: Expression::Literal(Literal::Integer(2)),
                })),
                operator: BinaryOperator::Equals,
                right: Expression::Literal(Literal::Integer(0)),
            })),
        );
        let err = validate_expression_types(&expr).expect_err("expected InvalidArgumentType");
        assert_eq!(err.to_string(), "syntax error: InvalidArgumentType");
    }

    #[test]
    fn quantifier_accepts_numeric_list_with_numeric_predicate() {
        let expr = quantifier_expr(
            Expression::List(vec![
                Expression::Literal(Literal::Integer(1)),
                Expression::Literal(Literal::Integer(3)),
            ]),
            Expression::Binary(Box::new(BinaryExpression {
                left: Expression::Binary(Box::new(BinaryExpression {
                    left: Expression::Variable("x".to_string()),
                    operator: BinaryOperator::Modulo,
                    right: Expression::Literal(Literal::Integer(2)),
                })),
                operator: BinaryOperator::Equals,
                right: Expression::Literal(Literal::Integer(1)),
            })),
        );
        validate_expression_types(&expr).expect("numeric list should be accepted");
    }

    #[test]
    fn quantifier_accepts_non_numeric_list_when_predicate_not_numeric() {
        let expr = quantifier_expr(
            Expression::List(vec![
                Expression::Literal(Literal::String("Alice".to_string())),
                Expression::Literal(Literal::String("Bob".to_string())),
            ]),
            Expression::Binary(Box::new(BinaryExpression {
                left: Expression::PropertyAccess(PropertyAccess {
                    variable: "x".to_string(),
                    property: "name".to_string(),
                }),
                operator: BinaryOperator::IsNotNull,
                right: Expression::Literal(Literal::Null),
            })),
        );
        validate_expression_types(&expr).expect("non-numeric predicate should be accepted");
    }

    #[test]
    fn rejects_unknown_function_at_compile_time() {
        let expr = Expression::FunctionCall(FunctionCall {
            name: "foo".to_string(),
            args: vec![Expression::Variable("n".to_string())],
        });
        let err = validate_expression_types(&expr).expect_err("expected UnknownFunction");
        assert_eq!(err.to_string(), "syntax error: UnknownFunction");
    }

    #[test]
    fn accepts_supported_sign_function() {
        let expr = Expression::FunctionCall(FunctionCall {
            name: "sign".to_string(),
            args: vec![Expression::Literal(Literal::Integer(-1))],
        });
        validate_expression_types(&expr).expect("sign should be recognized");
    }
}

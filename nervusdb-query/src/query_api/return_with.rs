use super::{
    Error, Expression, HashSet, Literal, Plan, Result, compile_projection_aggregation,
    extract_variables_from_expr, validate_expression_types,
};

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

pub(super) fn compile_return_plan(
    input: Plan,
    ret: &crate::ast::ReturnClause,
) -> Result<(Plan, Vec<String>)> {
    let (mut plan, project_cols) = compile_projection_aggregation(input, &ret.items, false)?;

    if let Some(limit) = &ret.limit {
        validate_skip_or_limit_expression(limit)?;
        plan = Plan::Limit {
            input: Box::new(plan),
            limit: limit.clone(),
        };
    }

    Ok((plan, project_cols))
}

use crate::ast::{BinaryOperator, Expression, Literal, UnaryOperator};

fn binary_operator_symbol(operator: &BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Equals => "=",
        BinaryOperator::NotEquals => "<>",
        BinaryOperator::LessThan => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::GreaterThan => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::And => "AND",
        BinaryOperator::Or => "OR",
        BinaryOperator::Xor => "XOR",
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Modulo => "%",
        BinaryOperator::Power => "^",
        BinaryOperator::In => "IN",
        BinaryOperator::StartsWith => "STARTS WITH",
        BinaryOperator::EndsWith => "ENDS WITH",
        BinaryOperator::Contains => "CONTAINS",
        BinaryOperator::HasLabel => ":",
        BinaryOperator::IsNull => "IS NULL",
        BinaryOperator::IsNotNull => "IS NOT NULL",
    }
}

fn unary_operator_symbol(operator: &UnaryOperator) -> &'static str {
    match operator {
        UnaryOperator::Not => "NOT ",
        UnaryOperator::Negate => "-",
    }
}

fn binary_precedence(operator: &BinaryOperator) -> u8 {
    match operator {
        BinaryOperator::Or => 1,
        BinaryOperator::Xor => 2,
        BinaryOperator::And => 3,
        BinaryOperator::Equals
        | BinaryOperator::NotEquals
        | BinaryOperator::LessThan
        | BinaryOperator::LessEqual
        | BinaryOperator::GreaterThan
        | BinaryOperator::GreaterEqual
        | BinaryOperator::In
        | BinaryOperator::StartsWith
        | BinaryOperator::EndsWith
        | BinaryOperator::Contains
        | BinaryOperator::HasLabel
        | BinaryOperator::IsNull
        | BinaryOperator::IsNotNull => 4,
        BinaryOperator::Add | BinaryOperator::Subtract => 5,
        BinaryOperator::Multiply | BinaryOperator::Divide | BinaryOperator::Modulo => 6,
        BinaryOperator::Power => 7,
    }
}

fn format_binary_operand(
    expr: &Expression,
    parent_operator: &BinaryOperator,
    is_left_operand: bool,
) -> String {
    let rendered = expression_alias_fragment(expr);
    let Expression::Binary(child) = expr else {
        return rendered;
    };

    let parent_prec = binary_precedence(parent_operator);
    let child_prec = binary_precedence(&child.operator);
    let needs_parentheses = if child_prec < parent_prec {
        true
    } else if child_prec > parent_prec {
        false
    } else if is_left_operand {
        matches!(parent_operator, BinaryOperator::Power)
    } else {
        matches!(
            parent_operator,
            BinaryOperator::Subtract
                | BinaryOperator::Divide
                | BinaryOperator::Modulo
                | BinaryOperator::Power
        )
    };

    if needs_parentheses {
        format!("({rendered})")
    } else {
        rendered
    }
}

fn is_simple_property_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn expression_alias_fragment(expr: &Expression) -> String {
    match expr {
        Expression::Variable(name) => name.clone(),
        Expression::PropertyAccess(pa) => format!("{}.{}", pa.variable, pa.property),
        Expression::Literal(Literal::String(s)) if s == "*" => "*".to_string(),
        Expression::Literal(Literal::Integer(n)) => n.to_string(),
        Expression::Literal(Literal::Float(n)) => n.to_string(),
        Expression::Literal(Literal::Boolean(b)) => b.to_string(),
        Expression::Literal(Literal::String(s)) => format!("'{}'", s),
        Expression::Literal(Literal::Null) => "null".to_string(),
        Expression::Parameter(name) => format!("${}", name),
        Expression::FunctionCall(call) => {
            if call.name.eq_ignore_ascii_case("__distinct") && call.args.len() == 1 {
                return format!("distinct {}", expression_alias_fragment(&call.args[0]));
            }
            if call.name.eq_ignore_ascii_case("__index") && call.args.len() == 2 {
                return format!(
                    "{}[{}]",
                    expression_alias_fragment(&call.args[0]),
                    expression_alias_fragment(&call.args[1])
                );
            }
            if call.name.eq_ignore_ascii_case("__getprop") && call.args.len() == 2 {
                let raw_base = expression_alias_fragment(&call.args[0]);
                let base = if matches!(
                    call.args[0],
                    Expression::Variable(_) | Expression::PropertyAccess(_)
                ) {
                    raw_base
                } else {
                    format!("({raw_base})")
                };
                if let Expression::Literal(Literal::String(key)) = &call.args[1] {
                    if is_simple_property_name(key) {
                        return format!("{base}.{key}");
                    }
                    return format!("{base}['{}']", key.replace('\'', "\\'"));
                }
                return format!("{base}[{}]", expression_alias_fragment(&call.args[1]));
            }
            let args = call
                .args
                .iter()
                .map(expression_alias_fragment)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", call.name.to_lowercase(), args)
        }
        Expression::Binary(b) => match b.operator {
            BinaryOperator::IsNull | BinaryOperator::IsNotNull => {
                format!(
                    "{} {}",
                    expression_alias_fragment(&b.left),
                    binary_operator_symbol(&b.operator)
                )
            }
            BinaryOperator::HasLabel => {
                let rhs = match &b.right {
                    Expression::Literal(Literal::String(label)) => label.clone(),
                    _ => expression_alias_fragment(&b.right),
                };
                format!("{}:{}", expression_alias_fragment(&b.left), rhs)
            }
            _ => format!(
                "{} {} {}",
                format_binary_operand(&b.left, &b.operator, true),
                binary_operator_symbol(&b.operator),
                format_binary_operand(&b.right, &b.operator, false)
            ),
        },
        Expression::Unary(u) => format!(
            "{}{}",
            unary_operator_symbol(&u.operator),
            expression_alias_fragment(&u.operand)
        ),
        Expression::List(items) => format!(
            "[{}]",
            items
                .iter()
                .map(expression_alias_fragment)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expression::Map(map) => {
            let inner = map
                .properties
                .iter()
                .map(|pair| format!("{}: {}", pair.key, expression_alias_fragment(&pair.value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{}}}", inner)
        }
        Expression::Case(_) => "case(...)".to_string(),
        Expression::Exists(_) => "exists(...)".to_string(),
        _ => "...".to_string(),
    }
}

pub(super) fn default_projection_alias(expr: &Expression, index: usize) -> String {
    let alias = expression_alias_fragment(expr);
    if alias.is_empty() || alias == "..." || alias.len() > 120 {
        format!("expr_{}", index)
    } else {
        alias
    }
}

pub(super) fn default_aggregate_alias(call: &crate::ast::FunctionCall, index: usize) -> String {
    let name = call.name.to_lowercase();
    if call.args.is_empty() {
        return format!("{}()", name);
    }

    if call.args.len() == 1 {
        return format!("{}({})", name, expression_alias_fragment(&call.args[0]));
    }

    let args = call
        .args
        .iter()
        .map(expression_alias_fragment)
        .collect::<Vec<_>>()
        .join(", ");
    let alias = format!("{}({})", name, args);

    if alias.len() > 80 {
        format!("agg_{}", index)
    } else {
        alias
    }
}

#[cfg(test)]
mod tests {
    use super::{default_aggregate_alias, default_projection_alias};
    use crate::ast::{BinaryExpression, BinaryOperator, Expression, FunctionCall, Literal};

    #[test]
    fn projection_alias_falls_back_for_too_long_fragment() {
        let expr = Expression::Literal(Literal::String("x".repeat(128)));
        let alias = default_projection_alias(&expr, 7);
        assert_eq!(alias, "expr_7");
    }

    #[test]
    fn aggregate_alias_for_single_arg_function() {
        let call = FunctionCall {
            name: "COUNT".to_string(),
            args: vec![Expression::Variable("n".to_string())],
        };
        let alias = default_aggregate_alias(&call, 0);
        assert_eq!(alias, "count(n)");
    }

    #[test]
    fn aggregate_alias_renders_distinct_wrapper_as_distinct_keyword() {
        let call = FunctionCall {
            name: "COUNT".to_string(),
            args: vec![Expression::FunctionCall(FunctionCall {
                name: "__distinct".to_string(),
                args: vec![Expression::Variable("p".to_string())],
            })],
        };
        let alias = default_aggregate_alias(&call, 0);
        assert_eq!(alias, "count(distinct p)");
    }

    #[test]
    fn projection_alias_preserves_parenthesized_precedence() {
        let expr = Expression::Binary(Box::new(BinaryExpression {
            operator: BinaryOperator::Multiply,
            left: Expression::Binary(Box::new(BinaryExpression {
                operator: BinaryOperator::Divide,
                left: Expression::Literal(Literal::Integer(12)),
                right: Expression::Literal(Literal::Integer(4)),
            })),
            right: Expression::Binary(Box::new(BinaryExpression {
                operator: BinaryOperator::Subtract,
                left: Expression::Literal(Literal::Integer(3)),
                right: Expression::Binary(Box::new(BinaryExpression {
                    operator: BinaryOperator::Multiply,
                    left: Expression::Literal(Literal::Integer(2)),
                    right: Expression::Literal(Literal::Integer(4)),
                })),
            })),
        }));

        let alias = default_projection_alias(&expr, 0);
        assert_eq!(alias, "12 / 4 * (3 - 2 * 4)");
    }
}

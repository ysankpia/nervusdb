use crate::ast::{BinaryOperator, Expression};
use std::collections::{BTreeMap, HashSet};

pub(super) fn extract_predicates(
    expr: &Expression,
    map: &mut BTreeMap<String, BTreeMap<String, Expression>>,
) {
    if let Expression::Binary(bin) = expr {
        if matches!(bin.operator, BinaryOperator::And) {
            extract_predicates(&bin.left, map);
            extract_predicates(&bin.right, map);
        } else if matches!(bin.operator, BinaryOperator::Equals) {
            let mut check_eq = |left: &Expression, right: &Expression| {
                if let Expression::PropertyAccess(pa) = left {
                    match right {
                        Expression::Literal(_) | Expression::Parameter(_) => {
                            map.entry(pa.variable.clone())
                                .or_default()
                                .insert(pa.property.clone(), right.clone());
                        }
                        _ => {}
                    }
                }
            };
            check_eq(&bin.left, &bin.right);
            check_eq(&bin.right, &bin.left);
        }
    }
}

pub(super) fn extract_variables_from_expr(expr: &Expression, vars: &mut HashSet<String>) {
    match expr {
        Expression::Variable(v) => {
            vars.insert(v.clone());
        }
        Expression::PropertyAccess(pa) => {
            vars.insert(pa.variable.clone());
        }
        Expression::FunctionCall(f) => {
            for arg in &f.args {
                extract_variables_from_expr(arg, vars);
            }
        }
        Expression::Binary(b) => {
            extract_variables_from_expr(&b.left, vars);
            extract_variables_from_expr(&b.right, vars);
        }
        Expression::Unary(u) => {
            extract_variables_from_expr(&u.operand, vars);
        }
        Expression::List(l) => {
            for item in l {
                extract_variables_from_expr(item, vars);
            }
        }
        Expression::Map(m) => {
            for pair in &m.properties {
                extract_variables_from_expr(&pair.value, vars);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_predicates, extract_variables_from_expr};
    use crate::ast::{
        BinaryExpression, BinaryOperator, Expression, FunctionCall, Literal, PropertyAccess,
    };
    use std::collections::{BTreeMap, HashSet};

    #[test]
    fn extract_variables_walks_nested_expressions() {
        let expr = Expression::FunctionCall(FunctionCall {
            name: "f".to_string(),
            args: vec![
                Expression::PropertyAccess(PropertyAccess {
                    variable: "n".to_string(),
                    property: "name".to_string(),
                }),
                Expression::Binary(Box::new(BinaryExpression {
                    left: Expression::Variable("m".to_string()),
                    operator: BinaryOperator::Add,
                    right: Expression::Variable("k".to_string()),
                })),
            ],
        });

        let mut vars = HashSet::new();
        extract_variables_from_expr(&expr, &mut vars);

        assert!(vars.contains("n"));
        assert!(vars.contains("m"));
        assert!(vars.contains("k"));
    }

    #[test]
    fn extract_predicates_collects_equality_predicates() {
        let left_eq = Expression::Binary(Box::new(BinaryExpression {
            left: Expression::PropertyAccess(PropertyAccess {
                variable: "n".to_string(),
                property: "name".to_string(),
            }),
            operator: BinaryOperator::Equals,
            right: Expression::Literal(Literal::String("Alice".to_string())),
        }));
        let right_eq = Expression::Binary(Box::new(BinaryExpression {
            left: Expression::Literal(Literal::Integer(42)),
            operator: BinaryOperator::Equals,
            right: Expression::PropertyAccess(PropertyAccess {
                variable: "n".to_string(),
                property: "age".to_string(),
            }),
        }));
        let expr = Expression::Binary(Box::new(BinaryExpression {
            left: left_eq,
            operator: BinaryOperator::And,
            right: right_eq,
        }));

        let mut map: BTreeMap<String, BTreeMap<String, Expression>> = BTreeMap::new();
        extract_predicates(&expr, &mut map);

        let n_map = map.get("n").expect("n predicates");
        assert!(n_map.contains_key("name"));
        assert!(n_map.contains_key("age"));
    }
}

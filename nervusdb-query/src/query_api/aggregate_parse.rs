use crate::ast::{Expression, Literal};
use crate::error::{Error, Result};

fn unwrap_distinct_argument(expr: &Expression) -> (Expression, bool) {
    if let Expression::FunctionCall(call) = expr
        && call.name == "__distinct"
        && call.args.len() == 1
    {
        return (call.args[0].clone(), true);
    }

    (expr.clone(), false)
}

pub(super) fn parse_aggregate_function(
    call: &crate::ast::FunctionCall,
) -> Result<Option<crate::ast::AggregateFunction>> {
    let name = call.name.to_lowercase();
    match name.as_str() {
        "count" => {
            if call.args.is_empty() {
                Ok(Some(crate::ast::AggregateFunction::Count(None)))
            } else if call.args.len() == 1 {
                let (arg, distinct) = unwrap_distinct_argument(&call.args[0]);
                if let Expression::Literal(Literal::String(s)) = &arg
                    && s == "*"
                {
                    return Ok(Some(crate::ast::AggregateFunction::Count(None)));
                }

                if distinct {
                    Ok(Some(crate::ast::AggregateFunction::CountDistinct(arg)))
                } else {
                    Ok(Some(crate::ast::AggregateFunction::Count(Some(arg))))
                }
            } else {
                Err(Error::Other("COUNT takes 0 or 1 argument".into()))
            }
        }
        "sum" => {
            if call.args.len() != 1 {
                return Err(Error::Other("SUM takes exactly 1 argument".into()));
            }
            let (arg, distinct) = unwrap_distinct_argument(&call.args[0]);
            if distinct {
                Ok(Some(crate::ast::AggregateFunction::SumDistinct(arg)))
            } else {
                Ok(Some(crate::ast::AggregateFunction::Sum(arg)))
            }
        }
        "avg" => {
            if call.args.len() != 1 {
                return Err(Error::Other("AVG takes exactly 1 argument".into()));
            }
            let (arg, distinct) = unwrap_distinct_argument(&call.args[0]);
            if distinct {
                Ok(Some(crate::ast::AggregateFunction::AvgDistinct(arg)))
            } else {
                Ok(Some(crate::ast::AggregateFunction::Avg(arg)))
            }
        }
        "min" => {
            if call.args.len() != 1 {
                return Err(Error::Other("MIN takes exactly 1 argument".into()));
            }
            let (arg, distinct) = unwrap_distinct_argument(&call.args[0]);
            if distinct {
                Ok(Some(crate::ast::AggregateFunction::MinDistinct(arg)))
            } else {
                Ok(Some(crate::ast::AggregateFunction::Min(arg)))
            }
        }
        "max" => {
            if call.args.len() != 1 {
                return Err(Error::Other("MAX takes exactly 1 argument".into()));
            }
            let (arg, distinct) = unwrap_distinct_argument(&call.args[0]);
            if distinct {
                Ok(Some(crate::ast::AggregateFunction::MaxDistinct(arg)))
            } else {
                Ok(Some(crate::ast::AggregateFunction::Max(arg)))
            }
        }
        "collect" => {
            if call.args.len() != 1 {
                return Err(Error::Other("COLLECT takes exactly 1 argument".into()));
            }
            let (arg, distinct) = unwrap_distinct_argument(&call.args[0]);
            if distinct {
                Ok(Some(crate::ast::AggregateFunction::CollectDistinct(arg)))
            } else {
                Ok(Some(crate::ast::AggregateFunction::Collect(arg)))
            }
        }
        "percentiledisc" => {
            if call.args.len() != 2 {
                return Err(Error::Other(
                    "PERCENTILEDISC takes exactly 2 arguments".into(),
                ));
            }
            Ok(Some(crate::ast::AggregateFunction::PercentileDisc(
                call.args[0].clone(),
                call.args[1].clone(),
            )))
        }
        "percentilecont" => {
            if call.args.len() != 2 {
                return Err(Error::Other(
                    "PERCENTILECONT takes exactly 2 arguments".into(),
                ));
            }
            Ok(Some(crate::ast::AggregateFunction::PercentileCont(
                call.args[0].clone(),
                call.args[1].clone(),
            )))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_aggregate_function;
    use crate::ast::{AggregateFunction, Expression, FunctionCall, Literal};

    #[test]
    fn parse_count_star_is_count_none() {
        let call = FunctionCall {
            name: "count".to_string(),
            args: vec![Expression::Literal(Literal::String("*".to_string()))],
        };
        let parsed = parse_aggregate_function(&call).expect("parse should succeed");
        assert!(matches!(parsed, Some(AggregateFunction::Count(None))));
    }

    #[test]
    fn parse_collect_distinct_form() {
        let distinct = Expression::FunctionCall(FunctionCall {
            name: "__distinct".to_string(),
            args: vec![Expression::Variable("n".to_string())],
        });
        let call = FunctionCall {
            name: "collect".to_string(),
            args: vec![distinct],
        };
        let parsed = parse_aggregate_function(&call).expect("parse should succeed");
        assert!(matches!(
            parsed,
            Some(AggregateFunction::CollectDistinct(Expression::Variable(_)))
        ));
    }

    #[test]
    fn rejects_sum_with_wrong_arity() {
        let call = FunctionCall {
            name: "sum".to_string(),
            args: vec![],
        };
        let err = parse_aggregate_function(&call).expect_err("sum() should fail");
        assert!(err.to_string().contains("SUM takes exactly 1 argument"));
    }

    #[test]
    fn parse_percentile_disc_form() {
        let call = FunctionCall {
            name: "percentileDisc".to_string(),
            args: vec![
                Expression::Variable("n".to_string()),
                Expression::Literal(Literal::Float(0.5)),
            ],
        };
        let parsed = parse_aggregate_function(&call).expect("parse should succeed");
        assert!(matches!(
            parsed,
            Some(AggregateFunction::PercentileDisc(
                Expression::Variable(_),
                Expression::Literal(Literal::Float(_))
            ))
        ));
    }
}

use super::{Result, Row, Value};
use crate::ast::AggregateFunction;
use crate::evaluator::{evaluate_expression_value, order_compare};
use nervusdb_api::GraphSnapshot;

pub(super) fn execute_aggregate<'a, S: GraphSnapshot + 'a>(
    snapshot: &'a S,
    input: Box<dyn Iterator<Item = Result<Row>> + 'a>,
    group_by: Vec<String>,
    aggregates: Vec<(AggregateFunction, String)>,
    params: &'a crate::query_api::Params,
) -> Box<dyn Iterator<Item = Result<Row>> + 'a> {
    // Collect all rows and group them
    let mut groups: std::collections::HashMap<Vec<Value>, Vec<Row>> =
        std::collections::HashMap::new();

    for item in input {
        let row = match item {
            Ok(r) => r,
            Err(e) => return Box::new(std::iter::once(Err(e))),
        };

        let key: Vec<Value> = group_by
            .iter()
            .filter_map(|var| {
                row.cols
                    .iter()
                    .find(|(k, _)| k == var)
                    .map(|(_, v)| v.clone())
            })
            .collect();

        groups.entry(key).or_default().push(row);
    }

    // Cypher aggregate semantics: no grouping keys still yields one row on empty input.
    if groups.is_empty() && group_by.is_empty() {
        groups.insert(Vec::new(), Vec::new());
    }

    // Convert to result rows
    let results: Vec<Result<Row>> = groups
        .into_iter()
        .map(|(key, rows)| {
            // Build group key row
            let mut result = Row::default();
            for (i, var) in group_by.iter().enumerate() {
                if i < key.len() {
                    result = result.with(var, key[i].clone());
                }
            }

            // Compute aggregates
            for (func, alias) in &aggregates {
                let value = match func {
                    AggregateFunction::Count(None) => {
                        // COUNT(*)
                        Value::Int(rows.len() as i64)
                    }
                    AggregateFunction::Count(Some(expr)) => {
                        // COUNT(expr) - count non-null values
                        let count = rows
                            .iter()
                            .filter(|r| {
                                !matches!(
                                    evaluate_expression_value(expr, r, snapshot, params),
                                    Value::Null
                                )
                            })
                            .count();
                        Value::Int(count as i64)
                    }
                    AggregateFunction::CountDistinct(expr) => {
                        let mut distinct_values: Vec<Value> = Vec::new();
                        for row in &rows {
                            let value = evaluate_expression_value(expr, row, snapshot, params);
                            if value == Value::Null {
                                continue;
                            }
                            if !distinct_values.iter().any(|existing| existing == &value) {
                                distinct_values.push(value);
                            }
                        }
                        Value::Int(distinct_values.len() as i64)
                    }
                    AggregateFunction::Sum(expr) => {
                        let mut saw_float = false;
                        let mut int_sum: i128 = 0;
                        let mut float_sum: f64 = 0.0;

                        for row in &rows {
                            match evaluate_expression_value(expr, row, snapshot, params) {
                                Value::Int(i) => {
                                    int_sum += i as i128;
                                    float_sum += i as f64;
                                }
                                Value::Float(f) => {
                                    saw_float = true;
                                    float_sum += f;
                                }
                                _ => {}
                            }
                        }

                        if saw_float {
                            Value::Float(float_sum)
                        } else {
                            Value::Int(int_sum as i64)
                        }
                    }
                    AggregateFunction::SumDistinct(expr) => {
                        let mut distinct_values: Vec<Value> = Vec::new();
                        for row in &rows {
                            let value = evaluate_expression_value(expr, row, snapshot, params);
                            if value == Value::Null {
                                continue;
                            }
                            if !distinct_values.iter().any(|existing| existing == &value) {
                                distinct_values.push(value);
                            }
                        }

                        let mut saw_float = false;
                        let mut int_sum: i128 = 0;
                        let mut float_sum: f64 = 0.0;
                        for value in distinct_values {
                            match value {
                                Value::Int(i) => {
                                    int_sum += i as i128;
                                    float_sum += i as f64;
                                }
                                Value::Float(f) => {
                                    saw_float = true;
                                    float_sum += f;
                                }
                                _ => {}
                            }
                        }

                        if saw_float {
                            Value::Float(float_sum)
                        } else {
                            Value::Int(int_sum as i64)
                        }
                    }
                    AggregateFunction::Avg(expr) => {
                        let values: Vec<f64> = rows
                            .iter()
                            .filter_map(|r| {
                                match evaluate_expression_value(expr, r, snapshot, params) {
                                    Value::Float(f) => Some(f),
                                    Value::Int(i) => Some(i as f64),
                                    _ => None,
                                }
                            })
                            .collect();
                        if values.is_empty() {
                            Value::Null
                        } else {
                            Value::Float(values.iter().sum::<f64>() / values.len() as f64)
                        }
                    }
                    AggregateFunction::AvgDistinct(expr) => {
                        let mut distinct_values: Vec<Value> = Vec::new();
                        for row in &rows {
                            let value = evaluate_expression_value(expr, row, snapshot, params);
                            if value == Value::Null {
                                continue;
                            }
                            if !distinct_values.iter().any(|existing| existing == &value) {
                                distinct_values.push(value);
                            }
                        }

                        let numeric: Vec<f64> = distinct_values
                            .into_iter()
                            .filter_map(|value| match value {
                                Value::Float(f) => Some(f),
                                Value::Int(i) => Some(i as f64),
                                _ => None,
                            })
                            .collect();

                        if numeric.is_empty() {
                            Value::Null
                        } else {
                            Value::Float(numeric.iter().sum::<f64>() / numeric.len() as f64)
                        }
                    }
                    AggregateFunction::Min(expr) => {
                        let min_val = rows
                            .iter()
                            .filter_map(|r| {
                                let v = evaluate_expression_value(expr, r, snapshot, params);
                                if v == Value::Null { None } else { Some(v) }
                            })
                            .min_by(|a, b| order_compare(a, b));
                        min_val.unwrap_or(Value::Null)
                    }
                    AggregateFunction::MinDistinct(expr) => {
                        let mut distinct_values: Vec<Value> = Vec::new();
                        for row in &rows {
                            let value = evaluate_expression_value(expr, row, snapshot, params);
                            if value == Value::Null {
                                continue;
                            }
                            if !distinct_values.iter().any(|existing| existing == &value) {
                                distinct_values.push(value);
                            }
                        }

                        distinct_values
                            .into_iter()
                            .min_by(|a, b| order_compare(a, b))
                            .unwrap_or(Value::Null)
                    }
                    AggregateFunction::Max(expr) => {
                        let max_val = rows
                            .iter()
                            .filter_map(|r| {
                                let v = evaluate_expression_value(expr, r, snapshot, params);
                                if v == Value::Null { None } else { Some(v) }
                            })
                            .max_by(|a, b| order_compare(a, b));
                        max_val.unwrap_or(Value::Null)
                    }
                    AggregateFunction::MaxDistinct(expr) => {
                        let mut distinct_values: Vec<Value> = Vec::new();
                        for row in &rows {
                            let value = evaluate_expression_value(expr, row, snapshot, params);
                            if value == Value::Null {
                                continue;
                            }
                            if !distinct_values.iter().any(|existing| existing == &value) {
                                distinct_values.push(value);
                            }
                        }

                        distinct_values
                            .into_iter()
                            .max_by(|a, b| order_compare(a, b))
                            .unwrap_or(Value::Null)
                    }
                    AggregateFunction::Collect(expr) => {
                        let values: Vec<Value> = rows
                            .iter()
                            .map(|r| evaluate_expression_value(expr, r, snapshot, params))
                            .filter(|v| *v != Value::Null)
                            .collect();
                        Value::List(values)
                    }
                    AggregateFunction::CollectDistinct(expr) => {
                        let mut distinct_values: Vec<Value> = Vec::new();
                        for row in &rows {
                            let value = evaluate_expression_value(expr, row, snapshot, params);
                            if value == Value::Null {
                                continue;
                            }
                            if !distinct_values.iter().any(|existing| existing == &value) {
                                distinct_values.push(value);
                            }
                        }
                        Value::List(distinct_values)
                    }
                };
                result = result.with(alias, value);
            }

            Ok(result)
        })
        .collect();

    Box::new(results.into_iter())
}

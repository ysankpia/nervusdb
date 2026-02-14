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
                    AggregateFunction::PercentileDisc(value_expr, percentile_expr) => {
                        evaluate_percentile_disc(
                            &rows,
                            value_expr,
                            percentile_expr,
                            snapshot,
                            params,
                        )?
                    }
                    AggregateFunction::PercentileCont(value_expr, percentile_expr) => {
                        evaluate_percentile_cont(
                            &rows,
                            value_expr,
                            percentile_expr,
                            snapshot,
                            params,
                        )?
                    }
                };
                result = result.with(alias, value);
            }

            Ok(result)
        })
        .collect();

    Box::new(results.into_iter())
}

fn evaluate_percentile_disc<S: GraphSnapshot>(
    rows: &[Row],
    value_expr: &crate::ast::Expression,
    percentile_expr: &crate::ast::Expression,
    snapshot: &S,
    params: &crate::query_api::Params,
) -> Result<Value> {
    let mut values = collect_numeric_values(rows, value_expr, snapshot, params);
    if values.is_empty() {
        return Ok(Value::Null);
    }
    values.sort_by(|(left, _), (right, _)| left.total_cmp(right));

    let Some(percentile) = resolve_percentile(rows, percentile_expr, snapshot, params)? else {
        return Ok(Value::Null);
    };

    let count = values.len();
    let rank = if percentile <= 0.0 {
        1
    } else {
        (percentile * count as f64).ceil() as usize
    };
    let index = rank.saturating_sub(1).min(count - 1);
    Ok(values[index].1.clone())
}

fn evaluate_percentile_cont<S: GraphSnapshot>(
    rows: &[Row],
    value_expr: &crate::ast::Expression,
    percentile_expr: &crate::ast::Expression,
    snapshot: &S,
    params: &crate::query_api::Params,
) -> Result<Value> {
    let mut values = collect_numeric_values(rows, value_expr, snapshot, params);
    if values.is_empty() {
        return Ok(Value::Null);
    }
    values.sort_by(|(left, _), (right, _)| left.total_cmp(right));

    let Some(percentile) = resolve_percentile(rows, percentile_expr, snapshot, params)? else {
        return Ok(Value::Null);
    };

    let sorted: Vec<f64> = values.into_iter().map(|(num, _)| num).collect();
    let max_index = sorted.len() - 1;
    let position = percentile * max_index as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;

    if lower == upper {
        return Ok(Value::Float(sorted[lower]));
    }

    let lower_value = sorted[lower];
    let upper_value = sorted[upper];
    let ratio = position - lower as f64;
    Ok(Value::Float(
        lower_value + (upper_value - lower_value) * ratio,
    ))
}

fn collect_numeric_values<S: GraphSnapshot>(
    rows: &[Row],
    value_expr: &crate::ast::Expression,
    snapshot: &S,
    params: &crate::query_api::Params,
) -> Vec<(f64, Value)> {
    rows.iter()
        .filter_map(|row| {
            let value = evaluate_expression_value(value_expr, row, snapshot, params);
            match value {
                Value::Int(i) => Some((i as f64, Value::Int(i))),
                Value::Float(f) => Some((f, Value::Float(f))),
                _ => None,
            }
        })
        .collect()
}

fn resolve_percentile<S: GraphSnapshot>(
    rows: &[Row],
    percentile_expr: &crate::ast::Expression,
    snapshot: &S,
    params: &crate::query_api::Params,
) -> Result<Option<f64>> {
    let Some(row) = rows.first() else {
        return Ok(None);
    };

    let percentile = match evaluate_expression_value(percentile_expr, row, snapshot, params) {
        Value::Int(i) => i as f64,
        Value::Float(f) => f,
        Value::Null => return Ok(None),
        _ => return Ok(None),
    };

    if !(0.0..=1.0).contains(&percentile) {
        return Err(crate::error::Error::Other(
            "runtime error: NumberOutOfRange".to_string(),
        ));
    }

    Ok(Some(percentile))
}

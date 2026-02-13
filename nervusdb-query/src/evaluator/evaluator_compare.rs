use super::evaluator_numeric::value_as_f64;
use super::evaluator_temporal_math::{compare_time_of_day, compare_time_with_offset};
use super::evaluator_temporal_parse::parse_temporal_string;
use super::{TemporalValue, Value};
use std::cmp::Ordering;

pub(super) fn compare_values<F>(left: &Value, right: &Value, cmp: F) -> Value
where
    F: Fn(Ordering) -> bool,
{
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Value::Null,
        (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => {
            compare_numbers_for_range(left, right, &cmp)
        }
        (Value::Bool(l), Value::Bool(r)) => Value::Bool(cmp(l.cmp(r))),
        (Value::String(l), Value::String(r)) => {
            Value::Bool(cmp(compare_strings_with_temporal(l, r)))
        }
        (Value::List(l), Value::List(r)) => compare_lists_for_range(l, r, &cmp),
        _ => Value::Null,
    }
}

fn compare_numbers_for_range<F>(left: &Value, right: &Value, cmp: &F) -> Value
where
    F: Fn(Ordering) -> bool,
{
    let (l, r) = match (value_as_f64(left), value_as_f64(right)) {
        (Some(l), Some(r)) => (l, r),
        _ => return Value::Null,
    };
    if l.is_nan() || r.is_nan() {
        return Value::Bool(false);
    }
    l.partial_cmp(&r)
        .map(|ord| Value::Bool(cmp(ord)))
        .unwrap_or(Value::Null)
}

fn compare_lists_for_range<F>(left: &[Value], right: &[Value], cmp: &F) -> Value
where
    F: Fn(Ordering) -> bool,
{
    for (l, r) in left.iter().zip(right.iter()) {
        match compare_value_for_list_ordering(l, r) {
            Some(Ordering::Equal) => {}
            Some(ord) => return Value::Bool(cmp(ord)),
            None => return Value::Null,
        }
    }
    Value::Bool(cmp(left.len().cmp(&right.len())))
}

fn compare_value_for_list_ordering(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Null, Value::Null) => Some(Ordering::Equal),
        (Value::Null, _) => Some(Ordering::Greater),
        (_, Value::Null) => Some(Ordering::Less),
        _ => order_compare_non_null(left, right),
    }
}

fn compare_lists_ordering(left: &[Value], right: &[Value]) -> Option<Ordering> {
    for (l, r) in left.iter().zip(right.iter()) {
        match compare_value_for_list_ordering(l, r) {
            Some(Ordering::Equal) => {}
            non_eq => return non_eq,
        }
    }
    Some(left.len().cmp(&right.len()))
}

fn value_order_rank(value: &Value) -> u8 {
    match value {
        Value::Map(_) => 0,
        Value::NodeId(_) | Value::ExternalId(_) | Value::Node(_) => 1,
        Value::EdgeKey(_) | Value::Relationship(_) => 2,
        Value::List(_) => 3,
        Value::Path(_) | Value::ReifiedPath(_) => 4,
        Value::String(_) => 5,
        Value::Bool(_) => 6,
        Value::Int(_) | Value::Float(_) => 7,
        Value::DateTime(_) => 8,
        Value::Blob(_) => 9,
        Value::Null => 10,
    }
}

fn compare_f64_with_nan(left: f64, right: f64) -> Ordering {
    match (left.is_nan(), right.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => left.partial_cmp(&right).unwrap_or(Ordering::Equal),
    }
}

fn node_key(value: &Value) -> Option<u64> {
    match value {
        Value::NodeId(id) => Some(u64::from(*id)),
        Value::ExternalId(id) => Some(*id),
        Value::Node(node) => Some(u64::from(node.id)),
        _ => None,
    }
}

fn relationship_key(value: &Value) -> Option<nervusdb_api::EdgeKey> {
    match value {
        Value::EdgeKey(key) => Some(*key),
        Value::Relationship(rel) => Some(rel.key),
        _ => None,
    }
}

fn path_key(value: &Value) -> Option<(Vec<u64>, Vec<nervusdb_api::EdgeKey>)> {
    match value {
        Value::Path(path) => Some((
            path.nodes.iter().map(|id| u64::from(*id)).collect(),
            path.edges.clone(),
        )),
        Value::ReifiedPath(path) => Some((
            path.nodes.iter().map(|node| u64::from(node.id)).collect(),
            path.relationships.iter().map(|rel| rel.key).collect(),
        )),
        _ => None,
    }
}

pub(super) fn order_compare_non_null(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Bool(l), Value::Bool(r)) => Some(l.cmp(r)),
        (Value::Int(l), Value::Int(r)) => Some(l.cmp(r)),
        (Value::Float(l), Value::Float(r)) => Some(compare_f64_with_nan(*l, *r)),
        (Value::Int(l), Value::Float(r)) => Some(compare_f64_with_nan(*l as f64, *r)),
        (Value::Float(l), Value::Int(r)) => Some(compare_f64_with_nan(*l, *r as f64)),
        (Value::String(l), Value::String(r)) => Some(compare_strings_with_temporal(l, r)),
        _ => {
            let rank_cmp = value_order_rank(left).cmp(&value_order_rank(right));
            if rank_cmp != Ordering::Equal {
                return Some(rank_cmp);
            }

            match (left, right) {
                (Value::Map(l), Value::Map(r)) => l.partial_cmp(r),
                (Value::List(l), Value::List(r)) => compare_lists_ordering(l, r),
                (Value::DateTime(l), Value::DateTime(r)) => Some(l.cmp(r)),
                (Value::Blob(l), Value::Blob(r)) => Some(l.cmp(r)),
                _ => {
                    if let (Some(l), Some(r)) = (node_key(left), node_key(right)) {
                        return Some(l.cmp(&r));
                    }
                    if let (Some(l), Some(r)) = (relationship_key(left), relationship_key(right)) {
                        return Some(l.cmp(&r));
                    }
                    if let (Some((l_nodes, l_edges)), Some((r_nodes, r_edges))) =
                        (path_key(left), path_key(right))
                    {
                        let node_order = l_nodes.cmp(&r_nodes);
                        if node_order != Ordering::Equal {
                            return Some(node_order);
                        }
                        return Some(l_edges.cmp(&r_edges));
                    }
                    left.partial_cmp(right)
                }
            }
        }
    }
}

fn compare_strings_with_temporal(left: &str, right: &str) -> Ordering {
    match (parse_temporal_string(left), parse_temporal_string(right)) {
        (Some(TemporalValue::Date(l)), Some(TemporalValue::Date(r))) => l.cmp(&r),
        (Some(TemporalValue::LocalTime(l)), Some(TemporalValue::LocalTime(r))) => {
            compare_time_of_day(l, r)
        }
        (
            Some(TemporalValue::Time {
                time: lt,
                offset: lo,
            }),
            Some(TemporalValue::Time {
                time: rt,
                offset: ro,
            }),
        ) => compare_time_with_offset(lt, lo, rt, ro),
        (Some(TemporalValue::LocalDateTime(l)), Some(TemporalValue::LocalDateTime(r))) => l.cmp(&r),
        (Some(TemporalValue::DateTime(l)), Some(TemporalValue::DateTime(r))) => l.cmp(&r),
        _ => left.cmp(right),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nervusdb_api::EdgeKey;
    use std::collections::BTreeMap;

    #[test]
    fn mixed_type_order_matches_with_orderby1_expectations() {
        let edge = EdgeKey {
            src: 1,
            rel: 7,
            dst: 2,
        };
        let mut values = vec![
            Value::NodeId(1),
            Value::EdgeKey(edge),
            Value::Path(crate::executor::PathValue {
                nodes: vec![1, 2],
                edges: vec![edge],
            }),
            Value::Float(1.5),
            Value::List(vec![Value::String("list".to_string())]),
            Value::String("text".to_string()),
            Value::Null,
            Value::Bool(false),
            Value::Float(f64::NAN),
            Value::Map(BTreeMap::from([(
                "a".to_string(),
                Value::String("map".to_string()),
            )])),
        ];

        values.sort_by(super::super::order_compare);

        assert!(matches!(values[0], Value::Map(_)));
        assert!(matches!(values[1], Value::NodeId(_)));
        assert!(matches!(values[2], Value::EdgeKey(_)));
        assert!(matches!(values[3], Value::List(_)));
        assert!(matches!(values[4], Value::Path(_)));
        assert!(matches!(values[5], Value::String(_)));
        assert!(matches!(values[6], Value::Bool(false)));
        assert!(matches!(values[7], Value::Float(f) if !f.is_nan()));
        assert!(matches!(values[8], Value::Float(f) if f.is_nan()));
        assert!(matches!(values[9], Value::Null));
    }

    #[test]
    fn list_ordering_treats_null_elements_as_highest() {
        let mut lists = vec![
            Value::List(vec![]),
            Value::List(vec![Value::String("a".to_string())]),
            Value::List(vec![Value::String("a".to_string()), Value::Int(1)]),
            Value::List(vec![Value::Int(1)]),
            Value::List(vec![Value::Int(1), Value::String("a".to_string())]),
            Value::List(vec![Value::Int(1), Value::Null]),
            Value::List(vec![Value::Null, Value::Int(1)]),
            Value::List(vec![Value::Null, Value::Int(2)]),
        ];

        lists.sort_by(super::super::order_compare);
        let mut descending = lists.clone();
        descending.reverse();

        assert_eq!(
            descending[..4].to_vec(),
            vec![
                Value::List(vec![Value::Null, Value::Int(2)]),
                Value::List(vec![Value::Null, Value::Int(1)]),
                Value::List(vec![Value::Int(1), Value::Null]),
                Value::List(vec![Value::Int(1), Value::String("a".to_string())]),
            ]
        );
    }
}

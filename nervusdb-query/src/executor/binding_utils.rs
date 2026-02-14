use super::{EdgeKey, Row, Value};
use nervusdb_api::InternalNodeId;

pub(super) fn apply_optional_unbinds_row(mut row: Row, optional_unbind: &[String]) -> Row {
    for alias in optional_unbind {
        row = row.with(alias.clone(), Value::Null);
    }
    row
}

pub(super) fn value_node_id(value: &Value) -> Option<InternalNodeId> {
    match value {
        Value::NodeId(id) => Some(*id),
        Value::Node(node) => Some(node.id),
        _ => None,
    }
}

pub(super) fn row_matches_node_binding(row: &Row, alias: &str, candidate: InternalNodeId) -> bool {
    match row.get(alias) {
        None => true,
        Some(Value::Null) => false,
        Some(value) => value_node_id(value).is_some_and(|id| id == candidate),
    }
}

pub(super) fn row_contains_all_bindings(candidate: &Row, outer: &Row) -> bool {
    outer.cols.iter().all(|(key, outer_value)| {
        candidate
            .get(key)
            .is_some_and(|candidate_value| binding_values_equal(candidate_value, outer_value))
    })
}

fn value_edge_key(value: &Value) -> Option<EdgeKey> {
    match value {
        Value::EdgeKey(edge) => Some(*edge),
        Value::Relationship(rel) => Some(rel.key),
        _ => None,
    }
}

fn binding_values_equal(candidate: &Value, outer: &Value) -> bool {
    if let (Some(candidate_id), Some(outer_id)) = (value_node_id(candidate), value_node_id(outer)) {
        return candidate_id == outer_id;
    }
    if let (Some(candidate_edge), Some(outer_edge)) =
        (value_edge_key(candidate), value_edge_key(outer))
    {
        return candidate_edge == outer_edge;
    }
    candidate == outer
}

#[cfg(test)]
mod tests {
    use super::{
        apply_optional_unbinds_row, row_contains_all_bindings, row_matches_node_binding,
        value_node_id,
    };
    use crate::executor::{NodeValue, RelationshipValue, Row, Value};
    use nervusdb_api::EdgeKey;
    use nervusdb_api::InternalNodeId;
    use std::collections::BTreeMap;

    #[test]
    fn optional_unbind_sets_aliases_to_null() {
        let row = Row::new(vec![
            ("a".to_string(), Value::Int(1)),
            ("b".to_string(), Value::String("x".to_string())),
        ]);
        let out = apply_optional_unbinds_row(row, &["b".to_string(), "c".to_string()]);
        assert_eq!(out.get("a"), Some(&Value::Int(1)));
        assert_eq!(out.get("b"), Some(&Value::Null));
        assert_eq!(out.get("c"), Some(&Value::Null));
    }

    #[test]
    fn value_node_id_extracts_from_node_and_node_id() {
        let node = NodeValue {
            id: 42,
            labels: Vec::new(),
            properties: BTreeMap::new(),
        };
        assert_eq!(value_node_id(&Value::NodeId(7)), Some(7));
        assert_eq!(value_node_id(&Value::Node(node)), Some(42));
        assert_eq!(value_node_id(&Value::Int(7)), None);
    }

    #[test]
    fn row_binding_match_obeys_null_and_equality_rules() {
        let candidate: InternalNodeId = 8;
        let mut row = Row::new(vec![]);
        assert!(row_matches_node_binding(&row, "n", candidate));

        row = row.with("n".to_string(), Value::Null);
        assert!(!row_matches_node_binding(&row, "n", candidate));

        row = Row::new(vec![("n".to_string(), Value::NodeId(candidate))]);
        assert!(row_matches_node_binding(&row, "n", candidate));
        assert!(!row_matches_node_binding(&row, "n", candidate + 1));
    }

    #[test]
    fn row_contains_all_bindings_requires_all_outer_pairs() {
        let candidate = Row::new(vec![
            ("a".to_string(), Value::Int(1)),
            ("b".to_string(), Value::String("x".to_string())),
            ("c".to_string(), Value::Bool(true)),
        ]);
        let outer_ok = Row::new(vec![
            ("a".to_string(), Value::Int(1)),
            ("b".to_string(), Value::String("x".to_string())),
        ]);
        let outer_fail = Row::new(vec![
            ("a".to_string(), Value::Int(2)),
            ("b".to_string(), Value::String("x".to_string())),
        ]);

        assert!(row_contains_all_bindings(&candidate, &outer_ok));
        assert!(!row_contains_all_bindings(&candidate, &outer_fail));
    }

    #[test]
    fn row_contains_all_bindings_allows_node_and_relationship_identity_equivalence() {
        let edge = EdgeKey {
            src: 1,
            rel: 2,
            dst: 3,
        };
        let candidate = Row::new(vec![
            ("n".to_string(), Value::NodeId(9)),
            ("r".to_string(), Value::EdgeKey(edge)),
        ]);
        let outer = Row::new(vec![
            (
                "n".to_string(),
                Value::Node(NodeValue {
                    id: 9,
                    labels: vec!["X".to_string()],
                    properties: BTreeMap::new(),
                }),
            ),
            (
                "r".to_string(),
                Value::Relationship(RelationshipValue {
                    key: edge,
                    rel_type: "R".to_string(),
                    properties: BTreeMap::new(),
                }),
            ),
        ]);

        assert!(row_contains_all_bindings(&candidate, &outer));
    }
}

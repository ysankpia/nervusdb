use super::{BTreeSet, Error, Expression, Result};

#[derive(Debug, Clone, Default)]
pub(super) struct CompiledMergeSetItems {
    pub(super) property_items: Vec<(String, String, Expression)>,
    pub(super) map_items: Vec<(String, Expression, bool)>,
    pub(super) label_items: Vec<(String, Vec<String>)>,
}

pub(super) fn extract_merge_pattern_vars(pattern: &crate::ast::Pattern) -> BTreeSet<String> {
    let mut vars = BTreeSet::new();
    for el in &pattern.elements {
        match el {
            crate::ast::PathElement::Node(n) => {
                if let Some(v) = &n.variable {
                    vars.insert(v.clone());
                }
            }
            crate::ast::PathElement::Relationship(r) => {
                if let Some(v) = &r.variable {
                    vars.insert(v.clone());
                }
            }
        }
    }
    vars
}

pub(super) fn compile_merge_set_items(
    merge_vars: &BTreeSet<String>,
    set_clauses: Vec<crate::ast::SetClause>,
) -> Result<CompiledMergeSetItems> {
    let mut compiled = CompiledMergeSetItems::default();

    for set_clause in set_clauses {
        for item in set_clause.items {
            if !merge_vars.contains(&item.property.variable) {
                return Err(Error::Other(format!(
                    "syntax error: UndefinedVariable ({})",
                    item.property.variable
                )));
            }
            compiled.property_items.push((
                item.property.variable,
                item.property.property,
                item.value,
            ));
        }
        for label_item in set_clause.labels {
            if !merge_vars.contains(&label_item.variable) {
                return Err(Error::Other(format!(
                    "syntax error: UndefinedVariable ({})",
                    label_item.variable
                )));
            }
            compiled
                .label_items
                .push((label_item.variable, label_item.labels));
        }
        for map_item in set_clause.map_items {
            if !merge_vars.contains(&map_item.variable) {
                return Err(Error::Other(format!(
                    "syntax error: UndefinedVariable ({})",
                    map_item.variable
                )));
            }
            compiled
                .map_items
                .push((map_item.variable, map_item.value, map_item.append));
        }
    }

    Ok(compiled)
}

#[cfg(test)]
mod tests {
    use super::{compile_merge_set_items, extract_merge_pattern_vars};
    use crate::ast::{
        Clause, Expression, LabelSetItem, Literal, MapSetItem, MatchClause, NodePattern,
        PathElement, Pattern, PropertyAccess, PropertyMap, PropertyPair, RelationshipDirection,
        RelationshipPattern, SetClause, SetItem,
    };

    #[test]
    fn compile_merge_set_items_keeps_map_items_for_runtime_execution() {
        let pattern = Pattern {
            variable: None,
            elements: vec![
                PathElement::Node(NodePattern {
                    variable: Some("a".to_string()),
                    labels: vec![],
                    properties: None,
                }),
                PathElement::Relationship(RelationshipPattern {
                    variable: Some("r".to_string()),
                    types: vec!["TYPE".to_string()],
                    direction: RelationshipDirection::LeftToRight,
                    properties: None,
                    variable_length: None,
                }),
                PathElement::Node(NodePattern {
                    variable: Some("b".to_string()),
                    labels: vec![],
                    properties: None,
                }),
            ],
        };
        let merge_vars = extract_merge_pattern_vars(&pattern);
        let set_clauses = vec![SetClause {
            items: vec![SetItem {
                property: PropertyAccess {
                    variable: "r".to_string(),
                    property: "name".to_string(),
                },
                value: Expression::Literal(Literal::String("x".to_string())),
            }],
            map_items: vec![
                MapSetItem {
                    variable: "r".to_string(),
                    value: Expression::Map(PropertyMap {
                        properties: vec![PropertyPair {
                            key: "k".to_string(),
                            value: Expression::Literal(Literal::Integer(1)),
                        }],
                    }),
                    append: true,
                },
                MapSetItem {
                    variable: "r".to_string(),
                    value: Expression::Variable("a".to_string()),
                    append: false,
                },
            ],
            labels: vec![LabelSetItem {
                variable: "a".to_string(),
                labels: vec!["A".to_string()],
            }],
        }];

        let compiled =
            compile_merge_set_items(&merge_vars, set_clauses).expect("compile merge set");
        assert_eq!(compiled.property_items.len(), 1);
        assert_eq!(compiled.map_items.len(), 2);
        assert_eq!(compiled.label_items.len(), 1);
    }

    #[test]
    fn extract_merge_pattern_vars_collects_node_and_rel_aliases() {
        let query_clause = Clause::Match(MatchClause {
            optional: false,
            patterns: vec![Pattern {
                variable: None,
                elements: vec![
                    PathElement::Node(NodePattern {
                        variable: Some("n".to_string()),
                        labels: vec![],
                        properties: None,
                    }),
                    PathElement::Relationship(RelationshipPattern {
                        variable: Some("r".to_string()),
                        types: vec![],
                        direction: RelationshipDirection::LeftToRight,
                        properties: None,
                        variable_length: None,
                    }),
                    PathElement::Node(NodePattern {
                        variable: Some("m".to_string()),
                        labels: vec![],
                        properties: None,
                    }),
                ],
            }],
        });
        let Clause::Match(match_clause) = query_clause else {
            unreachable!();
        };
        let vars = extract_merge_pattern_vars(&match_clause.patterns[0]);
        assert!(vars.contains("n"));
        assert!(vars.contains("r"));
        assert!(vars.contains("m"));
    }
}

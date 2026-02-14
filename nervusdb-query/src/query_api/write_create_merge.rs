use super::{
    BTreeMap, BindingKind, Error, Plan, Result, extract_output_var_kinds,
    validate_create_property_vars, variable_already_bound_error,
};
use crate::ast::PathElement;

pub(super) fn compile_create_plan(
    input: Plan,
    create_clause: crate::ast::CreateClause,
) -> Result<Plan> {
    if create_clause.patterns.is_empty() {
        return Err(Error::Other("CREATE pattern cannot be empty".into()));
    }

    let mut known_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();
    extract_output_var_kinds(&input, &mut known_bindings);

    let mut plan = input;
    for pattern in create_clause.patterns {
        if pattern.elements.is_empty() {
            return Err(Error::Other("CREATE pattern cannot be empty".into()));
        }

        let rel_count = pattern
            .elements
            .iter()
            .filter(|el| matches!(el, PathElement::Relationship(_)))
            .count();

        for element in &pattern.elements {
            match element {
                PathElement::Node(node) => {
                    if let Some(var) = &node.variable {
                        let already_bound = known_bindings.contains_key(var);
                        let has_new_constraints =
                            !node.labels.is_empty() || node.properties.is_some();

                        if already_bound {
                            if has_new_constraints || rel_count == 0 {
                                return Err(variable_already_bound_error(var));
                            }
                        } else {
                            known_bindings.insert(var.clone(), BindingKind::Node);
                        }
                    }

                    validate_create_property_vars(&node.properties, &known_bindings)?;
                }
                PathElement::Relationship(rel) => {
                    if rel.variable_length.is_some() {
                        return Err(Error::Other("syntax error: CreatingVarLength".into()));
                    }

                    if rel.direction == crate::ast::RelationshipDirection::Undirected {
                        return Err(Error::Other(
                            "syntax error: RequiresDirectedRelationship".into(),
                        ));
                    }

                    if rel.types.len() != 1 {
                        return Err(Error::Other(
                            "syntax error: NoSingleRelationshipType".into(),
                        ));
                    }

                    if let Some(var) = &rel.variable {
                        if known_bindings.contains_key(var) {
                            return Err(variable_already_bound_error(var));
                        }
                        known_bindings.insert(var.clone(), BindingKind::Relationship);
                    }

                    validate_create_property_vars(&rel.properties, &known_bindings)?;
                }
            }
        }

        if let Some(path_var) = &pattern.variable {
            if known_bindings.contains_key(path_var) {
                return Err(variable_already_bound_error(path_var));
            }
            known_bindings.insert(path_var.clone(), BindingKind::Path);
        }

        plan = Plan::Create {
            input: Box::new(plan),
            pattern,
            merge: false,
        };
    }

    Ok(plan)
}

pub(super) fn compile_merge_plan(
    input: Plan,
    merge_clause: crate::ast::MergeClause,
) -> Result<Plan> {
    let pattern = merge_clause.pattern;
    if pattern.elements.is_empty() {
        return Err(Error::Other("MERGE pattern cannot be empty".into()));
    }

    let mut known_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();
    extract_output_var_kinds(&input, &mut known_bindings);

    let rel_count = pattern
        .elements
        .iter()
        .filter(|el| matches!(el, PathElement::Relationship(_)))
        .count();

    for element in &pattern.elements {
        match element {
            PathElement::Node(node) => {
                if let Some(var) = &node.variable {
                    let already_bound = known_bindings.contains_key(var);
                    let has_new_constraints = !node.labels.is_empty() || node.properties.is_some();

                    if already_bound {
                        if has_new_constraints || rel_count == 0 {
                            return Err(variable_already_bound_error(var));
                        }
                    } else {
                        known_bindings.insert(var.clone(), BindingKind::Node);
                    }
                }

                validate_create_property_vars(&node.properties, &known_bindings)?;
            }
            PathElement::Relationship(rel) => {
                if rel.variable_length.is_some() {
                    return Err(Error::Other("syntax error: CreatingVarLength".into()));
                }

                if rel.types.len() != 1 {
                    return Err(Error::Other(
                        "syntax error: NoSingleRelationshipType".into(),
                    ));
                }

                if let Some(var) = &rel.variable {
                    if known_bindings.contains_key(var) {
                        return Err(variable_already_bound_error(var));
                    }
                    known_bindings.insert(var.clone(), BindingKind::Relationship);
                }

                validate_create_property_vars(&rel.properties, &known_bindings)?;
            }
        }
    }

    if let Some(path_var) = &pattern.variable {
        if known_bindings.contains_key(path_var) {
            return Err(variable_already_bound_error(path_var));
        }
    }

    Ok(Plan::Create {
        input: Box::new(input),
        pattern,
        merge: true,
    })
}

use super::{BTreeMap, BindingKind};

pub(super) fn first_relationship_is_bound(
    pattern: &crate::ast::Pattern,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> bool {
    match pattern.elements.get(1) {
        Some(crate::ast::PathElement::Relationship(rel)) => rel
            .variable
            .as_ref()
            .and_then(|name| known_bindings.get(name))
            .is_some_and(|kind| {
                if rel.variable_length.is_some() {
                    matches!(kind, BindingKind::RelationshipList | BindingKind::Unknown)
                } else {
                    matches!(kind, BindingKind::Relationship | BindingKind::Unknown)
                }
            }),
        _ => false,
    }
}

pub(super) fn pattern_has_bound_relationship(
    pattern: &crate::ast::Pattern,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> bool {
    pattern.elements.iter().any(|element| match element {
        crate::ast::PathElement::Relationship(rel) => rel
            .variable
            .as_ref()
            .and_then(|name| known_bindings.get(name))
            .is_some_and(|kind| {
                if rel.variable_length.is_some() {
                    matches!(kind, BindingKind::RelationshipList | BindingKind::Unknown)
                } else {
                    matches!(kind, BindingKind::Relationship | BindingKind::Unknown)
                }
            }),
        _ => false,
    })
}

pub(super) fn build_optional_unbind_aliases(
    known_bindings: &BTreeMap<String, BindingKind>,
    src_alias: &str,
    dst_alias: &str,
    edge_alias: Option<&str>,
    path_alias: Option<&str>,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut push_alias = |alias: &str| {
        if !out.iter().any(|existing| existing == alias) {
            out.push(alias.to_string());
        }
    };

    if !is_binding_compatible(known_bindings, src_alias, BindingKind::Node) {
        push_alias(src_alias);
    }
    if !is_binding_compatible(known_bindings, dst_alias, BindingKind::Node) {
        push_alias(dst_alias);
    }
    if let Some(alias) = edge_alias
        && !is_edge_binding_compatible(known_bindings, alias)
    {
        push_alias(alias);
    }
    if let Some(alias) = path_alias
        && !is_binding_compatible(known_bindings, alias, BindingKind::Path)
    {
        push_alias(alias);
    }

    out
}

fn is_edge_binding_compatible(known_bindings: &BTreeMap<String, BindingKind>, alias: &str) -> bool {
    matches!(
        known_bindings.get(alias),
        Some(BindingKind::Relationship | BindingKind::RelationshipList | BindingKind::Unknown)
    )
}

pub(super) fn maybe_reanchor_pattern(
    pattern: crate::ast::Pattern,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> crate::ast::Pattern {
    // Patterns are expected as Node-(Rel-Node)* chains.
    if pattern.elements.len() < 3 || pattern.elements.len() % 2 == 0 {
        return pattern;
    }

    let (first, last) = match (pattern.elements.first(), pattern.elements.last()) {
        (Some(crate::ast::PathElement::Node(first)), Some(crate::ast::PathElement::Node(last))) => {
            (first, last)
        }
        _ => return pattern,
    };

    let first_bound = is_bound_node_alias(first, known_bindings);
    let last_bound = is_bound_node_alias(last, known_bindings);

    if first_bound || !last_bound {
        return pattern;
    }

    let mut reversed_elements = Vec::with_capacity(pattern.elements.len());
    for element in pattern.elements.into_iter().rev() {
        match element {
            crate::ast::PathElement::Node(node) => {
                reversed_elements.push(crate::ast::PathElement::Node(node))
            }
            crate::ast::PathElement::Relationship(mut rel) => {
                rel.direction = reverse_relationship_direction(&rel.direction);
                reversed_elements.push(crate::ast::PathElement::Relationship(rel));
            }
        }
    }

    crate::ast::Pattern {
        variable: pattern.variable,
        elements: reversed_elements,
    }
}

fn reverse_relationship_direction(
    direction: &crate::ast::RelationshipDirection,
) -> crate::ast::RelationshipDirection {
    match direction {
        crate::ast::RelationshipDirection::LeftToRight => {
            crate::ast::RelationshipDirection::RightToLeft
        }
        crate::ast::RelationshipDirection::RightToLeft => {
            crate::ast::RelationshipDirection::LeftToRight
        }
        crate::ast::RelationshipDirection::Undirected => {
            crate::ast::RelationshipDirection::Undirected
        }
    }
}

fn is_bound_node_alias(
    node: &crate::ast::NodePattern,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> bool {
    node.variable
        .as_ref()
        .and_then(|name| known_bindings.get(name))
        .is_some_and(|kind| matches!(kind, BindingKind::Node | BindingKind::Unknown))
}

fn is_binding_compatible(
    known_bindings: &BTreeMap<String, BindingKind>,
    alias: &str,
    expected: BindingKind,
) -> bool {
    matches!(
        known_bindings.get(alias),
        Some(kind) if *kind == expected || *kind == BindingKind::Unknown
    )
}

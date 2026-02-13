use super::{BTreeSet, Error, Expression, Result};

#[derive(Debug, Clone, Default)]
pub(super) struct CompiledMergeSetItems {
    pub(super) property_items: Vec<(String, String, Expression)>,
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
        }
    }

    Ok(compiled)
}

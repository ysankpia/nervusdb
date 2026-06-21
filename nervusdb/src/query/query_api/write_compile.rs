use super::write_validation::validate_delete_expression;
use super::{
    BTreeMap, BindingKind, Error, Plan, Result, ensure_no_pattern_predicate,
    extract_output_var_kinds, extract_variables_from_expr,
};

pub(super) fn compile_set_plan_v2(input: Plan, set: crate::query::ast::SetClause) -> Result<Plan> {
    let mut plan = input;
    let mut known_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();
    extract_output_var_kinds(&plan, &mut known_bindings);

    let mut prop_items = Vec::new();
    for item in set.items {
        if !known_bindings.contains_key(&item.property.variable) {
            return Err(Error::Other(format!(
                "syntax error: UndefinedVariable ({})",
                item.property.variable
            )));
        }

        let mut refs = std::collections::HashSet::new();
        extract_variables_from_expr(&item.value, &mut refs);
        for var in refs {
            if !known_bindings.contains_key(&var) {
                return Err(Error::Other(format!(
                    "syntax error: UndefinedVariable ({})",
                    var
                )));
            }
        }

        ensure_no_pattern_predicate(&item.value)?;
        prop_items.push((item.property.variable, item.property.property, item.value));
    }
    if !prop_items.is_empty() {
        plan = Plan::SetProperty {
            input: Box::new(plan),
            items: prop_items,
        };
    }

    if !set.map_items.is_empty() {
        return Err(Error::Other(
            "syntax error: SET map assignment is outside Mini-Cypher 0.1".to_string(),
        ));
    }
    if !set.labels.is_empty() {
        return Err(Error::Other(
            "syntax error: SET labels is outside Mini-Cypher 0.1".to_string(),
        ));
    }

    Ok(plan)
}

pub(super) fn compile_delete_plan_v2(
    input: Plan,
    delete: crate::query::ast::DeleteClause,
) -> Result<Plan> {
    let mut known_bindings = BTreeMap::new();
    extract_output_var_kinds(&input, &mut known_bindings);

    for expr in &delete.expressions {
        validate_delete_expression(expr, &known_bindings)?;
    }

    Ok(Plan::Delete {
        input: Box::new(input),
        detach: delete.detach,
        expressions: delete.expressions,
    })
}

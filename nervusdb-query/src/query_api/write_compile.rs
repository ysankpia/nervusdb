use super::write_validation::validate_delete_expression;
use super::{
    BTreeMap, BindingKind, Error, Plan, Result, ensure_no_pattern_predicate,
    extract_output_var_kinds, extract_variables_from_expr,
};

pub(super) fn compile_set_plan_v2(input: Plan, set: crate::ast::SetClause) -> Result<Plan> {
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

    let mut map_items = Vec::new();
    for item in set.map_items {
        if !known_bindings.contains_key(&item.variable) {
            return Err(Error::Other(format!(
                "syntax error: UndefinedVariable ({})",
                item.variable
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
        map_items.push((item.variable, item.value, item.append));
    }
    if !map_items.is_empty() {
        plan = Plan::SetPropertiesFromMap {
            input: Box::new(plan),
            items: map_items,
        };
    }

    let mut label_items = Vec::new();
    for item in set.labels {
        if !known_bindings.contains_key(&item.variable) {
            return Err(Error::Other(format!(
                "syntax error: UndefinedVariable ({})",
                item.variable
            )));
        }
        label_items.push((item.variable, item.labels));
    }
    if !label_items.is_empty() {
        plan = Plan::SetLabels {
            input: Box::new(plan),
            items: label_items,
        };
    }

    Ok(plan)
}

pub(super) fn compile_remove_plan_v2(
    input: Plan,
    remove: crate::ast::RemoveClause,
) -> Result<Plan> {
    let mut plan = input;

    let mut prop_items = Vec::with_capacity(remove.properties.len());
    for prop in remove.properties {
        prop_items.push((prop.variable, prop.property));
    }
    if !prop_items.is_empty() {
        plan = Plan::RemoveProperty {
            input: Box::new(plan),
            items: prop_items,
        };
    }

    let mut label_items = Vec::new();
    for item in remove.labels {
        label_items.push((item.variable, item.labels));
    }
    if !label_items.is_empty() {
        plan = Plan::RemoveLabels {
            input: Box::new(plan),
            items: label_items,
        };
    }

    Ok(plan)
}

pub(super) fn compile_unwind_plan(input: Plan, unwind: crate::ast::UnwindClause) -> Plan {
    Plan::Unwind {
        input: Box::new(input),
        expression: unwind.expression,
        alias: unwind.alias,
    }
}

pub(super) fn compile_delete_plan_v2(
    input: Plan,
    delete: crate::ast::DeleteClause,
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

use super::{
    BTreeMap, BindingKind, Error, Expression, Literal, Plan, Result, Value, is_internal_path_alias,
};

pub(super) fn variable_already_bound_error(var: &str) -> Error {
    Error::Other(format!("syntax error: VariableAlreadyBound ({var})"))
}

fn variable_type_conflict_error(var: &str, existing: BindingKind, incoming: BindingKind) -> Error {
    Error::Other(format!(
        "syntax error: VariableTypeConflict ({var}: existing={existing:?}, incoming={incoming:?})"
    ))
}

fn register_pattern_binding(
    var: &str,
    incoming: BindingKind,
    known_bindings: &BTreeMap<String, BindingKind>,
    local_bindings: &mut BTreeMap<String, BindingKind>,
) -> Result<()> {
    if let Some(existing) = local_bindings.get(var).copied() {
        if existing == BindingKind::Path || incoming == BindingKind::Path {
            return Err(variable_already_bound_error(var));
        }
        // Self-loops like MATCH (a)-[:R]->(a) are valid and commonly used.
        if existing == BindingKind::Node && incoming == BindingKind::Node {
            return Ok(());
        }
        return Err(variable_type_conflict_error(var, existing, incoming));
    }

    if let Some(existing) = known_bindings.get(var).copied() {
        match (existing, incoming) {
            // Correlated variables flowing into subqueries may be unknown at compile time.
            (BindingKind::Unknown, _) | (_, BindingKind::Unknown) => {}
            (BindingKind::Path, _) | (_, BindingKind::Path) => {
                return Err(variable_already_bound_error(var));
            }
            // Re-using a previously bound variable with the same role is valid.
            (a, b) if a == b => {}
            (a, b) => return Err(variable_type_conflict_error(var, a, b)),
        }
    }

    local_bindings.insert(var.to_string(), incoming);
    Ok(())
}

pub(super) fn validate_match_pattern_bindings(
    pattern: &crate::ast::Pattern,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> Result<()> {
    let mut local_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();

    if let Some(path_var) = &pattern.variable {
        if known_bindings.contains_key(path_var) || local_bindings.contains_key(path_var) {
            return Err(variable_already_bound_error(path_var));
        }
        local_bindings.insert(path_var.clone(), BindingKind::Path);
    }

    for element in &pattern.elements {
        match element {
            crate::ast::PathElement::Node(n) => {
                if let Some(var) = &n.variable {
                    register_pattern_binding(
                        var,
                        BindingKind::Node,
                        known_bindings,
                        &mut local_bindings,
                    )?;
                }
            }
            crate::ast::PathElement::Relationship(r) => {
                if let Some(var) = &r.variable {
                    let incoming_kind = if r.variable_length.is_some() {
                        BindingKind::RelationshipList
                    } else {
                        BindingKind::Relationship
                    };
                    register_pattern_binding(
                        var,
                        incoming_kind,
                        known_bindings,
                        &mut local_bindings,
                    )?;
                }
            }
        }
    }

    Ok(())
}

fn merge_binding_kind(vars: &mut BTreeMap<String, BindingKind>, name: String, kind: BindingKind) {
    if let Some(existing) = vars.get(&name).copied() {
        if existing == kind {
            return;
        }
        vars.insert(name, BindingKind::Unknown);
        return;
    }
    vars.insert(name, kind);
}

fn value_binding_kind(value: &Value) -> BindingKind {
    match value {
        Value::NodeId(_) | Value::Node(_) => BindingKind::Node,
        Value::EdgeKey(_) | Value::Relationship(_) => BindingKind::Relationship,
        Value::List(values)
            if !values.is_empty()
                && values
                    .iter()
                    .all(|item| matches!(item, Value::EdgeKey(_) | Value::Relationship(_))) =>
        {
            BindingKind::RelationshipList
        }
        Value::Path(_) | Value::ReifiedPath(_) => BindingKind::Path,
        _ => BindingKind::Scalar,
    }
}

pub(super) fn infer_expression_binding_kind(
    expr: &Expression,
    vars: &BTreeMap<String, BindingKind>,
) -> BindingKind {
    match expr {
        Expression::Variable(name) => vars.get(name).copied().unwrap_or(BindingKind::Unknown),
        Expression::FunctionCall(call) => {
            if call.name.eq_ignore_ascii_case("__nervus_singleton_path") {
                return BindingKind::Path;
            }
            if call.name.eq_ignore_ascii_case("coalesce") {
                let mut inferred = BindingKind::Unknown;
                for arg in &call.args {
                    if matches!(arg, Expression::Literal(Literal::Null)) {
                        continue;
                    }
                    let kind = infer_expression_binding_kind(arg, vars);
                    if kind == BindingKind::Unknown {
                        continue;
                    }
                    if inferred == BindingKind::Unknown {
                        inferred = kind;
                    } else if inferred != kind {
                        return BindingKind::Unknown;
                    }
                }
                return inferred;
            }
            BindingKind::Scalar
        }
        Expression::Literal(Literal::Null) => BindingKind::Unknown,
        Expression::Literal(_)
        | Expression::Parameter(_)
        | Expression::PropertyAccess(_)
        | Expression::Binary(_)
        | Expression::Unary(_)
        | Expression::Map(_) => BindingKind::Scalar,
        Expression::List(items) => infer_list_binding_kind(items, vars),
        _ => BindingKind::Unknown,
    }
}

fn infer_list_binding_kind(
    items: &[Expression],
    vars: &BTreeMap<String, BindingKind>,
) -> BindingKind {
    if items.is_empty() {
        return BindingKind::Scalar;
    }

    let mut saw_concrete = false;
    for item in items {
        match infer_expression_binding_kind(item, vars) {
            BindingKind::Relationship | BindingKind::RelationshipList => {
                saw_concrete = true;
            }
            BindingKind::Unknown => {}
            _ => return BindingKind::Scalar,
        }
    }

    if saw_concrete {
        BindingKind::RelationshipList
    } else {
        BindingKind::Scalar
    }
}

pub(super) fn extract_output_var_kinds(plan: &Plan, vars: &mut BTreeMap<String, BindingKind>) {
    match plan {
        Plan::ReturnOne => {}
        Plan::NodeScan { alias, .. } => {
            merge_binding_kind(vars, alias.clone(), BindingKind::Node);
        }
        Plan::MatchOut {
            src_alias,
            dst_alias,
            edge_alias,
            path_alias,
            input,
            ..
        }
        | Plan::MatchIn {
            src_alias,
            dst_alias,
            edge_alias,
            path_alias,
            input,
            ..
        }
        | Plan::MatchUndirected {
            src_alias,
            dst_alias,
            edge_alias,
            path_alias,
            input,
            ..
        } => {
            if let Some(p) = input {
                extract_output_var_kinds(p, vars);
            }
            merge_binding_kind(vars, src_alias.clone(), BindingKind::Node);
            merge_binding_kind(vars, dst_alias.clone(), BindingKind::Node);
            if let Some(e) = edge_alias {
                merge_binding_kind(vars, e.clone(), BindingKind::Relationship);
            }
            if let Some(p) = path_alias
                && !is_internal_path_alias(p)
            {
                merge_binding_kind(vars, p.clone(), BindingKind::Path);
            }
        }
        Plan::MatchOutVarLen {
            src_alias,
            dst_alias,
            edge_alias,
            path_alias,
            input,
            ..
        } => {
            if let Some(p) = input {
                extract_output_var_kinds(p, vars);
            }
            merge_binding_kind(vars, src_alias.clone(), BindingKind::Node);
            merge_binding_kind(vars, dst_alias.clone(), BindingKind::Node);
            if let Some(e) = edge_alias {
                merge_binding_kind(vars, e.clone(), BindingKind::RelationshipList);
            }
            if let Some(p) = path_alias
                && !is_internal_path_alias(p)
            {
                merge_binding_kind(vars, p.clone(), BindingKind::Path);
            }
        }
        Plan::MatchBoundRel {
            input,
            rel_alias,
            src_alias,
            dst_alias,
            path_alias,
            ..
        } => {
            extract_output_var_kinds(input, vars);
            merge_binding_kind(vars, rel_alias.clone(), BindingKind::Relationship);
            merge_binding_kind(vars, src_alias.clone(), BindingKind::Node);
            merge_binding_kind(vars, dst_alias.clone(), BindingKind::Node);
            if let Some(p) = path_alias
                && !is_internal_path_alias(p)
            {
                merge_binding_kind(vars, p.clone(), BindingKind::Path);
            }
        }
        Plan::Filter { input, .. }
        | Plan::Skip { input, .. }
        | Plan::Limit { input, .. }
        | Plan::OrderBy { input, .. }
        | Plan::Distinct { input } => extract_output_var_kinds(input, vars),
        Plan::OptionalWhereFixup {
            outer,
            filtered,
            null_aliases,
        } => {
            extract_output_var_kinds(filtered, vars);
            extract_output_var_kinds(outer, vars);
            for alias in null_aliases {
                if !is_internal_path_alias(alias) {
                    vars.insert(alias.clone(), BindingKind::Unknown);
                }
            }
        }
        Plan::Project { input, projections } => {
            extract_output_var_kinds(input, vars);
            let mut projected_aliases = std::collections::BTreeSet::new();
            for (alias, expr) in projections {
                let kind = infer_expression_binding_kind(expr, vars);
                vars.insert(alias.clone(), kind);
                projected_aliases.insert(alias.clone());
            }
            vars.retain(|name, _| projected_aliases.contains(name));
        }
        Plan::Aggregate {
            input,
            group_by,
            aggregates,
        } => {
            extract_output_var_kinds(input, vars);
            let mut output_names = std::collections::BTreeSet::new();
            for key in group_by {
                let kind = vars.get(key).copied().unwrap_or(BindingKind::Unknown);
                vars.insert(key.clone(), kind);
                output_names.insert(key.clone());
            }
            for (_, alias) in aggregates {
                vars.insert(alias.clone(), BindingKind::Unknown);
                output_names.insert(alias.clone());
            }
            vars.retain(|name, _| output_names.contains(name));
        }
        Plan::Unwind { input, alias, .. } => {
            extract_output_var_kinds(input, vars);
            vars.insert(alias.clone(), BindingKind::Unknown);
        }
        Plan::Union { left, right, .. } => {
            extract_output_var_kinds(left, vars);
            extract_output_var_kinds(right, vars);
        }
        Plan::CartesianProduct { left, right } => {
            extract_output_var_kinds(left, vars);
            extract_output_var_kinds(right, vars);
        }
        Plan::Apply {
            input,
            subquery,
            alias: _,
        } => {
            extract_output_var_kinds(input, vars);
            let mut subquery_vars = BTreeMap::new();
            extract_output_var_kinds(subquery, &mut subquery_vars);
            for (name, kind) in subquery_vars {
                merge_binding_kind(vars, name, kind);
            }
        }
        Plan::ProcedureCall {
            input,
            name: _,
            args: _,
            yields,
        } => {
            extract_output_var_kinds(input, vars);
            for (name, alias) in yields {
                vars.insert(
                    alias.clone().unwrap_or_else(|| name.clone()),
                    BindingKind::Unknown,
                );
            }
        }
        Plan::IndexSeek {
            alias, fallback, ..
        } => {
            extract_output_var_kinds(fallback, vars);
            merge_binding_kind(vars, alias.clone(), BindingKind::Node);
        }
        Plan::Foreach { input, .. } => extract_output_var_kinds(input, vars),
        Plan::Values { rows } => {
            for row in rows {
                for (name, value) in row.columns() {
                    merge_binding_kind(vars, name.clone(), value_binding_kind(value));
                }
            }
        }
        Plan::Create { input, pattern, .. } => {
            extract_output_var_kinds(input, vars);
            for el in &pattern.elements {
                match el {
                    crate::ast::PathElement::Node(n) => {
                        if let Some(var) = &n.variable {
                            merge_binding_kind(vars, var.clone(), BindingKind::Node);
                        }
                    }
                    crate::ast::PathElement::Relationship(r) => {
                        if let Some(var) = &r.variable {
                            let kind = if r.variable_length.is_some() {
                                BindingKind::RelationshipList
                            } else {
                                BindingKind::Relationship
                            };
                            merge_binding_kind(vars, var.clone(), kind);
                        }
                    }
                }
            }
            if let Some(path_var) = &pattern.variable {
                merge_binding_kind(vars, path_var.clone(), BindingKind::Path);
            }
        }
        Plan::Delete { input, .. }
        | Plan::SetProperty { input, .. }
        | Plan::SetPropertiesFromMap { input, .. }
        | Plan::SetLabels { input, .. }
        | Plan::RemoveProperty { input, .. }
        | Plan::RemoveLabels { input, .. } => {
            extract_output_var_kinds(input, vars);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expression;

    #[test]
    fn extract_output_var_kinds_keeps_new_match_alias_after_project_input() {
        let with_project = Plan::Project {
            input: Box::new(Plan::NodeScan {
                alias: "a".to_string(),
                label: None,
                optional: false,
            }),
            projections: vec![("a".to_string(), Expression::Variable("a".to_string()))],
        };
        let plan = Plan::MatchOut {
            input: Some(Box::new(with_project)),
            src_alias: "a".to_string(),
            rels: vec![],
            edge_alias: None,
            dst_alias: "b".to_string(),
            dst_labels: vec![],
            src_prebound: true,
            limit: None,
            project: vec![],
            project_external: false,
            optional: false,
            optional_unbind: vec![],
            path_alias: None,
        };

        let mut vars = BTreeMap::new();
        extract_output_var_kinds(&plan, &mut vars);
        assert_eq!(vars.get("a"), Some(&BindingKind::Node));
        assert_eq!(vars.get("b"), Some(&BindingKind::Node));
    }

    #[test]
    fn extract_output_var_kinds_apply_preserves_input_aliases() {
        let input = Plan::NodeScan {
            alias: "p".to_string(),
            label: Some("Person".to_string()),
            optional: false,
        };
        let subquery = Plan::Project {
            input: Box::new(Plan::ReturnOne),
            projections: vec![("deg".to_string(), Expression::Literal(Literal::Integer(1)))],
        };
        let plan = Plan::Apply {
            input: Box::new(input),
            subquery: Box::new(subquery),
            alias: None,
        };

        let mut vars = BTreeMap::new();
        extract_output_var_kinds(&plan, &mut vars);

        assert_eq!(vars.get("p"), Some(&BindingKind::Node));
        assert_eq!(vars.get("deg"), Some(&BindingKind::Scalar));
    }
}

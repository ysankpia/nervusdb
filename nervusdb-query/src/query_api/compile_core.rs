use super::{
    BTreeMap, BindingKind, Clause, Error, Expression, Plan, Query, Result, compile_create_plan,
    compile_delete_plan_v2, compile_match_plan, compile_return_plan, compile_set_plan_v2,
    extract_output_var_kinds, extract_predicates, validate_expression_types,
    validate_where_expression_bindings,
};

pub(crate) struct CompiledQuery {
    pub(crate) plan: Plan,
}

pub(crate) fn compile_m3_plan(query: Query, initial_input: Option<Plan>) -> Result<CompiledQuery> {
    validate_query_scope(&query)?;

    let mut plan: Option<Plan> = initial_input;
    let mut clauses = query.clauses.iter().peekable();
    let mut next_anon_id = 0u32;

    while let Some(clause) = clauses.next() {
        match clause {
            Clause::Match(m) => {
                if m.optional {
                    return Err(outside_0_1("OPTIONAL MATCH"));
                }

                let mut predicates = BTreeMap::new();
                if let Some(Clause::Where(w)) = clauses.peek() {
                    extract_predicates(&w.expression, &mut predicates);
                }

                plan = Some(compile_match_plan(
                    plan,
                    m.clone(),
                    &predicates,
                    &mut next_anon_id,
                )?);
            }
            Clause::Where(w) => {
                if plan.is_none() {
                    return Err(Error::Other("WHERE cannot be the first clause".into()));
                }

                let mut where_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();
                if let Some(current_plan) = &plan {
                    extract_output_var_kinds(current_plan, &mut where_bindings);
                }

                validate_expression_types(&w.expression)?;
                validate_where_expression_bindings(&w.expression, &where_bindings)?;

                plan = Some(Plan::Filter {
                    input: Box::new(plan.unwrap()),
                    predicate: w.expression.clone(),
                });
            }
            Clause::Call(_) => return Err(outside_0_1("CALL")),
            Clause::With(_) => return Err(outside_0_1("WITH")),
            Clause::Return(r) => {
                let input = plan.unwrap_or(Plan::ReturnOne);
                let (p, _) = compile_return_plan(input, r)?;
                plan = Some(p);
                if clauses.peek().is_some() {
                    return Err(Error::NotImplemented(
                        "Clauses after RETURN are not supported",
                    ));
                }
                return Ok(CompiledQuery {
                    plan: plan.unwrap(),
                });
            }
            Clause::Create(c) => {
                let input = plan.unwrap_or(Plan::ReturnOne);
                plan = Some(compile_create_plan(input, c.clone())?);
            }
            Clause::Merge(_) => {
                return Err(Error::Other(
                    "syntax error: MERGE is outside Mini-Cypher 0.1".to_string(),
                ));
            }
            Clause::Set(s) => {
                let input = plan.ok_or_else(|| Error::Other("SET need input".into()))?;
                plan = Some(compile_set_plan_v2(input, s.clone())?);
            }
            Clause::Remove(_) => return Err(outside_0_1("REMOVE")),
            Clause::Delete(d) => {
                let input = plan.ok_or_else(|| Error::Other("DELETE need input".into()))?;
                plan = Some(compile_delete_plan_v2(input, d.clone())?);
            }
            Clause::Unwind(_) => return Err(outside_0_1("UNWIND")),
            Clause::Union(_) => return Err(outside_0_1("UNION")),
            Clause::Foreach(_) => return Err(outside_0_1("FOREACH")),
        }
    }

    // If we exit loop without RETURN
    // For update queries (CREATE/DELETE/SET), this is valid if we return count?
    // M3 requires RETURN usually for read.
    // Spec says: "query without RETURN" is error for read queries.
    // Write queries might return stats?
    // Existing code returned "query without RETURN" error.
    // We'll stick to that unless it's a write-only query?
    // Let's enforce RETURN for now as per previous logic, unless we tracked we did logical writes?
    // But previous `prepare` returns `Result<CompiledQuery>`.

    // If plan exists here, but no RETURN hit.
    // For queries ending in update clauses (CREATE, DELETE, etc.), this is valid.
    if let Some(plan) = plan {
        return Ok(CompiledQuery { plan });
    }

    Err(Error::NotImplemented("Empty query"))
}

fn outside_0_1(feature: &'static str) -> Error {
    Error::Other(format!(
        "syntax error: {feature} is outside Mini-Cypher 0.1"
    ))
}

fn validate_query_scope(query: &Query) -> Result<()> {
    for clause in &query.clauses {
        match clause {
            Clause::Match(m) => {
                if m.optional {
                    return Err(outside_0_1("OPTIONAL MATCH"));
                }
                for pattern in &m.patterns {
                    validate_pattern_scope(pattern)?;
                }
            }
            Clause::Create(c) => {
                for pattern in &c.patterns {
                    validate_pattern_scope(pattern)?;
                }
            }
            Clause::Return(r) => {
                if r.distinct {
                    return Err(outside_0_1("RETURN DISTINCT"));
                }
                if r.order_by.is_some() {
                    return Err(outside_0_1("ORDER BY"));
                }
                if r.skip.is_some() {
                    return Err(outside_0_1("SKIP"));
                }
                for item in &r.items {
                    validate_expression_scope(&item.expression)?;
                }
                if let Some(limit) = &r.limit {
                    validate_expression_scope(limit)?;
                }
            }
            Clause::Where(w) => validate_expression_scope(&w.expression)?,
            Clause::Set(s) => {
                if !s.map_items.is_empty() {
                    return Err(outside_0_1("SET map assignment"));
                }
                if !s.labels.is_empty() {
                    return Err(outside_0_1("SET labels"));
                }
                for item in &s.items {
                    validate_expression_scope(&item.value)?;
                }
            }
            Clause::Delete(d) => {
                for expr in &d.expressions {
                    validate_expression_scope(expr)?;
                }
            }
            Clause::Merge(_) => return Err(outside_0_1("MERGE")),
            Clause::Unwind(_) => return Err(outside_0_1("UNWIND")),
            Clause::Call(_) => return Err(outside_0_1("CALL")),
            Clause::With(_) => return Err(outside_0_1("WITH")),
            Clause::Remove(_) => return Err(outside_0_1("REMOVE")),
            Clause::Union(_) => return Err(outside_0_1("UNION")),
            Clause::Foreach(_) => return Err(outside_0_1("FOREACH")),
        }
    }
    Ok(())
}

fn validate_pattern_scope(pattern: &crate::ast::Pattern) -> Result<()> {
    if pattern.variable.is_some() {
        return Err(outside_0_1("named paths"));
    }
    for element in &pattern.elements {
        match element {
            crate::ast::PathElement::Node(node) => {
                if let Some(props) = &node.properties {
                    for pair in &props.properties {
                        validate_expression_scope(&pair.value)?;
                    }
                }
            }
            crate::ast::PathElement::Relationship(rel) => {
                if rel.variable_length.is_some() {
                    return Err(outside_0_1("variable-length paths"));
                }
                if let Some(props) = &rel.properties {
                    for pair in &props.properties {
                        validate_expression_scope(&pair.value)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_expression_scope(expr: &Expression) -> Result<()> {
    match expr {
        Expression::FunctionCall(call) => {
            let name = call.name.to_ascii_lowercase();
            if matches!(
                name.as_str(),
                "count"
                    | "sum"
                    | "avg"
                    | "min"
                    | "max"
                    | "collect"
                    | "percentiledisc"
                    | "percentilecont"
            ) {
                return Err(outside_0_1("aggregation"));
            }
            for arg in &call.args {
                validate_expression_scope(arg)?;
            }
        }
        Expression::Exists(_) => return Err(outside_0_1("EXISTS")),
        Expression::ListComprehension(_) => return Err(outside_0_1("list comprehension")),
        Expression::PatternComprehension(_) => return Err(outside_0_1("pattern comprehension")),
        Expression::Binary(binary) => {
            validate_expression_scope(&binary.left)?;
            validate_expression_scope(&binary.right)?;
        }
        Expression::Unary(unary) => validate_expression_scope(&unary.operand)?,
        Expression::List(items) => {
            for item in items {
                validate_expression_scope(item)?;
            }
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                validate_expression_scope(&pair.value)?;
            }
        }
        Expression::Case(case_expr) => {
            if let Some(expr) = &case_expr.expression {
                validate_expression_scope(expr)?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                validate_expression_scope(when_expr)?;
                validate_expression_scope(then_expr)?;
            }
            if let Some(expr) = &case_expr.else_expression {
                validate_expression_scope(expr)?;
            }
        }
        Expression::Literal(_)
        | Expression::Variable(_)
        | Expression::PropertyAccess(_)
        | Expression::Parameter(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::compile_m3_plan;

    fn compile_query(cypher: &str) -> crate::error::Result<()> {
        let query = crate::parser::Parser::parse(cypher)?;
        compile_m3_plan(query, None).map(|_| ())
    }

    #[test]
    fn non_0_1_syntax_is_rejected() {
        for (cypher, expected) in [
            (
                "OPTIONAL MATCH (n) RETURN n",
                "syntax error: OPTIONAL MATCH is outside Mini-Cypher 0.1",
            ),
            (
                "MATCH (n) WITH n RETURN n",
                "syntax error: WITH is outside Mini-Cypher 0.1",
            ),
            (
                "RETURN 1 UNION RETURN 2",
                "syntax error: UNION is outside Mini-Cypher 0.1",
            ),
            (
                "UNWIND [1] AS x RETURN x",
                "syntax error: UNWIND is outside Mini-Cypher 0.1",
            ),
            (
                "MERGE (n)",
                "syntax error: MERGE is outside Mini-Cypher 0.1",
            ),
            (
                "FOREACH (x IN [1] | CREATE (n))",
                "syntax error: FOREACH is outside Mini-Cypher 0.1",
            ),
            (
                "CALL db.labels()",
                "syntax error: CALL is outside Mini-Cypher 0.1",
            ),
            (
                "MATCH (n) REMOVE n.name",
                "syntax error: REMOVE is outside Mini-Cypher 0.1",
            ),
            (
                "MATCH (n) RETURN DISTINCT n",
                "syntax error: RETURN DISTINCT is outside Mini-Cypher 0.1",
            ),
            (
                "MATCH (n) RETURN n ORDER BY n.name",
                "syntax error: ORDER BY is outside Mini-Cypher 0.1",
            ),
            (
                "MATCH (n) RETURN n SKIP 1",
                "syntax error: SKIP is outside Mini-Cypher 0.1",
            ),
            (
                "MATCH p = (a)-[:T]->(b) RETURN p",
                "syntax error: named paths is outside Mini-Cypher 0.1",
            ),
            (
                "MATCH (a)-[:T*1..2]->(b) RETURN b",
                "syntax error: variable-length paths is outside Mini-Cypher 0.1",
            ),
            (
                "MATCH (n) SET n:Person",
                "syntax error: SET labels is outside Mini-Cypher 0.1",
            ),
            (
                "MATCH (n) SET n = {name: 'Ada'}",
                "syntax error: SET map assignment is outside Mini-Cypher 0.1",
            ),
            (
                "MATCH (n) SET n += {name: 'Ada'}",
                "syntax error: SET map update is outside Mini-Cypher 0.1",
            ),
            (
                "MATCH (n) RETURN count(*)",
                "syntax error: aggregation is outside Mini-Cypher 0.1",
            ),
            (
                "RETURN EXISTS { MATCH (n) }",
                "syntax error: EXISTS is outside Mini-Cypher 0.1",
            ),
            (
                "RETURN [x IN [1] | x] AS xs",
                "syntax error: list comprehension is outside Mini-Cypher 0.1",
            ),
            (
                "MATCH (n) RETURN [(n)-->(m) | m] AS ms",
                "syntax error: pattern comprehension is outside Mini-Cypher 0.1",
            ),
        ] {
            let err = compile_query(cypher).expect_err("syntax should be outside 0.1");
            assert_eq!(err.to_string(), expected, "{cypher}");
        }
    }

    #[test]
    fn return_undefined_variable_fails_compile_time() {
        let err = compile_query("MATCH () RETURN foo")
            .expect_err("RETURN with undefined variable should fail");
        assert_eq!(err.to_string(), "syntax error: UndefinedVariable (foo)");
    }

    #[test]
    fn return_star_requires_variables_in_scope() {
        let err =
            compile_query("MATCH () RETURN *").expect_err("RETURN * without bindings should fail");
        assert_eq!(err.to_string(), "syntax error: NoVariablesInScope");
    }

    #[test]
    fn with_star_is_outside_0_1() {
        let err = compile_query("MATCH () WITH * CREATE ()")
            .expect_err("WITH should be outside Mini-Cypher 0.1");
        assert_eq!(
            err.to_string(),
            "syntax error: WITH is outside Mini-Cypher 0.1"
        );
    }

    #[test]
    fn named_paths_are_rejected_at_compile_time() {
        let err = compile_query("MATCH p = (a) RETURN labels(p) AS l")
            .expect_err("named paths should be outside 0.1");
        assert_eq!(
            err.to_string(),
            "syntax error: named paths is outside Mini-Cypher 0.1"
        );
    }

    #[test]
    fn type_on_node_rejected_at_compile_time() {
        let err = compile_query("MATCH (r) RETURN type(r)")
            .expect_err("type(node) should fail at compile time");
        assert_eq!(err.to_string(), "syntax error: InvalidArgumentType");
    }

    #[test]
    fn map_literal_with_unbound_value_variable_fails() {
        let err = compile_query("RETURN {k1: k2} AS literal")
            .expect_err("map value variable must be in scope");
        assert_eq!(err.to_string(), "syntax error: UndefinedVariable (k2)");
    }
}

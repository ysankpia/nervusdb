use crate::ast::{BinaryOperator, CallClause, Clause, Expression, Literal, Query};
use crate::error::{Error, Result};
use crate::executor::{Plan, Row, Value, execute_plan, execute_write};
use nervusdb_v2_api::GraphSnapshot;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteSemantics {
    Default,
    Merge,
}

/// Query parameters for parameterized Cypher queries.
///
/// # Example
///
/// ```ignore
/// let mut params = Params::new();
/// params.insert("name", Value::String("Alice".to_string()));
/// let results: Vec<_> = query.execute_streaming(&snapshot, &params).collect();
/// ```
#[derive(Debug, Clone, Default)]
pub struct Params {
    inner: BTreeMap<String, Value>,
}

impl Params {
    /// Creates a new empty parameters map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a parameter value.
    ///
    /// Parameters are referenced in Cypher queries using `$name` syntax.
    pub fn insert(&mut self, name: impl Into<String>, value: Value) {
        self.inner.insert(name.into(), value);
    }

    /// Gets a parameter value by name.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.inner.get(name)
    }
}

/// A compiled Cypher query ready for execution.
///
/// Created by [`prepare()`]. The query plan is optimized once
/// and can be executed multiple times with different parameters.
#[derive(Debug, Clone)]
pub struct PreparedQuery {
    plan: Plan,
    explain: Option<String>,
    write: WriteSemantics,
    merge_on_create_items: Vec<(String, String, Expression)>,
    merge_on_match_items: Vec<(String, String, Expression)>,
}

impl PreparedQuery {
    /// Executes a read query and returns a streaming iterator.
    ///
    /// The returned iterator yields `Result<Row>`, where each row
    /// represents a result record. Errors can occur during execution
    /// (e.g., type mismatches, missing variables).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let query = prepare("MATCH (n)-[:1]->(m) RETURN n, m LIMIT 10").unwrap();
    /// let rows: Vec<_> = query
    ///     .execute_streaming(&snapshot, &Params::new())
    ///     .collect::<Result<_>>()
    ///     .unwrap();
    /// ```
    pub fn execute_streaming<'a, S: GraphSnapshot + 'a>(
        &'a self,
        snapshot: &'a S,
        params: &'a Params,
    ) -> impl Iterator<Item = Result<Row>> + 'a {
        if let Some(plan) = &self.explain {
            let it: Box<dyn Iterator<Item = Result<Row>> + 'a> = Box::new(std::iter::once(Ok(
                Row::default().with("plan", Value::String(plan.clone())),
            )));
            return it;
        }
        Box::new(execute_plan(snapshot, &self.plan, params))
    }

    /// Executes a write query (CREATE/DELETE) with a write transaction.
    ///
    /// Returns the number of entities created/deleted.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let query = prepare("CREATE (n)").unwrap();
    /// let mut txn = db.begin_write();
    /// let count = query.execute_write(&snapshot, &mut txn, &Params::new()).unwrap();
    /// txn.commit().unwrap();
    /// ```
    pub fn execute_write<S: GraphSnapshot>(
        &self,
        snapshot: &S,
        txn: &mut impl crate::executor::WriteableGraph,
        params: &Params,
    ) -> Result<u32> {
        if self.explain.is_some() {
            return Err(Error::Other(
                "EXPLAIN cannot be executed as a write query".into(),
            ));
        }
        match self.write {
            WriteSemantics::Default => execute_write(&self.plan, snapshot, txn, params),
            WriteSemantics::Merge => crate::executor::execute_merge(
                &self.plan,
                snapshot,
                txn,
                params,
                &self.merge_on_create_items,
                &self.merge_on_match_items,
            ),
        }
    }

    pub fn is_explain(&self) -> bool {
        self.explain.is_some()
    }

    /// Returns the explained plan string if this query was an EXPLAIN query.
    pub fn explain_string(&self) -> Option<&str> {
        self.explain.as_deref()
    }
}

/// Parses and prepares a Cypher query for execution.
///
/// # Supported Cypher (v2 M3)
///
/// - `RETURN 1` - Constant return
/// - `MATCH (n)-[:<u32>]->(m) RETURN n, m LIMIT k` - Single-hop pattern match
/// - `MATCH (n)-[:<u32>]->(m) WHERE n.prop = 'value' RETURN n, m` - With WHERE filter
/// - `CREATE (n)` / `CREATE (n {k: v})` - Create nodes
/// - `CREATE (a)-[:1]->(b)` - Create edges
/// - `MATCH (n)-[:1]->(m) DELETE n` / `DETACH DELETE n` - Delete nodes/edges
/// - `EXPLAIN <query>` - Show compiled plan (no execution)
///
/// Returns an error for unsupported Cypher constructs.
pub fn prepare(cypher: &str) -> Result<PreparedQuery> {
    if let Some(inner) = strip_explain_prefix(cypher) {
        if inner.is_empty() {
            return Err(Error::Other("EXPLAIN requires a query".into()));
        }
        let (query, merge_subclauses) = crate::parser::Parser::parse_with_merge_subclauses(inner)?;
        let mut merge_subclauses = VecDeque::from(merge_subclauses);
        let compiled = compile_m3_plan(query, &mut merge_subclauses, None)?;
        if !merge_subclauses.is_empty() {
            return Err(Error::Other(
                "internal error: unconsumed MERGE subclauses".into(),
            ));
        }
        let explain = Some(render_plan(&compiled.plan));
        return Ok(PreparedQuery {
            plan: compiled.plan,
            explain,
            write: compiled.write,
            merge_on_create_items: compiled.merge_on_create_items,
            merge_on_match_items: compiled.merge_on_match_items,
        });
    }

    let (query, merge_subclauses) = crate::parser::Parser::parse_with_merge_subclauses(cypher)?;
    let mut merge_subclauses = VecDeque::from(merge_subclauses);
    let compiled = compile_m3_plan(query, &mut merge_subclauses, None)?;
    if !merge_subclauses.is_empty() {
        return Err(Error::Other(
            "internal error: unconsumed MERGE subclauses".into(),
        ));
    }
    Ok(PreparedQuery {
        plan: compiled.plan,
        explain: None,
        write: compiled.write,
        merge_on_create_items: compiled.merge_on_create_items,
        merge_on_match_items: compiled.merge_on_match_items,
    })
}

fn strip_explain_prefix(input: &str) -> Option<&str> {
    let trimmed = input.trim_start();
    if trimmed.len() < "EXPLAIN".len() {
        return None;
    }
    let (head, tail) = trimmed.split_at("EXPLAIN".len());
    if !head.eq_ignore_ascii_case("EXPLAIN") {
        return None;
    }
    if let Some(next) = tail.chars().next()
        && !next.is_whitespace()
    {
        // Avoid matching `EXPLAINED`, etc.
        return None;
    }
    Some(tail.trim_start())
}

fn render_plan(plan: &Plan) -> String {
    fn indent(n: usize) -> String {
        "  ".repeat(n)
    }

    fn go(out: &mut String, plan: &Plan, depth: usize) {
        let pad = indent(depth);
        match plan {
            Plan::ReturnOne => {
                let _ = writeln!(out, "{pad}ReturnOne");
            }
            Plan::Values { rows } => {
                let _ = writeln!(out, "{pad}Values(rows={})", rows.len());
            }
            Plan::Create { input, pattern } => {
                let _ = writeln!(out, "{pad}Create(pattern={pattern:?})");
                go(out, input, depth + 1);
            }
            Plan::Foreach {
                input,
                variable,
                list,
                sub_plan,
            } => {
                let _ = writeln!(out, "{pad}Foreach(var={variable}, list={list:?})");
                go(out, input, depth + 1);
                let _ = writeln!(out, "{pad}  SubPlan:");
                go(out, sub_plan, depth + 2);
            }

            Plan::NodeScan { alias, label } => {
                let _ = writeln!(out, "{pad}NodeScan(alias={alias}, label={label:?})");
            }
            Plan::MatchOut {
                input: _,
                src_alias,
                rels,
                edge_alias,
                dst_alias,
                limit,
                project: _,
                project_external: _,
                optional,
                path_alias,
            } => {
                let opt_str = if *optional { " OPTIONAL" } else { "" };
                let path_str = if let Some(p) = path_alias {
                    format!(" path={p}")
                } else {
                    "".to_string()
                };
                let _ = writeln!(
                    out,
                    "{pad}MatchOut{opt_str}(src={src_alias}, rels={rels:?}, edge={edge_alias:?}, dst={dst_alias}, limit={limit:?}{path_str})"
                );
            }
            Plan::MatchOutVarLen {
                input: _,
                src_alias,
                rels,
                edge_alias,
                dst_alias,
                min_hops,
                max_hops,
                limit,
                project: _,
                project_external: _,
                optional,
                path_alias,
            } => {
                let opt_str = if *optional { " OPTIONAL" } else { "" };
                let path_str = if let Some(p) = path_alias {
                    format!(" path={p}")
                } else {
                    "".to_string()
                };
                let _ = writeln!(
                    out,
                    "{pad}MatchOutVarLen{opt_str}(src={src_alias}, rels={rels:?}, edge={edge_alias:?}, dst={dst_alias}, min={min_hops}, max={max_hops:?}, limit={limit:?}{path_str})"
                );
            }
            Plan::MatchIn {
                input,
                src_alias,
                rels,
                edge_alias,
                dst_alias,
                limit,
                optional,
                path_alias,
            } => {
                let opt_str = if *optional { " OPTIONAL" } else { "" };
                let path_str = if let Some(p) = path_alias {
                    format!(" path={p}")
                } else {
                    "".to_string()
                };
                let _ = writeln!(
                    out,
                    "{pad}MatchIn{opt_str}(src={src_alias}, rels={rels:?}, edge={edge_alias:?}, dst={dst_alias}, limit={limit:?}{path_str})"
                );
                if let Some(p) = input {
                    go(out, p, depth + 1);
                }
            }
            Plan::MatchUndirected {
                input,
                src_alias,
                rels,
                edge_alias,
                dst_alias,
                limit,
                optional,
                path_alias,
            } => {
                let opt_str = if *optional { " OPTIONAL" } else { "" };
                let path_str = if let Some(p) = path_alias {
                    format!(" path={p}")
                } else {
                    "".to_string()
                };
                let _ = writeln!(
                    out,
                    "{pad}MatchUndirected{opt_str}(src={src_alias}, rels={rels:?}, edge={edge_alias:?}, dst={dst_alias}, limit={limit:?}{path_str})"
                );
                if let Some(p) = input {
                    go(out, p, depth + 1);
                }
            }
            Plan::Filter { input, predicate } => {
                let _ = writeln!(out, "{pad}Filter(predicate={predicate:?})");
                go(out, input, depth + 1);
            }
            Plan::Project { input, projections } => {
                let _ = writeln!(out, "{pad}Project(len={})", projections.len());
                go(out, input, depth + 1);
            }
            Plan::Aggregate {
                input,
                group_by,
                aggregates,
            } => {
                let _ = writeln!(
                    out,
                    "{pad}Aggregate(group_by={group_by:?}, aggregates={aggregates:?})"
                );
                go(out, input, depth + 1);
            }
            Plan::OrderBy { input, items } => {
                let _ = writeln!(out, "{pad}OrderBy(items={items:?})");
                go(out, input, depth + 1);
            }
            Plan::Skip { input, skip } => {
                let _ = writeln!(out, "{pad}Skip(skip={skip})");
                go(out, input, depth + 1);
            }
            Plan::Limit { input, limit } => {
                let _ = writeln!(out, "{pad}Limit(limit={limit})");
                go(out, input, depth + 1);
            }
            Plan::CartesianProduct { left, right } => {
                let _ = writeln!(out, "{pad}CartesianProduct");
                go(out, left, depth + 1);
                go(out, right, depth + 1);
            }
            Plan::Apply {
                input,
                subquery,
                alias,
            } => {
                let _ = writeln!(out, "{pad}Apply(alias={alias:?})");
                go(out, input, depth + 1);
                let _ = writeln!(out, "{pad}  Subquery:");
                go(out, subquery, depth + 2);
            }
            Plan::ProcedureCall {
                input,
                name,
                args: _,
                yields,
            } => {
                let yields_str = yields
                    .iter()
                    .map(|(n, a)| {
                        format!(
                            "{n}{}",
                            a.as_ref()
                                .map(|ali| format!(" AS {ali}"))
                                .unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(
                    out,
                    "{pad}ProcedureCall(name={}, yields=[{}])",
                    name.join("."),
                    yields_str
                );
                go(out, input, depth + 1);
            }
            Plan::Distinct { input } => {
                let _ = writeln!(out, "{pad}Distinct");
                go(out, input, depth + 1);
            }

            Plan::Delete {
                input,
                detach,
                expressions,
            } => {
                let _ = writeln!(
                    out,
                    "{pad}Delete(detach={detach}, expressions={expressions:?})"
                );
                go(out, input, depth + 1);
            }
            Plan::Unwind {
                input,
                expression,
                alias,
            } => {
                let _ = writeln!(out, "{pad}Unwind(alias={alias}, expression={expression:?})");
                go(out, input, depth + 1);
            }
            Plan::Union { left, right, all } => {
                let _ = writeln!(out, "{pad}Union(all={all})");
                go(out, left, depth + 1);
                go(out, right, depth + 1);
            }
            Plan::SetProperty { input, items } => {
                let _ = writeln!(out, "{pad}SetProperty(items={items:?})");
                go(out, input, depth + 1);
            }
            Plan::RemoveProperty { input, items } => {
                let _ = writeln!(out, "{pad}RemoveProperty(items={items:?})");
                go(out, input, depth + 1);
            }
            Plan::IndexSeek {
                alias,
                label,
                field,
                value_expr,
                fallback: _fallback,
            } => {
                let _ = writeln!(
                    out,
                    "{pad}IndexSeek(alias={alias}, label={label}, field={field}, value={value_expr:?})"
                );
                // We don't render fallback to avoid noise, as it's just the unoptimized plan
            }
        }
    }

    let mut out = String::new();
    go(&mut out, plan, 0);
    out.trim_end().to_string()
}

struct CompiledQuery {
    plan: Plan,
    write: WriteSemantics,
    merge_on_create_items: Vec<(String, String, Expression)>,
    merge_on_match_items: Vec<(String, String, Expression)>,
}

fn compile_m3_plan(
    query: Query,
    merge_subclauses: &mut VecDeque<crate::parser::MergeSubclauses>,
    initial_input: Option<Plan>,
) -> Result<CompiledQuery> {
    let mut plan: Option<Plan> = initial_input;
    let mut clauses = query.clauses.iter().peekable();
    let mut write_semantics = WriteSemantics::Default;
    let mut merge_on_create_items: Vec<(String, String, Expression)> = Vec::new();
    let mut merge_on_match_items: Vec<(String, String, Expression)> = Vec::new();

    while let Some(clause) = clauses.next() {
        match clause {
            Clause::Match(m) => {
                // Check ahead for WHERE to optimize immediately
                let mut predicates = BTreeMap::new();
                if let Some(Clause::Where(w)) = clauses.peek() {
                    extract_predicates(&w.expression, &mut predicates);
                }

                plan = Some(compile_match_plan(plan, m.clone(), &predicates)?);
            }
            Clause::Where(w) => {
                // If we didn't consume it optimization (e.g. complex filter not indexable), add filter plan
                // Note: compile_match_plan consumes predicates that CAN be pushed down.
                // We need a way to know if it was fully consumed?
                // For MVP: Simplest approach is to ALWAYS add Filter plan if we have a WHERE clause,
                // and rely on `try_optimize_nodescan_filter` inside `compile_match_plan` or similar.
                // But `compile_match_plan` currently takes predicates.
                // Let's refine: `compile_match_plan` applies index seeks.
                // Any remaining filtering logic must be applied.
                // Current `compile_match_plan` logic in existing code didn't return unused predicates.
                // Let's just always apply a Filter plan for safety in this refactor,
                // OR checking if we just did a Match.
                // Actually, the previous implementation extracted predicates and passed them to match.
                // If we want to support WHERE after WITH, we need `Plan::Filter`.
                // If it's WHERE after MATCH, we want index optimization.

                // Strategy: if previous clause was MATCH, we already peeked and optimized.
                // But if the optimization didn't cover everything, we still need a Filter?
                // Existing `compile_match_plan` handles index seek vs scan + filter.
                // So if we passed predicates to `compile_match_plan`, we might be done?
                // Let's look at `compile_match_plan` (not visible here but I recall it).
                // It likely constructs a Scan + Filter or IndexSeek.
                // So if we just handled a Match, we "consumed" the Where for implementation purposes.
                // But we need to skip the Where clause in the iterator if we 'peeked' it?
                // Using `peeking` to optimize is good. But we need to advance the iterator if we use it.
                // Let's change loop logic to handle WHERE inside MATCH case, or skip it here.

                // Revised Strategy:
                // Handle WHERE here only if it wasn't consumed by a preceding MATCH?
                // Or: MATCH consumes the next WHERE if present.
                // If we find a standalone WHERE (e.g. after WITH), we compile it as Filter.
                // To do this clean:
                // If MATCH case peeks and sees WHERE, it *should* consume it.
                // So we need to `clauses.next()` inside MATCH case?
                // Rust iterators don't let you consume from `peek`.
                // So we just check behavior.

                if plan.is_none() {
                    return Err(Error::Other("WHERE cannot be the first clause".into()));
                }
                plan = Some(Plan::Filter {
                    input: Box::new(plan.unwrap()),
                    predicate: w.expression.clone(),
                });
            }
            Clause::Call(c) => match c {
                CallClause::Subquery(sub_query) => {
                    let input = plan.unwrap_or(Plan::ReturnOne);
                    let sub_query_compiled =
                        compile_m3_plan(sub_query.clone(), merge_subclauses, None)?;
                    plan = Some(Plan::Apply {
                        input: Box::new(input),
                        subquery: Box::new(sub_query_compiled.plan),
                        alias: None,
                    });
                }
                CallClause::Procedure(proc_call) => {
                    let input = plan.unwrap_or(Plan::ReturnOne);
                    let mut yields = Vec::new();
                    if let Some(items) = &proc_call.yields {
                        for item in items {
                            yields.push((item.name.clone(), item.alias.clone()));
                        }
                    }
                    plan = Some(Plan::ProcedureCall {
                        input: Box::new(input),
                        name: proc_call.name.clone(),
                        args: proc_call.arguments.clone(),
                        yields,
                    });
                }
            },
            Clause::With(w) => {
                let input = plan.unwrap_or(Plan::ReturnOne);
                plan = Some(compile_with_plan(input, w)?);
            }
            Clause::Return(r) => {
                let input = plan.unwrap_or(Plan::ReturnOne);
                let (p, _) = compile_return_plan(input, r)?;
                plan = Some(p);
                // If there are more clauses after RETURN, it might be an error or valid?
                // In standard Cypher, RETURN is terminal UNLESS followed by UNION.
                // Check if any clauses left?
                if let Some(next_clause) = clauses.peek() {
                    // Allow UNION to follow RETURN
                    if !matches!(next_clause, Clause::Union(_)) {
                        return Err(Error::NotImplemented(
                            "Clauses after RETURN are not supported",
                        ));
                    }
                    // Continue loop to process UNION
                } else {
                    // No more clauses, return successfully
                    return Ok(CompiledQuery {
                        plan: plan.unwrap(),
                        write: write_semantics,
                        merge_on_create_items,
                        merge_on_match_items,
                    });
                }
            }
            Clause::Create(c) => {
                let input = plan.unwrap_or(Plan::ReturnOne);
                plan = Some(compile_create_plan(input, c.clone())?);
            }
            Clause::Merge(m) => {
                write_semantics = WriteSemantics::Merge;
                if plan.is_none() {
                    let sub = merge_subclauses.pop_front().ok_or_else(|| {
                        Error::Other("internal error: missing MERGE subclauses".into())
                    })?;
                    let merge_vars = extract_merge_pattern_vars(&m.pattern);
                    merge_on_create_items = compile_merge_set_items(&merge_vars, sub.on_create)?;
                    merge_on_match_items = compile_merge_set_items(&merge_vars, sub.on_match)?;
                    plan = Some(compile_merge_plan(Plan::ReturnOne, m.clone())?);
                } else {
                    return Err(Error::NotImplemented("Chained MERGE not supported yet"));
                }
            }
            Clause::Set(s) => {
                let input = plan.ok_or_else(|| Error::Other("SET need input".into()))?;
                // We need to associate WHERE?
                // SET doesn't have its own WHERE. It operates on rows.
                plan = Some(compile_set_plan_v2(input, s.clone())?);
            }
            Clause::Remove(r) => {
                let input = plan.ok_or_else(|| Error::Other("REMOVE need input".into()))?;
                plan = Some(compile_remove_plan_v2(input, r.clone())?);
            }
            Clause::Delete(d) => {
                let input = plan.ok_or_else(|| Error::Other("DELETE need input".into()))?;
                plan = Some(compile_delete_plan_v2(input, d.clone())?);

                // If DELETE is not terminal, we might have issues if we detach/delete nodes used later?
                // But for now, let's allow it.
            }
            Clause::Unwind(u) => {
                let input = plan.unwrap_or(Plan::ReturnOne);
                plan = Some(compile_unwind_plan(input, u.clone()));
            }
            Clause::Union(u) => {
                // UNION logic: current plan is the "left" side; the clause's nested query is the "right" side
                let left_plan =
                    plan.ok_or_else(|| Error::Other("UNION requires left query part".into()))?;
                let right_compiled = compile_m3_plan(u.query.clone(), merge_subclauses, None)?;
                plan = Some(Plan::Union {
                    left: Box::new(left_plan),
                    right: Box::new(right_compiled.plan),
                    all: u.all,
                });
            }
            Clause::Foreach(f) => {
                let input = plan.unwrap_or(Plan::ReturnOne);
                plan = Some(compile_foreach_plan(input, f.clone(), merge_subclauses)?);
            }
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
        return Ok(CompiledQuery {
            plan,
            write: write_semantics,
            merge_on_create_items,
            merge_on_match_items,
        });
    }

    Err(Error::NotImplemented("Empty query"))
}

fn extract_merge_pattern_vars(pattern: &crate::ast::Pattern) -> BTreeSet<String> {
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

fn compile_merge_set_items(
    merge_vars: &BTreeSet<String>,
    set_clauses: Vec<crate::ast::SetClause>,
) -> Result<Vec<(String, String, Expression)>> {
    let mut items = Vec::new();
    for set_clause in set_clauses {
        for item in set_clause.items {
            if !merge_vars.contains(&item.property.variable) {
                return Err(Error::Other(format!(
                    "MERGE ON CREATE/ON MATCH SET references unknown variable '{}'",
                    item.property.variable
                )));
            }
            items.push((item.property.variable, item.property.property, item.value));
        }
    }
    Ok(items)
}

fn compile_with_plan(input: Plan, with: &crate::ast::WithClause) -> Result<Plan> {
    // 1. Projection / Aggregation
    // WITH is identical to RETURN in structure: items, orderBy, skip, limit, where.
    // It projects the input to a new set of variables.

    let (mut plan, _) = compile_projection_aggregation(input, &with.items)?;

    // 2. WHERE
    if let Some(w) = &with.where_clause {
        plan = Plan::Filter {
            input: Box::new(plan),
            predicate: w.expression.clone(),
        };
    }

    // 3. ORDER BY
    if let Some(order_by) = &with.order_by {
        let items = compile_order_by_items(order_by)?;
        plan = Plan::OrderBy {
            input: Box::new(plan),
            items,
        };
    }

    // 4. SKIP
    if let Some(skip) = with.skip {
        plan = Plan::Skip {
            input: Box::new(plan),
            skip,
        };
    }

    // 5. LIMIT
    if let Some(limit) = with.limit {
        plan = Plan::Limit {
            input: Box::new(plan),
            limit,
        };
    }

    Ok(plan)
}

// Shared logic for RETURN and WITH
fn compile_return_plan(input: Plan, ret: &crate::ast::ReturnClause) -> Result<(Plan, Vec<String>)> {
    let (mut plan, project_cols) = compile_projection_aggregation(input, &ret.items)?;

    if let Some(order_by) = &ret.order_by {
        let items = compile_order_by_items(order_by)?;
        plan = Plan::OrderBy {
            input: Box::new(plan),
            items,
        };
    }

    if let Some(skip) = ret.skip {
        plan = Plan::Skip {
            input: Box::new(plan),
            skip,
        };
    }

    if let Some(limit) = ret.limit {
        plan = Plan::Limit {
            input: Box::new(plan),
            limit,
        };
    }

    if ret.distinct {
        plan = Plan::Distinct {
            input: Box::new(plan),
        };
    }

    Ok((plan, project_cols))
}

fn compile_projection_aggregation(
    input: Plan,
    items: &[crate::ast::ReturnItem],
) -> Result<(Plan, Vec<String>)> {
    let mut aggregates: Vec<(crate::ast::AggregateFunction, String)> = Vec::new();
    let mut project_cols: Vec<String> = Vec::new(); // Final output columns

    // We categorize items:
    // 1. Aggregates -> Goes to Plan::Aggregate
    // 2. Non-aggregates -> Must be grouping keys. Need to be projected BEFORE aggregation.

    let mut pre_projections = Vec::new(); // For Plan::Project before Aggregate
    let mut group_by = Vec::new(); // For Plan::Aggregate
    let mut is_aggregation = false;
    let mut projected_aliases = std::collections::HashSet::new();

    // First pass: identify if it is an aggregation and collect items
    for (i, item) in items.iter().enumerate() {
        // Check for aggregation function
        let mut found_agg = false;
        if let Expression::FunctionCall(call) = &item.expression
            && let Some(agg) = parse_aggregate_function(call)?
        {
            found_agg = true;
            is_aggregation = true;
            let alias = item.alias.clone().unwrap_or_else(|| format!("agg_{}", i));
            aggregates.push((agg, alias.clone()));
            project_cols.push(alias);

            // Capture dependencies
            let mut deps = std::collections::HashSet::new();
            for arg in &call.args {
                extract_variables_from_expr(arg, &mut deps);
            }
            // We will add deps to pre_projections AFTER loop or handle logic carefully.
            // If we add them here, they are "implicit" projections.
            // We need them to evaluate the aggregate.
            // BUT, if the same variable is ALSO a grouping key later in the list...
            // It's okay. Duplicates in pre_projections might be inefficient but usually fine if logic uses aliases.
            // However, Plan::Aggregate uses grouping keys to form groups.
            // Implicit deps are just "extra columns" passed through.
            for dep in deps {
                if !projected_aliases.contains(&dep) {
                    pre_projections.push((dep.clone(), Expression::Variable(dep.clone())));
                    projected_aliases.insert(dep);
                }
            }
        }

        if !found_agg {
            let alias = item
                .alias
                .clone()
                .unwrap_or_else(|| match &item.expression {
                    Expression::Variable(name) => name.clone(),
                    Expression::PropertyAccess(pa) => format!("{}.{}", pa.variable, pa.property),
                    Expression::FunctionCall(call) => {
                        // Generate descriptive alias for function calls like "length(p)"
                        if call.args.is_empty() {
                            format!("{}()", call.name)
                        } else if call.args.len() == 1 {
                            // For single arg functions, try to use variable name
                            if let Expression::Variable(arg) = &call.args[0] {
                                format!("{}({})", call.name, arg)
                            } else {
                                format!("{}(...)", call.name)
                            }
                        } else {
                            format!("{}(...)", call.name)
                        }
                    }
                    _ => format!("expr_{}", i),
                });

            // Even if variable, we project it to ensure it's available and aliased correctly
            if !projected_aliases.contains(&alias) {
                pre_projections.push((alias.clone(), item.expression.clone()));
                projected_aliases.insert(alias.clone());
            }

            // If we are aggregating, this alias becomes a grouping key
            group_by.push(alias.clone());
            project_cols.push(alias);
        }
    }

    if is_aggregation {
        // 1. Pre-project grouping keys
        // Input -> Project(keys) -> Aggregate(keys)
        // If pre_projections is empty (e.g. `RETURN count(*)`), check implicit group by?
        // OpenCypher: fail if mixed agg and non-agg without grouping.
        // We assume valid cypher for now.

        // We only project if there are grouping keys.
        // If there are NO grouping keys (global agg like `count(*)`), Plan::Project inputs nothing?
        // If pre_projections is empty, Plan::Project would produce empty rows?
        // Yes. `count(*)` counts rows. Empty rows are fine (as long as count is correct).
        // But wait, Plan::Project logic: `input_iter.map(... new_row ...)`.
        // If projections empty, `new_row` is empty.
        // Rows still exist (one per input).
        // Aggregate count(*) counts them. Correct.

        // However, if we discard `n` (not in pre_projections), and we do `count(n)`,
        // we might fail evaluating `n`.
        // T305 MVP: Stick to `count(*)` or counting grouping keys.
        // If user does `WITH n, count(m)`, we error or panic.
        // We assume safe MVP scope.

        let plan = if !pre_projections.is_empty() {
            Plan::Project {
                input: Box::new(input),
                projections: pre_projections,
            }
        } else {
            // Pass through input if no grouping keys?
            // No, if we pass through, we keep ALL variables.
            // Then Aggregate groups by "nothing" (empty group_by).
            // This works for Global Aggregation.
            input
        };

        let plan = Plan::Aggregate {
            input: Box::new(plan),
            group_by,
            aggregates,
        };
        Ok((plan, project_cols))
    } else {
        // Simple Projection
        Ok((
            Plan::Project {
                input: Box::new(input),
                projections: pre_projections,
            },
            project_cols,
        ))
    }
}

fn compile_order_by_items(
    order_by: &crate::ast::OrderByClause,
) -> Result<Vec<(Expression, crate::ast::Direction)>> {
    Ok(order_by
        .items
        .iter()
        .map(|item| (item.expression.clone(), item.direction.clone()))
        .collect())
}

// Adapters for SET/REMOVE/DELETE since we changed signature to take input
fn compile_set_plan_v2(input: Plan, set: crate::ast::SetClause) -> Result<Plan> {
    // Convert SetItems to (var, key, expr)
    let mut items = Vec::new();
    for item in set.items {
        items.push((item.property.variable, item.property.property, item.value));
    }

    Ok(Plan::SetProperty {
        input: Box::new(input),
        items,
    })
}

fn compile_remove_plan_v2(input: Plan, remove: crate::ast::RemoveClause) -> Result<Plan> {
    // Convert properties to (var, key)
    let mut items = Vec::with_capacity(remove.properties.len());
    for prop in remove.properties {
        items.push((prop.variable, prop.property));
    }

    Ok(Plan::RemoveProperty {
        input: Box::new(input),
        items,
    })
}

fn compile_unwind_plan(input: Plan, unwind: crate::ast::UnwindClause) -> Plan {
    Plan::Unwind {
        input: Box::new(input),
        expression: unwind.expression,
        alias: unwind.alias,
    }
}

fn compile_delete_plan_v2(input: Plan, delete: crate::ast::DeleteClause) -> Result<Plan> {
    Ok(Plan::Delete {
        input: Box::new(input),
        detach: delete.detach,
        expressions: delete.expressions,
    })
}

fn compile_create_plan(input: Plan, create_clause: crate::ast::CreateClause) -> Result<Plan> {
    // M3 CREATE supports:
    // - CREATE (n {prop: val}) - single node with properties
    // - CREATE (n)-[:rel]->(m) - single-hop pattern
    // - CREATE (n {a: 1})-[:1]->(m {b: 2}) - pattern with properties
    // Validate pattern length for MVP
    if create_clause.pattern.elements.is_empty() {
        return Err(Error::Other("CREATE pattern cannot be empty".into()));
    }

    // Labels are now supported!

    // MVP: Only support up to 3 elements (node, rel, node)
    if create_clause.pattern.elements.len() > 3 {
        return Err(Error::NotImplemented(
            "CREATE with more than 3 pattern elements in v2 M3",
        ));
    }

    Ok(Plan::Create {
        input: Box::new(input),
        pattern: create_clause.pattern,
    })
}

fn compile_merge_plan(input: Plan, merge_clause: crate::ast::MergeClause) -> Result<Plan> {
    let pattern = merge_clause.pattern;
    if pattern.elements.is_empty() {
        return Err(Error::Other("MERGE pattern cannot be empty".into()));
    }
    if pattern.elements.len() != 1 && pattern.elements.len() != 3 {
        return Err(Error::NotImplemented(
            "MERGE supports only single-node or single-hop patterns in v2 M3",
        ));
    }

    // For MVP, MERGE needs stable identity -> require property maps on nodes.
    for el in &pattern.elements {
        if let crate::ast::PathElement::Node(n) = el {
            let Some(props) = &n.properties else {
                return Err(Error::NotImplemented(
                    "MERGE requires a non-empty node property map in v2 M3",
                ));
            };
            if props.properties.is_empty() {
                return Err(Error::NotImplemented(
                    "MERGE requires a non-empty node property map in v2 M3",
                ));
            }
            if n.labels.len() > 1 {
                return Err(Error::NotImplemented("MERGE with multiple labels in v2 M3"));
            }
        }
    }

    if pattern.elements.len() == 3 {
        let rel_pat = match &pattern.elements[1] {
            crate::ast::PathElement::Relationship(r) => r,
            _ => {
                return Err(Error::Other(
                    "MERGE pattern must have relationship in middle".into(),
                ));
            }
        };
        if !matches!(
            rel_pat.direction,
            crate::ast::RelationshipDirection::LeftToRight
        ) {
            return Err(Error::NotImplemented(
                "MERGE supports only -> direction in v2 M3",
            ));
        }
        if rel_pat.types.is_empty() {
            return Err(Error::Other("MERGE relationship requires a type".into()));
        }
        if rel_pat.types.len() > 1 {
            return Err(Error::NotImplemented(
                "MERGE with multiple rel types in v2 M3",
            ));
        }
        if rel_pat.variable_length.is_some() {
            return Err(Error::NotImplemented(
                "MERGE does not support variable-length relationships in v2 M3",
            ));
        }
        if let Some(props) = &rel_pat.properties
            && !props.properties.is_empty()
        {
            return Err(Error::NotImplemented(
                "MERGE relationship properties not supported in v2 M3",
            ));
        }
    }

    Ok(Plan::Create {
        input: Box::new(input),
        pattern,
    })
}

fn compile_match_plan(
    input: Option<Plan>,
    m: crate::ast::MatchClause,
    predicates: &BTreeMap<String, BTreeMap<String, Expression>>,
) -> Result<Plan> {
    let mut plan = input;
    let mut known_vars = BTreeSet::new();
    if let Some(p) = &plan {
        extract_output_vars(p, &mut known_vars);
    }

    for pattern in m.patterns {
        if pattern.elements.is_empty() {
            return Err(Error::Other("pattern cannot be empty".into()));
        }

        let first_node_alias = match &pattern.elements[0] {
            crate::ast::PathElement::Node(n) => n
                .variable
                .clone()
                .ok_or(Error::NotImplemented("anonymous start node"))?,
            _ => return Err(Error::Other("pattern must start with a node".into())),
        };

        if known_vars.contains(&first_node_alias) {
            // Join via expansion (start node is already in plan)
            plan = Some(compile_pattern_chain(
                plan,
                &pattern,
                predicates,
                m.optional,
                &known_vars,
            )?);
        } else {
            // Start a new component
            let sub_plan =
                compile_pattern_chain(None, &pattern, predicates, m.optional, &known_vars)?;
            if let Some(existing) = plan {
                plan = Some(Plan::CartesianProduct {
                    left: Box::new(existing),
                    right: Box::new(sub_plan),
                });
            } else {
                plan = Some(sub_plan);
            }
        }

        // Update known_vars after each pattern
        if let Some(p) = &plan {
            extract_output_vars(p, &mut known_vars);
        }
    }

    plan.ok_or_else(|| Error::Other("No patterns in MATCH".into()))
}

fn compile_pattern_chain(
    input: Option<Plan>,
    pattern: &crate::ast::Pattern,
    predicates: &BTreeMap<String, BTreeMap<String, Expression>>,
    optional: bool,
    known_vars: &BTreeSet<String>,
) -> Result<Plan> {
    if pattern.elements.is_empty() {
        return Err(Error::Other("pattern cannot be empty".into()));
    }

    let src_node_el = match &pattern.elements[0] {
        crate::ast::PathElement::Node(n) => n,
        _ => return Err(Error::Other("pattern must start with a node".into())),
    };

    let src_alias = src_node_el
        .variable
        .as_deref()
        .ok_or(Error::NotImplemented("anonymous start node"))?
        .to_string();
    let src_label = src_node_el.labels.first().cloned();

    let mut local_predicates = predicates.clone();
    let mut plan = if let Some(existing_plan) = input {
        // Expansion Join: src_alias must be in known_vars
        if !known_vars.contains(&src_alias) {
            return Err(Error::Other(format!(
                "Join variable '{}' not found in plan",
                src_alias
            )));
        }

        // Apply filters for inline properties of this node
        extend_predicates_from_properties(
            &src_alias,
            &src_node_el.properties,
            &mut local_predicates,
        );

        apply_filters_for_alias(existing_plan, &src_alias, &local_predicates)
    } else {
        // Build Initial Plan (Scan or IndexSeek)
        extend_predicates_from_properties(
            &src_alias,
            &src_node_el.properties,
            &mut local_predicates,
        );

        let mut start_plan = Plan::NodeScan {
            alias: src_alias.clone(),
            label: src_label.clone(),
        };

        // Try IndexSeek optimization
        if let Some(label_name) = &src_label
            && let Some(var_preds) = local_predicates.get(&src_alias)
            && let Some((field, val_expr)) = var_preds.iter().next()
        {
            start_plan = Plan::IndexSeek {
                alias: src_alias.clone(),
                label: label_name.clone(),
                field: field.clone(),
                value_expr: val_expr.clone(),
                fallback: Box::new(start_plan),
            };
        }

        apply_filters_for_alias(start_plan, &src_alias, &local_predicates)
    };

    // Subsequent hops
    let mut i = 1;
    let mut curr_src_alias = src_alias;

    while i < pattern.elements.len() {
        if i + 1 >= pattern.elements.len() {
            return Err(Error::Other("pattern must end with a node".into()));
        }

        let rel_el = match &pattern.elements[i] {
            crate::ast::PathElement::Relationship(r) => r,
            _ => return Err(Error::Other("expected relationship at odd index".into())),
        };
        let dst_node_el = match &pattern.elements[i + 1] {
            crate::ast::PathElement::Node(n) => n,
            _ => return Err(Error::Other("expected node at even index".into())),
        };

        let dst_alias = dst_node_el
            .variable
            .as_deref()
            .ok_or(Error::NotImplemented("anonymous dest node"))?
            .to_string();

        let edge_alias = rel_el.variable.clone();
        let rel_types = rel_el.types.clone();

        let path_alias = pattern.variable.clone();

        if let Some(var_len) = &rel_el.variable_length {
            match rel_el.direction {
                crate::ast::RelationshipDirection::LeftToRight => {
                    plan = Plan::MatchOutVarLen {
                        input: Some(Box::new(plan)),
                        src_alias: curr_src_alias.clone(),
                        dst_alias: dst_alias.clone(),
                        edge_alias: edge_alias.clone(),
                        rels: rel_types,
                        min_hops: var_len.min.unwrap_or(1),
                        max_hops: var_len.max,
                        limit: None,
                        project: Vec::new(),
                        project_external: false,
                        optional,
                        path_alias,
                    };
                }
                _ => {
                    return Err(Error::NotImplemented(
                        "Variable length relationships currently only support -> direction",
                    ));
                }
            }
        } else {
            match rel_el.direction {
                crate::ast::RelationshipDirection::LeftToRight => {
                    plan = Plan::MatchOut {
                        input: Some(Box::new(plan)),
                        src_alias: curr_src_alias.clone(),
                        dst_alias: dst_alias.clone(),
                        edge_alias: edge_alias.clone(),
                        rels: rel_types,
                        limit: None,
                        project: Vec::new(),
                        project_external: false,
                        optional,
                        path_alias,
                    };
                }
                crate::ast::RelationshipDirection::RightToLeft => {
                    plan = Plan::MatchIn {
                        input: Some(Box::new(plan)),
                        src_alias: curr_src_alias.clone(),
                        dst_alias: dst_alias.clone(),
                        edge_alias: edge_alias.clone(),
                        rels: rel_types,
                        limit: None,
                        optional,
                        path_alias,
                    };
                }
                crate::ast::RelationshipDirection::Undirected => {
                    plan = Plan::MatchUndirected {
                        input: Some(Box::new(plan)),
                        src_alias: curr_src_alias.clone(),
                        dst_alias: dst_alias.clone(),
                        edge_alias: edge_alias.clone(),
                        rels: rel_types,
                        limit: None,
                        optional,
                        path_alias,
                    };
                }
            }
        }

        // Extract properties from dst node and relationship
        extend_predicates_from_properties(
            &dst_alias,
            &dst_node_el.properties,
            &mut local_predicates,
        );
        if let Some(ea) = &edge_alias {
            extend_predicates_from_properties(ea, &rel_el.properties, &mut local_predicates);
        }

        // Apply filters
        plan = apply_filters_for_alias(plan, &dst_alias, &local_predicates);
        if let Some(ea) = &edge_alias {
            plan = apply_filters_for_alias(plan, ea, &local_predicates);
        }

        curr_src_alias = dst_alias;
        i += 2;
    }

    Ok(plan)
}

fn apply_filters_for_alias(
    plan: Plan,
    alias: &str,
    local_predicates: &BTreeMap<String, BTreeMap<String, Expression>>,
) -> Plan {
    if let Some(var_preds) = local_predicates.get(alias) {
        let mut combined_pred: Option<Expression> = None;
        for (field, val_expr) in var_preds {
            let prop_access = Expression::PropertyAccess(crate::ast::PropertyAccess {
                variable: alias.to_string(),
                property: field.clone(),
            });
            let eq_expr = Expression::Binary(Box::new(crate::ast::BinaryExpression {
                operator: crate::ast::BinaryOperator::Equals,
                left: prop_access,
                right: val_expr.clone(),
            }));

            combined_pred = match combined_pred {
                Some(prev) => Some(Expression::Binary(Box::new(crate::ast::BinaryExpression {
                    operator: crate::ast::BinaryOperator::And,
                    left: prev,
                    right: eq_expr,
                }))),
                None => Some(eq_expr),
            };
        }

        if let Some(predicate) = combined_pred {
            return Plan::Filter {
                input: Box::new(plan),
                predicate,
            };
        }
    }
    plan
}

/// Helper to convert inline map properties to predicates
fn extend_predicates_from_properties(
    variable: &str,
    properties: &Option<crate::ast::PropertyMap>,
    predicates: &mut BTreeMap<String, BTreeMap<String, Expression>>,
) {
    if let Some(props) = properties {
        for prop in &props.properties {
            predicates
                .entry(variable.to_string())
                .or_default()
                .insert(prop.key.clone(), prop.value.clone());
        }
    }
}

fn extract_output_vars(plan: &Plan, vars: &mut BTreeSet<String>) {
    match plan {
        Plan::ReturnOne => {}
        Plan::NodeScan { alias, .. } => {
            vars.insert(alias.clone());
        }
        Plan::MatchOut {
            src_alias,
            dst_alias,
            edge_alias,
            path_alias,
            input,
            ..
        }
        | Plan::MatchOutVarLen {
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
            vars.insert(src_alias.clone());
            vars.insert(dst_alias.clone());
            if let Some(e) = edge_alias {
                vars.insert(e.clone());
            }
            if let Some(p) = path_alias {
                vars.insert(p.clone());
            }
            if let Some(p) = input {
                extract_output_vars(p, vars);
            }
        }
        Plan::Filter { input, .. }
        | Plan::Skip { input, .. }
        | Plan::Limit { input, .. }
        | Plan::OrderBy { input, .. }
        | Plan::Distinct { input } => extract_output_vars(input, vars),
        Plan::Project { input, projections } => {
            extract_output_vars(input, vars);
            for (alias, _) in projections {
                vars.insert(alias.clone());
            }
        }
        Plan::Aggregate {
            input,
            group_by,
            aggregates,
        } => {
            extract_output_vars(input, vars);
            vars.extend(group_by.clone());
            for (_, alias) in aggregates {
                vars.insert(alias.clone());
            }
        }
        Plan::Unwind { input, alias, .. } => {
            extract_output_vars(input, vars);
            vars.insert(alias.clone());
        }
        Plan::Union { left, right, .. } => {
            extract_output_vars(left, vars);
            extract_output_vars(right, vars);
        }
        Plan::CartesianProduct { left, right } => {
            extract_output_vars(left, vars);
            extract_output_vars(right, vars);
        }
        Plan::Apply {
            input,
            subquery,
            alias: _,
        } => {
            extract_output_vars(input, vars);
            extract_output_vars(subquery, vars);
        }
        Plan::ProcedureCall {
            input,
            name: _,
            args: _,
            yields,
        } => {
            extract_output_vars(input, vars);
            for (name, alias) in yields {
                vars.insert(alias.clone().unwrap_or_else(|| name.clone()));
            }
        }
        Plan::IndexSeek {
            alias, fallback, ..
        } => {
            vars.insert(alias.clone());
            extract_output_vars(fallback, vars);
        }
        Plan::Foreach { input, .. } => extract_output_vars(input, vars),
        Plan::Values { .. } => {}
        Plan::Create { input, pattern } => {
            extract_output_vars(input, vars);
            vars.extend(extract_merge_pattern_vars(pattern));
        }
        Plan::Delete { input, .. }
        | Plan::SetProperty { input, .. }
        | Plan::RemoveProperty { input, .. } => {
            extract_output_vars(input, vars);
        }
    }
}

fn parse_aggregate_function(
    call: &crate::ast::FunctionCall,
) -> Result<Option<crate::ast::AggregateFunction>> {
    let name = call.name.to_lowercase();
    match name.as_str() {
        "count" => {
            if call.args.is_empty() {
                Ok(Some(crate::ast::AggregateFunction::Count(None)))
            } else if call.args.len() == 1 {
                if let Expression::Literal(Literal::String(s)) = &call.args[0]
                    && s == "*"
                {
                    return Ok(Some(crate::ast::AggregateFunction::Count(None)));
                }
                Ok(Some(crate::ast::AggregateFunction::Count(Some(
                    call.args[0].clone(),
                ))))
            } else {
                Err(Error::Other("COUNT takes 0 or 1 argument".into()))
            }
        }
        "sum" => {
            if call.args.len() != 1 {
                return Err(Error::Other("SUM takes exactly 1 argument".into()));
            }
            Ok(Some(crate::ast::AggregateFunction::Sum(
                call.args[0].clone(),
            )))
        }
        "avg" => {
            if call.args.len() != 1 {
                return Err(Error::Other("AVG takes exactly 1 argument".into()));
            }
            Ok(Some(crate::ast::AggregateFunction::Avg(
                call.args[0].clone(),
            )))
        }
        "min" => {
            if call.args.len() != 1 {
                return Err(Error::Other("MIN takes exactly 1 argument".into()));
            }
            Ok(Some(crate::ast::AggregateFunction::Min(
                call.args[0].clone(),
            )))
        }
        "max" => {
            if call.args.len() != 1 {
                return Err(Error::Other("MAX takes exactly 1 argument".into()));
            }
            Ok(Some(crate::ast::AggregateFunction::Max(
                call.args[0].clone(),
            )))
        }
        "collect" => {
            if call.args.len() != 1 {
                return Err(Error::Other("COLLECT takes exactly 1 argument".into()));
            }
            Ok(Some(crate::ast::AggregateFunction::Collect(
                call.args[0].clone(),
            )))
        }
        _ => Ok(None),
    }
}

fn extract_predicates(expr: &Expression, map: &mut BTreeMap<String, BTreeMap<String, Expression>>) {
    if let Expression::Binary(bin) = expr {
        if matches!(bin.operator, BinaryOperator::And) {
            extract_predicates(&bin.left, map);
            extract_predicates(&bin.right, map);
        } else if matches!(bin.operator, BinaryOperator::Equals) {
            let mut check_eq = |left: &Expression, right: &Expression| {
                if let Expression::PropertyAccess(pa) = left {
                    match right {
                        Expression::Literal(_) | Expression::Parameter(_) => {
                            map.entry(pa.variable.clone())
                                .or_default()
                                .insert(pa.property.clone(), right.clone());
                        }
                        _ => {}
                    }
                }
            };
            check_eq(&bin.left, &bin.right);
            check_eq(&bin.right, &bin.left);
        }
    }
}

fn extract_variables_from_expr(expr: &Expression, vars: &mut std::collections::HashSet<String>) {
    match expr {
        Expression::Variable(v) => {
            vars.insert(v.clone());
        }
        Expression::PropertyAccess(pa) => {
            vars.insert(pa.variable.clone());
        }
        Expression::FunctionCall(f) => {
            for arg in &f.args {
                extract_variables_from_expr(arg, vars);
            }
        }
        Expression::Binary(b) => {
            extract_variables_from_expr(&b.left, vars);
            extract_variables_from_expr(&b.right, vars);
        }
        Expression::Unary(u) => {
            extract_variables_from_expr(&u.operand, vars);
        }
        Expression::List(l) => {
            for item in l {
                extract_variables_from_expr(item, vars);
            }
        }
        Expression::Map(m) => {
            for pair in &m.properties {
                extract_variables_from_expr(&pair.value, vars);
            }
        }
        _ => {}
    }
}

fn compile_foreach_plan(
    input: Plan,
    foreach: crate::ast::ForeachClause,
    merge_subclauses: &mut VecDeque<crate::parser::MergeSubclauses>,
) -> Result<Plan> {
    // Compile updates sub-plan with a placeholder input
    let initial_input = Some(Plan::Values { rows: vec![] });

    // Wrap updates in a Query structure for compilation
    let sub_query = Query {
        clauses: foreach.updates,
    };

    let compiled_sub = compile_m3_plan(sub_query, merge_subclauses, initial_input)?;

    Ok(Plan::Foreach {
        input: Box::new(input),
        variable: foreach.variable,
        list: foreach.list,
        sub_plan: Box::new(compiled_sub.plan),
    })
}

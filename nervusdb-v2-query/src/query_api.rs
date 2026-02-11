use crate::ast::{BinaryOperator, CallClause, Clause, Expression, Literal, Query};
use crate::error::{Error, Result};
use crate::executor::{Plan, Row, Value, execute_plan, execute_write};
use nervusdb_v2_api::GraphSnapshot;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteSemantics {
    Default,
    Merge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingKind {
    Node,
    Relationship,
    Path,
    Scalar,
    Unknown,
}

const INTERNAL_PATH_ALIAS_PREFIX: &str = "__nervus_internal_path_";

fn alloc_internal_path_alias(next_anon_id: &mut u32) -> String {
    let alias = format!("{INTERNAL_PATH_ALIAS_PREFIX}{}", *next_anon_id);
    *next_anon_id += 1;
    alias
}

fn is_internal_path_alias(alias: &str) -> bool {
    alias.starts_with(INTERNAL_PATH_ALIAS_PREFIX)
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

    pub fn execute_mixed<S: GraphSnapshot>(
        &self,
        snapshot: &S,
        txn: &mut impl crate::executor::WriteableGraph,
        params: &Params,
    ) -> Result<(
        Vec<std::collections::HashMap<String, crate::executor::Value>>,
        u32,
    )> {
        if self.explain.is_some() {
            return Err(Error::Other(
                "EXPLAIN cannot be executed as a mixed query".into(),
            ));
        }

        if plan_contains_write(&self.plan) {
            return match self.write {
                WriteSemantics::Default => {
                    let (write_count, write_rows) = crate::executor::execute_write_with_rows(
                        &self.plan, snapshot, txn, params,
                    )?;

                    let mut results: Vec<
                        std::collections::HashMap<String, crate::executor::Value>,
                    > = write_rows
                        .into_iter()
                        .map(|row| {
                            let mut map = std::collections::HashMap::new();
                            for (k, v) in row.columns().iter().cloned() {
                                map.insert(k, v);
                            }
                            map
                        })
                        .collect();

                    if matches!(
                        &self.plan,
                        crate::executor::Plan::Create { .. }
                            | crate::executor::Plan::Delete { .. }
                            | crate::executor::Plan::SetProperty { .. }
                            | crate::executor::Plan::SetLabels { .. }
                            | crate::executor::Plan::RemoveProperty { .. }
                            | crate::executor::Plan::RemoveLabels { .. }
                            | crate::executor::Plan::Foreach { .. }
                    ) {
                        results.clear();
                    }

                    Ok((results, write_count))
                }
                WriteSemantics::Merge => {
                    let (write_count, write_rows) = crate::executor::execute_merge_with_rows(
                        &self.plan,
                        snapshot,
                        txn,
                        params,
                        &self.merge_on_create_items,
                        &self.merge_on_match_items,
                    )?;
                    let results: Vec<std::collections::HashMap<String, crate::executor::Value>> =
                        write_rows
                            .into_iter()
                            .map(|row| {
                                let mut map = std::collections::HashMap::new();
                                for (k, v) in row.columns().iter().cloned() {
                                    map.insert(k, v);
                                }
                                map
                            })
                            .collect();
                    Ok((results, write_count))
                }
            };
        }

        let rows: Vec<_> = crate::executor::execute_plan(snapshot, &self.plan, params).collect();
        let mut results = Vec::new();

        for row_res in rows {
            let row = row_res?;
            let mut map = std::collections::HashMap::new();
            for (k, v) in row.columns().iter().cloned() {
                map.insert(k, v);
            }
            results.push(map);
        }

        Ok((results, 0))
    }

    pub fn is_explain(&self) -> bool {
        self.explain.is_some()
    }

    /// Returns the explained plan string if this query was an EXPLAIN query.
    pub fn explain_string(&self) -> Option<&str> {
        self.explain.as_deref()
    }
}

fn plan_contains_write(plan: &Plan) -> bool {
    match plan {
        Plan::Create { .. }
        | Plan::Delete { .. }
        | Plan::SetProperty { .. }
        | Plan::SetLabels { .. }
        | Plan::RemoveProperty { .. }
        | Plan::RemoveLabels { .. }
        | Plan::Foreach { .. } => true,
        Plan::Filter { input, .. }
        | Plan::Project { input, .. }
        | Plan::Limit { input, .. }
        | Plan::Skip { input, .. }
        | Plan::OrderBy { input, .. }
        | Plan::Distinct { input }
        | Plan::Unwind { input, .. }
        | Plan::Aggregate { input, .. }
        | Plan::ProcedureCall { input, .. }
        | Plan::MatchBoundRel { input, .. } => plan_contains_write(input),
        Plan::OptionalWhereFixup {
            outer, filtered, ..
        } => plan_contains_write(outer) || plan_contains_write(filtered),
        Plan::IndexSeek { fallback, .. } => plan_contains_write(fallback),
        Plan::MatchOut { input, .. }
        | Plan::MatchIn { input, .. }
        | Plan::MatchUndirected { input, .. }
        | Plan::MatchOutVarLen { input, .. } => input.as_deref().is_some_and(plan_contains_write),
        Plan::Apply {
            input, subquery, ..
        } => plan_contains_write(input) || plan_contains_write(subquery),
        Plan::CartesianProduct { left, right } | Plan::Union { left, right, .. } => {
            plan_contains_write(left) || plan_contains_write(right)
        }
        Plan::NodeScan { .. } | Plan::ReturnOne | Plan::Values { .. } => false,
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
    let prefix_len = "EXPLAIN".len();
    if trimmed.len() < prefix_len {
        return None;
    }
    let head = trimmed.get(..prefix_len)?;
    if !head.eq_ignore_ascii_case("EXPLAIN") {
        return None;
    }
    let tail = trimmed.get(prefix_len..)?;
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

            Plan::NodeScan {
                alias,
                label,
                optional,
            } => {
                let opt = if *optional { " OPTIONAL" } else { "" };
                let _ = writeln!(out, "{pad}NodeScan{opt}(alias={alias}, label={label:?})");
            }
            Plan::MatchOut {
                input: _,
                src_alias,
                rels,
                edge_alias,
                dst_alias,
                dst_labels: _,
                src_prebound: _,
                limit,
                project: _,
                project_external: _,
                optional,
                optional_unbind: _,
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
                dst_labels: _,
                src_prebound: _,
                direction,
                min_hops,
                max_hops,
                limit,
                project: _,
                project_external: _,
                optional,
                optional_unbind: _,
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
                    "{pad}MatchOutVarLen{opt_str}(src={src_alias}, rels={rels:?}, edge={edge_alias:?}, dst={dst_alias}, dir={direction:?}, min={min_hops}, max={max_hops:?}, limit={limit:?}{path_str})"
                );
            }
            Plan::MatchIn {
                input,
                src_alias,
                rels,
                edge_alias,
                dst_alias,
                dst_labels: _,
                src_prebound: _,
                limit,
                optional,
                optional_unbind: _,
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
                dst_labels: _,
                src_prebound: _,
                limit,
                optional,
                optional_unbind: _,
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
            Plan::MatchBoundRel {
                input,
                rel_alias,
                src_alias,
                dst_alias,
                dst_labels: _,
                src_prebound: _,
                rels,
                direction,
                optional,
                optional_unbind: _,
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
                    "{pad}MatchBoundRel{opt_str}(rel={rel_alias}, src={src_alias}, rels={rels:?}, dst={dst_alias}, dir={direction:?}{path_str})"
                );
                go(out, input, depth + 1);
            }
            Plan::Filter { input, predicate } => {
                let _ = writeln!(out, "{pad}Filter(predicate={predicate:?})");
                go(out, input, depth + 1);
            }
            Plan::OptionalWhereFixup {
                outer,
                filtered,
                null_aliases,
            } => {
                let _ = writeln!(
                    out,
                    "{pad}OptionalWhereFixup(null_aliases={null_aliases:?})"
                );
                let _ = writeln!(out, "{pad}  Outer:");
                go(out, outer, depth + 2);
                let _ = writeln!(out, "{pad}  Filtered:");
                go(out, filtered, depth + 2);
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
            Plan::SetLabels { input, items } => {
                let _ = writeln!(out, "{pad}SetLabels(items={items:?})");
                go(out, input, depth + 1);
            }
            Plan::RemoveProperty { input, items } => {
                let _ = writeln!(out, "{pad}RemoveProperty(items={items:?})");
                go(out, input, depth + 1);
            }
            Plan::RemoveLabels { input, items } => {
                let _ = writeln!(out, "{pad}RemoveLabels(items={items:?})");
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
    let mut next_anon_id = 0u32;
    let mut pending_optional_where_fixup: Option<(Plan, Vec<String>)> = None;

    while let Some(clause) = clauses.next() {
        if !matches!(clause, Clause::Match(_) | Clause::Where(_)) {
            pending_optional_where_fixup = None;
        }

        match clause {
            Clause::Match(m) => {
                // Check ahead for WHERE to optimize immediately
                let mut predicates = BTreeMap::new();
                if let Some(Clause::Where(w)) = clauses.peek() {
                    extract_predicates(&w.expression, &mut predicates);
                }

                let previous_plan = plan.clone().unwrap_or(Plan::ReturnOne);
                let mut before_kinds: BTreeMap<String, BindingKind> = BTreeMap::new();
                if let Some(existing_plan) = &plan {
                    extract_output_var_kinds(existing_plan, &mut before_kinds);
                }

                let mut compiled_match = m.clone();
                if compiled_match.optional {
                    // OPTIONAL 语义由 OptionalWhereFixup 在子句边界统一处理，
                    // 避免多跳链路逐 hop 产出多余 null 行。
                    compiled_match.optional = false;
                }

                plan = Some(compile_match_plan(
                    plan,
                    compiled_match,
                    &predicates,
                    &mut next_anon_id,
                )?);

                if m.optional {
                    let mut after_kinds: BTreeMap<String, BindingKind> = BTreeMap::new();
                    if let Some(compiled_plan) = &plan {
                        extract_output_var_kinds(compiled_plan, &mut after_kinds);
                    }
                    let aliases = after_kinds
                        .keys()
                        .filter(|name| !before_kinds.contains_key(*name))
                        .cloned()
                        .collect::<Vec<_>>();

                    if matches!(clauses.peek(), Some(Clause::Where(_))) {
                        pending_optional_where_fixup = Some((previous_plan, aliases));
                    } else {
                        plan = Some(Plan::OptionalWhereFixup {
                            outer: Box::new(previous_plan),
                            filtered: Box::new(plan.unwrap()),
                            null_aliases: aliases,
                        });
                        pending_optional_where_fixup = None;
                    }
                } else {
                    pending_optional_where_fixup = None;
                }
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

                let filtered = Plan::Filter {
                    input: Box::new(plan.unwrap()),
                    predicate: w.expression.clone(),
                };

                if let Some((outer_plan, null_aliases)) = pending_optional_where_fixup.take() {
                    plan = Some(Plan::OptionalWhereFixup {
                        outer: Box::new(outer_plan),
                        filtered: Box::new(filtered),
                        null_aliases,
                    });
                } else {
                    plan = Some(filtered);
                }
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
                // For chained MERGE, each MERGE can follow previous plan
                let input = plan.unwrap_or(Plan::ReturnOne);
                let sub = merge_subclauses.pop_front().ok_or_else(|| {
                    Error::Other("internal error: missing MERGE subclauses".into())
                })?;
                let merge_vars = extract_merge_pattern_vars(&m.pattern);
                merge_on_create_items = compile_merge_set_items(&merge_vars, sub.on_create)?;
                merge_on_match_items = compile_merge_set_items(&merge_vars, sub.on_match)?;
                plan = Some(compile_merge_plan(input, m.clone())?);
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
                    "syntax error: UndefinedVariable ({})",
                    item.property.variable
                )));
            }
            items.push((item.property.variable, item.property.property, item.value));
        }
    }
    Ok(items)
}

fn is_definitely_non_boolean(expr: &Expression) -> bool {
    match expr {
        Expression::Literal(Literal::Boolean(_) | Literal::Null) => false,
        Expression::Literal(_) | Expression::List(_) | Expression::Map(_) => true,
        Expression::Unary(u) => match u.operator {
            crate::ast::UnaryOperator::Not => is_definitely_non_boolean(&u.operand),
            crate::ast::UnaryOperator::Negate => true,
        },
        Expression::Binary(b) => match b.operator {
            BinaryOperator::Equals
            | BinaryOperator::NotEquals
            | BinaryOperator::LessThan
            | BinaryOperator::LessEqual
            | BinaryOperator::GreaterThan
            | BinaryOperator::GreaterEqual
            | BinaryOperator::And
            | BinaryOperator::Or
            | BinaryOperator::Xor
            | BinaryOperator::In
            | BinaryOperator::StartsWith
            | BinaryOperator::EndsWith
            | BinaryOperator::Contains
            | BinaryOperator::HasLabel
            | BinaryOperator::IsNull
            | BinaryOperator::IsNotNull => false,
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Modulo
            | BinaryOperator::Power => true,
        },
        Expression::Parameter(_)
        | Expression::Variable(_)
        | Expression::PropertyAccess(_)
        | Expression::FunctionCall(_)
        | Expression::Case(_)
        | Expression::Exists(_)
        | Expression::ListComprehension(_)
        | Expression::PatternComprehension(_) => false,
    }
}

fn is_definitely_non_list_literal(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::Literal(
            Literal::Boolean(_) | Literal::Integer(_) | Literal::Float(_) | Literal::String(_)
        ) | Expression::Map(_)
    )
}

fn validate_expression_types(expr: &Expression) -> Result<()> {
    match expr {
        Expression::Unary(u) => {
            validate_expression_types(&u.operand)?;
            if matches!(u.operator, crate::ast::UnaryOperator::Not)
                && is_definitely_non_boolean(&u.operand)
            {
                return Err(Error::Other(
                    "syntax error: InvalidArgumentType".to_string(),
                ));
            }
            Ok(())
        }
        Expression::Binary(b) => {
            validate_expression_types(&b.left)?;
            validate_expression_types(&b.right)?;
            if matches!(b.operator, BinaryOperator::In) && is_definitely_non_list_literal(&b.right)
            {
                return Err(Error::Other(
                    "syntax error: InvalidArgumentType".to_string(),
                ));
            }
            if matches!(
                b.operator,
                BinaryOperator::And | BinaryOperator::Or | BinaryOperator::Xor
            ) && (is_definitely_non_boolean(&b.left) || is_definitely_non_boolean(&b.right))
            {
                return Err(Error::Other(
                    "syntax error: InvalidArgumentType".to_string(),
                ));
            }
            Ok(())
        }
        Expression::FunctionCall(call) => {
            for arg in &call.args {
                validate_expression_types(arg)?;
            }
            if call.name.eq_ignore_ascii_case("properties") {
                if call.args.len() != 1 {
                    return Err(Error::Other(
                        "syntax error: InvalidArgumentType".to_string(),
                    ));
                }
                if matches!(
                    call.args[0],
                    Expression::Literal(Literal::Integer(_) | Literal::Float(_))
                        | Expression::Literal(Literal::String(_))
                        | Expression::Literal(Literal::Boolean(_))
                        | Expression::List(_)
                ) {
                    return Err(Error::Other(
                        "syntax error: InvalidArgumentType".to_string(),
                    ));
                }
            }
            Ok(())
        }
        Expression::List(items) => {
            for item in items {
                validate_expression_types(item)?;
            }
            Ok(())
        }
        Expression::ListComprehension(list_comp) => {
            validate_expression_types(&list_comp.list)?;
            if let Some(where_expr) = &list_comp.where_expression {
                validate_expression_types(where_expr)?;
            }
            if let Some(map_expr) = &list_comp.map_expression {
                validate_expression_types(map_expr)?;
            }
            Ok(())
        }
        Expression::PatternComprehension(pattern_comp) => {
            for element in &pattern_comp.pattern.elements {
                match element {
                    crate::ast::PathElement::Node(node) => {
                        if let Some(props) = &node.properties {
                            for pair in &props.properties {
                                validate_expression_types(&pair.value)?;
                            }
                        }
                    }
                    crate::ast::PathElement::Relationship(rel) => {
                        if let Some(props) = &rel.properties {
                            for pair in &props.properties {
                                validate_expression_types(&pair.value)?;
                            }
                        }
                    }
                }
            }
            if let Some(where_expr) = &pattern_comp.where_expression {
                validate_expression_types(where_expr)?;
            }
            validate_expression_types(&pattern_comp.projection)?;
            Ok(())
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                validate_expression_types(&pair.value)?;
            }
            Ok(())
        }
        Expression::Case(case_expr) => {
            if let Some(test) = &case_expr.expression {
                validate_expression_types(test)?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                validate_expression_types(when_expr)?;
                validate_expression_types(then_expr)?;
            }
            if let Some(otherwise) = &case_expr.else_expression {
                validate_expression_types(otherwise)?;
            }
            Ok(())
        }
        Expression::Exists(exists) => {
            match exists.as_ref() {
                crate::ast::ExistsExpression::Pattern(pattern) => {
                    for element in &pattern.elements {
                        match element {
                            crate::ast::PathElement::Node(node) => {
                                if let Some(props) = &node.properties {
                                    for pair in &props.properties {
                                        validate_expression_types(&pair.value)?;
                                    }
                                }
                            }
                            crate::ast::PathElement::Relationship(rel) => {
                                if let Some(props) = &rel.properties {
                                    for pair in &props.properties {
                                        validate_expression_types(&pair.value)?;
                                    }
                                }
                            }
                        }
                    }
                }
                crate::ast::ExistsExpression::Subquery(subquery) => {
                    for clause in &subquery.clauses {
                        match clause {
                            Clause::Where(w) => validate_expression_types(&w.expression)?,
                            Clause::With(w) => {
                                for item in &w.items {
                                    validate_expression_types(&item.expression)?;
                                }
                                if let Some(where_clause) = &w.where_clause {
                                    validate_expression_types(&where_clause.expression)?;
                                }
                            }
                            Clause::Return(r) => {
                                for item in &r.items {
                                    validate_expression_types(&item.expression)?;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_where_expression_bindings(
    expr: &Expression,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> Result<()> {
    let mut local_scopes: Vec<HashSet<String>> = Vec::new();
    validate_where_expression_variables(expr, known_bindings, &mut local_scopes)?;

    validate_pattern_predicate_bindings(expr, known_bindings)?;
    if matches!(
        infer_expression_binding_kind(expr, known_bindings),
        BindingKind::Node | BindingKind::Relationship | BindingKind::Path
    ) {
        return Err(Error::Other(
            "syntax error: InvalidArgumentType".to_string(),
        ));
    }
    Ok(())
}

fn validate_where_expression_variables(
    expr: &Expression,
    known_bindings: &BTreeMap<String, BindingKind>,
    local_scopes: &mut Vec<HashSet<String>>,
) -> Result<()> {
    match expr {
        Expression::Variable(var) => {
            if !is_locally_bound(local_scopes, var) && !known_bindings.contains_key(var) {
                return Err(Error::Other(format!(
                    "syntax error: UndefinedVariable ({})",
                    var
                )));
            }
        }
        Expression::PropertyAccess(pa) => {
            if !is_locally_bound(local_scopes, &pa.variable)
                && !known_bindings.contains_key(&pa.variable)
            {
                return Err(Error::Other(format!(
                    "syntax error: UndefinedVariable ({})",
                    pa.variable
                )));
            }
        }
        Expression::Unary(u) => {
            validate_where_expression_variables(&u.operand, known_bindings, local_scopes)?;
        }
        Expression::Binary(b) => {
            validate_where_expression_variables(&b.left, known_bindings, local_scopes)?;
            validate_where_expression_variables(&b.right, known_bindings, local_scopes)?;
        }
        Expression::FunctionCall(call) => {
            for arg in &call.args {
                validate_where_expression_variables(arg, known_bindings, local_scopes)?;
            }
        }
        Expression::List(items) => {
            for item in items {
                validate_where_expression_variables(item, known_bindings, local_scopes)?;
            }
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                validate_where_expression_variables(&pair.value, known_bindings, local_scopes)?;
            }
        }
        Expression::Case(case_expr) => {
            if let Some(test_expr) = &case_expr.expression {
                validate_where_expression_variables(test_expr, known_bindings, local_scopes)?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                validate_where_expression_variables(when_expr, known_bindings, local_scopes)?;
                validate_where_expression_variables(then_expr, known_bindings, local_scopes)?;
            }
            if let Some(else_expr) = &case_expr.else_expression {
                validate_where_expression_variables(else_expr, known_bindings, local_scopes)?;
            }
        }
        Expression::ListComprehension(list_comp) => {
            validate_where_expression_variables(&list_comp.list, known_bindings, local_scopes)?;
            let mut scope = HashSet::new();
            scope.insert(list_comp.variable.clone());
            local_scopes.push(scope);
            if let Some(where_expr) = &list_comp.where_expression {
                validate_where_expression_variables(where_expr, known_bindings, local_scopes)?;
            }
            if let Some(map_expr) = &list_comp.map_expression {
                validate_where_expression_variables(map_expr, known_bindings, local_scopes)?;
            }
            local_scopes.pop();
        }
        Expression::PatternComprehension(pattern_comp) => {
            let scope = collect_pattern_local_variables(&pattern_comp.pattern);
            local_scopes.push(scope);

            for element in &pattern_comp.pattern.elements {
                match element {
                    crate::ast::PathElement::Node(node) => {
                        if let Some(props) = &node.properties {
                            for pair in &props.properties {
                                validate_where_expression_variables(
                                    &pair.value,
                                    known_bindings,
                                    local_scopes,
                                )?;
                            }
                        }
                    }
                    crate::ast::PathElement::Relationship(rel) => {
                        if let Some(props) = &rel.properties {
                            for pair in &props.properties {
                                validate_where_expression_variables(
                                    &pair.value,
                                    known_bindings,
                                    local_scopes,
                                )?;
                            }
                        }
                    }
                }
            }

            if let Some(where_expr) = &pattern_comp.where_expression {
                validate_where_expression_variables(where_expr, known_bindings, local_scopes)?;
            }
            validate_where_expression_variables(
                &pattern_comp.projection,
                known_bindings,
                local_scopes,
            )?;

            local_scopes.pop();
        }
        Expression::Exists(_) | Expression::Parameter(_) | Expression::Literal(_) => {}
    }
    Ok(())
}

fn is_locally_bound(local_scopes: &[HashSet<String>], var: &str) -> bool {
    local_scopes.iter().rev().any(|scope| scope.contains(var))
}

fn collect_pattern_local_variables(pattern: &crate::ast::Pattern) -> HashSet<String> {
    let mut vars = HashSet::new();
    if let Some(path_var) = &pattern.variable {
        vars.insert(path_var.clone());
    }

    for element in &pattern.elements {
        match element {
            crate::ast::PathElement::Node(node) => {
                if let Some(var) = &node.variable {
                    vars.insert(var.clone());
                }
            }
            crate::ast::PathElement::Relationship(rel) => {
                if let Some(var) = &rel.variable {
                    vars.insert(var.clone());
                }
            }
        }
    }

    vars
}

fn validate_pattern_predicate_bindings(
    expr: &Expression,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> Result<()> {
    match expr {
        Expression::Exists(exists_expr) => match exists_expr.as_ref() {
            crate::ast::ExistsExpression::Pattern(pattern) => {
                if pattern.elements.len() < 3 {
                    return Err(Error::Other(
                        "syntax error: InvalidArgumentType".to_string(),
                    ));
                }
                if let Some(path_var) = &pattern.variable
                    && !known_bindings.contains_key(path_var)
                {
                    return Err(Error::Other(format!(
                        "syntax error: UndefinedVariable ({})",
                        path_var
                    )));
                }
                for element in &pattern.elements {
                    match element {
                        crate::ast::PathElement::Node(node) => {
                            if let Some(var) = &node.variable
                                && !known_bindings.contains_key(var)
                            {
                                return Err(Error::Other(format!(
                                    "syntax error: UndefinedVariable ({})",
                                    var
                                )));
                            }
                            if let Some(props) = &node.properties {
                                for pair in &props.properties {
                                    validate_pattern_predicate_bindings(
                                        &pair.value,
                                        known_bindings,
                                    )?;
                                }
                            }
                        }
                        crate::ast::PathElement::Relationship(rel) => {
                            if let Some(var) = &rel.variable
                                && !known_bindings.contains_key(var)
                            {
                                return Err(Error::Other(format!(
                                    "syntax error: UndefinedVariable ({})",
                                    var
                                )));
                            }
                            if let Some(props) = &rel.properties {
                                for pair in &props.properties {
                                    validate_pattern_predicate_bindings(
                                        &pair.value,
                                        known_bindings,
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
            crate::ast::ExistsExpression::Subquery(subquery) => {
                for clause in &subquery.clauses {
                    match clause {
                        Clause::Where(w) => {
                            validate_pattern_predicate_bindings(&w.expression, known_bindings)?
                        }
                        Clause::With(w) => {
                            for item in &w.items {
                                validate_pattern_predicate_bindings(
                                    &item.expression,
                                    known_bindings,
                                )?;
                            }
                            if let Some(where_clause) = &w.where_clause {
                                validate_pattern_predicate_bindings(
                                    &where_clause.expression,
                                    known_bindings,
                                )?;
                            }
                        }
                        Clause::Return(r) => {
                            for item in &r.items {
                                validate_pattern_predicate_bindings(
                                    &item.expression,
                                    known_bindings,
                                )?;
                            }
                        }
                        _ => {}
                    }
                }
            }
        },
        Expression::Unary(u) => validate_pattern_predicate_bindings(&u.operand, known_bindings)?,
        Expression::Binary(b) => {
            validate_pattern_predicate_bindings(&b.left, known_bindings)?;
            validate_pattern_predicate_bindings(&b.right, known_bindings)?;
        }
        Expression::FunctionCall(call) => {
            for arg in &call.args {
                validate_pattern_predicate_bindings(arg, known_bindings)?;
            }
        }
        Expression::List(items) => {
            for item in items {
                validate_pattern_predicate_bindings(item, known_bindings)?;
            }
        }
        Expression::ListComprehension(list_comp) => {
            validate_pattern_predicate_bindings(&list_comp.list, known_bindings)?;
            if let Some(where_expr) = &list_comp.where_expression {
                validate_pattern_predicate_bindings(where_expr, known_bindings)?;
            }
            if let Some(map_expr) = &list_comp.map_expression {
                validate_pattern_predicate_bindings(map_expr, known_bindings)?;
            }
        }
        Expression::PatternComprehension(pattern_comp) => {
            let mut scoped = known_bindings.clone();
            for var in collect_pattern_local_variables(&pattern_comp.pattern) {
                scoped.entry(var).or_insert(BindingKind::Unknown);
            }

            for element in &pattern_comp.pattern.elements {
                match element {
                    crate::ast::PathElement::Node(node) => {
                        if let Some(props) = &node.properties {
                            for pair in &props.properties {
                                validate_pattern_predicate_bindings(&pair.value, &scoped)?;
                            }
                        }
                    }
                    crate::ast::PathElement::Relationship(rel) => {
                        if let Some(props) = &rel.properties {
                            for pair in &props.properties {
                                validate_pattern_predicate_bindings(&pair.value, &scoped)?;
                            }
                        }
                    }
                }
            }

            if let Some(where_expr) = &pattern_comp.where_expression {
                validate_pattern_predicate_bindings(where_expr, &scoped)?;
            }
            validate_pattern_predicate_bindings(&pattern_comp.projection, &scoped)?;
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                validate_pattern_predicate_bindings(&pair.value, known_bindings)?;
            }
        }
        Expression::Case(case_expr) => {
            if let Some(test_expr) = &case_expr.expression {
                validate_pattern_predicate_bindings(test_expr, known_bindings)?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                validate_pattern_predicate_bindings(when_expr, known_bindings)?;
                validate_pattern_predicate_bindings(then_expr, known_bindings)?;
            }
            if let Some(else_expr) = &case_expr.else_expression {
                validate_pattern_predicate_bindings(else_expr, known_bindings)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn ensure_no_pattern_predicate(expr: &Expression) -> Result<()> {
    if contains_pattern_predicate(expr) {
        return Err(Error::Other("syntax error: UnexpectedSyntax".to_string()));
    }
    Ok(())
}

fn contains_pattern_predicate(expr: &Expression) -> bool {
    match expr {
        Expression::Exists(exists_expr) => {
            matches!(
                exists_expr.as_ref(),
                crate::ast::ExistsExpression::Pattern(_)
            )
        }
        Expression::Unary(u) => contains_pattern_predicate(&u.operand),
        Expression::Binary(b) => {
            contains_pattern_predicate(&b.left) || contains_pattern_predicate(&b.right)
        }
        Expression::FunctionCall(call) => call.args.iter().any(contains_pattern_predicate),
        Expression::List(items) => items.iter().any(contains_pattern_predicate),
        Expression::ListComprehension(list_comp) => {
            contains_pattern_predicate(&list_comp.list)
                || list_comp
                    .where_expression
                    .as_ref()
                    .is_some_and(contains_pattern_predicate)
                || list_comp
                    .map_expression
                    .as_ref()
                    .is_some_and(contains_pattern_predicate)
        }
        Expression::PatternComprehension(pattern_comp) => {
            pattern_comp
                .where_expression
                .as_ref()
                .is_some_and(contains_pattern_predicate)
                || contains_pattern_predicate(&pattern_comp.projection)
                || pattern_comp
                    .pattern
                    .elements
                    .iter()
                    .any(|element| match element {
                        crate::ast::PathElement::Node(node) => {
                            node.properties.as_ref().is_some_and(|props| {
                                props
                                    .properties
                                    .iter()
                                    .any(|pair| contains_pattern_predicate(&pair.value))
                            })
                        }
                        crate::ast::PathElement::Relationship(rel) => {
                            rel.properties.as_ref().is_some_and(|props| {
                                props
                                    .properties
                                    .iter()
                                    .any(|pair| contains_pattern_predicate(&pair.value))
                            })
                        }
                    })
        }
        Expression::Map(map) => map
            .properties
            .iter()
            .any(|pair| contains_pattern_predicate(&pair.value)),
        Expression::Case(case_expr) => {
            case_expr
                .expression
                .as_ref()
                .is_some_and(contains_pattern_predicate)
                || case_expr.when_clauses.iter().any(|(when_expr, then_expr)| {
                    contains_pattern_predicate(when_expr) || contains_pattern_predicate(then_expr)
                })
                || case_expr
                    .else_expression
                    .as_ref()
                    .is_some_and(contains_pattern_predicate)
        }
        _ => false,
    }
}

fn compile_with_plan(input: Plan, with: &crate::ast::WithClause) -> Result<Plan> {
    // 1. Projection / Aggregation
    // WITH is identical to RETURN in structure: items, orderBy, skip, limit, where.
    // It projects the input to a new set of variables.

    let has_aggregation = with
        .items
        .iter()
        .any(|item| contains_aggregate_expression(&item.expression));
    let mut input_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();
    extract_output_var_kinds(&input, &mut input_bindings);

    let (mut plan, project_cols) = compile_projection_aggregation(input, &with.items)?;

    // 2. WHERE
    if let Some(w) = &with.where_clause {
        validate_expression_types(&w.expression)?;
        let mut where_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();
        extract_output_var_kinds(&plan, &mut where_bindings);
        if !has_aggregation {
            for (name, kind) in &input_bindings {
                where_bindings.entry(name.clone()).or_insert(*kind);
            }
        }
        validate_where_expression_bindings(&w.expression, &where_bindings)?;

        let mut passthrough: Vec<String> = Vec::new();
        if !has_aggregation {
            let projected: HashSet<String> = project_cols.iter().cloned().collect();
            let mut used = HashSet::new();
            extract_variables_from_expr(&w.expression, &mut used);
            passthrough = used
                .into_iter()
                .filter(|name| !projected.contains(name) && input_bindings.contains_key(name))
                .collect();
            passthrough.sort();
            passthrough.dedup();

            if !passthrough.is_empty()
                && let Plan::Project {
                    input,
                    mut projections,
                } = plan
            {
                for name in &passthrough {
                    projections.push((name.clone(), Expression::Variable(name.clone())));
                }
                plan = Plan::Project { input, projections };
            }
        }

        plan = Plan::Filter {
            input: Box::new(plan),
            predicate: w.expression.clone(),
        };

        if !has_aggregation && !passthrough.is_empty() {
            plan = Plan::Project {
                input: Box::new(plan),
                projections: project_cols
                    .iter()
                    .cloned()
                    .map(|name| (name.clone(), Expression::Variable(name)))
                    .collect(),
            };
        }
    }

    // 3. ORDER BY
    if let Some(order_by) = &with.order_by {
        let rewrite_bindings: Vec<(Expression, String)> = with
            .items
            .iter()
            .filter_map(|item| {
                item.alias
                    .as_ref()
                    .map(|alias| (item.expression.clone(), alias.clone()))
            })
            .collect();

        let mut normalized = order_by.clone();
        for item in &mut normalized.items {
            item.expression = rewrite_order_expression(&item.expression, &rewrite_bindings);
        }

        validate_order_by_scope(&normalized, &project_cols, &with.items)?;
        let items = compile_order_by_items(&normalized)?;
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
        validate_order_by_scope(order_by, &project_cols, &ret.items)?;
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

fn binary_operator_symbol(operator: &BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Equals => "=",
        BinaryOperator::NotEquals => "<>",
        BinaryOperator::LessThan => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::GreaterThan => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::And => "AND",
        BinaryOperator::Or => "OR",
        BinaryOperator::Xor => "XOR",
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Modulo => "%",
        BinaryOperator::Power => "^",
        BinaryOperator::In => "IN",
        BinaryOperator::StartsWith => "STARTS WITH",
        BinaryOperator::EndsWith => "ENDS WITH",
        BinaryOperator::Contains => "CONTAINS",
        BinaryOperator::HasLabel => ":",
        BinaryOperator::IsNull => "IS NULL",
        BinaryOperator::IsNotNull => "IS NOT NULL",
    }
}

fn unary_operator_symbol(operator: &crate::ast::UnaryOperator) -> &'static str {
    match operator {
        crate::ast::UnaryOperator::Not => "NOT ",
        crate::ast::UnaryOperator::Negate => "-",
    }
}

fn is_simple_property_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn expression_alias_fragment(expr: &Expression) -> String {
    match expr {
        Expression::Variable(name) => name.clone(),
        Expression::PropertyAccess(pa) => format!("{}.{}", pa.variable, pa.property),
        Expression::Literal(Literal::String(s)) if s == "*" => "*".to_string(),
        Expression::Literal(Literal::Integer(n)) => n.to_string(),
        Expression::Literal(Literal::Float(n)) => n.to_string(),
        Expression::Literal(Literal::Boolean(b)) => b.to_string(),
        Expression::Literal(Literal::String(s)) => format!("'{}'", s),
        Expression::Literal(Literal::Null) => "null".to_string(),
        Expression::Parameter(name) => format!("${}", name),
        Expression::FunctionCall(call) => {
            if call.name.eq_ignore_ascii_case("__index") && call.args.len() == 2 {
                return format!(
                    "{}[{}]",
                    expression_alias_fragment(&call.args[0]),
                    expression_alias_fragment(&call.args[1])
                );
            }
            if call.name.eq_ignore_ascii_case("__getprop") && call.args.len() == 2 {
                let raw_base = expression_alias_fragment(&call.args[0]);
                let base = if matches!(
                    call.args[0],
                    Expression::Variable(_) | Expression::PropertyAccess(_)
                ) {
                    raw_base
                } else {
                    format!("({raw_base})")
                };
                if let Expression::Literal(Literal::String(key)) = &call.args[1] {
                    if is_simple_property_name(key) {
                        return format!("{base}.{key}");
                    }
                    return format!("{base}['{}']", key.replace('\'', "\\'"));
                }
                return format!("{base}[{}]", expression_alias_fragment(&call.args[1]));
            }
            let args = call
                .args
                .iter()
                .map(expression_alias_fragment)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", call.name.to_lowercase(), args)
        }
        Expression::Binary(b) => match b.operator {
            BinaryOperator::IsNull => {
                format!(
                    "{} {}",
                    expression_alias_fragment(&b.left),
                    binary_operator_symbol(&b.operator)
                )
            }
            BinaryOperator::IsNotNull => {
                format!(
                    "{} {}",
                    expression_alias_fragment(&b.left),
                    binary_operator_symbol(&b.operator)
                )
            }
            BinaryOperator::HasLabel => format!(
                "{}:{}",
                expression_alias_fragment(&b.left),
                expression_alias_fragment(&b.right)
            ),
            _ => format!(
                "{} {} {}",
                expression_alias_fragment(&b.left),
                binary_operator_symbol(&b.operator),
                expression_alias_fragment(&b.right)
            ),
        },
        Expression::Unary(u) => format!(
            "{}{}",
            unary_operator_symbol(&u.operator),
            expression_alias_fragment(&u.operand)
        ),
        Expression::List(items) => format!(
            "[{}]",
            items
                .iter()
                .map(expression_alias_fragment)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expression::Map(map) => {
            let inner = map
                .properties
                .iter()
                .map(|pair| format!("{}: {}", pair.key, expression_alias_fragment(&pair.value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{}}}", inner)
        }
        Expression::Case(_) => "case(...)".to_string(),
        Expression::Exists(_) => "exists(...)".to_string(),
        _ => "...".to_string(),
    }
}

fn default_projection_alias(expr: &Expression, index: usize) -> String {
    let alias = expression_alias_fragment(expr);
    if alias.is_empty() || alias == "..." || alias.len() > 120 {
        format!("expr_{}", index)
    } else {
        alias
    }
}

fn default_aggregate_alias(call: &crate::ast::FunctionCall, index: usize) -> String {
    let name = call.name.to_lowercase();
    if call.args.is_empty() {
        return format!("{}()", name);
    }

    if call.args.len() == 1 {
        return format!("{}({})", name, expression_alias_fragment(&call.args[0]));
    }

    let args = call
        .args
        .iter()
        .map(expression_alias_fragment)
        .collect::<Vec<_>>()
        .join(", ");
    let alias = format!("{}({})", name, args);

    if alias.len() > 80 {
        format!("agg_{}", index)
    } else {
        alias
    }
}

fn is_simple_group_expression(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::Variable(_) | Expression::PropertyAccess(_)
    )
}

fn contains_function_call_named(expr: &Expression, target: &str) -> bool {
    match expr {
        Expression::FunctionCall(call) => {
            if call.name.eq_ignore_ascii_case(target) {
                return true;
            }
            call.args
                .iter()
                .any(|arg| contains_function_call_named(arg, target))
        }
        Expression::Binary(b) => {
            contains_function_call_named(&b.left, target)
                || contains_function_call_named(&b.right, target)
        }
        Expression::Unary(u) => contains_function_call_named(&u.operand, target),
        Expression::List(items) => items
            .iter()
            .any(|item| contains_function_call_named(item, target)),
        Expression::Map(map) => map
            .properties
            .iter()
            .any(|pair| contains_function_call_named(&pair.value, target)),
        Expression::Case(case_expr) => {
            case_expr
                .expression
                .as_ref()
                .is_some_and(|expr| contains_function_call_named(expr, target))
                || case_expr.when_clauses.iter().any(|(w, t)| {
                    contains_function_call_named(w, target)
                        || contains_function_call_named(t, target)
                })
                || case_expr
                    .else_expression
                    .as_ref()
                    .is_some_and(|expr| contains_function_call_named(expr, target))
        }
        _ => false,
    }
}

fn expression_uses_allowed_group_refs(
    expr: &Expression,
    grouping_keys: &[(Expression, String)],
    grouping_aliases: &std::collections::HashSet<String>,
) -> bool {
    match expr {
        Expression::Literal(_) | Expression::Parameter(_) => true,
        Expression::Variable(v) => {
            grouping_aliases.contains(v)
                || grouping_keys.iter().any(|(group_expr, alias)| {
                    alias == v || matches!(group_expr, Expression::Variable(name) if name == v)
                })
        }
        Expression::PropertyAccess(pa) => {
            let dotted = format!("{}.{}", pa.variable, pa.property);
            grouping_aliases.contains(&dotted) || grouping_keys.iter().any(|(group_expr, alias)| {
                alias == &dotted
                    || matches!(group_expr, Expression::PropertyAccess(group_pa) if group_pa == pa)
                    || matches!(group_expr, Expression::Variable(var) if var == &pa.variable)
            })
        }
        Expression::Unary(u) => {
            expression_uses_allowed_group_refs(&u.operand, grouping_keys, grouping_aliases)
        }
        Expression::Binary(b) => {
            expression_uses_allowed_group_refs(&b.left, grouping_keys, grouping_aliases)
                && expression_uses_allowed_group_refs(&b.right, grouping_keys, grouping_aliases)
        }
        Expression::FunctionCall(call) => call
            .args
            .iter()
            .all(|arg| expression_uses_allowed_group_refs(arg, grouping_keys, grouping_aliases)),
        Expression::List(items) => items
            .iter()
            .all(|item| expression_uses_allowed_group_refs(item, grouping_keys, grouping_aliases)),
        Expression::Map(map) => map.properties.iter().all(|pair| {
            expression_uses_allowed_group_refs(&pair.value, grouping_keys, grouping_aliases)
        }),
        Expression::Case(case_expr) => {
            case_expr.expression.as_ref().map_or(true, |expr| {
                expression_uses_allowed_group_refs(expr, grouping_keys, grouping_aliases)
            }) && case_expr.when_clauses.iter().all(|(when_expr, then_expr)| {
                expression_uses_allowed_group_refs(when_expr, grouping_keys, grouping_aliases)
                    && expression_uses_allowed_group_refs(
                        then_expr,
                        grouping_keys,
                        grouping_aliases,
                    )
            }) && case_expr.else_expression.as_ref().map_or(true, |expr| {
                expression_uses_allowed_group_refs(expr, grouping_keys, grouping_aliases)
            })
        }
        _ => false,
    }
}

fn validate_aggregate_mixed_expression(
    expr: &Expression,
    grouping_keys: &[(Expression, String)],
    grouping_aliases: &std::collections::HashSet<String>,
) -> Result<()> {
    if !contains_aggregate_expression(expr) {
        return Ok(());
    }

    let valid = validate_aggregate_mixed_expression_impl(expr, grouping_keys, grouping_aliases)?;
    if valid {
        Ok(())
    } else {
        Err(Error::Other(
            "syntax error: AmbiguousAggregationExpression".to_string(),
        ))
    }
}

fn validate_aggregate_mixed_expression_impl(
    expr: &Expression,
    grouping_keys: &[(Expression, String)],
    grouping_aliases: &std::collections::HashSet<String>,
) -> Result<bool> {
    if !contains_aggregate_expression(expr) {
        return Ok(expression_uses_allowed_group_refs(
            expr,
            grouping_keys,
            grouping_aliases,
        ));
    }

    match expr {
        Expression::FunctionCall(call) => {
            if parse_aggregate_function(call)?.is_some() {
                return Ok(true);
            }

            for arg in &call.args {
                if !validate_aggregate_mixed_expression_impl(arg, grouping_keys, grouping_aliases)?
                {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Expression::Binary(b) => Ok(validate_aggregate_mixed_expression_impl(
            &b.left,
            grouping_keys,
            grouping_aliases,
        )? && validate_aggregate_mixed_expression_impl(
            &b.right,
            grouping_keys,
            grouping_aliases,
        )?),
        Expression::Unary(u) => {
            validate_aggregate_mixed_expression_impl(&u.operand, grouping_keys, grouping_aliases)
        }
        Expression::List(items) => {
            for item in items {
                if !validate_aggregate_mixed_expression_impl(item, grouping_keys, grouping_aliases)?
                {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                if !validate_aggregate_mixed_expression_impl(
                    &pair.value,
                    grouping_keys,
                    grouping_aliases,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Expression::Case(case_expr) => {
            if let Some(test_expr) = &case_expr.expression
                && !validate_aggregate_mixed_expression_impl(
                    test_expr,
                    grouping_keys,
                    grouping_aliases,
                )?
            {
                return Ok(false);
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                if !validate_aggregate_mixed_expression_impl(
                    when_expr,
                    grouping_keys,
                    grouping_aliases,
                )? {
                    return Ok(false);
                }
                if !validate_aggregate_mixed_expression_impl(
                    then_expr,
                    grouping_keys,
                    grouping_aliases,
                )? {
                    return Ok(false);
                }
            }
            if let Some(else_expr) = &case_expr.else_expression
                && !validate_aggregate_mixed_expression_impl(
                    else_expr,
                    grouping_keys,
                    grouping_aliases,
                )?
            {
                return Ok(false);
            }
            Ok(true)
        }
        _ => Ok(expression_uses_allowed_group_refs(
            expr,
            grouping_keys,
            grouping_aliases,
        )),
    }
}

fn collect_aggregate_calls(
    expr: &Expression,
    out: &mut Vec<(Expression, crate::ast::AggregateFunction, String)>,
) -> Result<()> {
    match expr {
        Expression::FunctionCall(call) => {
            if let Some(agg) = parse_aggregate_function(call)? {
                for arg in &call.args {
                    if contains_aggregate_expression(arg) {
                        return Err(Error::Other("syntax error: NestedAggregation".to_string()));
                    }
                    if contains_function_call_named(arg, "rand") {
                        return Err(Error::Other(
                            "syntax error: NonConstantExpression".to_string(),
                        ));
                    }
                }

                if !out.iter().any(|(existing, _, _)| existing == expr) {
                    let alias = format!("__agg_{}", out.len());
                    out.push((expr.clone(), agg, alias));
                }
                return Ok(());
            }

            for arg in &call.args {
                collect_aggregate_calls(arg, out)?;
            }
            Ok(())
        }
        Expression::Binary(b) => {
            collect_aggregate_calls(&b.left, out)?;
            collect_aggregate_calls(&b.right, out)
        }
        Expression::Unary(u) => collect_aggregate_calls(&u.operand, out),
        Expression::List(items) => {
            for item in items {
                collect_aggregate_calls(item, out)?;
            }
            Ok(())
        }
        Expression::Map(map) => {
            for pair in &map.properties {
                collect_aggregate_calls(&pair.value, out)?;
            }
            Ok(())
        }
        Expression::Case(case_expr) => {
            if let Some(test_expr) = &case_expr.expression {
                collect_aggregate_calls(test_expr, out)?;
            }
            for (when_expr, then_expr) in &case_expr.when_clauses {
                collect_aggregate_calls(when_expr, out)?;
                collect_aggregate_calls(then_expr, out)?;
            }
            if let Some(else_expr) = &case_expr.else_expression {
                collect_aggregate_calls(else_expr, out)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn resolve_aggregate_alias(expr: &Expression, mappings: &[(Expression, String)]) -> Option<String> {
    mappings
        .iter()
        .find_map(|(aggregate_expr, alias)| (aggregate_expr == expr).then(|| alias.clone()))
}

fn rewrite_aggregate_references(
    expr: &Expression,
    mappings: &[(Expression, String)],
) -> Expression {
    if let Some(alias) = resolve_aggregate_alias(expr, mappings) {
        return Expression::Variable(alias);
    }

    match expr {
        Expression::Binary(b) => Expression::Binary(Box::new(crate::ast::BinaryExpression {
            left: rewrite_aggregate_references(&b.left, mappings),
            operator: b.operator.clone(),
            right: rewrite_aggregate_references(&b.right, mappings),
        })),
        Expression::Unary(u) => Expression::Unary(Box::new(crate::ast::UnaryExpression {
            operator: u.operator.clone(),
            operand: rewrite_aggregate_references(&u.operand, mappings),
        })),
        Expression::FunctionCall(call) => Expression::FunctionCall(crate::ast::FunctionCall {
            name: call.name.clone(),
            args: call
                .args
                .iter()
                .map(|arg| rewrite_aggregate_references(arg, mappings))
                .collect(),
        }),
        Expression::List(items) => Expression::List(
            items
                .iter()
                .map(|item| rewrite_aggregate_references(item, mappings))
                .collect(),
        ),
        Expression::Map(map) => Expression::Map(crate::ast::PropertyMap {
            properties: map
                .properties
                .iter()
                .map(|pair| crate::ast::PropertyPair {
                    key: pair.key.clone(),
                    value: rewrite_aggregate_references(&pair.value, mappings),
                })
                .collect(),
        }),
        Expression::Case(case_expr) => Expression::Case(Box::new(crate::ast::CaseExpression {
            expression: case_expr
                .expression
                .as_ref()
                .map(|expr| rewrite_aggregate_references(expr, mappings)),
            when_clauses: case_expr
                .when_clauses
                .iter()
                .map(|(when_expr, then_expr)| {
                    (
                        rewrite_aggregate_references(when_expr, mappings),
                        rewrite_aggregate_references(then_expr, mappings),
                    )
                })
                .collect(),
            else_expression: case_expr
                .else_expression
                .as_ref()
                .map(|expr| rewrite_aggregate_references(expr, mappings)),
        })),
        _ => expr.clone(),
    }
}

fn rewrite_group_key_references(
    expr: &Expression,
    grouping_keys: &[(Expression, String)],
) -> Expression {
    if let Some(alias) = grouping_keys
        .iter()
        .find_map(|(group_expr, alias)| (group_expr == expr).then(|| alias.clone()))
    {
        return Expression::Variable(alias);
    }

    match expr {
        Expression::Binary(b) => Expression::Binary(Box::new(crate::ast::BinaryExpression {
            left: rewrite_group_key_references(&b.left, grouping_keys),
            operator: b.operator.clone(),
            right: rewrite_group_key_references(&b.right, grouping_keys),
        })),
        Expression::Unary(u) => Expression::Unary(Box::new(crate::ast::UnaryExpression {
            operator: u.operator.clone(),
            operand: rewrite_group_key_references(&u.operand, grouping_keys),
        })),
        Expression::FunctionCall(call) => Expression::FunctionCall(crate::ast::FunctionCall {
            name: call.name.clone(),
            args: call
                .args
                .iter()
                .map(|arg| rewrite_group_key_references(arg, grouping_keys))
                .collect(),
        }),
        Expression::List(items) => Expression::List(
            items
                .iter()
                .map(|item| rewrite_group_key_references(item, grouping_keys))
                .collect(),
        ),
        Expression::Map(map) => Expression::Map(crate::ast::PropertyMap {
            properties: map
                .properties
                .iter()
                .map(|pair| crate::ast::PropertyPair {
                    key: pair.key.clone(),
                    value: rewrite_group_key_references(&pair.value, grouping_keys),
                })
                .collect(),
        }),
        Expression::Case(case_expr) => Expression::Case(Box::new(crate::ast::CaseExpression {
            expression: case_expr
                .expression
                .as_ref()
                .map(|expr| rewrite_group_key_references(expr, grouping_keys)),
            when_clauses: case_expr
                .when_clauses
                .iter()
                .map(|(when_expr, then_expr)| {
                    (
                        rewrite_group_key_references(when_expr, grouping_keys),
                        rewrite_group_key_references(then_expr, grouping_keys),
                    )
                })
                .collect(),
            else_expression: case_expr
                .else_expression
                .as_ref()
                .map(|expr| rewrite_group_key_references(expr, grouping_keys)),
        })),
        _ => expr.clone(),
    }
}

fn compile_projection_aggregation(
    input: Plan,
    items: &[crate::ast::ReturnItem],
) -> Result<(Plan, Vec<String>)> {
    for item in items {
        ensure_no_pattern_predicate(&item.expression)?;
        validate_expression_types(&item.expression)?;
    }

    // RETURN * / WITH * expansion.
    if items.len() == 1
        && items[0].alias.is_none()
        && matches!(&items[0].expression, Expression::Literal(Literal::String(s)) if s == "*")
    {
        let mut vars = BTreeMap::new();
        extract_output_var_kinds(&input, &mut vars);
        let cols: Vec<String> = vars.keys().cloned().collect();
        let projections: Vec<(String, Expression)> = cols
            .iter()
            .map(|name| (name.clone(), Expression::Variable(name.clone())))
            .collect();

        return Ok((
            Plan::Project {
                input: Box::new(input),
                projections,
            },
            cols,
        ));
    }

    let mut resolved_items: Vec<(Expression, String, bool)> = Vec::with_capacity(items.len());
    let mut has_aggregation = false;
    for (i, item) in items.iter().enumerate() {
        let contains_agg = contains_aggregate_expression(&item.expression);
        if contains_agg {
            has_aggregation = true;
        }

        let alias = if let Some(alias) = &item.alias {
            alias.clone()
        } else if let Expression::FunctionCall(call) = &item.expression {
            if parse_aggregate_function(call)?.is_some() {
                default_aggregate_alias(call, i)
            } else {
                default_projection_alias(&item.expression, i)
            }
        } else {
            default_projection_alias(&item.expression, i)
        };

        resolved_items.push((item.expression.clone(), alias, contains_agg));
    }

    if !has_aggregation {
        let projections: Vec<(String, Expression)> = resolved_items
            .iter()
            .map(|(expr, alias, _)| (alias.clone(), expr.clone()))
            .collect();
        let project_cols: Vec<String> = resolved_items
            .iter()
            .map(|(_, alias, _)| alias.clone())
            .collect();

        return Ok((
            Plan::Project {
                input: Box::new(input),
                projections,
            },
            project_cols,
        ));
    }

    let mut pre_projections: Vec<(String, Expression)> = Vec::new();
    let mut projected_aliases = std::collections::HashSet::new();
    let mut grouping_keys: Vec<(Expression, String)> = Vec::new();
    let mut grouping_aliases = std::collections::HashSet::new();
    let mut group_by: Vec<String> = Vec::new();

    for (expr, alias, contains_agg) in &resolved_items {
        if *contains_agg {
            continue;
        }

        if !projected_aliases.contains(alias) {
            pre_projections.push((alias.clone(), expr.clone()));
            projected_aliases.insert(alias.clone());
        }

        group_by.push(alias.clone());
        grouping_aliases.insert(alias.clone());
        if is_simple_group_expression(expr) {
            grouping_keys.push((expr.clone(), alias.clone()));
        }
    }

    let mut aggregate_exprs: Vec<(Expression, crate::ast::AggregateFunction, String)> = Vec::new();
    for (expr, _alias, contains_agg) in &resolved_items {
        if !*contains_agg {
            continue;
        }

        validate_aggregate_mixed_expression(expr, &grouping_keys, &grouping_aliases)?;
        collect_aggregate_calls(expr, &mut aggregate_exprs)?;
    }

    if aggregate_exprs.is_empty() {
        return Err(Error::Other("syntax error: InvalidAggregation".to_string()));
    }

    for (_, agg, _) in &aggregate_exprs {
        match agg {
            crate::ast::AggregateFunction::Count(None) => {}
            crate::ast::AggregateFunction::Count(Some(expr))
            | crate::ast::AggregateFunction::CountDistinct(expr)
            | crate::ast::AggregateFunction::Sum(expr)
            | crate::ast::AggregateFunction::SumDistinct(expr)
            | crate::ast::AggregateFunction::Avg(expr)
            | crate::ast::AggregateFunction::AvgDistinct(expr)
            | crate::ast::AggregateFunction::Min(expr)
            | crate::ast::AggregateFunction::MinDistinct(expr)
            | crate::ast::AggregateFunction::Max(expr)
            | crate::ast::AggregateFunction::MaxDistinct(expr)
            | crate::ast::AggregateFunction::Collect(expr)
            | crate::ast::AggregateFunction::CollectDistinct(expr) => {
                let mut deps = std::collections::HashSet::new();
                extract_variables_from_expr(expr, &mut deps);
                for dep in deps {
                    if !projected_aliases.contains(&dep) {
                        pre_projections.push((dep.clone(), Expression::Variable(dep.clone())));
                        projected_aliases.insert(dep);
                    }
                }
            }
        }
    }

    let aggregate_aliases: Vec<(Expression, String)> = aggregate_exprs
        .iter()
        .map(|(expr, _, alias)| (expr.clone(), alias.clone()))
        .collect();

    let mut final_projections: Vec<(String, Expression)> = Vec::new();
    let mut project_cols: Vec<String> = Vec::new();
    for (expr, alias, contains_agg) in resolved_items {
        let final_expr = if contains_agg {
            let rewritten = rewrite_aggregate_references(&expr, &aggregate_aliases);
            rewrite_group_key_references(&rewritten, &grouping_keys)
        } else {
            Expression::Variable(alias.clone())
        };

        project_cols.push(alias.clone());
        final_projections.push((alias, final_expr));
    }

    let plan = if !pre_projections.is_empty() {
        Plan::Project {
            input: Box::new(input),
            projections: pre_projections,
        }
    } else {
        input
    };

    let plan = Plan::Aggregate {
        input: Box::new(plan),
        group_by,
        aggregates: aggregate_exprs
            .into_iter()
            .map(|(_, aggregate, alias)| (aggregate, alias))
            .collect(),
    };

    let plan = Plan::Project {
        input: Box::new(plan),
        projections: final_projections,
    };

    Ok((plan, project_cols))
}

fn validate_order_by_scope(
    order_by: &crate::ast::OrderByClause,
    project_cols: &[String],
    projection_items: &[crate::ast::ReturnItem],
) -> Result<()> {
    let mut scope: std::collections::HashSet<String> = project_cols.iter().cloned().collect();
    for item in projection_items {
        extract_variables_from_expr(&item.expression, &mut scope);
    }

    for item in &order_by.items {
        if contains_aggregate_expression(&item.expression) {
            return Err(Error::Other("syntax error: InvalidAggregation".to_string()));
        }

        let mut used = std::collections::HashSet::new();
        extract_variables_from_expr(&item.expression, &mut used);
        for var in used {
            if !scope.contains(&var) {
                return Err(Error::Other(format!(
                    "syntax error: UndefinedVariable ({})",
                    var
                )));
            }
        }
    }

    Ok(())
}

fn rewrite_order_expression(expr: &Expression, bindings: &[(Expression, String)]) -> Expression {
    for (pattern, alias) in bindings {
        if expr == pattern {
            return Expression::Variable(alias.clone());
        }
    }

    match expr {
        Expression::Binary(b) => Expression::Binary(Box::new(crate::ast::BinaryExpression {
            left: rewrite_order_expression(&b.left, bindings),
            operator: b.operator.clone(),
            right: rewrite_order_expression(&b.right, bindings),
        })),
        Expression::Unary(u) => Expression::Unary(Box::new(crate::ast::UnaryExpression {
            operator: u.operator.clone(),
            operand: rewrite_order_expression(&u.operand, bindings),
        })),
        Expression::FunctionCall(f) => Expression::FunctionCall(crate::ast::FunctionCall {
            name: f.name.clone(),
            args: f
                .args
                .iter()
                .map(|arg| rewrite_order_expression(arg, bindings))
                .collect(),
        }),
        Expression::List(items) => Expression::List(
            items
                .iter()
                .map(|item| rewrite_order_expression(item, bindings))
                .collect(),
        ),
        Expression::Map(map) => Expression::Map(crate::ast::PropertyMap {
            properties: map
                .properties
                .iter()
                .map(|pair| crate::ast::PropertyPair {
                    key: pair.key.clone(),
                    value: rewrite_order_expression(&pair.value, bindings),
                })
                .collect(),
        }),
        _ => expr.clone(),
    }
}

fn contains_aggregate_expression(expr: &Expression) -> bool {
    match expr {
        Expression::FunctionCall(call) => {
            let name = call.name.to_lowercase();
            if matches!(
                name.as_str(),
                "count" | "sum" | "avg" | "min" | "max" | "collect"
            ) {
                return true;
            }
            call.args.iter().any(contains_aggregate_expression)
        }
        Expression::Binary(b) => {
            contains_aggregate_expression(&b.left) || contains_aggregate_expression(&b.right)
        }
        Expression::Unary(u) => contains_aggregate_expression(&u.operand),
        Expression::List(items) => items.iter().any(contains_aggregate_expression),
        Expression::Map(map) => map
            .properties
            .iter()
            .any(|pair| contains_aggregate_expression(&pair.value)),
        _ => false,
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

fn compile_remove_plan_v2(input: Plan, remove: crate::ast::RemoveClause) -> Result<Plan> {
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

fn validate_create_property_vars(
    props: &Option<crate::ast::PropertyMap>,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> Result<()> {
    if let Some(properties) = props {
        for prop in &properties.properties {
            let mut refs = std::collections::HashSet::new();
            extract_variables_from_expr(&prop.value, &mut refs);
            for var in refs {
                if !known_bindings.contains_key(&var) {
                    return Err(Error::Other(format!(
                        "syntax error: UndefinedVariable ({})",
                        var
                    )));
                }
            }
        }
    }
    Ok(())
}

fn compile_create_plan(input: Plan, create_clause: crate::ast::CreateClause) -> Result<Plan> {
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
            .filter(|el| matches!(el, crate::ast::PathElement::Relationship(_)))
            .count();

        for element in &pattern.elements {
            match element {
                crate::ast::PathElement::Node(node) => {
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
                crate::ast::PathElement::Relationship(rel) => {
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
        };
    }

    Ok(plan)
}

fn compile_merge_plan(input: Plan, merge_clause: crate::ast::MergeClause) -> Result<Plan> {
    let pattern = merge_clause.pattern;
    if pattern.elements.is_empty() {
        return Err(Error::Other("MERGE pattern cannot be empty".into()));
    }

    let mut known_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();
    extract_output_var_kinds(&input, &mut known_bindings);

    let rel_count = pattern
        .elements
        .iter()
        .filter(|el| matches!(el, crate::ast::PathElement::Relationship(_)))
        .count();

    for element in &pattern.elements {
        match element {
            crate::ast::PathElement::Node(node) => {
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
            crate::ast::PathElement::Relationship(rel) => {
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
    })
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

fn first_relationship_is_bound(
    pattern: &crate::ast::Pattern,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> bool {
    match pattern.elements.get(1) {
        Some(crate::ast::PathElement::Relationship(rel)) => {
            rel.variable_length.is_none()
                && rel
                    .variable
                    .as_ref()
                    .and_then(|name| known_bindings.get(name))
                    .is_some_and(|kind| {
                        matches!(kind, BindingKind::Relationship | BindingKind::Unknown)
                    })
        }
        _ => false,
    }
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

fn build_optional_unbind_aliases(
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
        && !is_binding_compatible(known_bindings, alias, BindingKind::Relationship)
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

fn maybe_reanchor_pattern(
    pattern: crate::ast::Pattern,
    known_bindings: &BTreeMap<String, BindingKind>,
) -> crate::ast::Pattern {
    if pattern.elements.len() != 3 {
        return pattern;
    }

    let (first, rel, last) = match (
        &pattern.elements[0],
        &pattern.elements[1],
        &pattern.elements[2],
    ) {
        (
            crate::ast::PathElement::Node(first),
            crate::ast::PathElement::Relationship(rel),
            crate::ast::PathElement::Node(last),
        ) => (first, rel, last),
        _ => return pattern,
    };

    let first_bound = is_bound_node_alias(first, known_bindings);
    let last_bound = is_bound_node_alias(last, known_bindings);

    if first_bound || !last_bound {
        return pattern;
    }

    let mut flipped_rel = rel.clone();
    flipped_rel.direction = reverse_relationship_direction(&flipped_rel.direction);

    crate::ast::Pattern {
        variable: pattern.variable,
        elements: vec![
            crate::ast::PathElement::Node(last.clone()),
            crate::ast::PathElement::Relationship(flipped_rel),
            crate::ast::PathElement::Node(first.clone()),
        ],
    }
}

fn compile_match_plan(
    input: Option<Plan>,
    m: crate::ast::MatchClause,
    predicates: &BTreeMap<String, BTreeMap<String, Expression>>,
    next_anon_id: &mut u32,
) -> Result<Plan> {
    let mut plan = input;
    let mut known_bindings: BTreeMap<String, BindingKind> = BTreeMap::new();
    if let Some(p) = &plan {
        extract_output_var_kinds(p, &mut known_bindings);
    }

    for raw_pattern in m.patterns {
        let pattern = maybe_reanchor_pattern(raw_pattern, &known_bindings);
        if pattern.elements.is_empty() {
            return Err(Error::Other("pattern cannot be empty".into()));
        }
        validate_match_pattern_bindings(&pattern, &known_bindings)?;

        let first_node_alias = match &pattern.elements[0] {
            crate::ast::PathElement::Node(n) => {
                if let Some(v) = &n.variable {
                    v.clone()
                } else {
                    // Generate anonymous variable name
                    let name = format!("_gen_{}", next_anon_id);
                    *next_anon_id += 1;
                    name
                }
            }
            _ => return Err(Error::Other("pattern must start with a node".into())),
        };

        let join_via_bound_node = matches!(
            known_bindings.get(&first_node_alias),
            Some(BindingKind::Node | BindingKind::Unknown)
        );
        let join_via_bound_relationship = first_relationship_is_bound(&pattern, &known_bindings);

        if join_via_bound_node || join_via_bound_relationship {
            // Join via expansion (bound start node) or via already-bound relationship variable.
            plan = Some(compile_pattern_chain(
                plan,
                &pattern,
                predicates,
                m.optional,
                &known_bindings,
                next_anon_id,
            )?);
        } else {
            // Start a new component
            let sub_plan = compile_pattern_chain(
                None,
                &pattern,
                predicates,
                m.optional,
                &known_bindings,
                next_anon_id,
            )?;
            if let Some(existing) = plan {
                plan = Some(Plan::CartesianProduct {
                    left: Box::new(existing),
                    right: Box::new(sub_plan),
                });
            } else {
                plan = Some(sub_plan);
            }
        }

        // Update known bindings after each pattern.
        if let Some(p) = &plan {
            known_bindings.clear();
            extract_output_var_kinds(p, &mut known_bindings);
        }
    }

    plan.ok_or_else(|| Error::Other("No patterns in MATCH".into()))
}

fn compile_pattern_chain(
    input: Option<Plan>,
    pattern: &crate::ast::Pattern,
    predicates: &BTreeMap<String, BTreeMap<String, Expression>>,
    optional: bool,
    known_bindings: &BTreeMap<String, BindingKind>,
    next_anon_id: &mut u32,
) -> Result<Plan> {
    if pattern.elements.is_empty() {
        return Err(Error::Other("pattern cannot be empty".into()));
    }

    let src_node_el = match &pattern.elements[0] {
        crate::ast::PathElement::Node(n) => n,
        _ => return Err(Error::Other("pattern must start with a node".into())),
    };

    let src_alias = if let Some(v) = &src_node_el.variable {
        v.clone()
    } else {
        // Generate anonymous variable name
        let name = format!("_gen_{}", next_anon_id);
        *next_anon_id += 1;
        name
    };
    let src_label = src_node_el.labels.first().cloned();

    let mut local_predicates = predicates.clone();
    let mut plan = if let Some(existing_plan) = input {
        let src_is_bound = matches!(
            known_bindings.get(&src_alias),
            Some(BindingKind::Node | BindingKind::Unknown)
        );
        let first_rel_is_bound = first_relationship_is_bound(pattern, known_bindings);

        if !src_is_bound && !first_rel_is_bound {
            return Err(Error::Other(format!(
                "Join variable '{}' not found in plan",
                src_alias
            )));
        }

        if src_is_bound {
            // Apply filters for inline properties of this node only when source alias is already bound.
            extend_predicates_from_properties(
                &src_alias,
                &src_node_el.properties,
                &mut local_predicates,
            );
            apply_filters_for_alias(existing_plan, &src_alias, &local_predicates)
        } else {
            existing_plan
        }
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
            optional,
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
    let mut curr_src_alias = src_alias.clone();
    let mut local_bound_aliases: BTreeSet<String> = BTreeSet::new();
    let chain_path_alias = pattern
        .variable
        .clone()
        .or_else(|| Some(alloc_internal_path_alias(next_anon_id)));

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

        let dst_alias = if let Some(v) = &dst_node_el.variable {
            v.clone()
        } else {
            // Generate anonymous variable name
            let name = format!("_gen_{}", next_anon_id);
            *next_anon_id += 1;
            name
        };

        let edge_alias = rel_el.variable.clone();
        let rel_types = rel_el.types.clone();
        let dst_labels = dst_node_el.labels.clone();
        let src_prebound =
            is_bound_before_local(known_bindings, &local_bound_aliases, &curr_src_alias);

        let path_alias = chain_path_alias.clone();
        let optional_unbind = build_optional_unbind_aliases(
            known_bindings,
            &curr_src_alias,
            &dst_alias,
            edge_alias.as_deref(),
            path_alias.as_deref(),
        );

        if let Some(var_len) = &rel_el.variable_length {
            plan = Plan::MatchOutVarLen {
                input: Some(Box::new(plan)),
                src_alias: curr_src_alias.clone(),
                dst_alias: dst_alias.clone(),
                dst_labels: dst_labels.clone(),
                src_prebound,
                edge_alias: edge_alias.clone(),
                rels: rel_types,
                direction: rel_el.direction.clone(),
                min_hops: var_len.min.unwrap_or(1),
                max_hops: var_len.max,
                limit: None,
                project: Vec::new(),
                project_external: false,
                optional,
                optional_unbind: optional_unbind.clone(),
                path_alias,
            };
        } else if let Some(rel_alias) = &edge_alias
            && matches!(
                known_bindings.get(rel_alias),
                Some(BindingKind::Relationship | BindingKind::Unknown)
            )
        {
            plan = Plan::MatchBoundRel {
                input: Box::new(plan),
                rel_alias: rel_alias.clone(),
                src_alias: curr_src_alias.clone(),
                dst_alias: dst_alias.clone(),
                dst_labels: dst_labels.clone(),
                src_prebound,
                rels: rel_types,
                direction: rel_el.direction.clone(),
                optional,
                optional_unbind: optional_unbind.clone(),
                path_alias,
            };
        } else {
            match rel_el.direction {
                crate::ast::RelationshipDirection::LeftToRight => {
                    plan = Plan::MatchOut {
                        input: Some(Box::new(plan)),
                        src_alias: curr_src_alias.clone(),
                        dst_alias: dst_alias.clone(),
                        dst_labels: dst_labels.clone(),
                        src_prebound,
                        edge_alias: edge_alias.clone(),
                        rels: rel_types,
                        limit: None,
                        project: Vec::new(),
                        project_external: false,
                        optional,
                        optional_unbind: optional_unbind.clone(),
                        path_alias,
                    };
                }
                crate::ast::RelationshipDirection::RightToLeft => {
                    plan = Plan::MatchIn {
                        input: Some(Box::new(plan)),
                        src_alias: curr_src_alias.clone(),
                        dst_alias: dst_alias.clone(),
                        dst_labels: dst_labels.clone(),
                        src_prebound,
                        edge_alias: edge_alias.clone(),
                        rels: rel_types,
                        limit: None,
                        optional,
                        optional_unbind: optional_unbind.clone(),
                        path_alias,
                    };
                }
                crate::ast::RelationshipDirection::Undirected => {
                    plan = Plan::MatchUndirected {
                        input: Some(Box::new(plan)),
                        src_alias: curr_src_alias.clone(),
                        dst_alias: dst_alias.clone(),
                        dst_labels: dst_labels.clone(),
                        src_prebound,
                        edge_alias: edge_alias.clone(),
                        rels: rel_types,
                        limit: None,
                        optional,
                        optional_unbind: optional_unbind.clone(),
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

        local_bound_aliases.insert(dst_alias.clone());
        if let Some(ea) = &edge_alias {
            local_bound_aliases.insert(ea.clone());
        }
        if let Some(pa) = &pattern.variable {
            local_bound_aliases.insert(pa.clone());
        }

        curr_src_alias = dst_alias;
        i += 2;
    }

    if pattern.elements.len() == 1
        && let Some(path_alias) = &pattern.variable
    {
        let mut vars = BTreeMap::new();
        extract_output_var_kinds(&plan, &mut vars);
        let mut projections: Vec<(String, Expression)> = vars
            .keys()
            .map(|name| (name.clone(), Expression::Variable(name.clone())))
            .collect();
        projections.push((
            path_alias.clone(),
            Expression::FunctionCall(crate::ast::FunctionCall {
                name: "__nervus_singleton_path".to_string(),
                args: vec![Expression::Variable(src_alias.clone())],
            }),
        ));
        plan = Plan::Project {
            input: Box::new(plan),
            projections,
        };
    }

    Ok(plan)
}

fn is_bound_before_local(
    known_bindings: &BTreeMap<String, BindingKind>,
    local_bound_aliases: &BTreeSet<String>,
    alias: &str,
) -> bool {
    if local_bound_aliases.contains(alias) {
        return true;
    }
    if !known_bindings.contains_key(alias) {
        return false;
    }
    matches!(
        known_bindings.get(alias),
        Some(BindingKind::Node | BindingKind::Unknown)
    )
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

fn variable_already_bound_error(var: &str) -> Error {
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

fn validate_match_pattern_bindings(
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
                    register_pattern_binding(
                        var,
                        BindingKind::Relationship,
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
        Value::Path(_) | Value::ReifiedPath(_) => BindingKind::Path,
        _ => BindingKind::Scalar,
    }
}

fn infer_expression_binding_kind(
    expr: &Expression,
    vars: &BTreeMap<String, BindingKind>,
) -> BindingKind {
    match expr {
        Expression::Variable(name) => vars.get(name).copied().unwrap_or(BindingKind::Unknown),
        Expression::FunctionCall(call) => {
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
        Expression::Literal(_)
        | Expression::Parameter(_)
        | Expression::PropertyAccess(_)
        | Expression::Binary(_)
        | Expression::Unary(_)
        | Expression::List(_)
        | Expression::Map(_) => BindingKind::Scalar,
        _ => BindingKind::Unknown,
    }
}

fn extract_output_var_kinds(plan: &Plan, vars: &mut BTreeMap<String, BindingKind>) {
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
            if let Some(p) = input {
                extract_output_var_kinds(p, vars);
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
            merge_binding_kind(vars, rel_alias.clone(), BindingKind::Relationship);
            merge_binding_kind(vars, src_alias.clone(), BindingKind::Node);
            merge_binding_kind(vars, dst_alias.clone(), BindingKind::Node);
            if let Some(p) = path_alias
                && !is_internal_path_alias(p)
            {
                merge_binding_kind(vars, p.clone(), BindingKind::Path);
            }
            extract_output_var_kinds(input, vars);
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
            extract_output_var_kinds(subquery, vars);
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
            merge_binding_kind(vars, alias.clone(), BindingKind::Node);
            extract_output_var_kinds(fallback, vars);
        }
        Plan::Foreach { input, .. } => extract_output_var_kinds(input, vars),
        Plan::Values { rows } => {
            for row in rows {
                for (name, value) in row.columns() {
                    merge_binding_kind(vars, name.clone(), value_binding_kind(value));
                }
            }
        }
        Plan::Create { input, pattern } => {
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
                            merge_binding_kind(vars, var.clone(), BindingKind::Relationship);
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
        | Plan::SetLabels { input, .. }
        | Plan::RemoveProperty { input, .. }
        | Plan::RemoveLabels { input, .. } => {
            extract_output_var_kinds(input, vars);
        }
    }
}

fn unwrap_distinct_argument(expr: &Expression) -> (Expression, bool) {
    if let Expression::FunctionCall(call) = expr
        && call.name == "__distinct"
        && call.args.len() == 1
    {
        return (call.args[0].clone(), true);
    }

    (expr.clone(), false)
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
                let (arg, distinct) = unwrap_distinct_argument(&call.args[0]);
                if let Expression::Literal(Literal::String(s)) = &arg
                    && s == "*"
                {
                    return Ok(Some(crate::ast::AggregateFunction::Count(None)));
                }

                if distinct {
                    Ok(Some(crate::ast::AggregateFunction::CountDistinct(arg)))
                } else {
                    Ok(Some(crate::ast::AggregateFunction::Count(Some(arg))))
                }
            } else {
                Err(Error::Other("COUNT takes 0 or 1 argument".into()))
            }
        }
        "sum" => {
            if call.args.len() != 1 {
                return Err(Error::Other("SUM takes exactly 1 argument".into()));
            }
            let (arg, distinct) = unwrap_distinct_argument(&call.args[0]);
            if distinct {
                Ok(Some(crate::ast::AggregateFunction::SumDistinct(arg)))
            } else {
                Ok(Some(crate::ast::AggregateFunction::Sum(arg)))
            }
        }
        "avg" => {
            if call.args.len() != 1 {
                return Err(Error::Other("AVG takes exactly 1 argument".into()));
            }
            let (arg, distinct) = unwrap_distinct_argument(&call.args[0]);
            if distinct {
                Ok(Some(crate::ast::AggregateFunction::AvgDistinct(arg)))
            } else {
                Ok(Some(crate::ast::AggregateFunction::Avg(arg)))
            }
        }
        "min" => {
            if call.args.len() != 1 {
                return Err(Error::Other("MIN takes exactly 1 argument".into()));
            }
            let (arg, distinct) = unwrap_distinct_argument(&call.args[0]);
            if distinct {
                Ok(Some(crate::ast::AggregateFunction::MinDistinct(arg)))
            } else {
                Ok(Some(crate::ast::AggregateFunction::Min(arg)))
            }
        }
        "max" => {
            if call.args.len() != 1 {
                return Err(Error::Other("MAX takes exactly 1 argument".into()));
            }
            let (arg, distinct) = unwrap_distinct_argument(&call.args[0]);
            if distinct {
                Ok(Some(crate::ast::AggregateFunction::MaxDistinct(arg)))
            } else {
                Ok(Some(crate::ast::AggregateFunction::Max(arg)))
            }
        }
        "collect" => {
            if call.args.len() != 1 {
                return Err(Error::Other("COLLECT takes exactly 1 argument".into()));
            }
            let (arg, distinct) = unwrap_distinct_argument(&call.args[0]);
            if distinct {
                Ok(Some(crate::ast::AggregateFunction::CollectDistinct(arg)))
            } else {
                Ok(Some(crate::ast::AggregateFunction::Collect(arg)))
            }
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
    // Compile updates sub-plan with a scoped placeholder input.
    // It must include both upstream bindings and FOREACH iteration variable,
    // otherwise CREATE property validation may incorrectly reject variables
    // referenced inside FOREACH bodies.
    let mut known_bindings = BTreeMap::new();
    extract_output_var_kinds(&input, &mut known_bindings);

    let mut seed_row = Row::default();
    for name in known_bindings.keys() {
        seed_row = seed_row.with(name.clone(), Value::Null);
    }
    seed_row = seed_row.with(foreach.variable.clone(), Value::Null);

    let initial_input = Some(Plan::Values {
        rows: vec![seed_row],
    });

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

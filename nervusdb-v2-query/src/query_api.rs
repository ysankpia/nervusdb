use crate::ast::{Clause, Expression, Literal, Query, RelationshipDirection};
use crate::error::{Error, Result};
use crate::executor::{Plan, Row, Value, execute_plan, execute_write};
use nervusdb_v2_api::GraphSnapshot;
use std::collections::BTreeMap;
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
        execute_plan(snapshot, &self.plan, params)
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
            WriteSemantics::Merge => {
                crate::executor::execute_merge(&self.plan, snapshot, txn, params)
            }
        }
    }

    pub fn is_explain(&self) -> bool {
        self.explain.is_some()
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
        let query = crate::parser::Parser::parse(inner)?;
        let compiled = compile_m3_plan(query)?;
        let explain = Some(render_plan(&compiled.plan));
        return Ok(PreparedQuery {
            plan: compiled.plan,
            explain,
            write: compiled.write,
        });
    }

    let query = crate::parser::Parser::parse(cypher)?;
    let compiled = compile_m3_plan(query)?;
    Ok(PreparedQuery {
        plan: compiled.plan,
        explain: None,
        write: compiled.write,
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
            Plan::NodeScan { alias, label } => {
                let _ = writeln!(out, "{pad}NodeScan(alias={alias}, label={label:?})");
            }
            Plan::MatchOut {
                src_alias,
                rel,
                edge_alias,
                dst_alias,
                limit,
                project: _,
                project_external: _,
            } => {
                let _ = writeln!(
                    out,
                    "{pad}MatchOut(src={src_alias}, rel={rel:?}, edge={edge_alias:?}, dst={dst_alias}, limit={limit:?})"
                );
            }
            Plan::MatchOutVarLen {
                src_alias,
                rel,
                edge_alias,
                dst_alias,
                min_hops,
                max_hops,
                limit,
                project: _,
                project_external: _,
            } => {
                let _ = writeln!(
                    out,
                    "{pad}MatchOutVarLen(src={src_alias}, rel={rel:?}, edge={edge_alias:?}, dst={dst_alias}, min={min_hops}, max={max_hops:?}, limit={limit:?})"
                );
            }
            Plan::Filter { input, predicate } => {
                let _ = writeln!(out, "{pad}Filter(predicate={predicate:?})");
                go(out, input, depth + 1);
            }
            Plan::Project { input, columns } => {
                let _ = writeln!(out, "{pad}Project(columns={columns:?})");
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
            Plan::Distinct { input } => {
                let _ = writeln!(out, "{pad}Distinct");
                go(out, input, depth + 1);
            }
            Plan::Create { pattern } => {
                let _ = writeln!(out, "{pad}Create(pattern={pattern:?})");
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
        }
    }

    let mut out = String::new();
    go(&mut out, plan, 0);
    out.trim_end().to_string()
}

struct CompiledQuery {
    plan: Plan,
    write: WriteSemantics,
}

fn compile_m3_plan(query: Query) -> Result<CompiledQuery> {
    // Supported shapes (M3):
    // - RETURN 1
    // - MATCH (n)-[:<u32>]->(m) [WHERE ...] RETURN n,m [LIMIT k]
    // - MATCH (n)-[:<u32>]->(m) [WHERE ...] DELETE n [DETACH]
    let mut match_clause = None;
    let mut where_clause = None;
    let mut return_clause = None;
    let mut delete_clause = None;

    for clause in query.clauses {
        match clause {
            Clause::Match(m) => match_clause = Some(m),
            Clause::Where(w) => where_clause = Some(w),
            Clause::Return(r) => return_clause = Some(r),
            Clause::With(_) => return Err(Error::NotImplemented("WITH in v2 M3")),
            Clause::Create(c) => {
                let plan = compile_create_plan(c)?;
                return Ok(CompiledQuery {
                    plan,
                    write: WriteSemantics::Default,
                });
            }
            Clause::Merge(m) => {
                let plan = compile_merge_plan(m)?;
                return Ok(CompiledQuery {
                    plan,
                    write: WriteSemantics::Merge,
                });
            }
            Clause::Unwind(_) => return Err(Error::NotImplemented("UNWIND in v2 M3")),
            Clause::Call(_) => return Err(Error::NotImplemented("CALL in v2 M3")),
            Clause::Set(_) => return Err(Error::NotImplemented("SET in v2 M3")),
            Clause::Delete(d) => delete_clause = Some(d),
            Clause::Union(_) => return Err(Error::NotImplemented("UNION in v2 M3")),
        }
    }

    // Handle DELETE clause
    if let Some(delete) = delete_clause {
        let plan = compile_delete_plan(match_clause, where_clause, delete)?;
        return Ok(CompiledQuery {
            plan,
            write: WriteSemantics::Default,
        });
    }

    let Some(ret) = return_clause else {
        return Err(Error::NotImplemented("query without RETURN"));
    };

    if match_clause.is_none() {
        if ret.items.len() == 1
            && let Expression::Literal(Literal::Number(n)) = &ret.items[0].expression
            && (*n - 1.0).abs() < f64::EPSILON
        {
            return Ok(CompiledQuery {
                plan: Plan::ReturnOne,
                write: WriteSemantics::Default,
            });
        }
        return Err(Error::NotImplemented("RETURN-only query (except RETURN 1)"));
    }

    let m = match_clause.unwrap();
    if m.optional {
        return Err(Error::NotImplemented("OPTIONAL MATCH in v2 M3"));
    }

    let mut plan = match m.pattern.elements.len() {
        1 => {
            let node = match &m.pattern.elements[0] {
                crate::ast::PathElement::Node(n) => n,
                _ => return Err(Error::Other("pattern must be a node".into())),
            };

            if node.properties.is_some() {
                return Err(Error::NotImplemented(
                    "node pattern properties in v2 M3 (use WHERE)",
                ));
            }
            let alias = node
                .variable
                .as_deref()
                .ok_or(Error::NotImplemented("anonymous node"))?
                .to_string();

            Plan::NodeScan {
                alias,
                label: node.labels.first().cloned(),
            }
        }
        3 => {
            let src = match &m.pattern.elements[0] {
                crate::ast::PathElement::Node(n) => n,
                _ => return Err(Error::Other("pattern must start with node".into())),
            };
            let rel_pat = match &m.pattern.elements[1] {
                crate::ast::PathElement::Relationship(r) => r,
                _ => return Err(Error::Other("expected relationship in middle".into())),
            };
            let dst = match &m.pattern.elements[2] {
                crate::ast::PathElement::Node(n) => n,
                _ => return Err(Error::Other("pattern must end with node".into())),
            };

            if src.properties.is_some() || dst.properties.is_some() {
                return Err(Error::NotImplemented(
                    "node pattern properties in v2 M3 (use WHERE)",
                ));
            }
            if rel_pat.properties.is_some() {
                return Err(Error::NotImplemented(
                    "relationship pattern properties in v2 M3 (use WHERE)",
                ));
            }

            let src_alias = src
                .variable
                .as_deref()
                .ok_or(Error::NotImplemented("anonymous node"))?
                .to_string();
            let dst_alias = dst
                .variable
                .as_deref()
                .ok_or(Error::NotImplemented("anonymous node"))?
                .to_string();

            if !matches!(rel_pat.direction, RelationshipDirection::LeftToRight) {
                return Err(Error::NotImplemented("only -> direction in v2 M3"));
            }

            let rel = rel_pat.types.first().cloned();

            let edge_alias = rel_pat.variable.clone();

            // Don't use embedded limit when we have RETURN limit - we'll use separate Limit node
            let limit = if ret.limit.is_some() { None } else { ret.limit };

            if let Some(var_len) = &rel_pat.variable_length {
                let min_hops = var_len.min.unwrap_or(1);
                let max_hops = var_len.max;
                if min_hops == 0 {
                    return Err(Error::NotImplemented(
                        "0-length variable-length paths in v2 M3",
                    ));
                }
                if let Some(max_hops) = max_hops
                    && max_hops < min_hops
                {
                    return Err(Error::Other(
                        "invalid variable-length range: max < min".into(),
                    ));
                }
                Plan::MatchOutVarLen {
                    src_alias,
                    rel,
                    edge_alias,
                    dst_alias,
                    min_hops,
                    max_hops,
                    limit,
                    project: Vec::new(),
                    project_external: false,
                }
            } else {
                Plan::MatchOut {
                    src_alias,
                    rel,
                    edge_alias,
                    dst_alias,
                    limit,
                    project: Vec::new(),
                    project_external: false,
                }
            }
        }
        _ => {
            return Err(Error::NotImplemented(
                "only single-node or single-hop patterns in v2 M3",
            ));
        }
    };

    let mut project: Vec<String> = Vec::new();
    for item in &ret.items {
        if item.alias.is_some() {
            return Err(Error::NotImplemented("RETURN aliases in v2 M3"));
        }
        let Expression::Variable(name) = &item.expression else {
            return Err(Error::NotImplemented(
                "only variable projections in v2 M3 (use RETURN 1 or RETURN <var>...)",
            ));
        };
        project.push(name.clone());
    }

    match &mut plan {
        Plan::MatchOut { project: p, .. } | Plan::MatchOutVarLen { project: p, .. } => {
            *p = project.clone();
        }
        Plan::NodeScan { .. } => {}
        Plan::ReturnOne
        | Plan::Filter { .. }
        | Plan::Project { .. }
        | Plan::Aggregate { .. }
        | Plan::OrderBy { .. }
        | Plan::Skip { .. }
        | Plan::Limit { .. }
        | Plan::Distinct { .. }
        | Plan::Create { .. }
        | Plan::Delete { .. } => {}
    }

    // Fail-fast: RETURN variables must exist in the row shape produced by the base plan.
    let available: Vec<&str> = match &plan {
        Plan::NodeScan { alias, .. } => vec![alias.as_str()],
        Plan::MatchOut {
            src_alias,
            edge_alias,
            dst_alias,
            ..
        }
        | Plan::MatchOutVarLen {
            src_alias,
            edge_alias,
            dst_alias,
            ..
        } => {
            let mut out = vec![src_alias.as_str(), dst_alias.as_str()];
            if let Some(edge_alias) = edge_alias.as_deref() {
                out.push(edge_alias);
            }
            out
        }
        _ => Vec::new(),
    };
    for col in &project {
        if !available.iter().any(|v| v == &col.as_str()) {
            return Err(Error::Other(format!("unknown variable in RETURN: {col}")));
        }
    }

    // Add WHERE filter if present
    if let Some(w) = where_clause {
        plan = Plan::Filter {
            input: Box::new(plan),
            predicate: w.expression,
        };
    }

    // Add projection after filtering (to preserve columns for WHERE evaluation)
    plan = Plan::Project {
        input: Box::new(plan),
        columns: project,
    };

    // Add ORDER BY if present
    if let Some(order_by) = &ret.order_by {
        let items: Vec<(String, crate::ast::Direction)> = order_by
            .items
            .iter()
            .map(|item| {
                let col = match &item.expression {
                    Expression::Variable(v) => v.clone(),
                    _ => {
                        return Err(Error::NotImplemented(
                            "ORDER BY with non-variable expression in v2 M3",
                        ));
                    }
                };
                let direction = item.direction.clone();
                Ok((col, direction))
            })
            .collect::<Result<Vec<_>>>()?;
        plan = Plan::OrderBy {
            input: Box::new(plan),
            items,
        };
    }

    // Add SKIP if present
    if let Some(skip) = ret.skip {
        plan = Plan::Skip {
            input: Box::new(plan),
            skip,
        };
    }

    // Add LIMIT if present (override embedded limit in MatchOut)
    if let Some(limit) = ret.limit {
        plan = Plan::Limit {
            input: Box::new(plan),
            limit,
        };
    }

    // Add DISTINCT if present
    if ret.distinct {
        plan = Plan::Distinct {
            input: Box::new(plan),
        };
    }

    Ok(CompiledQuery {
        plan,
        write: WriteSemantics::Default,
    })
}

/// Compile a CREATE clause into a Plan
fn compile_create_plan(create_clause: crate::ast::CreateClause) -> Result<Plan> {
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
        pattern: create_clause.pattern,
    })
}

fn compile_merge_plan(merge_clause: crate::ast::MergeClause) -> Result<Plan> {
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

    Ok(Plan::Create { pattern })
}

/// Compile a DELETE clause into a Plan
fn compile_delete_plan(
    match_clause: Option<crate::ast::MatchClause>,
    where_clause: Option<crate::ast::WhereClause>,
    delete_clause: crate::ast::DeleteClause,
) -> Result<Plan> {
    // DELETE requires a MATCH clause to find nodes/edges to delete
    let Some(m) = match_clause else {
        return Err(Error::Other(
            "DELETE requires a preceding MATCH clause in v2 M3".into(),
        ));
    };

    // Validate pattern
    if m.optional {
        return Err(Error::NotImplemented("OPTIONAL MATCH with DELETE in v2 M3"));
    }

    if m.pattern.elements.len() != 1 && m.pattern.elements.len() != 3 {
        return Err(Error::NotImplemented(
            "only single-node or single-hop patterns with DELETE in v2 M3",
        ));
    }

    if m.pattern.elements.len() == 1 {
        let node = match &m.pattern.elements[0] {
            crate::ast::PathElement::Node(n) => n,
            _ => return Err(Error::Other("pattern must be a node".into())),
        };

        if !node.labels.is_empty() {
            return Err(Error::NotImplemented("labels with DELETE in v2 M3"));
        }
        if node.properties.is_some() {
            return Err(Error::NotImplemented(
                "node pattern properties with DELETE in v2 M3 (use WHERE)",
            ));
        }

        let alias = node
            .variable
            .as_deref()
            .ok_or(Error::NotImplemented("anonymous node in MATCH for DELETE"))?
            .to_string();

        let mut input_plan = Plan::NodeScan {
            alias: alias.clone(),
            label: node.labels.first().cloned(),
        };
        if let Some(w) = where_clause {
            input_plan = Plan::Filter {
                input: Box::new(input_plan),
                predicate: w.expression,
            };
        }
        input_plan = Plan::Project {
            input: Box::new(input_plan),
            columns: vec![alias],
        };

        return Ok(Plan::Delete {
            input: Box::new(input_plan),
            detach: delete_clause.detach,
            expressions: delete_clause.expressions,
        });
    }

    let src = match &m.pattern.elements[0] {
        crate::ast::PathElement::Node(n) => n,
        _ => return Err(Error::Other("pattern must start with node".into())),
    };
    let rel_pat = match &m.pattern.elements[1] {
        crate::ast::PathElement::Relationship(r) => r,
        _ => return Err(Error::Other("expected relationship in middle".into())),
    };
    let dst = match &m.pattern.elements[2] {
        crate::ast::PathElement::Node(n) => n,
        _ => return Err(Error::Other("pattern must end with node".into())),
    };

    if src.properties.is_some() || dst.properties.is_some() {
        return Err(Error::NotImplemented(
            "node pattern properties with DELETE in v2 M3 (use WHERE)",
        ));
    }
    if rel_pat.properties.is_some() {
        return Err(Error::NotImplemented(
            "relationship pattern properties with DELETE in v2 M3 (use WHERE)",
        ));
    }

    let src_alias = src
        .variable
        .as_deref()
        .ok_or(Error::NotImplemented("anonymous node in MATCH for DELETE"))?
        .to_string();
    let dst_alias = dst
        .variable
        .as_deref()
        .ok_or(Error::NotImplemented("anonymous node in MATCH for DELETE"))?
        .to_string();

    if !matches!(rel_pat.direction, RelationshipDirection::LeftToRight) {
        return Err(Error::NotImplemented(
            "only -> direction with DELETE in v2 M3",
        ));
    }

    let rel = rel_pat.types.first().cloned();

    // Build the input plan (MATCH pattern)
    // Clone aliases for multiple uses
    let src_alias_1 = src_alias.clone();
    let src_alias_2 = src_alias.clone();
    let dst_alias_1 = dst_alias.clone();
    let dst_alias_2 = dst_alias.clone();
    let mut input_plan = Plan::MatchOut {
        src_alias,
        rel,
        edge_alias: rel_pat.variable.clone(),
        dst_alias,
        limit: None,
        project: vec![src_alias_1, dst_alias_1],
        project_external: false,
    };

    // Add WHERE filter if present
    if let Some(w) = where_clause {
        input_plan = Plan::Filter {
            input: Box::new(input_plan),
            predicate: w.expression,
        };
    }

    // Add projection to preserve columns for DELETE variable resolution
    input_plan = Plan::Project {
        input: Box::new(input_plan),
        columns: vec![src_alias_2, dst_alias_2],
    };

    // Build DELETE plan with input
    Ok(Plan::Delete {
        input: Box::new(input_plan),
        detach: delete_clause.detach,
        expressions: delete_clause.expressions,
    })
}

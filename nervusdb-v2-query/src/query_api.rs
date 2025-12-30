use crate::ast::{BinaryOperator, Clause, Expression, Literal, Query, RelationshipDirection};
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
                input: _,
                src_alias,
                rel,
                edge_alias,
                dst_alias,
                limit,
                project: _,
                project_external: _,
                optional,
            } => {
                let opt_str = if *optional { " OPTIONAL" } else { "" };
                let _ = writeln!(
                    out,
                    "{pad}MatchOut{opt_str}(src={src_alias}, rel={rel:?}, edge={edge_alias:?}, dst={dst_alias}, limit={limit:?})"
                );
            }
            Plan::MatchOutVarLen {
                input: _,
                src_alias,
                rel,
                edge_alias,
                dst_alias,
                min_hops,
                max_hops,
                limit,
                project: _,
                project_external: _,
                optional,
            } => {
                let opt_str = if *optional { " OPTIONAL" } else { "" };
                let _ = writeln!(
                    out,
                    "{pad}MatchOutVarLen{opt_str}(src={src_alias}, rel={rel:?}, edge={edge_alias:?}, dst={dst_alias}, min={min_hops}, max={max_hops:?}, limit={limit:?})"
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
            Plan::SetProperty { input, items } => {
                let _ = writeln!(out, "{pad}SetProperty(items={items:?})");
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
}

fn compile_m3_plan(query: Query) -> Result<CompiledQuery> {
    let mut matches = Vec::new();
    let mut where_clause = None;
    let mut return_clause = None;
    let mut set_clause = None;
    let mut delete_clause = None;

    for clause in query.clauses {
        match clause {
            Clause::Match(m) => matches.push(m),
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
            Clause::Set(s) => set_clause = Some(s),
            Clause::Delete(d) => delete_clause = Some(d),
            Clause::Union(_) => return Err(Error::NotImplemented("UNION in v2 M3")),
        }
    }

    if let Some(delete) = delete_clause {
        if set_clause.is_some() {
            return Err(Error::NotImplemented("Mixing SET and DELETE in v2 M3"));
        }
        if matches.len() > 1 {
            return Err(Error::NotImplemented("Multiple MATCH with DELETE in v2 M3"));
        }
        let m = matches.into_iter().next();
        let plan = compile_delete_plan(m, where_clause, delete)?;
        return Ok(CompiledQuery {
            plan,
            write: WriteSemantics::Default,
        });
    }

    if let Some(set) = set_clause {
        if matches.len() > 1 {
            return Err(Error::NotImplemented("Multiple MATCH with SET in v2 M3"));
        }
        let m = matches.into_iter().next();
        let plan = compile_set_plan(m, where_clause, set)?;
        return Ok(CompiledQuery {
            plan,
            write: WriteSemantics::Default,
        });
    }

    // Predicate extraction for optimization
    let mut predicates = BTreeMap::new();
    if let Some(w) = &where_clause {
        extract_predicates(&w.expression, &mut predicates);
    }

    // Build Match Chain
    let mut plan: Option<Plan> = None;
    for m in matches {
        plan = Some(compile_match_plan(plan, m, &predicates)?);
    }

    // Check if we have a plan, or handle RETURN 1
    let mut plan = if let Some(p) = plan {
        p
    } else {
        if let Some(ret) = &return_clause {
            if ret.items.len() == 1 {
                if let Expression::Literal(Literal::Number(n)) = &ret.items[0].expression {
                    if (*n - 1.0).abs() < f64::EPSILON {
                        return Ok(CompiledQuery {
                            plan: Plan::ReturnOne,
                            write: WriteSemantics::Default,
                        });
                    }
                }
            }
        }
        return Err(Error::NotImplemented(
            "Query must have at least one MATCH (or be RETURN 1)",
        ));
    };

    let Some(ret) = return_clause else {
        return Err(Error::NotImplemented("query without RETURN"));
    };

    // Calculate projection
    // Calculate projection or aggregation
    let mut group_by: Vec<String> = Vec::new();
    let mut aggregates: Vec<(crate::ast::AggregateFunction, String)> = Vec::new(); // (Func, Alias)
    let mut project: Vec<String> = Vec::new();
    let mut is_aggregation = false;

    for (i, item) in ret.items.iter().enumerate() {
        match &item.expression {
            Expression::Variable(name) => {
                group_by.push(name.clone());
                project.push(name.clone());
            }
            Expression::FunctionCall(call) => {
                // Check if it is an aggregate function
                if let Some(agg) = parse_aggregate_function(call)? {
                    is_aggregation = true;
                    // Generate alias if missing
                    let alias = item.alias.clone().unwrap_or_else(|| format!("agg_{}", i));
                    aggregates.push((agg, alias.clone()));
                    project.push(alias);
                } else {
                    return Err(Error::NotImplemented(
                        "non-aggregate function calls in RETURN",
                    ));
                }
            }
            _ => {
                return Err(Error::NotImplemented(
                    "only variables or aggregates in RETURN clause in v2 M3",
                ));
            }
        }
    }

    // Add WHERE filter if present
    if let Some(w) = where_clause {
        let filter_plan = Plan::Filter {
            input: Box::new(plan),
            predicate: w.expression.clone(),
        };
        plan = try_optimize_nodescan_filter(filter_plan, w.expression);
    }

    if is_aggregation {
        plan = Plan::Aggregate {
            input: Box::new(plan),
            group_by,
            aggregates,
        };
        // Aggregate produces the final columns, so we don't need a separate Project
        // UNLESS we want to reorder/rename. But implicit project from Aggregate is effectively the output row.
        // The Aggregate executor builds rows with [group_by_cols..., aggregate_cols...]
        // We might need to map them to the requested order?
        // For MVP, checking order might be complex. Let's assume Aggregate returns all keys + aggregates.
        // But the user expect explicit minimal columns.
        // Let's wrap in Project to ensure correct order/selection if needed?
        // Actually, execute_aggregate returns rows with specific columns.
        // We should ensure it returns ONLY the requested columns in usage?
        // But execute_aggregate constructs rows with aliased values.
        // Ideally we project the final result to match `project` list order.
        plan = Plan::Project {
            input: Box::new(plan),
            columns: project,
        };
    } else {
        // Standard Projection
        plan = Plan::Project {
            input: Box::new(plan),
            columns: project,
        };
    }

    // Add ORDER BY
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

    // Add SKIP
    if let Some(skip) = ret.skip {
        plan = Plan::Skip {
            input: Box::new(plan),
            skip,
        };
    }

    // Add LIMIT
    if let Some(limit) = ret.limit {
        plan = Plan::Limit {
            input: Box::new(plan),
            limit,
        };
    }

    // Add DISTINCT
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

fn try_optimize_nodescan_filter(plan: Plan, _predicate: Expression) -> Plan {
    // 1. Unwrap input logic - needs to be a NodeScan
    // The input to Filter is boxed, so we need to inspect the structure we just created
    // But here we passed 'plan' which is Filter{NodeScan...}
    let (input, predicate) = match &plan {
        Plan::Filter { input, predicate } => (input, predicate),
        _ => return plan,
    };

    let Plan::NodeScan { alias, label } = input.as_ref() else {
        return plan;
    };

    // 2. Must have label to use index
    let Some(label) = label else {
        return plan;
    };

    // 3. Check predicate
    // Helper to check for equality with a property
    let check_eq =
        |left: &Expression, right: &Expression| -> Option<(String, String, Expression)> {
            // Return (variable, property, value_expr)
            if let Expression::PropertyAccess(pa) = left {
                if &pa.variable == alias {
                    // Check if right is literal or parameter
                    match right {
                        Expression::Literal(_) | Expression::Parameter(_) => {
                            return Some((pa.variable.clone(), pa.property.clone(), right.clone()));
                        }
                        _ => {}
                    }
                }
            }
            None
        };

    if let Expression::Binary(bin) = &predicate {
        // Access fields of BinaryExpression (box)
        if matches!(bin.operator, crate::ast::BinaryOperator::Equals) {
            // Check left = right
            if let Some((v, p, val)) = check_eq(&bin.left, &bin.right) {
                return Plan::IndexSeek {
                    alias: v,
                    label: label.clone(),
                    field: p,
                    value_expr: val,
                    fallback: Box::new(plan.clone()),
                };
            }
            // Check right = left
            if let Some((v, p, val)) = check_eq(&bin.right, &bin.left) {
                return Plan::IndexSeek {
                    alias: v,
                    label: label.clone(),
                    field: p,
                    value_expr: val,
                    fallback: Box::new(plan.clone()),
                };
            }
        }
    }

    plan
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
        input: None,
        src_alias,
        rel,
        edge_alias: rel_pat.variable.clone(),
        dst_alias,
        limit: None,
        project: vec![src_alias_1, dst_alias_1],
        project_external: false,
        optional: false,
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

fn compile_set_plan(
    match_clause: Option<crate::ast::MatchClause>,
    where_clause: Option<crate::ast::WhereClause>,
    set_clause: crate::ast::SetClause,
) -> Result<Plan> {
    // SET requires a preceding MATCH clause
    let Some(m) = match_clause else {
        return Err(Error::Other(
            "SET requires a preceding MATCH clause in v2 M3".into(),
        ));
    };

    if m.optional {
        return Err(Error::NotImplemented("OPTIONAL MATCH with SET in v2 M3"));
    }

    let mut plan = match m.pattern.elements.len() {
        1 => {
            let node = match &m.pattern.elements[0] {
                crate::ast::PathElement::Node(n) => n,
                _ => return Err(Error::Other("pattern must be a node".into())),
            };
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
        // TODO: Support SET on relationships (length 3 pattern)
        _ => {
            return Err(Error::NotImplemented(
                "only single-node patterns with SET in v2 M3",
            ));
        }
    };

    // Add WHERE filter if present
    if let Some(w) = where_clause {
        let filter_plan = Plan::Filter {
            input: Box::new(plan),
            predicate: w.expression.clone(),
        };
        plan = try_optimize_nodescan_filter(filter_plan, w.expression);
    }

    // Convert SetItems to (var, key, expr)
    let mut items = Vec::new();
    for item in set_clause.items {
        // SetItem is a struct in this AST version, so we handle it directly
        items.push((item.property.variable, item.property.property, item.value));
    }

    Ok(Plan::SetProperty {
        input: Box::new(plan),
        items,
    })
}

fn compile_match_plan(
    input: Option<Plan>,
    m: crate::ast::MatchClause,
    predicates: &BTreeMap<String, BTreeMap<String, Expression>>,
) -> Result<Plan> {
    match m.pattern.elements.len() {
        1 => {
            if input.is_some() {
                // If input exists, we don't support disconnected MATCH (n) yet.
                return Err(Error::NotImplemented(
                    "Multiple disconnected MATCH clauses not supported (Cartesian product) in v2 M3",
                ));
            }
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

            let label = node.labels.first().cloned();

            // Optimizer: Try IndexSeek if there is a predicate.
            if let Some(label_name) = &label {
                if let Some(var_preds) = predicates.get(&alias) {
                    if let Some((field, val_expr)) = var_preds.iter().next() {
                        // For MVP, we return IndexSeek with fallback.
                        // It will try index at runtime, and if not found, use fallback scan.
                        return Ok(Plan::IndexSeek {
                            alias: alias.clone(),
                            label: label_name.clone(),
                            field: field.clone(),
                            value_expr: val_expr.clone(),
                            fallback: Box::new(Plan::NodeScan {
                                alias: alias.clone(),
                                label: label.clone(),
                            }),
                        });
                    }
                }
            }

            // Selection: Choose smallest label if multiple options (future)
            // or just use stats to warn or adjust (T156).

            Ok(Plan::NodeScan { alias, label })
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

            if let Some(var_len) = &rel_pat.variable_length {
                let min_hops = var_len.min.unwrap_or(1);
                let max_hops = var_len.max;
                if min_hops == 0 {
                    return Err(Error::NotImplemented(
                        "0-length variable-length paths in v2 M3",
                    ));
                }
                if let Some(max) = max_hops {
                    if max < min_hops {
                        return Err(Error::Other(
                            "invalid variable-length range: max < min".into(),
                        ));
                    }
                }

                Ok(Plan::MatchOutVarLen {
                    input: input.map(Box::new),
                    src_alias,
                    rel,
                    edge_alias,
                    dst_alias,
                    min_hops,
                    max_hops,
                    limit: None,
                    project: Vec::new(),
                    project_external: false,
                    optional: m.optional,
                })
            } else {
                Ok(Plan::MatchOut {
                    input: input.map(Box::new),
                    src_alias,
                    rel,
                    edge_alias,
                    dst_alias,
                    limit: None,
                    project: Vec::new(),
                    project_external: false,
                    optional: m.optional,
                })
            }
        }
        _ => Err(Error::NotImplemented(
            "pattern length must be 1 or 3 in v2 M3",
        )),
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
                if let Expression::Literal(Literal::String(s)) = &call.args[0] {
                    if s == "*" {
                        return Ok(Some(crate::ast::AggregateFunction::Count(None)));
                    }
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
    match expr {
        Expression::Binary(bin) => {
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
        _ => {}
    }
}

use crate::ast::{BinaryOperator, CallClause, Clause, Expression, Literal, Query};
use crate::error::{Error, Result};
use crate::executor::{Plan, Row, Value, execute_plan, execute_write};
use nervusdb_api::GraphSnapshot;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

mod aggregate_parse;
mod ast_walk;
mod binding_analysis;
mod compile_core;
mod explain;
mod foreach_compile;
mod internal_alias;
mod match_anchor;
mod match_compile;
mod merge_set;
mod pattern_predicate;
mod plan;
mod plan_introspection;
mod plan_render;
mod planner;
mod prepare_entry;
mod prepared_query_impl;
mod projection_alias;
mod projection_compile;
mod return_with;
mod type_validation;
mod where_validation;
mod write_compile;
mod write_create_merge;
mod write_validation;
use aggregate_parse::parse_aggregate_function;
use ast_walk::{extract_predicates, extract_variables_from_expr};
use binding_analysis::{
    extract_output_var_kinds, infer_expression_binding_kind, validate_match_pattern_bindings,
    variable_already_bound_error,
};
use compile_core::compile_m3_plan;
use explain::strip_explain_prefix;
use foreach_compile::compile_foreach_plan;
use internal_alias::{alloc_internal_path_alias, is_internal_path_alias};
use match_anchor::{
    build_optional_unbind_aliases, first_relationship_is_bound, maybe_reanchor_pattern,
    pattern_has_bound_relationship,
};
use match_compile::compile_match_plan;
use merge_set::{compile_merge_set_items, extract_merge_pattern_vars};
use pattern_predicate::ensure_no_pattern_predicate;
use plan_introspection::plan_contains_write;
use plan_render::render_plan;
use projection_alias::{default_aggregate_alias, default_projection_alias};
use projection_compile::{
    compile_order_by_items, compile_projection_aggregation, contains_aggregate_expression,
    rewrite_order_expression, validate_order_by_aggregate_semantics, validate_order_by_scope,
};
use return_with::{compile_return_plan, compile_with_plan};
use type_validation::validate_expression_types;
use where_validation::validate_where_expression_bindings;
use write_compile::{
    compile_delete_plan_v2, compile_remove_plan_v2, compile_set_plan_v2, compile_unwind_plan,
};
use write_create_merge::{compile_create_plan, compile_merge_plan};
use write_validation::validate_create_property_vars;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteSemantics {
    Default,
    Merge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingKind {
    Node,
    Relationship,
    RelationshipList,
    Path,
    Scalar,
    Unknown,
}

const INTERNAL_PATH_ALIAS_PREFIX: &str = "__nervus_internal_path_";

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
    merge_on_create_labels: Vec<(String, Vec<String>)>,
    merge_on_match_labels: Vec<(String, Vec<String>)>,
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
    prepare_entry::prepare(cypher)
}

pub(crate) fn exists_subquery_has_rows<S: GraphSnapshot>(
    subquery: &Query,
    outer_row: &Row,
    snapshot: &S,
    params: &Params,
) -> Result<bool> {
    let mut merge_subclauses = VecDeque::new();
    let compiled = compile_m3_plan(
        subquery.clone(),
        &mut merge_subclauses,
        Some(Plan::Values {
            rows: vec![outer_row.clone()],
        }),
    )?;

    if plan_contains_write(&compiled.plan) {
        return Err(Error::Other(
            "syntax error: InvalidClauseComposition".to_string(),
        ));
    }

    let mut iter = execute_plan(snapshot, &compiled.plan, params);
    match iter.next() {
        Some(next_row) => {
            next_row?;
            Ok(true)
        }
        None => Ok(false),
    }
}

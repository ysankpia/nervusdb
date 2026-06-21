use crate::api::GraphSnapshot;
use crate::query::ast::{BinaryOperator, Clause, Expression, Literal, Query};
use crate::query::error::{Error, Result};
use crate::query::executor::{Plan, Row, Value, execute_plan, execute_write};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

mod ast_walk;
mod binding_analysis;
mod compile_core;
mod explain;
mod internal_alias;
mod match_anchor;
mod match_compile;
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
use ast_walk::{extract_predicates, extract_variables_from_expr};
use binding_analysis::{
    extract_output_var_kinds, infer_expression_binding_kind, validate_match_pattern_bindings,
    variable_already_bound_error,
};
use compile_core::compile_m3_plan;
use explain::strip_explain_prefix;
use internal_alias::{alloc_internal_path_alias, is_internal_path_alias};
use match_anchor::{
    build_optional_unbind_aliases, first_relationship_is_bound, maybe_reanchor_pattern,
    pattern_has_bound_relationship,
};
use match_compile::compile_match_plan;
use pattern_predicate::ensure_no_pattern_predicate;
use plan_introspection::plan_contains_write;
use plan_render::render_plan;
use projection_alias::default_projection_alias;
use projection_compile::compile_projection_aggregation;
use return_with::compile_return_plan;
use type_validation::validate_expression_types;
use where_validation::validate_where_expression_bindings;
use write_compile::{compile_delete_plan_v2, compile_set_plan_v2};
use write_create_merge::compile_create_plan;
use write_validation::validate_create_property_vars;

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

/// Execution resource limits applied to each query execution.
///
/// Defaults are tuned to a balanced profile for CI/runtime stability.
#[derive(Debug, Clone)]
pub struct ExecuteOptions {
    pub max_intermediate_rows: usize,
    pub max_collection_items: usize,
    pub soft_timeout_ms: u64,
    pub max_apply_rows_per_outer: usize,
}

impl Default for ExecuteOptions {
    fn default() -> Self {
        Self {
            max_intermediate_rows: 500_000,
            max_collection_items: 200_000,
            soft_timeout_ms: 5_000,
            max_apply_rows_per_outer: 200_000,
        }
    }
}

#[derive(Debug, Default)]
struct ExecutionRuntimeState {
    started_at: Option<Instant>,
}

#[derive(Debug, Default)]
struct ExecutionRuntime {
    state: Mutex<ExecutionRuntimeState>,
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
    execute_options: ExecuteOptions,
    runtime: Arc<ExecutionRuntime>,
}

impl Params {
    /// Creates a new empty parameters map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a parameter bag with custom execution options.
    pub fn with_execute_options(options: ExecuteOptions) -> Self {
        let mut params = Self::new();
        params.set_execute_options(options);
        params
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

    /// Returns execution options associated with this parameter bag.
    pub fn execute_options(&self) -> &ExecuteOptions {
        &self.execute_options
    }

    /// Overrides execution options used for subsequent query executions.
    pub fn set_execute_options(&mut self, options: ExecuteOptions) {
        self.execute_options = options;
    }

    pub(crate) fn begin_execution(&self) {
        if let Ok(mut state) = self.runtime.state.lock() {
            state.started_at = Some(Instant::now());
        }
    }

    pub(crate) fn check_timeout(&self, stage: &str) -> Result<()> {
        let timeout_ms = self.execute_options.soft_timeout_ms;
        if timeout_ms == 0 {
            return Ok(());
        }
        let elapsed_ms = {
            let state = self
                .runtime
                .state
                .lock()
                .map_err(|_| Error::Other("execution runtime lock poisoned".to_string()))?;
            state
                .started_at
                .map(|started| started.elapsed().as_millis() as usize)
                .unwrap_or(0)
        };
        let timeout_ms = timeout_ms as usize;
        if elapsed_ms > timeout_ms {
            return Err(Error::resource_limit_exceeded(
                crate::query::error::ResourceLimitKind::Timeout,
                timeout_ms,
                elapsed_ms,
                stage,
            ));
        }
        Ok(())
    }

    pub(crate) fn check_collection_size(&self, stage: &str, observed: usize) -> Result<()> {
        let configured_limit = self.execute_options.max_collection_items;
        let limit = if configured_limit == ExecuteOptions::default().max_collection_items
            && stage == "Function(range)"
        {
            configured_limit.max(1_100_000)
        } else {
            configured_limit
        };
        if observed > limit {
            return Err(Error::resource_limit_exceeded(
                crate::query::error::ResourceLimitKind::CollectionItems,
                limit,
                observed,
                stage,
            ));
        }
        Ok(())
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
}

/// Parses and prepares a Mini-Cypher 0.1 query for execution.
///
/// # Supported Mini-Cypher 0.1
///
/// - `RETURN 1` - Constant return
/// - `MATCH (n)` and `MATCH (n:Label)` - node and label scans
/// - `MATCH (a)-[:TYPE]->(b)` - directed one-hop traversal
/// - `MATCH (a)-[:TYPE]->(b)-[:TYPE]->(c)` - documented two-hop traversal
/// - `WHERE n.prop = 'value'` or `WHERE n.prop = 1` - simple equality filters
/// - `RETURN` of bound variables and simple properties
/// - `LIMIT`
/// - basic `CREATE`
/// - basic `SET n.key = value`
/// - basic `DELETE`
/// - `EXPLAIN <query>` - show compiled plan for supported queries
///
/// Returns an `outside Mini-Cypher 0.1` error for unsupported openCypher
/// constructs. The supported surface is defined in
/// `docs/reference/mini-cypher.md`.
pub fn prepare(cypher: &str) -> Result<PreparedQuery> {
    prepare_entry::prepare(cypher)
}

pub(crate) fn exists_subquery_has_rows<S: GraphSnapshot>(
    subquery: &Query,
    outer_row: &Row,
    snapshot: &S,
    params: &Params,
) -> Result<bool> {
    let compiled = compile_m3_plan(
        subquery.clone(),
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

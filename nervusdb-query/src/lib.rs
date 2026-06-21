//! NervusDB Mini-Cypher Query Engine
//!
//! Parsing, planning, and execution for the NervusDB 0.1 embedded graph core.
//!
//! # API
//!
//! | Function | Purpose |
//! |----------|---------|
//! | [`prepare`] | Parse a Cypher string into a [`PreparedQuery`] |
//! | [`query_collect`] | Parse + execute + collect all rows in one call |
//! | [`query_collect_params`] | Like `query_collect` with typed parameters |
//! | [`query_executor::execute_plan`] | Low-level plan execution (internal) |
//!
//! # Quick Start
//!
//! ```ignore
//! use nervusdb_query::{prepare, Params};
//!
//! let query = prepare("MATCH (n)-[:1]->(m) RETURN n, m LIMIT 10").unwrap();
//! let rows: Vec<_> = query
//!     .execute_streaming(&snapshot, &Params::new())
//!     .collect::<Result<_>>()
//!     .unwrap();
//! ```
//!
//! # Mini-Cypher 0.1 — Supported Features
//!
//! | Category | Examples |
//! |----------|----------|
//! | Node scan | `MATCH (n)`, `MATCH (n:Person)` |
//! | Traversal | `(a)-[:TYPE]->(b)`, documented 2-hop patterns |
//! | Property filter | `WHERE n.age = 30`, `WHERE n.name = 'Alice'` |
//! | Pagination | `LIMIT 10` |
//! | Projection | `RETURN n`, `RETURN n.name` |
//! | Write | `CREATE`, basic `SET n.x = v`, basic `DELETE n` |
//! | Plan debug | `EXPLAIN MATCH (n) RETURN n` |
//!
//! # Architecture
//!
//! - `parser::Parser` - Parses Cypher syntax into AST
//! - `executor::execute_plan` - Streams results from plan
//! - `evaluator` - Evaluates expressions (WHERE, RETURN)

pub mod ast;
pub mod error;
pub mod evaluator;
pub mod executor;
pub mod facade;
pub mod lexer;
pub mod parser;
pub mod query_api;

pub use error::{Error, ResourceLimitKind, Result};
pub use executor::{Row, Value, WriteableGraph};
pub use facade::{
    EdgeKey, ExternalId, GraphSnapshot, GraphStore, InternalNodeId, LabelId, PropertyValue,
    QueryExt, RelTypeId, query_collect,
};
pub use query_api::{ExecuteOptions, Params, PreparedQuery, prepare};

/// Parses a Cypher query string into an AST.
///
/// This is a low-level API. Most users should use [`prepare()`] instead,
/// which handles both parsing and planning.
pub fn parse(cypher: &str) -> Result<ast::Query> {
    parser::Parser::parse(cypher)
}

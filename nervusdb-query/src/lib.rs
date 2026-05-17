//! NervusDB Mini-Cypher Query Engine
//!
//! Provides parsing, planning, and execution for the query surface used by the
//! NervusDB 0.1 embedded core. The implementation contains historical support
//! for broader Cypher work, but only the Mini-Cypher surface documented in
//! `docs/reference/mini-cypher.md` defines 0.1 readiness.
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
//! # Mini-Cypher 0.1
//!
//! - `RETURN 1` - Constant return
//! - `MATCH (n)` / `MATCH (n:Label)` - Node scans
//! - `MATCH (a)-[:TYPE]->(b) RETURN a, b LIMIT k` - Single-hop pattern match
//! - `MATCH (a)-[:TYPE]->(b)-[:TYPE]->(c)` - Two-hop pattern match for 0.1 examples
//! - Simple property equality in `WHERE`
//! - `CREATE`, basic `DELETE`, and basic `SET` where already stable
//! - `EXPLAIN <query>` - Show compiled plan (no execution)
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

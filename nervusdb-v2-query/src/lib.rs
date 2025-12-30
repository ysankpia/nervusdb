//! NervusDB v2 Query Engine
//!
//! Provides Cypher query parsing, planning, and execution for NervusDB v2.
//!
//! # Quick Start
//!
//! ```ignore
//! use nervusdb_v2_query::{prepare, Params};
//!
//! let query = prepare("MATCH (n)-[:1]->(m) RETURN n, m LIMIT 10").unwrap();
//! let rows: Vec<_> = query
//!     .execute_streaming(&snapshot, &Params::new())
//!     .collect::<Result<_>>()
//!     .unwrap();
//! ```
//!
//! # Supported Cypher (v2 M3)
//!
//! - `RETURN 1` - Constant return
//! - `MATCH (n)-[:<u32>]->(m) RETURN n, m LIMIT k` - Single-hop pattern match
//! - `MATCH (n)-[:<u32>]->(m) WHERE n.prop = 'value' RETURN n, m` - With WHERE filter
//! - `CREATE (n)` / `CREATE (n {k: v})` - Create nodes
//! - `CREATE (a)-[:1]->(b)` - Create edges
//! - `MATCH (n)-[:1]->(m) DELETE n` / `DETACH DELETE n` - Delete nodes/edges
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

pub use error::{Error, Result};
pub use executor::{Row, Value, WriteableGraph};
pub use facade::{
    EdgeKey, ExternalId, GraphSnapshot, GraphStore, InternalNodeId, LabelId, PropertyValue,
    QueryExt, RelTypeId, query_collect,
};
pub use query_api::{Params, PreparedQuery, prepare};

/// Parses a Cypher query string into an AST.
///
/// This is a low-level API. Most users should use [`prepare()`] instead,
/// which handles both parsing and planning.
pub fn parse(cypher: &str) -> Result<ast::Query> {
    parser::Parser::parse(cypher)
}

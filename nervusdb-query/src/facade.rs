//! Query Facade - convenient methods for querying the graph.
//!
//! Provides a "SQLite-like" experience by adding convenient query methods
//! to any type that implements [`nervusdb_api::GraphSnapshot`].
//!
//! # Example
//!
//! ```rust,ignore
//! use nervusdb_query::{prepare, QueryExt};
//!
//! fn query_example(snapshot: &impl GraphSnapshot) {
//!     let rows = nervusdb_query::query_collect(
//!         snapshot,
//!         "MATCH (n)-[:1]->(m) RETURN n, m LIMIT 10",
//!         &Default::default(),
//!     ).unwrap();
//! }
//! ```
//!
//! # Re-export for convenience
//!
//! This module re-exports the following types from `nervusdb_api`:
//! - [`GraphSnapshot`] - The trait for snapshot access
//! - [`GraphStore`] - The trait for creating snapshots
//! - [`ExternalId`], [`InternalNodeId`], [`LabelId`], [`RelTypeId`] - ID types
//! - [`PropertyValue`] - Property value type
//! - [`EdgeKey`] - Edge key type

pub use nervusdb_api::{
    EdgeKey, ExternalId, GraphSnapshot, GraphStore, InternalNodeId, LabelId, PropertyValue,
    RelTypeId,
};

use crate::{Error, Params, Result, Row};

/// Executes a Cypher query and collects all results into a Vec.
///
/// This is a convenience function that combines parsing, planning, and execution
/// in a single call, similar to how SQLite works.
///
/// # Errors
///
/// Returns an error if the query is invalid or execution fails.
///
/// # Example
///
/// ```rust,ignore
/// use nervusdb_query::query_collect;
///
/// let rows = query_collect(
///     &snapshot,
///     "MATCH (n)-[:1]->(m) RETURN n, m LIMIT 10",
///     &Default::default(),
/// ).unwrap();
/// ```
pub fn query_collect<S: GraphSnapshot>(
    snapshot: &S,
    cypher: &str,
    params: &Params,
) -> Result<Vec<Row>> {
    let query = crate::query_api::prepare(cypher).map_err(|e| Error::Other(e.to_string()))?;
    let results: Vec<Result<Row>> = query.execute_streaming(snapshot, params).collect();
    results.into_iter().collect()
}

/// Extension trait providing convenient query methods.
///
/// This trait is automatically implemented for all types implementing
/// [`GraphSnapshot`], allowing a "SQLite-like" query experience.
///
/// # Example
///
/// ```rust,ignore
/// use nervusdb_query::QueryExt;
///
/// fn query_example(snapshot: &impl GraphSnapshot) {
///     let rows = snapshot.query(
///         "MATCH (n)-[:1]->(m) RETURN n, m LIMIT 10",
///         &Default::default(),
///     ).unwrap();
/// }
/// ```
pub trait QueryExt {
    /// Executes a Cypher query and collects all results into a Vec.
    ///
    /// This is equivalent to calling [`query_collect()`] directly.
    ///
    /// # Errors
    ///
    /// Returns an error if the query is invalid or execution fails.
    fn query(&self, cypher: &str, params: &Params) -> Result<Vec<Row>>;
}

impl<T: GraphSnapshot> QueryExt for T {
    fn query(&self, cypher: &str, params: &Params) -> Result<Vec<Row>> {
        query_collect(self, cypher, params)
    }
}

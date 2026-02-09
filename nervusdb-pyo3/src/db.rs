use super::types::{py_to_value, value_to_py};
use super::WriteTxn;
use crate::{classify_nervus_error, QueryStream};
use nervusdb_v2::Db as RustDb;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// NervusDB database handle.
///
/// Provides access to Cypher queries and transactions.
#[pyclass]
pub struct Db {
    pub(crate) inner: Option<RustDb>,
    ndb_path: PathBuf,
    active_write_txns: Arc<AtomicUsize>,
}

impl Db {
    fn execute_query_rows(
        &self,
        query: &str,
        params: Option<HashMap<String, Py<PyAny>>>,
        py: Python<'_>,
    ) -> PyResult<Vec<HashMap<String, Py<PyAny>>>> {
        let inner = self
            .inner
            .as_ref()
            .ok_or_else(|| classify_nervus_error("database is closed"))?;
        let snapshot = inner.snapshot();
        let prepared = nervusdb_v2_query::prepare(query).map_err(classify_nervus_error)?;

        let mut query_params = nervusdb_v2_query::Params::new();
        if let Some(p) = params {
            for (k, v) in p {
                let val_bound = v.bind(py);
                let val = py_to_value(val_bound)?;
                query_params.insert(k, val);
            }
        }

        let rows: Vec<_> = prepared
            .execute_streaming(&snapshot, &query_params)
            .collect::<Result<Vec<_>, _>>()
            .map_err(classify_nervus_error)?;

        let mut result = Vec::new();
        for row in rows {
            let mut row_map = HashMap::new();
            for (col, val) in row.columns() {
                let reified = val.reify(&snapshot).map_err(classify_nervus_error)?;
                row_map.insert(col.clone(), value_to_py(reified, py));
            }
            result.push(row_map);
        }

        Ok(result)
    }
}

#[pymethods]
impl Db {
    /// Open a database at the given path.
    ///
    /// The path can be:
    /// - A directory path: files will be created as `<path>.ndb` and `<path>.wal`
    /// - An explicit `.ndb` path
    ///
    /// Returns an error if the database cannot be opened.
    #[new]
    pub(crate) fn new(path: &str) -> PyResult<Self> {
        let inner = RustDb::open(path).map_err(classify_nervus_error)?;
        Ok(Self {
            inner: Some(inner),
            ndb_path: PathBuf::from(path),
            active_write_txns: Arc::new(AtomicUsize::new(0)),
        })
    }

    /// Execute a Cypher query and return materialized results.
    #[pyo3(signature = (query, params=None))]
    fn query(
        &self,
        query: &str,
        params: Option<HashMap<String, Py<PyAny>>>,
        py: Python<'_>,
    ) -> PyResult<Vec<HashMap<String, Py<PyAny>>>> {
        self.execute_query_rows(query, params, py)
    }

    /// Execute a Cypher query and return an iterator-like stream.
    ///
    /// This API is stable; internal implementation may evolve from materialized
    /// rows to chunked/true streaming without changing Python call sites.
    #[pyo3(signature = (query, params=None))]
    fn query_stream(
        &self,
        query: &str,
        params: Option<HashMap<String, Py<PyAny>>>,
        py: Python<'_>,
    ) -> PyResult<QueryStream> {
        let rows = self.execute_query_rows(query, params, py)?;
        Ok(QueryStream::new(rows))
    }

    /// Search for similar vectors.
    ///
    /// Args:
    ///     query: Query vector (list of floats)
    ///     k: Number of nearest neighbors to return
    ///
    /// Returns:
    ///     List of (node_id, distance) tuples
    fn search_vector(&self, query: Vec<f32>, k: usize) -> PyResult<Vec<(u32, f32)>> {
        let inner = self
            .inner
            .as_ref()
            .ok_or_else(|| classify_nervus_error("database is closed"))?;

        inner
            .search_vector(&query, k)
            .map_err(classify_nervus_error)
    }

    /// Begin a write transaction.
    ///
    /// Returns:
    ///     WriteTxn instance
    pub(crate) fn begin_write(slf: Py<Db>, py: Python<'_>) -> PyResult<WriteTxn> {
        let db_ref = slf.borrow(py);
        let inner = db_ref
            .inner
            .as_ref()
            .ok_or_else(|| classify_nervus_error("database is closed"))?;

        let counter = db_ref.active_write_txns.clone();
        counter.fetch_add(1, Ordering::SeqCst);

        let inner_txn = inner.begin_write();

        Ok(WriteTxn::new(inner_txn, slf.clone_ref(py), counter))
    }

    /// Explicitly closes the DB and performs a checkpoint-on-close.
    pub(crate) fn close(&mut self) -> PyResult<()> {
        if self.active_write_txns.load(Ordering::SeqCst) != 0 {
            return Err(classify_nervus_error(
                "Cannot close database: write transaction in progress",
            ));
        }
        if let Some(inner) = self.inner.take() {
            inner.close().map_err(classify_nervus_error)?;
        }
        Ok(())
    }

    /// Get the database file path.
    #[getter]
    fn path(&self) -> String {
        self.ndb_path.to_string_lossy().to_string()
    }
}

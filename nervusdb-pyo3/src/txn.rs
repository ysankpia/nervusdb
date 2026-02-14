use crate::classify_nervus_error;
use crate::db::Db;
use nervusdb::WriteTxn as RustWriteTxn;
use pyo3::prelude::*;
use std::mem::transmute;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Write transaction for NervusDB.
///
/// All modifications are buffered until commit() is called.
#[pyclass(unsendable)]
pub struct WriteTxn {
    inner: Option<RustWriteTxn<'static>>,
    db: Py<Db>,
    active_write_txns: Arc<AtomicUsize>,
    active: bool,
}

impl WriteTxn {
    pub fn new(txn: RustWriteTxn<'_>, db: Py<Db>, active_write_txns: Arc<AtomicUsize>) -> Self {
        // SAFETY: We hold a strong reference to `db` in the struct, ensuring the owner
        // stays alive as long as this transaction exists. The 'static lifetime is
        // a lie to the compiler, but it's safe because we enforce the lifetime relationship manually.
        let extended_txn = unsafe { transmute::<RustWriteTxn<'_>, RustWriteTxn<'static>>(txn) };
        Self {
            inner: Some(extended_txn),
            db,
            active_write_txns,
            active: true,
        }
    }

    fn finish(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        self.inner = None;
        self.active_write_txns.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Drop for WriteTxn {
    fn drop(&mut self) {
        self.finish();
    }
}

#[pymethods]
impl WriteTxn {
    /// Execute a Cypher write query.
    fn query(&mut self, py: Python<'_>, query: &str) -> PyResult<()> {
        let txn = self
            .inner
            .as_mut()
            .ok_or_else(|| classify_nervus_error("Transaction already finished"))?;

        // Get snapshot from the parent Db
        let db_ref = self.db.borrow(py);
        let inner_db = db_ref
            .inner
            .as_ref()
            .ok_or_else(|| classify_nervus_error("Database is closed"))?;
        let snapshot = inner_db.snapshot();

        let prepared = nervusdb_query::prepare(query).map_err(classify_nervus_error)?;

        prepared
            .execute_write(&snapshot, txn, &nervusdb_query::Params::new())
            .map_err(classify_nervus_error)?;

        Ok(())
    }

    /// Commit the transaction.
    fn commit(&mut self) -> PyResult<()> {
        let txn = self
            .inner
            .take()
            .ok_or_else(|| classify_nervus_error("Transaction already finished"))?;

        let res = txn.commit().map_err(classify_nervus_error);
        self.finish();
        res
    }

    /// Set vector embedding for a node.
    ///
    /// Args:
    ///     node_id: Internal Node ID (u32)
    ///     vector: List of floats
    fn set_vector(&mut self, node_id: u32, vector: Vec<f32>) -> PyResult<()> {
        let txn = self
            .inner
            .as_mut()
            .ok_or_else(|| classify_nervus_error("Transaction already finished"))?;
        txn.set_vector(node_id, vector)
            .map_err(classify_nervus_error)
    }

    /// Rollback the transaction.
    pub(crate) fn rollback(&mut self) {
        self.finish();
    }
}

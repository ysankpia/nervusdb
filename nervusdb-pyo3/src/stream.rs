use pyo3::prelude::*;
use std::collections::{HashMap, VecDeque};

/// Query stream iterator for Python.
///
/// Note: this first version materializes rows in Rust and exposes Python iteration
/// semantics (`for row in db.query_stream(...)`). It keeps the external API stable
/// and allows future internal upgrades to chunked/true streaming without breaking users.
#[pyclass]
pub struct QueryStream {
    rows: VecDeque<HashMap<String, Py<PyAny>>>,
    total_len: usize,
}

impl QueryStream {
    pub fn new(rows: Vec<HashMap<String, Py<PyAny>>>) -> Self {
        let total_len = rows.len();
        Self {
            rows: rows.into(),
            total_len,
        }
    }
}

#[pymethods]
impl QueryStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self) -> Option<HashMap<String, Py<PyAny>>> {
        self.rows.pop_front()
    }

    #[getter]
    fn len(&self) -> usize {
        self.total_len
    }
}

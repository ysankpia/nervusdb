//! Python bindings for NervusDB v2
#![allow(clippy::useless_conversion)]
//!
//! ```python
//! import nervusdb
//!
//! db = nervusdb.Db("my_graph.ndb")
//! db.query("CREATE (n:Person {name: 'Alice'})")
//! result = db.query("MATCH (n:Person) RETURN n.name")
//! ```

use pyo3::prelude::*;

mod db;
mod txn;
mod types;

pub use db::Db;
pub use txn::WriteTxn;

/// Open a NervusDB database.
/// This is a convenience function that aliases Db constructor.
#[pyfunction]
#[pyo3(signature = (path))]
fn open(path: &str) -> PyResult<Db> {
    Db::new(path)
}

/// Initialize the Python module.
#[pymodule]
#[pyo3(name = "nervusdb")]
fn nervusdb_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_class::<Db>()?;
    m.add_class::<WriteTxn>()?;
    m.add_class::<types::Node>()?;
    m.add_class::<types::Relationship>()?;
    m.add_class::<types::Path>()?;
    m.add("__version__", "2.0.0")?;
    Ok(())
}

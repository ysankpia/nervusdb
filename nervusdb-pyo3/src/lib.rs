//! Python bindings for NervusDB v2
#![allow(clippy::useless_conversion)]
//!
//! ```python
//! import nervusdb
//!
//! db = nervusdb.Db("my_graph.ndb")
//! rows = db.query("MATCH (n) RETURN n")
//! for row in db.query_stream("MATCH (n) RETURN n"):
//!     print(row)
//! ```

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

mod db;
mod stream;
mod txn;
mod types;

pub use db::Db;
pub use stream::QueryStream;
pub use txn::WriteTxn;

create_exception!(nervusdb, NervusError, PyException);
create_exception!(nervusdb, SyntaxError, NervusError);
create_exception!(nervusdb, ExecutionError, NervusError);
create_exception!(nervusdb, StorageError, NervusError);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorClass {
    Syntax,
    Execution,
    Storage,
}

fn classify_error_text(msg: &str) -> ErrorClass {
    let lower = msg.to_lowercase();

    if lower.contains("syntax")
        || lower.contains("parse")
        || lower.contains("unexpected token")
        || lower.contains("variabletypeconflict")
        || lower.contains("variablealreadybound")
    {
        ErrorClass::Syntax
    } else if lower.contains("wal")
        || lower.contains("checkpoint")
        || lower.contains("database is closed")
        || lower.contains("cannot close database")
        || lower.contains("io error")
        || lower.contains("permission denied")
        || lower.contains("no such file")
        || lower.contains("disk full")
    {
        ErrorClass::Storage
    } else {
        ErrorClass::Execution
    }
}

pub(crate) fn classify_nervus_error(msg: impl ToString) -> PyErr {
    let msg = msg.to_string();
    match classify_error_text(&msg) {
        ErrorClass::Syntax => SyntaxError::new_err(msg),
        ErrorClass::Storage => StorageError::new_err(msg),
        ErrorClass::Execution => ExecutionError::new_err(msg),
    }
}

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
    m.add_class::<QueryStream>()?;
    m.add_class::<types::Node>()?;
    m.add_class::<types::Relationship>()?;
    m.add_class::<types::Path>()?;

    m.add("NervusError", m.py().get_type_bound::<NervusError>())?;
    m.add("SyntaxError", m.py().get_type_bound::<SyntaxError>())?;
    m.add("ExecutionError", m.py().get_type_bound::<ExecutionError>())?;
    m.add("StorageError", m.py().get_type_bound::<StorageError>())?;

    m.add("__version__", "2.0.0")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{classify_error_text, ErrorClass};

    #[test]
    fn classify_maps_syntax_errors() {
        assert_eq!(
            classify_error_text("syntax error: unexpected token"),
            ErrorClass::Syntax
        );
        assert_eq!(
            classify_error_text("VariableTypeConflict: r"),
            ErrorClass::Syntax
        );
    }

    #[test]
    fn classify_maps_storage_errors() {
        assert_eq!(
            classify_error_text("database is closed"),
            ErrorClass::Storage
        );
        assert_eq!(
            classify_error_text("wal replay failed"),
            ErrorClass::Storage
        );
    }

    #[test]
    fn classify_maps_execution_errors_by_default() {
        assert_eq!(
            classify_error_text("not implemented: expression"),
            ErrorClass::Execution
        );
    }
}

#![allow(unsafe_op_in_unsafe_fn)]

use std::collections::HashMap;
use std::sync::Mutex;

use nervusdb_core::{query, triple::Fact, Database, Error, Options, QueryCriteria, Triple};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule};
use pyo3::Bound;
use pythonize::pythonize;
use serde_json::Value as JsonValue;

fn map_error(err: Error) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}

fn map_poison() -> PyErr {
    PyRuntimeError::new_err("database mutex poisoned")
}

fn with_db_mut<T>(
    db_mutex: &Mutex<Option<Database>>,
    f: impl FnOnce(&mut Database) -> Result<T, Error>,
) -> PyResult<T> {
    let mut guard = db_mutex.lock().map_err(|_| map_poison())?;
    let db = guard
        .as_mut()
        .ok_or_else(|| PyRuntimeError::new_err("database already closed"))?;
    f(db).map_err(map_error)
}

fn triples_from_tuples(values: Vec<(u64, u64, u64)>) -> Vec<Triple> {
    values
        .into_iter()
        .map(|(s, p, o)| Triple::new(s, p, o))
        .collect()
}

fn executor_value_to_json(value: query::executor::Value) -> JsonValue {
    match value {
        query::executor::Value::String(s) => JsonValue::String(s),
        query::executor::Value::Float(f) => serde_json::json!(f),
        query::executor::Value::Boolean(b) => JsonValue::Bool(b),
        query::executor::Value::Null => JsonValue::Null,
        query::executor::Value::Vector(v) => JsonValue::Array(
            v.into_iter()
                .map(|f| {
                    serde_json::Number::from_f64(f as f64)
                        .map(JsonValue::Number)
                        .unwrap_or(JsonValue::Null)
                })
                .collect(),
        ),
        query::executor::Value::Node(id) => serde_json::json!({ "id": id }),
        query::executor::Value::Relationship(triple) => serde_json::json!({
            "subject_id": triple.subject_id,
            "predicate_id": triple.predicate_id,
            "object_id": triple.object_id,
        }),
    }
}

fn extract_params_map(
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<HashMap<String, JsonValue>>> {
    match params {
        Some(obj) => {
            if obj.is_none() {
                return Ok(None);
            }
            let dict: HashMap<String, String> = obj.extract()?;
            Ok(Some(
                dict.into_iter()
                    .map(|(k, v)| (k, JsonValue::String(v)))
                    .collect(),
            ))
        }
        None => Ok(None),
    }
}

#[pyclass(module = "nervusdb", unsendable)]
pub struct DatabaseHandle {
    inner: Mutex<Option<Database>>,
}

#[pymethods]
impl DatabaseHandle {
    #[new]
    fn new(path: &str) -> PyResult<Self> {
        let db = Database::open(Options::new(path)).map_err(map_error)?;
        Ok(Self {
            inner: Mutex::new(Some(db)),
        })
    }

    #[getter]
    fn is_open(&self) -> PyResult<bool> {
        Ok(self.inner.lock().map_err(|_| map_poison())?.is_some())
    }

    fn close(&self) -> PyResult<()> {
        let mut guard = self.inner.lock().map_err(|_| map_poison())?;
        guard.take();
        Ok(())
    }

    fn intern(&self, value: &str) -> PyResult<u64> {
        with_db_mut(&self.inner, |db| db.intern(value))
    }

    fn resolve_id(&self, value: &str) -> PyResult<Option<u64>> {
        with_db_mut(&self.inner, |db| db.resolve_id(value))
    }

    fn resolve_str(&self, identifier: u64) -> PyResult<Option<String>> {
        with_db_mut(&self.inner, |db| db.resolve_str(identifier))
    }

    fn add_fact(&self, subject: &str, predicate: &str, object: &str) -> PyResult<(u64, u64, u64)> {
        let triple = with_db_mut(&self.inner, |db| {
            db.add_fact(Fact::new(subject, predicate, object))
        })?;
        Ok((triple.subject_id, triple.predicate_id, triple.object_id))
    }

    fn batch_add_triples(&self, triples: Vec<(u64, u64, u64)>) -> PyResult<u64> {
        if triples.is_empty() {
            return Ok(0);
        }
        let triples = triples_from_tuples(triples);
        with_db_mut(&self.inner, |db| {
            db.batch_insert(&triples).map(|n| n as u64)
        })
    }

    fn batch_delete_triples(&self, triples: Vec<(u64, u64, u64)>) -> PyResult<u64> {
        if triples.is_empty() {
            return Ok(0);
        }
        let triples = triples_from_tuples(triples);
        with_db_mut(&self.inner, |db| {
            db.batch_delete(&triples).map(|n| n as u64)
        })
    }

    #[pyo3(signature = (query, params=None))]
    fn execute_query(
        &self,
        py: Python<'_>,
        query: &str,
        params: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let params_map = extract_params_map(params)?;
        let rows = with_db_mut(&self.inner, |db| {
            db.execute_query_with_params(query, params_map.clone())
        })?;
        let result: Vec<HashMap<String, JsonValue>> = rows
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|(k, v)| (k, executor_value_to_json(v)))
                    .collect()
            })
            .collect();
        pythonize(py, &result).map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(text_signature = "(subject=None, predicate=None, object=None)")]
    fn query(
        &self,
        subject: Option<u64>,
        predicate: Option<u64>,
        object: Option<u64>,
    ) -> PyResult<Vec<(u64, u64, u64)>> {
        with_db_mut(&self.inner, |db| {
            let iter = db.query(QueryCriteria {
                subject_id: subject,
                predicate_id: predicate,
                object_id: object,
            });
            Ok(iter
                .map(|triple| (triple.subject_id, triple.predicate_id, triple.object_id))
                .collect())
        })
    }

    fn __enter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __exit__(
        &self,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_value: Option<&Bound<'_, PyAny>>,
        _traceback: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<bool> {
        self.close()?;
        Ok(false)
    }
}

#[pyfunction]
fn open(path: &str) -> PyResult<DatabaseHandle> {
    DatabaseHandle::new(path)
}

#[pymodule]
fn nervusdb(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<DatabaseHandle>()?;
    m.add_function(wrap_pyfunction!(open, m)?)?;
    Ok(())
}

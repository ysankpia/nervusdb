#![allow(unexpected_cfgs)]

use graphlite_core::{GraphError, GraphLite as CoreGraphLite, Value};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::{HashMap, HashSet};

create_exception!(graphlite, GraphLiteError, PyException);

fn to_py_err(err: GraphError) -> PyErr {
    GraphLiteError::new_err(err.to_string())
}

fn pyany_to_value(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    if let Ok(b) = obj.extract::<bool>() {
        Ok(Value::from(b))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(Value::from(i))
    } else if let Ok(f) = obj.extract::<f64>() {
        Ok(Value::from(f))
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(Value::from(s))
    } else {
        Ok(Value::from(obj.to_string()))
    }
}

fn value_to_py(py: Python<'_>, val: &Value) -> PyResult<PyObject> {
    match val {
        Value::Int(i) => Ok(i.into_py(py)),
        Value::Float(f) => Ok(f.into_py(py)),
        Value::String(s) => Ok(s.clone().into_py(py)),
        Value::Bool(b) => Ok(b.into_py(py)),
    }
}

fn extract_properties(dict_opt: Option<&Bound<'_, PyDict>>) -> PyResult<HashMap<String, Value>> {
    let mut map = HashMap::new();
    if let Some(dict) = dict_opt {
        for (k, v) in dict.iter() {
            let key = k.extract::<String>()?;
            let val = pyany_to_value(&v)?;
            map.insert(key, val);
        }
    }
    Ok(map)
}

#[pyclass(name = "GraphLite")]
pub struct PyGraphLite {
    inner: CoreGraphLite,
}

#[pymethods]
impl PyGraphLite {
    #[staticmethod]
    #[pyo3(signature = (path, pool_size = 1024))]
    pub fn open(path: &str, pool_size: usize) -> PyResult<Self> {
        let db = CoreGraphLite::open_with_pool_size(path, pool_size).map_err(to_py_err)?;
        Ok(Self { inner: db })
    }

    pub fn execute(&self, py: Python<'_>, cypher: &str) -> PyResult<PyObject> {
        let res = self.inner.execute(cypher).map_err(to_py_err)?;
        let dict = PyDict::new_bound(py);
        dict.set_item("nodes_created", res.nodes_created)?;
        dict.set_item("edges_created", res.edges_created)?;
        dict.set_item("nodes_deleted", res.nodes_deleted)?;
        dict.set_item("edges_deleted", res.edges_deleted)?;
        dict.set_item("properties_set", res.properties_set)?;
        dict.set_item("message", res.message)?;
        Ok(dict.into_py(py))
    }

    pub fn query(&self, py: Python<'_>, cypher: &str) -> PyResult<PyObject> {
        let res = self.inner.query_cypher(cypher).map_err(to_py_err)?;
        let list = PyList::empty_bound(py);

        for row in res.rows {
            let dict = PyDict::new_bound(py);
            for (idx, col_name) in res.columns.iter().enumerate() {
                if let Some(val) = row.values.get(idx) {
                    let py_val = value_to_py(py, val)?;
                    dict.set_item(col_name, py_val)?;
                } else {
                    dict.set_item(col_name, py.None())?;
                }
            }
            list.append(dict)?;
        }

        Ok(list.into_py(py))
    }

    #[pyo3(signature = (labels, properties = None))]
    pub fn add_node(
        &self,
        labels: Vec<String>,
        properties: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<u64> {
        let labels_set: HashSet<String> = labels.into_iter().collect();
        let props_map = extract_properties(properties)?;
        let id = self
            .inner
            .add_node(labels_set, props_map)
            .map_err(to_py_err)?;
        Ok(id)
    }

    #[pyo3(signature = (src, dst, edge_type, properties = None, weight = 1.0))]
    pub fn add_edge(
        &self,
        src: u64,
        dst: u64,
        edge_type: &str,
        properties: Option<&Bound<'_, PyDict>>,
        weight: f64,
    ) -> PyResult<u64> {
        let props_map = extract_properties(properties)?;
        let id = self
            .inner
            .add_edge(src, dst, edge_type, props_map, weight)
            .map_err(to_py_err)?;
        Ok(id)
    }

    #[pyo3(signature = (start, end, edge_type = None))]
    pub fn dijkstra(
        &self,
        start: u64,
        end: u64,
        edge_type: Option<&str>,
    ) -> PyResult<Option<(f64, Vec<u64>)>> {
        let res = self.inner.dijkstra(start, end, edge_type);
        Ok(res)
    }

    pub fn stats(&self, py: Python<'_>) -> PyResult<PyObject> {
        let stats = self.inner.buffer_stats();
        let dict = PyDict::new_bound(py);
        dict.set_item("capacity_frames", stats.capacity_frames)?;
        dict.set_item("used_frames", stats.used_frames)?;
        dict.set_item("dirty_frames", stats.dirty_frames)?;
        dict.set_item("cache_hits", stats.cache_hits)?;
        dict.set_item("cache_misses", stats.cache_misses)?;
        dict.set_item("hit_rate_percentage", stats.hit_rate_percentage)?;
        dict.set_item("disk_reads", stats.disk_reads)?;
        dict.set_item("disk_writes", stats.disk_writes)?;
        dict.set_item("file_size_bytes", stats.file_size_bytes)?;
        Ok(dict.into_py(py))
    }

    pub fn checkpoint(&self) -> PyResult<()> {
        self.inner.checkpoint().map_err(to_py_err)?;
        Ok(())
    }
}

#[pymodule]
fn graphlite(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyGraphLite>()?;
    m.add("GraphLiteError", m.py().get_type_bound::<GraphLiteError>())?;
    Ok(())
}
